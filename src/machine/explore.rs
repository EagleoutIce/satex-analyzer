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
/// Fresh entries per loop point before an entry counts as a re-entry.
const LOOP_ENTRIES: u32 = 16;

/// A choice the running command met: how many arms it has and which one
/// this run takes.
#[derive(Clone, Copy)]
pub(super) struct Choice {
    span: Span,
    by: Sym,
    arms: usize,
    arm: usize,
    /// One of the meanings or texts a join left: on this arm the run holds
    /// only where the join's sources agree with the path.
    joined: bool,
}

/// A path parked between two commands.
#[derive(Clone)]
pub(super) struct Config {
    state: PathState,
    env: PathEnv,
    key: Key,
    tag: Tag,
    /// It reached `\end` or the end of its input.
    ended: bool,
    /// [`Machine::crowd`] tried it against every other waiting path.
    crowded: bool,
}

/// Where a path stands, for joining: its input, bottom first and without
/// exhausted levels, its conditionals and its groups.
#[derive(PartialEq, Clone)]
struct Key {
    progress: u64,
    places: Vec<Place>,
    /// Each conditional's limit and opener, and whether it stands for any
    /// number of them.
    conds: Vec<((CondLimit, Span), bool)>,
    /// Each group's kind, and whether it stands for any number of them.
    groups: Vec<(GroupKind, bool)>,
    scans: Vec<(usize, bool)>,
}

/// What a join of paths at one place loses: which mode, switches and
/// waiting tokens go together (trace partitioning).
#[derive(PartialEq, Clone)]
struct Tag {
    mode: crate::mode::Modes,
    catcodes: CatcodeTable,
    end_line: EndLineChar,
    after_assignment: Option<Tok>,
    streams: Streams,
    after: Vec<Vec<Tok>>,
}

impl Key {
    fn new(state: &PathState, groups: &[(GroupKind, bool, Vec<Tok>)]) -> Key {
        let places = state.input.iter().filter_map(place).collect();
        Key {
            progress: state.source_progress - state.lines_read,
            places,
            conds: state.conds.iter().map(|c| ((c.limit, c.at), c.many)).collect(),
            groups: groups.iter().map(|g| (g.0, g.1)).collect(),
            scans: state.scans.iter().map(Accum::shape).collect(),
        }
    }
}

impl Tag {
    fn new(state: &PathState, groups: Vec<(GroupKind, bool, Vec<Tok>)>) -> Tag {
        Tag {
            mode: state.mode,
            catcodes: state.catcodes.clone(),
            end_line: state.end_line,
            after_assignment: state.after_assignment.map(|t| t.tok),
            streams: state.streams.clone(),
            after: groups.into_iter().map(|g| g.2).collect(),
        }
    }
}

impl Config {
    fn new(state: PathState, env: PathEnv, ended: bool) -> Config {
        let groups = env.groups();
        let (key, tag) = (Key::new(&state, &groups), Tag::new(&state, groups));
        Config { state, env, key, tag, ended, crowded: false }
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
#[derive(Clone)]
struct Head {
    span: Span,
    progress: u64,
    config: Config,
    passes: u32,
    /// The bounded text the last pass here began with.
    bound: Option<u32>,
}

/// Where a loop was entered: its input levels, conditionals and groups
/// then.
#[derive(Clone)]
struct Loop {
    id: LoopId,
    depth: (usize, usize, usize),
    passes: u32,
    /// How many names were assigned before the entry.
    recent: usize,
    /// The input levels below the entry's body, by identity and by place:
    /// a pass reads its next call without reading any of them on.
    levels: Vec<Level>,
    places: Vec<Place>,
    /// The entry's top level and where it stood: the list the entry's
    /// command was read from.
    top: Option<(Level, Place)>,
    /// The join of the states the passes began in, from the entry on.
    joined: Config,
    /// Summarized, its body ran once more from the widened state: what a
    /// pass writes under names the widening opened (`\csname a\the\n`)
    /// is only met by running it.
    rerun: bool,
}

/// One input level, by identity rather than by where it stands. Never by
/// a bare address: a freed level's address is reused, and where depends
/// on allocation order. A list is held, so its address stays its own.
#[derive(Clone)]
enum Level {
    Tokens(Rc<[Token]>),
    Returned(Vec<Token>),
    File(u32),
    Pseudo(Rc<[char]>),
    Boundary,
}

impl PartialEq for Level {
    fn eq(&self, other: &Level) -> bool {
        match (self, other) {
            (Level::Tokens(a), Level::Tokens(b)) => Rc::ptr_eq(a, b),
            (Level::Returned(a), Level::Returned(b)) => a == b,
            (Level::File(a), Level::File(b)) => a == b,
            (Level::Pseudo(a), Level::Pseudo(b)) => Rc::ptr_eq(a, b),
            (Level::Boundary, Level::Boundary) => true,
            _ => false,
        }
    }
}

fn level(frame: &Frame) -> Option<Level> {
    Some(match place(frame)? {
        Place::Tokens(t, _) => Level::Tokens(t),
        Place::Returned(t) => Level::Returned(t),
        Place::File(f, ..) => Level::File(u32::from(f)),
        Place::Pseudo(_, text, _) => Level::Pseudo(text),
        Place::Boundary => Level::Boundary,
    })
}

/// The paths of one [`Machine::drive`].
#[derive(Clone, Default)]
pub(super) struct Paths {
    /// Where they parted: their bindings are kept relative to it.
    fork: Option<Fork>,
    parked: Vec<Config>,
    /// Paths that left what the drive runs, waiting for the others.
    done: Vec<Config>,
    heads: Vec<Head>,
    /// How many paths waited after the last [`Machine::crowd`] that could
    /// not join them all.
    crowded: usize,
    /// Loop entries by content-free identity, with their pass counts: not
    /// undone on rewind, not retired with their heads.
    loops: Vec<Loop>,
    /// Loops summarized in this drive: met again, a path stops there.
    stopped: Vec<Loop>,
    /// Recursions widened past `recursion_depth`, each the join of every
    /// path cut there: one that adds nothing is covered and stops.
    cut: Vec<(Span, Config)>,
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
    /// Paths that ran `\end`, as they ended: the run ends in their state
    /// rather than in one that ran off the end of its input.
    job_ends: Vec<Config>,
    /// Paths that ran off the end of their input, as they ended.
    input_ends: Vec<Config>,
}

/// A dropped path: its error, where, and the diagnostics' counts the
/// command made (rolled back with it, given back when it runs on).
#[derive(Clone)]
struct Shelved {
    config: Config,
    error: String,
    at: Span,
    counts: Counts,
}

/// Diagnostics made, as (counter, how many).
type Counts = Vec<(usize, u32)>;

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

/// What a [`Machine::drive`] holds besides the machine: the state of the
/// command it runs within, and its paths. A checkpoint keeps it.
#[derive(Clone)]
pub(super) struct Drive {
    outer: (Vec<usize>, Vec<Choice>, usize, bool),
    outer_unit: (Unit, bool),
    nested: bool,
    paths: Paths,
    keep_errors: bool,
    mark: u64,
    commands: u64,
}

impl Drive {
    /// Every input stack its paths hold, and whether it is a path's that
    /// may still run, not a state kept for joins: a resumed run reads the
    /// files as they are now on each.
    pub(super) fn inputs(&mut self) -> impl Iterator<Item = (&mut super::Input, bool)> {
        let p = &mut self.paths;
        let live = (p.parked.iter_mut().chain(&mut p.done).chain(&mut p.arrived))
            .chain(p.job_ends.iter_mut().chain(&mut p.input_ends))
            .chain(p.shelved.iter_mut().map(|s| &mut s.config))
            .map(|c| (c, true));
        let kept = (p.heads.iter_mut().map(|h| &mut h.config))
            .chain(p.loops.iter_mut().chain(&mut p.stopped).map(|l| &mut l.joined))
            .chain(p.cut.iter_mut().map(|c| &mut c.1))
            .map(|c| (c, false));
        live.chain(kept).map(|(c, live)| (&mut c.state.input, live))
    }
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
        let i = self.choices.len();
        let scripted = i < self.script.len();
        // Memory is no reason here: it differs between runs, and the hard stop takes it.
        // A cut unwinding to the main loop reads nothing: no choice it
        // meets is one the document makes.
        if !self.replayable
            || i >= usize::from(self.limits.choices)
            || (!scripted && self.runs >= RUNS)
            || self.cutting()
        {
            return None;
        }
        // Replays count: each is work at the site. Only a new choice is
        // refused, so a replay follows the run it repeats.
        if self.hot_site(span) && !scripted {
            return None;
        }
        let arm = self.script.get(i).copied().unwrap_or(0);
        self.out.heat(|h| &mut h.forks, span);
        match self.output_active() {
            true => self.out.output_forks += 1,
            false => self.out.forks += 1,
        }
        self.choices.push(Choice { span, by, arms, arm, joined: false });
        Some(arm)
    }

    /// The arm [`Machine::branch`] gave does not exist, nor any after it:
    /// the path is no run of the document.
    pub(crate) fn no_arm(&mut self) {
        self.infeasible = true;
        self.halted = true;
    }

    /// Whether the running path took an arm over unknown text or over a
    /// meaning or text a join left: what it meets next holds only where
    /// that text has the arm's shape.
    pub(crate) fn took_unknown_arm(&self) -> bool {
        self.choices.iter().any(|c| c.joined || c.by == self.unknown || self.is_unknown_name(c.by))
    }

    /// The choice just made picks one of the meanings or texts a join left.
    pub(crate) fn joined_arm(&mut self) {
        if let Some(c) = self.choices.last_mut() {
            c.joined = true;
        }
    }

    /// Whether the run may be on one of several paths: what it does now
    /// need not happen in every run.
    pub(crate) fn on_paths(&self) -> bool {
        self.parted || self.env.forking() || !self.choices.is_empty()
    }

    /// Run commands on every path until `done` holds for all of them or
    /// they end.  Paths that reach `done` are joined and go on as one.
    pub(super) fn drive(&mut self, unit: Unit, done: impl Fn(&Self) -> bool) {
        // A run resumed from a checkpoint goes on with the drive it kept.
        let Drive { mut outer, outer_unit, nested, mut paths, keep_errors, mut mark, mut commands } =
            self.checkpoints.resume().unwrap_or_else(|| {
                let outer =
                    (std::mem::take(&mut self.script), std::mem::take(&mut self.choices), self.runs, self.replayable);
                let outer_unit = (std::mem::replace(&mut self.unit, unit), self.lone);
                // Inside a command, whose run again undoes what this one counts.
                let nested = self.replayable;
                self.replayable = true;
                let paths = Paths { top: !nested && matches!(unit, Unit::Command), ..Paths::default() };
                Drive {
                    outer,
                    outer_unit,
                    nested,
                    paths,
                    keep_errors: self.keep_errors,
                    mark: self.out.steps,
                    commands: 0,
                }
            });
        let mut ended = false;
        while !self.out.exhausted {
            // Between two commands of the run: the language server answers meanwhile.
            if !ended && paths.top {
                crate::progress::pause();
            }
            if !ended && paths.top && self.checkpoint_due() {
                let (outer, paths) = (outer.clone(), paths.clone());
                self.take_checkpoint(Drive { outer, outer_unit, nested, paths, keep_errors, mark, commands });
            }
            self.charge(&mut mark);
            if !nested {
                self.undo.clear();
            }
            let finished = ended || self.halted || done(self);
            // Once no path compiles, dropped paths run on at once: they are
            // behind the running one, so they meet it and join.
            if (finished || paths.failed)
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
            let peak = match self.output_active() {
                true => &mut self.out.peak_output_paths,
                false => &mut self.out.peak_paths,
            };
            if paths.parked.len() + 1 > *peak {
                *peak = paths.parked.len() + 1;
                if self.cfg.verbose >= 4 {
                    crate::progress::note!("satex: {} paths at {:?}", self.out.peak_paths, self.position_span());
                }
            }
            self.report_progress();
            self.report_paths(&paths, commands);
            self.report_memory(&paths);
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
                    if self.cfg.verbose >= 4 && paths.top {
                        let at = self.position_span();
                        let within: Vec<&str> = self.within.iter().rev().take(6).map(|(s, _)| self.name(*s)).collect();
                        crate::progress::note!(
                            "satex: a path ended at {at:?} in {within:?}, {} waiting",
                            paths.parked.len()
                        );
                    }
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
                // The one path left is no run: it is never where the
                // analysis ends while input is left.
                None if paths.fork.is_none() => match self.infeasible && self.halted && self.input_left() {
                    true => {
                        self.read_on_dropped(Vec::new());
                        continue;
                    }
                    false => break,
                },
                None => {
                    self.parted |= ended || self.halted;
                    if ended && paths.top && !self.infeasible {
                        let fork = paths.fork.as_ref().expect("checked above");
                        let end = Config::new(self.save_path(), self.env.path(fork), true);
                        match self.job_ended {
                            true => paths.job_ends.push(end),
                            false => paths.input_ends.push(end),
                        }
                    }
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
            if paths.parked.len() > self.alike_bound().max(paths.crowded) {
                self.crowd(&mut paths, &fork);
            }
            if let Some(i) = self.behind(&paths, live) {
                if live {
                    let env = self.env.path(&fork);
                    let config = Config::new(self.save_path(), env, ended);
                    // An ended path waits for the run's end, to join there.
                    match (ended && paths.top && !self.infeasible, self.job_ended) {
                        (true, true) => paths.job_ends.push(config),
                        (true, false) => paths.input_ends.push(config),
                        (false, _) => paths.parked.push(config),
                    }
                }
                let path = paths.parked.swap_remove(i);
                ended = path.ended;
                self.resume_config(&fork, path);
            } else if !live && !paths.arrived.is_empty() {
                self.join_arrived(&mut paths, &fork);
            } else if !live {
                let configs = std::mem::take(&mut paths.done);
                if configs.is_empty() {
                    if !ended && self.input_left() {
                        // Paths dropped for an error are the runs left:
                        // they go on ([`Machine::settle`]).
                        if !paths.shelved.is_empty() {
                            (paths.failed, self.halted) = (true, true);
                            paths.fork = Some(fork);
                            continue;
                        }
                        let written = self.env.path(&fork).syms();
                        self.env.unfork(fork);
                        self.read_on_dropped(written);
                        paths.heads.clear();
                        continue;
                    }
                    if ended && !self.out.exhausted {
                        self.end_with_the_job(&mut paths, &fork);
                    }
                    self.halted = true;
                    self.env.unfork(fork);
                    break;
                }
                let joined = self.join_done(&fork, &configs, true);
                // Paths that end apart (other conditionals open, text that
                // cannot be one) are the enclosing command's choice. Past
                // the choices it may make, they are joined all the same:
                // the deepest conditional and group stacks stand for all.
                let joined = match joined {
                    Some(joined) => joined,
                    None => {
                        let span = self.position_span();
                        self.swap_choices(&mut outer);
                        let arm = self.branch(self.unknown, configs.len(), span);
                        self.swap_choices(&mut outer);
                        let forced = match arm {
                            Some(_) => None,
                            None => self.join_done(&fork, &configs, false),
                        };
                        match forced {
                            Some(joined) => {
                                self.diagnose(
                                    Severity::Assumption,
                                    "unjoined-lookahead",
                                    span,
                                    "paths a look ahead parted into end apart past the choices one command may make; they are joined, as deep in conditionals and groups as the deepest".into(),
                                );
                                joined
                            }
                            None => {
                                if arm.is_none() {
                                    self.diagnose(
                                        Severity::Unsupported,
                                        "unjoined-lookahead",
                                        span,
                                        "paths a look ahead parted into end apart in different files past the choices one command may make; the first is followed".into(),
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
                        }
                    }
                };
                ended = false;
                self.resume_config(&fork, joined);
            }
            // Ended paths wait for the run's end: their fork stays.
            let idle = paths.parked.is_empty()
                && paths.done.is_empty()
                && paths.arrived.is_empty()
                && paths.job_ends.is_empty()
                && paths.input_ends.is_empty();
            if idle && paths.shelved.is_empty() {
                self.env.unfork(fork);
                paths.heads.clear();
            } else if idle {
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
        self.keep_errors = keep_errors;
        (self.script, self.choices, self.runs, self.replayable) = outer;
        (self.unit, self.lone) = outer_unit;
    }

    /// The run ends in the join of every path that ended as the last one
    /// did, by `\end` or off the end of its input. A last path that ran off
    /// its input without `\end` while others ran it is a fatal error ("***
    /// (job aborted, no legal \end found)", tex.web § 360): a document that
    /// compiles took none like it, so the run ends as those others did.
    fn end_with_the_job(&mut self, paths: &mut Paths, fork: &Fork) {
        let (jobs, inputs) = (std::mem::take(&mut paths.job_ends), std::mem::take(&mut paths.input_ends));
        let aborted = !self.job_ended && !jobs.is_empty() && self.cfg.compiles();
        if aborted {
            let span = self.position_span();
            self.report_dropped("*** (job aborted, no legal \\end found)", span);
        }
        let mut ends: Vec<Config> = match (aborted, self.job_ended) {
            (true, _) | (false, true) => jobs,
            (false, false) => inputs,
        };
        if !aborted {
            ends.push(Config::new(self.save_path(), self.env.path(fork), true));
        }
        if ends.len() < 2 {
            return;
        }
        let last = match self.join_done(fork, &ends, false) {
            Some(joined) => joined,
            None => ends.last().expect("not empty").clone_config(),
        };
        self.resume_config(fork, last);
    }

    /// Whether a file is still being read and the budget is not spent.
    fn input_left(&self) -> bool {
        !self.out.exhausted && self.input.iter().any(|f| matches!(f, Frame::File { mouth, .. } if !mouth.ending()))
    }

    /// Every path was dropped before the input ended: none was a run of the
    /// document as satex followed it, so what was approximated on the way
    /// went wrong. The run never ends there. It goes on from the last path
    /// as the guard reads on after a cut: what that path changed since the
    /// paths parted (`written`) and what the region assigned is unknown,
    /// the token lists left are given up down to the file, and an unknown
    /// token stands for what they would have done.
    fn read_on_dropped(&mut self, written: Vec<Sym>) {
        let span = self.position_span();
        if self.cfg.verbose >= 4 {
            crate::progress::note!(
                "satex: every path was dropped at {span:?}: halted {} infeasible {} cutting {} levels {}",
                self.halted,
                self.infeasible,
                self.cutting(),
                self.input.len()
            );
        }
        self.diagnose(
            Severity::Imprecision,
            "analysis-ended-early",
            span,
            "every path was dropped here before the input ended; the run goes on after the expansion they were in, and what it assigned is unknown".into(),
        );
        let syms = self.typed_syms();
        self.env.widen(&written, None, &crate::env::typed!(self, syms));
        (self.halted, self.infeasible) = (false, false);
        self.taint_after_cut();
        self.resync_now();
    }

    /// No other path waits, and the running one is finished or went on
    /// with a certain error. Then nothing compiles: the dropped paths run
    /// on, errors kept. Else they were no run of the document.
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
    fn made_counts(&self, mark: usize) -> Counts {
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
    /// still reads what it has left. A number's scan ([`Unit::Number`]) also
    /// needs the same conditionals, groups and scans. `None` when they
    /// cannot be one.
    fn join_done(&mut self, fork: &Fork, configs: &[Config], strict: bool) -> Option<Config> {
        // Forced, the join starts from the path deepest in groups.
        let start = match strict {
            true => 0,
            false => {
                (0..configs.len()).max_by_key(|&k| (configs[k].env.groups().len(), configs[k].state.conds.len()))?
            }
        };
        let mut order: Vec<Config> = vec![configs.get(start)?.clone_config()];
        order.extend(configs.iter().enumerate().filter(|&(k, _)| k != start).map(|(_, c)| c.clone_config()));
        let stacks = strict && matches!(self.unit, Unit::Number);
        let (mut joined, _) =
            self.fold(fork, order, 1, |x, y| same_code(&x.tag, &y.tag) && (!stacks || same_stacks(x, y)));
        (joined.len() == 1).then(|| joined.pop()).flatten()
    }

    /// Run one command on the running path, once for every sequence of
    /// arms its choices allow.  Without `head` it is not checked against
    /// the loop heads: it runs from one.
    fn explore(&mut self, paths: &mut Paths, head: bool) -> Next {
        let start = self.save_path();
        let file = self.file_level();
        let mut fork = self.env.fork();
        let own = paths.fork.is_none();
        let counters = (self.undo.len(), self.env.assignments, self.gaps_raised, self.gap_depth, self.tex_errors);
        self.script.clear();
        self.runs = 0;
        let mut outcomes = Vec::new();
        let mut first = None;
        let mut dropped: Vec<(Config, String, Counts)> = Vec::new();
        // Each run and each re-entry after a cut is paid, as in the drive.
        let mut mark = if head { self.out.steps.wrapping_sub(1) } else { self.out.steps };
        loop {
            self.charge(&mut mark);
            self.choices.clear();
            self.infeasible = false;
            self.at_join = false;
            (self.dropping, self.dropping_possible, self.dropping_breaks) = (None, false, false);
            let going = match self.unit {
                Unit::Command => self.command(),
                unit => self.scan_step(unit),
            };
            self.runs += 1;
            first = first.or(self.choices.first().copied());
            // A run that was cut ends the region it is in: other sequences
            // of arms would only be cut there again.
            let next = next_script(&self.choices, self.infeasible);
            let error = self.dropping.take().filter(|_| paths.top && !self.infeasible);
            // An error that may not be TeX's drops its path only where
            // another is surely a run: one that approximated nothing since
            // the paths parted. Else it goes on, as TeX recovers, unless the
            // recovery breaks library code's brackets: then it is contained.
            let error = match error {
                Some(e) if self.dropping_possible && !Self::exact_other(paths, &outcomes) => {
                    if going && self.dropping_breaks {
                        self.contain_fault(&e, file);
                    }
                    None
                }
                e => e,
            };
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
                if going && paths.top {
                    self.read_pending_fis();
                }
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

    /// Whether choices by `by` at `span` here entered as a loop more than
    /// `LOOP_ENTRIES` times: every further one is a re-entry.
    pub(crate) fn loop_capped(&self, span: Span, by: Sym) -> bool {
        let id = (span, by, self.source_progress - self.lines_read);
        self.scratch.loop_summaries.get(&id).is_some_and(|c| c.1 > LOOP_ENTRIES)
    }

    /// Whether a path other than the running one approximated nothing
    /// since the paths parted.
    fn exact_other(paths: &Paths, outcomes: &[Config]) -> bool {
        let mut others = paths.parked.iter().chain(&paths.arrived).chain(&paths.done).chain(outcomes);
        others.any(|c| !c.state.approximated)
    }

    /// A command that branched, begun in `start`, is a loop head. Met again
    /// in the same place with nothing read since, its state joins the
    /// head's, and paths go on from the join only when it grew. Met deeper
    /// in the input, it is a recursion, cut after as many levels as
    /// widening waits for.
    fn loop_head(&mut self, paths: &mut Paths, choice: Choice, start: PathState, mut outcomes: Vec<Config>) -> Next {
        let Choice { span, by, .. } = choice;
        self.out.heat(|h| &mut h.heads, span);
        let fork = paths.fork.clone().expect("paths parted");
        let progress = start.source_progress;
        let here = Config::new(start, self.env.path(&fork), false);
        let same = |h: &Head| h.span == span && h.progress == progress;
        let widen_after = u32::from(self.limits.widen_after);
        // Forced convergence: every re-entry counts, whatever its content.
        let id = (span, by, progress);
        let depth = (here.key.places.len(), here.key.conds.len(), here.key.groups.len());
        // A re-entry, not a later call: as deep in conditionals and groups
        // or deeper, and its call read from input above the level the
        // entry's call came from, which the pass produced. A sibling call
        // from that level, or from a level made after it ended, is an entry.
        let levels: Vec<Level> = here.state.input.iter().filter_map(level).collect();
        // Per level: a list left with no unknown text, so reading on in it
        // ends after as many commands as it has tokens.
        let known: Vec<bool> = live_frames(&here.state.input)
            .into_iter()
            .map(|f| {
                pending(&[f]).is_some_and(|t| !t.iter().any(|t| t.cs().is_some_and(|s| self.is_unknown_marker(s))))
            })
            .collect();
        let reentry = |l: &Loop| {
            l.id == id && l.depth.1 <= depth.1 && l.depth.2 <= depth.2 && {
                // Bottom up, the entry's levels still there: the same
                // list, or one as far into the same text. One read on
                // since means the call came after the entry ended.
                let (mut m, mut read) = (0, false);
                while m < l.levels.len().min(levels.len()) {
                    let equal = here.key.places.get(m) == Some(&l.places[m]);
                    if !equal {
                        read = l.levels[m] == levels[m];
                        break;
                    }
                    m += 1;
                }
                // A later command read on from the list the entry's
                // command came from, a list with no unknown text left,
                // as deep in conditionals and groups: the entry's call
                // ended, a sibling call (tex.web §§ 321-322: a list is
                // read on only once what came of it is done). Read from a
                // list that list called, it is a recursion: no bound.
                let sibling = !read
                    && m == l.levels.len()
                    && levels.len() == m + 1
                    && known.get(m) == Some(&true)
                    && (depth.1, depth.2) == (l.depth.1, l.depth.2)
                    && l.top.as_ref().is_some_and(|(top, at)| {
                        levels.get(m) == Some(top) && here.key.places.get(m).is_some_and(|p| p != at)
                    });
                // A level below the entry's top read on: every level
                // above it ended first (tex.web § 325), the entry's call
                // too, however deep this call runs.
                m > 0 && !sibling && !read
            }
        };
        // A summarized loop met again inside its own levels stops the path.
        // The summary's own path met the loop again: never dropped, it is
        // summarized again at once. What the loop writes was widened by the
        // first summary already. Met again and again, one more token
        // level below the entry goes each time, never a file.
        if self.cfg.verbose >= 4 && !here.state.summary_of.is_empty() {
            crate::progress::note!(
                "satex: head \\{} {span:?} at {depth:?} with summaries {:?}, stopped {:?}",
                self.name(by),
                here.state.summary_of.contains(&id),
                paths.stopped.iter().map(|l| (l.id == id, reentry(l))).collect::<Vec<_>>()
            );
        }
        if here.state.summary_of.contains(&id)
            && let Some(l) = paths.stopped.iter().position(reentry)
        {
            if !std::mem::replace(&mut paths.stopped[l].rerun, true) {
                return Next::Many(outcomes);
            }
            let entry = paths.stopped[l].depth;
            let exits =
                paths.parked.iter().chain(&paths.arrived).chain(&paths.done).any(|p| p.state.summary_of.contains(&id));
            paths.stopped[l].passes += 1;
            // Unwound to the entry, never below it: the callers' frames hold
            // their own closers (`\fi`, `\endgroup`) and the code after it.
            self.resume_config(&fork, here);
            let before = (self.input.iter().filter_map(place).count(), self.conds.len(), self.env.depth());
            self.unwind(entry, span);
            // Nothing left to unwind (source on top): one more unknown
            // would decide this head again with nothing read. The branch
            // runs as any other.
            if before == (self.input.iter().filter_map(place).count(), self.conds.len(), self.env.depth()) {
                // At the entry already: a path past the summary carries
                // the continuation, so this pass is covered and ends. Met
                // past `widen_after` times, it is the continuation itself:
                // widened and cut ([`Machine::end_covered`]).
                if exits || paths.stopped[l].passes > widen_after {
                    let here = Config::new(self.save_path(), self.env.path(&fork), false);
                    return self.end_covered(paths, &fork, l, here, span, depth);
                }
                return Next::Many(outcomes);
            }
            self.approximated = true;
            self.taint_after_cut();
            let unknown = self.unknown_text(Unk::Any, span);
            self.push_tokens(Rc::from(vec![unknown]));
            return self.explore(paths, false);
        }
        if let Some(l) = paths.stopped.iter().position(reentry) {
            // A pass of a loop already summarized: the summary's path went on
            // past it. What this pass wrote may hold anything on every path
            // waiting, which carries the summary's continuation; then it ends.
            return self.end_covered(paths, &fork, l, here, span, depth);
        }
        // A fresh entry (a later call, not a pass) starts its count anew,
        // but only `LOOP_ENTRIES` times per point: past that, an entry is a
        // re-entry, so every point is summarized after finitely many passes.
        let fresh = !paths.loops.iter().any(|l| l.id == id && reentry(l));
        let counts = self.scratch.loop_summaries.entry(id).or_default();
        counts.1 += u32::from(fresh);
        let (summaries, capped) = (counts.0, counts.1 > LOOP_ENTRIES);
        // An entry whose call ended is never unwound to.
        paths.loops.retain(|l| l.id != id || reentry(l));
        if !capped {
            paths.stopped.retain(|l| l.id != id);
        }
        // Summarized in another drive: cut at its first re-entry.
        let k = match summaries > 0 {
            true => 1,
            false => widen_after.max(1),
        };
        // What this path's pass began with, carried on by its arms.
        let bound = self.input_bound(&here.state.input);
        for c in outcomes.iter_mut() {
            let bounds = Rc::make_mut(&mut c.state.loop_bounds);
            bounds.retain(|(l, _)| *l != id);
            bounds.extend(bound.map(|b| (id, b)));
        }
        let entry = paths.loops.iter().find(|l| l.id == id).map(|l| (l.depth.0, l.depth.1, l.passes));
        let forced = match paths.loops.iter().position(|l| l.id == id) {
            Some(l) => {
                let (joined, _) = self.join_configs(&fork, &paths.loops[l].joined, &here, false, true);
                // A pass on strictly less bounded text is not counted:
                // such passes end by construction, each state exact.
                let before = here.state.loop_bounds.iter().find(|(l, _)| *l == id).map(|(_, b)| *b);
                let shrinks = matches!((before, bound), (Some(a), Some(b)) if b < a);
                let l = &mut paths.loops[l];
                l.passes += u32::from(!shrinks);
                l.joined = joined;
                l.passes >= k
            }
            None => {
                let below = levels.len().saturating_sub(1);
                let places: Vec<Place> = here.state.input.iter().filter_map(place).take(below).collect();
                let top = levels.get(below).cloned().zip(here.state.input.iter().filter_map(place).nth(below));
                let levels = levels[..below].to_vec();
                let joined = here.clone_config();
                paths.loops.push(Loop {
                    id,
                    depth,
                    passes: 0,
                    recent: self.env.recent_len(),
                    levels,
                    places,
                    top,
                    joined,
                    rerun: false,
                });
                keep_last(&mut paths.loops, self.limits.loop_heads);
                capped
            }
        };
        // A tail call from an `\else` arm re-enters under frames that hold
        // only `\fi`: compared without them and the conditionals they close.
        let syms = self.typed_syms();
        // A path that differs from a head in code it will still run (pgfmath
        // with a unary minus on its stack or without) is a head of its own.
        // Past `limits.heads_apart` such heads at one place they are joined.
        let crowded = paths.heads.iter().filter(|h| same(h)).count() >= usize::from(self.limits.heads_apart);
        let mut fits: Vec<bool> = {
            let typed = crate::env::typed!(self, syms);
            let slot = |sym: Sym| self.name(sym).starts_with(|c: char| c < ' ');
            // Only heads at this place are ever met here: the others are
            // never compared, a costly comparison of their bindings.
            paths.heads.iter().map(|h| same(h) && (crowded || data_apart(&h.config, &here, &typed, &slot))).collect()
        };
        // Forced, and no head kept apart is this one: it joins every head
        // here. A join that converges by widening needs no summary.
        if forced && !paths.heads.iter().zip(&fits).any(|(h, f)| *f && h.config.key == here.key) {
            fits = paths.heads.iter().map(same).collect();
        }
        // Forced, and a head here differs only in being fewer groups of one
        // kind deep: both stand in any number of them from where they part
        // (stacks as intervals), so the passes meet.
        let mut here = here;
        for (h, _) in paths.heads.iter_mut().zip(&fits).filter(|(h, f)| forced && **f && same(h)) {
            if let Some((head, widened)) = self.widen_stacks(&fork, &h.config, &here) {
                (h.config, here) = (head, widened);
                break;
            }
        }
        let is_fi =
            |tok: Tok| matches!(tok, Tok::Cs(s) if matches!(self.env.meaning(s), Meaning::Primitive(Primitive::Fi)));
        let here_bare = entry.and_then(|e| fi_tail(&here.key, (e.0, e.1), &is_fi));
        let bare = |k: &Key| entry.and_then(|e| fi_tail(k, (e.0, e.1), &is_fi)).unwrap_or_else(|| k.clone());
        // A path that differs from a head in code it will still run (pgfmath
        // with a unary minus on its stack or without) is a head of its own.
        let heads = || paths.heads.iter().enumerate().filter(|(i, _)| fits[*i]);
        let exact = heads().find(|(_, h)| same(h) && h.config.key == here.key).map(|(i, _)| i).or_else(|| {
            let b = here_bare.as_ref()?;
            heads().find(|(_, h)| same(h) && bare(&h.config.key) == *b).map(|(i, _)| i)
        });
        let folded_exact = exact.is_some_and(|h| paths.heads[h].config.key != here.key);
        // A head at the same place whose input differs only in the digits
        // of its token lists (a loop whose counter is in its argument,
        // expl3's `\__flag_height_loop:wN`): the same head, the counter
        // read as unknown digits ([`widen_input`]). Without this every
        // pass is a new head and none ever meets. Texts that differ in
        // more (pgfmath's parser, its expression shrinking) are new heads,
        // bounded by `recursion_depth` as before.
        let typed = crate::env::typed!(self, syms);
        let alike = exact
            .is_none()
            .then(|| heads().find(|(_, h)| same(h) && alike_places(&h.config.key, &here.key, &typed)).map(|(i, _)| i))
            .flatten();
        let folded_alike = alike.is_none() && exact.is_none() && here_bare.is_some();
        let alike = alike.or_else(|| {
            let b = here_bare.as_ref().filter(|_| exact.is_none())?;
            heads().find(|(_, h)| same(h) && alike_places(&bare(&h.config.key), b, &typed)).map(|(i, _)| i)
        });
        let folded = folded_exact || (folded_alike && alike.is_some());
        // Alike after folding is the fixpoint already: forced at once.
        let forced = forced || (folded && entry.is_some_and(|e| e.2 >= 1));
        let (forced_new, forced_join) = (forced, forced);
        // What the head compares with: here, its `\fi` tail dropped.
        let here_input = match folded {
            true => fi_frames_dropped(&here.state.input, entry.map_or(0, |e| e.0), &is_fi),
            false => here.state.input.clone(),
        };
        // Heads at one place whose token lists differ in more than digits
        // (an argument growing every pass): past `widen_after` of them, the
        // newest is the same head, its input widened.
        // A head whose input held more bounded text is an earlier pass of
        // a loop that ends by construction: neither kin nor deeper.
        let shrunk = |h: &Head| matches!((h.bound, bound), (Some(a), Some(b)) if b < a);
        let kin = |h: &Head| {
            let (a, b) = (&h.config.key, &here.key);
            let text = |p: &Place| matches!(p, Place::Tokens(..) | Place::Returned(..));
            same(h)
                && !shrunk(h)
                && (&a.conds, &a.groups, &a.scans) == (&b.conds, &b.groups, &b.scans)
                && a.places.len() == b.places.len()
                && a.places.iter().zip(&b.places).all(|(x, y)| x == y || (text(x) && text(y)))
        };
        let alike = alike.or_else(|| {
            let kins = if exact.is_none() { heads().filter(|(_, h)| kin(h)).count() } else { 0 };
            (kins > usize::from(self.limits.widen_after))
                .then(|| heads().rfind(|(_, h)| kin(h)).map(|(i, _)| i))
                .flatten()
        });
        // Whether the head's input grew by the widening: what it reads
        // next is then not covered by what it ran with.
        let mut input_grew = false;
        let (h, here) = match (exact, alike) {
            (Some(h), _) => (h, here),
            (None, Some(h)) => {
                let widen = paths.heads[h].passes >= widen_after;
                match self.widen_input(&paths.heads[h].config.state.input, &here_input, span, widen, true) {
                    Some(input) => {
                        let head = &mut paths.heads[h];
                        let widened = Config::new(
                            PathState { input: input.clone(), ..head.config.state.clone() },
                            head.config.env.clone(),
                            false,
                        );
                        input_grew = widened.key.places != head.config.key.places;
                        head.config = widened;
                        head.bound = bound;
                        (h, Config::new(PathState { input, ..here.state }, here.env, false))
                    }
                    None => (usize::MAX, here),
                }
            }
            (None, None) => (usize::MAX, here),
        };
        // A summary runs the body once more from the widened state, unless
        // a scan is in flight: that drive ends and would summarize anew.
        let cut = !here.key.scans.is_empty();
        if forced_new && (h == usize::MAX || (!folded && paths.heads[h].config.key != here.key)) {
            return self.summarize(paths, &fork, here, id, span, by, cut);
        }
        if h == usize::MAX {
            // Met again inside its own input, it is a recursion: every
            // frame below the one it ran from is still there.
            let deeper = |h: &Head| {
                let below = &h.config.key.places[..h.config.key.places.len().saturating_sub(1)];
                same(h)
                    && !shrunk(h)
                    && h.config.key.places.len() < here.key.places.len()
                    && here.key.places.starts_with(below)
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
                let (joined, _) = self.join_configs(&fork, &paths.heads[level].config, &here, true, false);
                self.resume_config(&fork, Config::new(here.state, joined.env, false));
                while self.input.iter().filter_map(place).count() > depth {
                    self.pop_frame();
                }
                let unknown = self.unknown_text(Unk::Any, span);
                self.scratch.summarized.insert(span);
                self.push_tokens(Rc::from(vec![unknown]));
                // Every path cut here joins one summary, widened, so the
                // cuts reach a fixpoint instead of each running on.
                let cut = Config::new(self.save_path(), self.env.path(&fork), false);
                let typed = crate::env::typed!(self, syms);
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
                    let Some(input) = self.widen_input(&paths.cut[i].1.state.input, &cut.state.input, span, true, true)
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
                let (joined, growth) = self.join_configs(&fork, &paths.cut[i].1, &cut, true, false);
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
            let typed = crate::env::typed!(self, syms);
            let live = |h: &Head| {
                let on = |key: &Key| !left(&h.config.key, key, &typed);
                on(&here.key) || paths.parked.iter().chain(&paths.arrived).any(|p| on(&p.key))
            };
            paths.heads.retain(|h| h.progress >= progress && live(h));
            paths.heads.push(Head { span, progress, config: here, passes: 0, bound });
            keep_last(&mut paths.heads, self.limits.loop_heads);
            self.out.loop_heads = self.out.loop_heads.max(paths.heads.len());
            return Next::Many(outcomes);
        }
        // A pass forced by the count of passes is joined widened first: if
        // that adds nothing the loop is at its fixpoint, and one widened
        // pass more may reach it; only one still growing is summarized.
        let widen = paths.heads[h].passes >= widen_after || forced_join;
        let (joined, mut growth) = self.join_configs(&fork, &paths.heads[h].config, &here, widen, !widen);
        if input_grew {
            growth.parts.push("input");
        }
        // The head ran on its join's arms ([`crate::env::Corr`]): a pass
        // in none of them is not covered, though the join holds it.
        if !self.env.covers(&fork, &paths.heads[h].config.env, &here.env) {
            growth.parts.push("arms");
        }
        if !growth.any() {
            return Next::None;
        }
        let forced_join = forced_join && paths.heads[h].passes > widen_after;
        // Still growing when widened, pass after pass: the widening does
        // not converge, so the loop is summarized (forced convergence).
        let diverges = widen && paths.heads[h].passes >= widen_after.saturating_mul(2);
        if forced_join || (diverges && paths.loops.iter().any(|l| l.id == id)) {
            return self.summarize(paths, &fork, here, id, span, by, cut);
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
        head.bound = bound;
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

    /// Two paths that stand in the same place, in fewer or more groups or
    /// conditionals of one kind above where they part: both widened to one
    /// standing for any number of that kind from there (stacks as
    /// intervals, [`crate::env::PathEnv::many_from`]). Only groups opened
    /// since the fork, plain ones with the same tokens waiting.
    fn widen_stacks(&mut self, fork: &Fork, a: &Config, b: &Config) -> Option<(Config, Config)> {
        let (x, y) = (&a.key, &b.key);
        if (x.progress, &x.places, &x.scans) != (y.progress, &y.places, &y.scans)
            || (x.conds == y.conds && x.groups == y.groups)
        {
            return None;
        }
        let groups = match x.groups == y.groups {
            true => None,
            false => {
                let (from, kind) = many_from(&x.groups, &y.groups)?;
                let (ga, gb) = (a.env.groups(), b.env.groups());
                let after = ga.get(from).or(gb.get(from)).map(|g| g.2.clone()).unwrap_or_default();
                let fits = from >= fork.depth()
                    && ga[from..].iter().chain(&gb[from..]).all(|g| g.2 == after)
                    && a.env.plain_from(from)
                    && b.env.plain_from(from);
                let waiting = if ga.len() > from { a.env.after_at(from) } else { b.env.after_at(from) };
                Some(fits.then(|| (from, kind, waiting, self.env.next_group_id()))?)
            }
        };
        let conds = match x.conds == y.conds {
            true => None,
            false => {
                let (from, _) = many_from(&x.conds, &y.conds)?;
                let frame = a.state.conds.get(from).or(b.state.conds.get(from))?.clone();
                let kind = |c: &[CondFrame]| c[from.min(c.len())..].iter().all(|f| f.kind == frame.kind);
                Some((kind(&a.state.conds) && kind(&b.state.conds)).then_some((from, frame))?)
            }
        };
        let widen = |c: &Config| {
            let mut env = c.env.clone();
            if let Some((from, kind, waiting, id)) = &groups {
                env.many_from(*from, *kind, waiting, *id);
            }
            let mut state = c.state.clone();
            if let Some((from, frame)) = &conds {
                let frames = Rc::make_mut(&mut state.conds);
                let deps = frames.iter().skip(from + 1).filter(|f| f.dep).count();
                match frames.get_mut(*from) {
                    Some(f) => f.many = true,
                    None => frames.push(CondFrame { many: true, dep: false, ..frame.clone() }),
                }
                frames.truncate(from + 1);
                let cds = Rc::make_mut(&mut state.cds);
                cds.truncate(cds.len().saturating_sub(deps));
            }
            Config::new(state, env, c.ended)
        };
        Some((widen(a), widen(b)))
    }

    /// A loop met `widen_after` times is summarized: what it wrote may hold
    /// anything, and its body runs once more from there (`cut` cuts at
    /// once). Then its input unwinds to where it was entered and reads one
    /// unknown token, and conditionals and groups it opened are assumed
    /// closed. Met again in this drive, it stops.
    #[allow(clippy::too_many_arguments)]
    fn summarize(
        &mut self,
        paths: &mut Paths,
        fork: &Fork,
        here: Config,
        id: LoopId,
        span: Span,
        by: Sym,
        cut: bool,
    ) -> Next {
        let Some(l) = paths.loops.iter().position(|l| l.id == id) else { return Next::None };
        let mut summarized = paths.loops.swap_remove(l);
        let depth = summarized.depth;
        if self.cfg.verbose >= 4 {
            crate::progress::note!("satex: loop \\{} at {span:?} summarized, entered at {depth:?}", self.name(by));
        }
        self.scratch.loop_summaries.entry(id).or_default().0 += 1;
        // What the passes began in, joined and widened: a name the passes
        // agree on keeps its value, one they differ in may hold any of them
        // or more.
        let (joined, _) = self.join_configs(fork, &summarized.joined, &here, true, true);
        // What a name the passes began with and this one disagree on would
        // hold after the passes not run is not known: the passes need not
        // have met a head to widen at (each deeper in conditionals or
        // groups), so their join only lists what they held so far.
        let began = std::mem::replace(&mut summarized.joined, here.clone_config());
        let written = here.env.differing_over(&began.env, |s| Some(self.env.at_fork(fork, s)));
        paths.stopped.push(summarized);
        keep_last(&mut paths.stopped, self.limits.loop_heads);
        self.resume_config(fork, Config::new(here.state, joined.env, false));
        let syms = self.typed_syms();
        self.env.widen(&written, None, &crate::env::typed!(self, syms));
        let here = Config::new(self.save_path(), self.env.path(fork), false);
        let env = here.env.clone();
        self.cut_loop(paths, fork, here, env, id, depth, span, by, cut)
    }

    /// A pass of a loop already summarized, met again: the summary's path
    /// went on past it. What this pass wrote may hold anything on every path
    /// waiting, which carries the summary's continuation; then it ends.
    fn end_covered(
        &mut self,
        paths: &mut Paths,
        fork: &Fork,
        l: usize,
        here: Config,
        span: Span,
        depth: (usize, usize, usize),
    ) -> Next {
        // A name the pass wrote and certainly put back to what the summary
        // holds (a nesting depth up and down again) stays as it is. An
        // unknown or joined value equal to the summary's is no such value:
        // it may differ in what it stands for, with the names it goes with.
        let summary = paths.stopped[l].joined.env.clone();
        let differ: std::collections::HashSet<Sym> =
            self.env.path(fork).differing_over(&summary, |s| Some(self.env.at_fork(fork, s))).into_iter().collect();
        let mut written = self.env.recent_since(paths.stopped[l].recent);
        written.retain(|s| differ.contains(s) || !self.env.slot(*s).is_some_and(|b| b.certain));
        let syms = self.typed_syms();
        if self.cfg.verbose >= 4 {
            crate::progress::note!("satex: loop at {span:?} met again at {depth:?}, covered by its summary");
        }
        // No waiting path went on past the summary (it ended, or the paths
        // waiting are other arms, not this loop's continuation): this pass
        // is the only continuation. It is cut, widened, and goes on with
        // an unknown token.
        let id = paths.stopped[l].id;
        let carried =
            paths.parked.iter().chain(&paths.arrived).chain(&paths.done).any(|p| p.state.summary_of.contains(&id));
        if !carried {
            let mut entry = paths.stopped[l].depth;
            // Met again with no source read since the last cut: the cut
            // left the text that leads back here. Each such meet drops one
            // more input level; with only source left, the line goes.
            let again = match self.scratch.recut {
                Some((s, p, k)) if s == span && p == here.key.progress => k + 1,
                _ => 0,
            };
            self.scratch.recut = Some((span, here.key.progress, again));
            entry.0 = entry.0.min(depth.0).saturating_sub(again as usize);
            if again > 0
                && entry.0
                    < self.input.iter().filter(|f| matches!(f, Frame::File { .. } | Frame::Pseudo { .. })).count()
            {
                let at = self.position_span();
                self.skip_line(at);
            }
            self.resume_config(fork, here);
            self.env.widen(&written, Some(&summary), &crate::env::typed!(self, syms));
            let here = Config::new(self.save_path(), self.env.path(fork), false);
            let env = here.env.clone();
            return self.cut_loop(paths, fork, here, env, id, entry, span, id.1, true);
        }
        for i in 0..paths.parked.len() + paths.arrived.len() + paths.done.len() {
            let (n, m) = (paths.parked.len(), paths.arrived.len());
            let c = match i {
                i if i < n => &paths.parked[i],
                i if i < n + m => &paths.arrived[i - n],
                i => &paths.done[i - n - m],
            };
            let ended = c.ended;
            self.resume_config(fork, c.clone_config());
            self.env.widen(&written, Some(&summary), &crate::env::typed!(self, syms));
            self.wild.union(&here.state.wild);
            let c = Config::new(self.save_path(), self.env.path(fork), ended);
            match i {
                i if i < n => paths.parked[i] = c,
                i if i < n + m => paths.arrived[i - n] = c,
                i => paths.done[i - n - m] = c,
            }
        }
        self.resume_config(fork, here);
        Next::None
    }

    /// A loop the verification pass did not close: its input unwinds to
    /// where it was entered and reads one unknown token.
    #[allow(clippy::too_many_arguments)]
    fn cut_loop(
        &mut self,
        paths: &mut Paths,
        fork: &Fork,
        here: Config,
        env: PathEnv,
        id: LoopId,
        depth: (usize, usize, usize),
        span: Span,
        by: Sym,
        cut: bool,
    ) -> Next {
        self.resume_config(fork, Config::new(here.state, env, false));
        if cut {
            self.unwind(depth, span);
        }
        self.diagnose(
            Severity::Imprecision,
            "recursion-widened",
            span,
            format!(
                "\\{} decides a loop met {} times; it is summarized: what it writes may hold anything",
                self.name(by),
                self.limits.widen_after
            ),
        );
        self.approximated = true;
        self.taint_after_cut();
        self.scratch.summarized.insert(span);
        if !self.scratch.summary_of.contains(&id) {
            Rc::make_mut(&mut self.scratch.summary_of).push(id);
        }
        if cut {
            let unknown = self.unknown_text(Unk::Any, span);
            self.push_tokens(Rc::from(vec![unknown]));
        }
        self.explore(paths, false)
    }

    /// How many conditionals and groups `t` closes, less those it opens.
    fn closing(&self, t: &Token) -> (i64, i64) {
        self.closing_in(t, &mut Vec::new())
    }

    /// [`Machine::closing`], through parameterless macros (`\close` for
    /// `\endgroup`) not met already in `seen`.
    fn closing_in(&self, t: &Token, seen: &mut Vec<Sym>) -> (i64, i64) {
        use crate::builtins::Primitive as P;
        let char_group = |cat: crate::tex::Catcode| match cat {
            crate::tex::Catcode::Begin => (0, -1),
            crate::tex::Catcode::End => (0, 1),
            _ => (0, 0),
        };
        match t.tok {
            Tok::Chr(_, cat) => char_group(cat),
            Tok::Cs(sym) => match self.env.meaning(sym) {
                Meaning::Primitive(P::Fi) => (1, 0),
                Meaning::Primitive(P::If(_)) => (-1, 0),
                Meaning::Primitive(P::BeginGroup) => (0, -1),
                Meaning::Primitive(P::EndGroup) => (0, 1),
                Meaning::Char(_, cat) => char_group(cat),
                Meaning::Macro(m) if m.parameter_text.items.is_empty() && !seen.contains(&sym) => {
                    seen.push(sym);
                    let sum = m.replacement_text.iter().map(|t| self.closing_in(t, seen));
                    let sum = sum.fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
                    seen.pop();
                    sum
                }
                _ => (0, 0),
            },
            _ => (0, 0),
        }
    }

    /// Drop the token lists above `depth.0` input levels, never source
    /// text, and close what their text opened, by identity: the
    /// conditionals and groups opened since the lowest dropped list was
    /// pushed ([`Machine::close_since`]). With no list dropped, those since
    /// `depth.1` conditionals and `depth.2` groups.
    pub(super) fn unwind(&mut self, depth: (usize, usize, usize), span: Span) {
        self.leak_call_recs();
        let mut at = None;
        // What the dropped text would itself have closed: closing that much
        // is what running it does, no assumption.
        let mut closes = (0i64, 0i64);
        while self.input.iter().filter_map(place).count() > depth.0
            && matches!(self.input.last(), Some(Frame::Tokens { .. } | Frame::Returned(_)))
        {
            let left: Vec<Token> = match self.input.last() {
                Some(Frame::Tokens { toks, pos, .. }) => toks[(*pos).min(toks.len())..].to_vec(),
                Some(Frame::Returned(t)) => t.clone(),
                _ => Vec::new(),
            };
            for t in &left {
                let (c, g) = self.closing(t);
                closes = (closes.0 + c, closes.1 + g);
            }
            at = lower(at, self.input.last());
            self.pop_frame();
        }
        self.mark_cut_scans();
        let at = at.unwrap_or((depth.1, self.env.group_id_at(depth.2), 0));
        // What the dropped text leaves open: any number of what the passes
        // opened, when one kind (stacks as intervals).
        let from = self.env.depth_upto(at.1);
        let open = (self.conds.len().saturating_sub(at.0) as i64, (self.env.depth() - from) as i64);
        if (open.0 > closes.0 || open.1 > closes.1) && self.many_since(at.0, from) {
            return;
        }
        let (groups, conds) = self.close_since(at);
        if (groups, conds) != (0, 0) && (conds as i64 > closes.0 || groups as i64 > closes.1) {
            self.diagnose(
                Severity::Assumption,
                "unbalanced-path",
                span,
                "a loop opened conditionals or groups it was summarized in; they are assumed closed".into(),
            );
        }
    }

    /// The conditionals above `conds` and the groups from `groups` up,
    /// each as one standing for any number of them, when each is of one
    /// kind; whether they were.
    fn many_since(&mut self, conds: usize, groups: usize) -> bool {
        let frames = &self.conds[conds.min(self.conds.len())..];
        let alike = frames.iter().all(|f| (f.limit, f.at, f.kind) == (frames[0].limit, frames[0].at, frames[0].kind));
        if !alike || !self.env.many_since(groups) {
            return false;
        }
        if conds < self.conds.len() {
            let frames = Rc::make_mut(&mut self.conds);
            let deps = frames.iter().skip(conds + 1).filter(|f| f.dep).count();
            frames[conds].many = true;
            frames.truncate(conds + 1);
            let cds = Rc::make_mut(&mut self.cds);
            cds.truncate(cds.len().saturating_sub(deps));
        }
        true
    }

    /// The closers in `text` of conditionals opened before it (`\fi`,
    /// `\else`, `\or`), and how many conditionals it leaves open.
    fn strays(&self, text: &[Token]) -> (Vec<usize>, usize) {
        let mut open = crate::machine::CondDepth::default();
        let strays = (0..text.len()).filter(|&i| open.feed(self.cond_prim(text[i])).is_some()).collect();
        (strays, open.0)
    }

    /// Whether `text` can be one token: its braces and its conditionals
    /// close inside it.
    fn whole(&self, text: &[Token]) -> bool {
        self.strays(text) == (Vec::new(), 0) && balanced(text)
    }

    /// Per length `k`, whether `text[..k]` is [`Self::whole`], in one pass.
    fn whole_prefixes(&self, text: &[Token]) -> Vec<bool> {
        let (mut open, mut depth, mut broken) = (crate::machine::CondDepth::default(), 0i64, false);
        let mut out = vec![true];
        for t in text {
            broken |= open.feed(self.cond_prim(*t)).is_some();
            depth += match t.tok {
                Tok::Chr(_, Catcode::Begin) => 1,
                Tok::Chr(_, Catcode::End) => -1,
                _ => 0,
            };
            broken |= depth < 0;
            out.push(!broken && depth == 0 && open.0 == 0);
        }
        out
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
        let (groups, growth) = self.fold(fork, arrived, 1, |a, b| same_stacks(a, b) && same_code(&a.tag, &b.tag));
        let (mut slots, mut parts) = (growth.slots, growth.parts);
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

    /// `configs` with each joined into the first before it that `same`
    /// holds for and whose input can be one with it ([`Machine::join_two`]),
    /// until no more than `limit` are left. Also what the joins grew.
    fn fold(
        &mut self,
        fork: &Fork,
        configs: Vec<Config>,
        limit: usize,
        same: impl Fn(&Config, &Config) -> bool,
    ) -> (Vec<Config>, Growth) {
        let mut growth = Growth::default();
        let mut out: Vec<Config> = Vec::new();
        let mut rest = configs.into_iter();
        while let Some(b) = rest.next() {
            if out.len() + rest.len() < limit {
                out.push(b);
                continue;
            }
            let joined = (0..out.len())
                .filter(|&i| same(&out[i], &b))
                .find_map(|i| Some((i, self.join_two(fork, &out[i], &b)?)));
            match joined {
                Some((i, (c, g))) => {
                    out[i] = c;
                    growth.slots.extend(g.slots);
                    growth.parts.extend(g.parts);
                }
                None => out.push(b),
            }
        }
        (out, growth)
    }

    /// Two paths as one ([`Machine::join_configs`]), their input too: kept
    /// where both read the same, else one text ([`Machine::widen_input`]).
    /// `None` when it cannot be one.
    fn join_two(&mut self, fork: &Fork, a: &Config, b: &Config) -> Option<(Config, Growth)> {
        let input = match a.key.places == b.key.places {
            true => None,
            false => Some(self.widen_input(&a.state.input, &b.state.input, self.position_span(), false, false)?),
        };
        let (joined, mut growth) = self.join_configs(fork, a, b, false, true);
        if a.tag.mode != b.tag.mode {
            growth.parts.push("mode");
        }
        let Some(input) = input else { return Some((joined, growth)) };
        growth.parts.push("input");
        Some((Config::new(PathState { input, ..joined.state }, joined.env, joined.ended), growth))
    }

    /// Where the running path stands in the source, as [`Key`] counts it.
    fn place_progress(&self) -> u64 {
        self.source_progress - self.lines_read
    }

    /// The running path joins a parked one at its place ([`partner`]).
    fn meet(&mut self, paths: &mut Paths, fork: &Fork) {
        if paths.parked.iter().any(|p| p.key.progress == self.place_progress()) {
            let live = Config::new(self.save_path(), self.env.path(fork), false);
            if partner(&paths.parked, &live).is_some() {
                let joined = self.absorb(paths, fork, live, true);
                self.resume_config(fork, joined);
            }
        }
    }

    /// Between two lines of a file, a path that stands where another does,
    /// in the same conditionals but in more groups: in the document's own
    /// text both stand in any number of the extra groups from where they
    /// part ([`Machine::widen_stacks`]), so the two can join. In a package,
    /// with `analysis.assume_packages_close_groups`, the extra groups are
    /// assumed closed, as a package closes what it opens.
    fn close_extra_groups(&mut self, paths: &mut Paths, fork: &Fork) {
        let places: Vec<Place> = self.input.iter().filter_map(place).collect();
        if !places.iter().all(|p| matches!(p, Place::File(..) | Place::Pseudo(..))) {
            return;
        }
        let here: Vec<(GroupKind, bool)> = self.env.groups().into_iter().map(|g| (g.0, g.1)).collect();
        let conds: Vec<((CondLimit, Span), bool)> = self.conds.iter().map(|c| ((c.limit, c.at), c.many)).collect();
        let Some(i) = paths.parked.iter().position(|p| {
            let k = &p.key;
            let n = k.groups.len().min(here.len());
            k.progress == self.place_progress()
                && k.places == places
                && k.conds == conds
                && k.groups.len() != here.len()
                && k.groups[..n] == here[..n]
        }) else {
            return;
        };
        let span = self.position_span();
        if self.own_file(span.file) || !self.cfg.analysis.assume_packages_close_groups {
            let live = Config::new(self.save_path(), self.env.path(fork), false);
            if let Some((parked, live)) = self.widen_stacks(fork, &paths.parked[i], &live) {
                paths.parked[i] = parked;
                self.resume_config(fork, live);
            }
            return;
        }
        let keep = paths.parked[i].key.groups.len().min(here.len());
        if paths.parked[i].key.groups.len() > here.len() {
            // The waiting path is the deeper one: it runs on from here.
            let other = paths.parked.swap_remove(i);
            let env = self.env.path(fork);
            paths.parked.push(Config::new(self.save_path(), env, false));
            self.resume_config(fork, other);
        }
        while self.env.depth() > keep {
            let nest = self.env.nest();
            let mut catcodes = std::mem::take(&mut self.catcodes);
            self.env.pop_group(&mut catcodes);
            self.catcodes = catcodes;
            self.group_dropped(nest, span);
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
    /// Paths a drive keeps apart before it joins those alike: at most
    /// [`Machine::path_bound`].
    fn alike_bound(&self) -> usize {
        usize::from(self.limits.alike).min(self.path_bound())
    }

    /// Paths a drive keeps apart: `limits.paths` between commands,
    /// `limits.choices` inside a scan, where the arms of a choice are
    /// meant to join as soon as they can.
    fn path_bound(&self) -> usize {
        match self.unit {
            Unit::Command => usize::from(self.limits.paths),
            _ => usize::from(self.limits.choices),
        }
    }

    /// More paths wait than `alike_bound`: pairs that are `alike` (same
    /// stacks, texts may differ) are joined with their input widened. Paths
    /// that are not alike cannot be one: they wait on, said once.
    fn crowd(&mut self, paths: &mut Paths, fork: &Fork) {
        let bound = self.alike_bound();
        let span = self.position_span();
        let (mut i, before) = (0, paths.parked.len());
        while paths.parked.len() > bound && i < paths.parked.len() {
            let joined = (i + 1..paths.parked.len()).find_map(|j| {
                let (a, b) = (&paths.parked[i], &paths.parked[j]);
                if (a.crowded && b.crowded) || !alike(a, b) {
                    return None;
                }
                let input = self.widen_input(&a.state.input, &b.state.input, span, false, false)?;
                Some((j, input))
            });
            let Some((j, input)) = joined else {
                i += 1;
                continue;
            };
            let b = paths.parked.swap_remove(j);
            let a = paths.parked.swap_remove(i);
            let widen = |c: Config| Config::new(PathState { input: input.clone(), ..c.state }, c.env, c.ended);
            let (joined, _) = self.join_configs(fork, &widen(a), &widen(b), false, true);
            paths.parked.push(joined);
        }
        // Tried again only once more wait: the rest cannot be one now.
        paths.parked.iter_mut().for_each(|c| c.crowded = true);
        paths.crowded = paths.parked.len();
        if paths.parked.len() == before {
            return;
        }
        self.diagnose(
            Severity::Imprecision,
            "paths-widened",
            span,
            format!("more than {bound} paths waited; those at one point were joined"),
        );
    }

    /// Two texts whose differing ends hold closers or openers of braces or
    /// conditionals: their common beginning and end are kept as they are,
    /// grown inward until what differs between them is whole, and that
    /// becomes one token.
    fn widen_middle(&mut self, p: &[Token], q: &[Token], span: Span, widen: bool, pass: bool) -> Option<Vec<Token>> {
        let mut pre = p.iter().zip(q).take_while(|(x, y)| x.tok == y.tok).count();
        let mut suf = p.iter().rev().zip(q.iter().rev()).take_while(|(x, y)| x.tok == y.tok).count();
        suf = suf.min(p.len().min(q.len()) - pre);
        let (a, b) = loop {
            let (a, b) = (&p[pre..p.len() - suf], &q[pre..q.len() - suf]);
            let ((la, ra), (lb, rb)) = (self.unclosed(a), self.unclosed(b));
            let (l, r) = (la || lb, ra || rb);
            if !l && !r {
                break (a, b);
            }
            if (l && pre == 0) || (r && suf == 0) {
                return None;
            }
            pre -= usize::from(l);
            suf -= usize::from(r);
        };
        let mut text = p[..pre].to_vec();
        text.push(self.join_texts(a, b, widen, pass, span)?);
        text.extend_from_slice(&p[p.len() - suf..]);
        Some(text)
    }

    /// Whether `text` closes a brace or conditional opened before it, and
    /// whether it opens one it leaves open.
    fn unclosed(&self, text: &[Token]) -> (bool, bool) {
        let (strays, open) = self.strays(text);
        let (mut depth, mut low) = (0i64, 0i64);
        for t in text {
            match t.tok {
                Tok::Chr(_, Catcode::Begin) => depth += 1,
                Tok::Chr(_, Catcode::End) => depth -= 1,
                _ => {}
            }
            low = low.min(depth);
        }
        (!strays.is_empty() || low < 0, open > 0 || depth > low)
    }

    /// The input of two paths that stand alike, joined. Level by level when
    /// they are as deep: equal levels are kept, and two token lists become
    /// their common end after one token standing for either beginning
    /// ([`Machine::one_of`]). Otherwise the shared bottom levels are kept,
    /// and what each reads above them becomes one such text. `None` when a
    /// file or a boundary differs, or code does between two paths (not
    /// two passes of one loop, `pass`): they stand at different points.
    fn widen_input(&mut self, a: &Input, b: &Input, span: Span, widen: bool, pass: bool) -> Option<Input> {
        let (a, b) = (live_frames(a), live_frames(b));
        let now = (self.conds.len(), self.env.group_id(), self.env.assignments);
        let tokens = |text: Vec<Token>, at| Frame::Tokens {
            held: text.len(),
            toks: Rc::from(text),
            pos: 0,
            expanding: None,
            upper: false,
            at,
        };
        if a.len() == b.len() {
            let mut input = Vec::with_capacity(a.len());
            for (x, y) in a.iter().zip(&b) {
                let frame = match (pending(&[x]), pending(&[y])) {
                    _ if place(x) == place(y) => (*x).clone(),
                    (Some(p), Some(q)) => tokens(self.widen_text(&p, &q, span, widen, pass)?, stamp(&[x, y], now)),
                    _ => return None,
                };
                input.push(frame);
            }
            return Some(input.into_iter().collect());
        }
        let shared = a.iter().zip(&b).take_while(|(x, y)| place(x) == place(y)).count();
        let (p, q) = (pending(&a[shared..])?, pending(&b[shared..])?);
        let mut input: Vec<Frame> = a[..shared].iter().map(|f| (*f).clone()).collect();
        let at = stamp(&a[shared..].iter().chain(&b[shared..]).copied().collect::<Vec<_>>(), now);
        input.push(tokens(self.widen_text(&p, &q, span, widen, pass)?, at));
        Some(input.into_iter().collect())
    }

    /// Two texts left to read, joined: their common end, after one token
    /// for either of what comes before it. `None` when a beginning is not
    /// brace balanced: one token cannot stand for a `}` closing a group
    /// opened before it.
    fn widen_text(&mut self, p: &[Token], q: &[Token], span: Span, widen: bool, pass: bool) -> Option<Vec<Token>> {
        let mut common = p.iter().rev().zip(q.iter().rev()).take_while(|(x, y)| x.tok == y.tok).count();
        if common == p.len() && common == q.len() {
            return Some(p.to_vec());
        }
        // A shorter common end, so what differs before it is balanced: an
        // argument that grows inside its braces is then one text.
        let (wp, wq) = (self.whole_prefixes(p), self.whole_prefixes(q));
        while common > 0 && !(wp[p.len() - common] && wq[q.len() - common]) {
            common -= 1;
        }
        let (head, tail) = (&p[..p.len() - common], &p[p.len() - common..]);
        let other = &q[..q.len() - common];
        if !self.whole(head) || !self.whole(other) {
            return self.widen_middle(p, q, span, widen, pass);
        }
        // Passes that differ only in their digits (a loop's counter) are
        // the text with unknown digits, which the next pass meets again as
        // it is. Between paths, digits in code are code: one of either
        // ([`Machine::join_texts`]), and unknown text once widened.
        let syms = self.typed_syms();
        let typed = crate::env::typed!(self, syms);
        let (p, q) = (crate::env::unknown_runs(head, &typed), crate::env::unknown_runs(other, &typed));
        if pass && same_text(&p, &q) {
            let mut text = p.to_vec();
            text.extend_from_slice(tail);
            return Some(text);
        }
        // What both begin with is kept too, when what differs after it is
        // balanced: a recursion whose passes differ in a letter at the
        // end of their input (`\__int_to_Base:nnnN`) then joins at that
        // letter, and its calls stay text a scan reads.
        let shared = head.iter().zip(other).take_while(|(x, y)| x.tok == y.tok).count();
        let shared = if self.whole(&head[shared..]) && self.whole(&other[shared..]) { shared } else { 0 };
        // Data that code still to run reads is part of that code: two
        // paths with it differ in code. Data read first (a value being
        // scanned) is data.
        if !pass && self.holds_code(&head[..shared]) {
            return None;
        }
        let one = self.join_texts(&head[shared..], &other[shared..], widen, pass, span)?;
        let mut text = head[..shared].to_vec();
        text.push(one);
        text.extend_from_slice(tail);
        Some(text)
    }

    /// An arm of the document's commands whose next tokens are `\\fi`s in
    /// token lists reads them now, as TeX's main loop would next: arms that
    /// leave a recursion at different depths then stand in one place and
    /// join when parked.
    fn read_pending_fis(&mut self) {
        while self.conds.last().is_some_and(|c| !c.many) && self.testing.last() != Some(&self.conds.len()) {
            let next = self.input.iter().rev().find_map(|f| match f {
                Frame::Tokens { toks, pos, .. } if *pos < toks.len() => Some(Some(toks[*pos])),
                Frame::Returned(t) if !t.is_empty() => Some(t.last().copied()),
                Frame::Tokens { .. } | Frame::Returned(_) => None,
                _ => Some(None),
            });
            let fi = next
                .flatten()
                .filter(|t| t.cs().is_some_and(|s| matches!(self.env.meaning(s), Meaning::Primitive(Primitive::Fi))));
            let Some(fi) = fi else { break };
            self.next_token();
            self.conditional_fi(fi.span);
        }
    }

    /// Park `config`, joined with a parked path at its place ([`partner`]).
    fn park(&mut self, paths: &mut Paths, fork: &Fork, config: Config) {
        let config = self.absorb(paths, fork, config, false);
        paths.parked.push(config);
    }

    /// `config` joined with every parked path at its place ([`partner`]),
    /// which leave the parked ones. `first`: its state is the one kept.
    fn absorb(&mut self, paths: &mut Paths, fork: &Fork, mut config: Config, first: bool) -> Config {
        while !config.ended
            && let Some(i) = partner(&paths.parked, &config)
        {
            let other = paths.parked.swap_remove(i);
            let (a, b) = if first { (&config, &other) } else { (&other, &config) };
            config = self.join_configs(fork, a, b, false, true).0;
        }
        config
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
        if !live || p.progress < self.place_progress() {
            return Some(i);
        }
        let here: Vec<Place> = self.input.iter().filter_map(place).collect();
        (p.progress == self.place_progress() && place_order(&p.places, &here).is_lt()).then_some(i)
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
        (self.env.assignments, self.gaps_raised, self.gap_depth, self.tex_errors, self.scratch.repeat_cut) =
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
                return self.unknown_text(Unk::Any, span);
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
            return self.unknown_text(Unk::Of(&texts), span);
        }
        let mut named: Vec<(u128, Rc<[Token]>)> =
            members.into_iter().map(|t| (text_hash(&t, |s| self.name(s).to_owned()), t)).collect();
        named.sort_by_key(|n| n.0);
        let class = self.class_name(named.iter().map(|n| &n.1[..]));
        let sym = self.intern(&format!("{prefix}{:x?}{class}", named.iter().map(|n| n.0).collect::<Vec<_>>()));
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

    /// One token for either of two balanced texts. Data with unknown parts
    /// is typed text, so a loop over it ends. Other data is one of either.
    /// Code still to run differs only between two passes of one loop
    /// (`pass`): the widening makes it unknown text that may hold either.
    /// Two paths that differ in code stand at different points: `None`,
    /// they never join.
    fn join_texts(&mut self, x: &[Token], y: &[Token], widen: bool, pass: bool, span: Span) -> Option<Token> {
        // The token stands where the text it joins came from, so a gap it
        // meets is told apart from gaps of other joins under one call.
        let span = x.first().or(y.first()).map_or(span, |t| t.span);
        if self.holds_code(x) || self.holds_code(y) {
            return pass.then(|| self.unknown_text(Unk::Of(&[x, y]), span));
        }
        // Data, typed or not: one of either, exact until widened.
        let marked = |t: &[Token]| {
            t.iter().any(|t| t.cs().is_some() && self.typed_text_of(*t).is_none() && self.one_of_members(*t).is_none())
        };
        if !widen && !marked(x) && !marked(y) {
            return Some(self.one_of(vec![Rc::from(x), Rc::from(y)], span));
        }
        // The texts of each one-of token stand side by side: their class
        // is the union, and one may be empty.
        let mut texts: Vec<Vec<Token>> = Vec::new();
        for text in [x, y] {
            let mut flat = Vec::new();
            for t in text {
                match self.one_of_members(*t) {
                    Some(members) => {
                        if members.iter().any(|m| m.is_empty()) {
                            texts.push(Vec::new());
                        }
                        members.iter().for_each(|m| flat.extend_from_slice(m));
                    }
                    None => flat.push(*t),
                }
            }
            // Typed text that may be empty: the text may be.
            if flat.iter().all(|t| self.typed_shape(*t).is_some_and(|(_, one, _)| !one)) {
                texts.push(Vec::new());
            }
            texts.push(flat);
        }
        let texts: Vec<&[Token]> = texts.iter().map(Vec::as_slice).collect();
        Some(self.unknown_text(Unk::Of(&texts), span))
    }

    /// Whether `text` holds code still to run: a control sequence or an
    /// active character, other than typed data or one of data texts.
    fn holds_code(&self, text: &[Token]) -> bool {
        let code = |t: &Token| match t.tok {
            Tok::Chr(_, Catcode::Active) => true,
            Tok::Cs(_) => self.typed_text_of(*t).is_none(),
            _ => false,
        };
        // One level: `one_of` keeps its members flat.
        text.iter().any(|t| code(t) && self.one_of_members(*t).is_none_or(|ms| ms.iter().any(|m| m.iter().any(code))))
    }

    /// Widening of a text that grows at its end (`\\aftergroup` per pass, an
    /// append): the common part, then any number of repetitions of what one
    /// side adds ([`Machine::repeat_token`]). A repetition already there
    /// absorbs one more. Any other difference is unknown text.
    pub(crate) fn repeat_join(&mut self, x: &[Token], y: &[Token], span: Span) -> Vec<Token> {
        let (short, long) = if x.len() <= y.len() { (x, y) } else { (y, x) };
        if !same_text(short, &long[..short.len()]) || !balanced(&long[short.len()..]) {
            return vec![self.unknown_text(Unk::Any, span)];
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

    /// The name part that spells the class of `texts` for a one-of token
    /// ([`crate::env::Typed::one_of_class`]): the mark, the class, `+` when no
    /// text is empty.
    fn class_name<'t>(&mut self, texts: impl Iterator<Item = &'t [Token]>) -> String {
        let syms = self.typed_syms();
        let typed = crate::env::typed!(self, syms);
        let (mut class, mut one) = (crate::tex::TextClass::EMPTY, true);
        for text in texts {
            one &= !text.is_empty();
            for t in text {
                let c = typed.classify(t).or_else(|| typed.one_of_class(t));
                class = class.union(c.map_or_else(|| crate::tex::TextClass::of(t.tok), |c| c.0));
            }
        }
        format!("{}{}{}", crate::env::CLASS_MARK, class.spell(), if one { '+' } else { '_' })
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
        let class = self.class_name([&[][..], text].into_iter());
        let sym =
            self.intern(&format!("{UNKNOWN_NAME}{ONE_OF}*{:x}{class}", text_hash(text, |s| self.name(s).to_owned())));
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
        // Only as the number prints: `00` is no text of 0.
        let digits = digits.filter(|d| d.len() == 1 || !d.starts_with('0'))?;
        digits.parse::<i64>().ok().map(Num::exact)
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
    /// At -vvvv, each new 256 MiB high: what the main tables hold.
    fn report_memory(&mut self, paths: &Paths) {
        let mb = crate::budget::in_use() >> 28;
        if self.cfg.verbose < 4 || mb <= self.out.memory_reported {
            return;
        }
        self.out.memory_reported = mb;
        let sum = |cs: &mut dyn Iterator<Item = &Config>| {
            cs.fold([0usize; 4], |mut a, c| {
                a[0] += 1;
                a[1] += c.env.len();
                a[2] += c
                    .state
                    .input
                    .iter()
                    .map(|f| match f {
                        Frame::Returned(t) => t.len(),
                        _ => 1,
                    })
                    .sum::<usize>();
                a[3] += c.state.call_recs.len() + c.state.scans.len();
                a
            })
        };
        let groups = [
            ("parked", sum(&mut paths.parked.iter())),
            ("done", sum(&mut paths.done.iter())),
            ("arrived", sum(&mut paths.arrived.iter())),
            ("heads", sum(&mut paths.heads.iter().map(|h| &h.config))),
            ("loops", sum(&mut paths.loops.iter().chain(&paths.stopped).map(|l| &l.joined))),
            ("cut", sum(&mut paths.cut.iter().map(|c| &c.1))),
            ("shelved", sum(&mut paths.shelved.iter().map(|s| &s.config))),
        ];
        let facts = &self.out.facts;
        crate::progress::note!(
            "satex: memory {} MiB at {:?}: paths {:?} | env trail/journal/recent/forks {:?} | defs {} exps {} occs {} diags {} | graph {} {} | names {} | calls {} {} | sites {} {} {} diag_seen {} | undo {} copies {} written {} reads {} keys {} | summaries {} {} gaps {}",
            crate::budget::in_use() >> 20,
            self.position_span(),
            groups,
            self.env.sizes(),
            facts.defs.len(),
            facts.expansions.len(),
            facts.occurrences.len(),
            facts.diagnostics.len(),
            self.out.graph.len(),
            self.out.graph.edge_count(),
            self.out.interner.len(),
            self.call_runs.len(),
            self.call_recs.len(),
            self.definition_sites.len(),
            self.expansion_sites.len(),
            self.cond_sites.len(),
            self.diag_seen.len(),
            self.undo.len(),
            self.copies.len(),
            self.written_lines.len(),
            self.name_reads.len(),
            self.key_names.len(),
            self.scratch.loop_summaries.len(),
            self.scratch.summarized.len(),
            self.scratch.cond_gaps.len(),
        );
    }

    fn report_paths(&self, paths: &Paths, commands: u64) {
        /// Commands between two reports.
        const EVERY: u64 = 20_000;
        /// Places a report names.
        const SHOWN: usize = 6;
        if self.cfg.verbose < 4 || paths.parked.is_empty() || !commands.is_multiple_of(EVERY) {
            return;
        }
        let mut places: HashMap<String, Vec<usize>> = HashMap::default();
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
        let mut openers: HashMap<String, usize> = HashMap::default();
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
                            .map(|((limit, at), _)| format!("{limit:?}@{}:{}", self.out.short_name(at.file), at.line))
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
            Place::Pseudo(id, _, at) => format!("scantokens {}:{}:{}@{at}", id.file, id.line, id.col),
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
    fn join_configs(&mut self, fork: &Fork, a: &Config, b: &Config, widen: bool, paired: bool) -> (Config, Growth) {
        let span = self.position_span();
        self.out.heat(|h| &mut h.joins, span);
        // Joins run many to a step: past `limits.memory` the run stops here.
        if crate::budget::over() {
            self.check_budget();
        }
        let after: Vec<Option<Vec<Token>>> = a
            .env
            .after()
            .into_iter()
            .zip(b.env.after())
            .map(|(x, y)| {
                (!same_text(x, y)).then(|| match widen {
                    true => self.repeat_join(x, y, span),
                    // Paths apart in this code never join ([`same_code`]):
                    // only passes of one loop meet here.
                    false => self.join_texts(x, y, false, true, span).into_iter().collect(),
                })
            })
            .collect();
        let syms = self.typed_syms();
        let typed = crate::env::typed!(self, syms);
        self.out.joined_slots += (a.env.len() + b.env.len()) as u64;
        let (env, slots, waiting) = self.env.join_paths(fork, &a.env, &b.env, &typed, widen, paired, &after);
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
                            let meaning = self.env.meaning(sym);
                            let real = !matches!(meaning, Meaning::Primitive(Primitive::Relax));
                            self.wild.add(prefix, suffix, true, real.then_some(meaning));
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
        // What each loop's last pass began with: the larger, so a pass
        // counts as shrinking only below both.
        if y.loop_bounds != x.loop_bounds {
            let bounds = Rc::make_mut(&mut x.loop_bounds);
            for &(id, b) in y.loop_bounds.iter() {
                match bounds.iter_mut().find(|(l, _)| *l == id) {
                    Some(e) => e.1 = e.1.max(b),
                    None => bounds.push((id, b)),
                }
            }
        }
        x.cut_taint = match (x.cut_taint, y.cut_taint) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        x.page_touched |= y.page_touched;
        x.page_grew |= y.page_grew;
        // What either path wrote may be in the files (tex.web § 1370).
        x.outputs = super::join_outputs(&x.outputs, &y.outputs);
        x.pending = super::join_pending(&x.pending, &y.pending);
        x.natural = crate::mode::join_natural(x.natural, y.natural);
        x.main_tail = x.main_tail.merge(y.main_tail);
        x.main_depth =
            crate::mode::Dim::range(x.main_depth.lo.min(y.main_depth.lo), x.main_depth.hi.max(y.main_depth.hi));
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
            let text = |t: Option<Token>| -> Rc<[Token]> { Rc::from(t.into_iter().collect::<Vec<_>>()) };
            let joined = match widen {
                true => {
                    self.unknown_text(Unk::Of(&[&text(x.after_assignment)[..], &text(y.after_assignment)[..]]), span)
                }
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
            text.push(self.unknown_text(Unk::Any, span));
        } else if widen {
            let token = self.unknown_text(Unk::Of(&[mp, mq]), span);
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
    n.gap_digits &= other.gap_digits;
    if n.number != other.number {
        n.number = None;
    }
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
/// source, the same conditionals and groups, the same code waiting.
fn alike(a: &Config, b: &Config) -> bool {
    same_code(&a.tag, &b.tag) && !a.ended && !b.ended && a.key.progress == b.key.progress && same_stacks(a, b)
}

/// Whether two paths stand in the same conditionals, groups and scans.
fn same_stacks(a: &Config, b: &Config) -> bool {
    let (x, y) = (&a.key, &b.key);
    x.conds == y.conds && x.groups == y.groups && x.scans == y.scans
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
    // A key per level, compared in order: a total order, as `sort_by`
    // needs. Text further back (more left to read) comes first.
    let key = |p: &Place| match p {
        Place::File(f, i, _) => (0, *f as usize, (*i as u64, 0)),
        Place::Pseudo(at, _, i) => (1, at.file as usize, (u64::from(at.line) << 32 | u64::from(at.col), *i)),
        Place::Tokens(t, i) => (2, usize::MAX - (t.len() - i), (0, 0)),
        Place::Returned(t) => (2, usize::MAX - t.len(), (0, 0)),
        Place::Boundary => (3, 0, (0, 0)),
    };
    let n = a.len().min(b.len());
    a[..n].iter().map(key).cmp(b[..n].iter().map(key)).then_with(|| b.len().cmp(&a.len()))
}

/// `at` and the stamp of `frame` if it is a token list, the lower of each.
pub(super) fn lower(at: Option<(usize, u64, u64)>, frame: Option<&Frame>) -> Option<(usize, u64, u64)> {
    let Some(Frame::Tokens { at: b, .. }) = frame else { return at };
    Some(at.map_or(*b, |a| (a.0.min(b.0), a.1.min(b.1), a.2.min(b.2))))
}

/// The stamp a list joined from `frames` keeps: the lowest of theirs, or
/// `now` for text given back, which dropped loses no earlier writes.
fn stamp(frames: &[&Frame], now: (usize, u64, u64)) -> (usize, u64, u64) {
    frames.iter().fold(None, |at, f| lower(at, Some(f))).unwrap_or(now)
}

/// The input levels that still have something to read.
fn live_frames(input: &Input) -> Vec<&Frame> {
    input.iter().filter(|frame| place(frame).is_some()).collect()
}

/// Where one input level stands, `None` once it is read.
pub(super) fn place(frame: &Frame) -> Option<Place> {
    match frame {
        Frame::File { mouth, .. } => Some(Place::File(mouth.file, mouth.offset(), mouth.ending())),
        Frame::Tokens { toks, pos, .. } => (*pos < toks.len()).then(|| Place::Tokens(toks.clone(), *pos)),
        Frame::Pseudo { mouth, at, pos, .. } => Some(Place::Pseudo(*at, mouth.text().clone(), mouth.offset() + pos)),
        Frame::Returned(tokens) => (!tokens.is_empty()).then(|| Place::Returned(tokens.clone())),
        Frame::Boundary => Some(Place::Boundary),
    }
}

/// A name for a text: 128 bits of it, which two texts that differ do not
/// share in any run satex will see. A control sequence counts by its name,
/// not its symbol number, so the name and the order of a set do not depend
/// on what the run interned before.
fn text_hash(text: &[Token], name: impl Fn(Sym) -> String) -> u128 {
    use std::hash::{Hash, Hasher};
    let half = |salt: u8| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        salt.hash(&mut hasher);
        for token in text {
            match token.tok {
                Tok::Cs(sym) => (0u8, name(sym)).hash(&mut hasher),
                tok => (1u8, tok).hash(&mut hasher),
            }
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

/// `key` without the token lists above the entry's `places` levels that
/// hold only `\fi`, and without the conditionals they close: `None`
/// unless they close every conditional opened past the entry's `conds`.
fn fi_tail(key: &Key, (places, conds): (usize, usize), is_fi: &dyn Fn(Tok) -> bool) -> Option<Key> {
    let mut fis = 0;
    let mut kept = Vec::with_capacity(key.places.len());
    for (i, p) in key.places.iter().enumerate() {
        match place_text(p) {
            Some(t) if i >= places && !t.is_empty() && t.iter().all(|x| is_fi(x.tok)) => fis += t.len(),
            _ => kept.push((*p).clone()),
        }
    }
    (fis > 0 && key.conds.len() == conds + fis).then(|| Key {
        progress: key.progress,
        places: kept,
        conds: key.conds[..conds].to_vec(),
        groups: key.groups.clone(),
        scans: key.scans.clone(),
    })
}

/// `input` without its `\fi`-only token lists above `places` live levels.
fn fi_frames_dropped(input: &Input, places: usize, is_fi: &dyn Fn(Tok) -> bool) -> Input {
    let mut live = 0;
    let mut out = Vec::with_capacity(input.len());
    for frame in &input.0 {
        if let Some(p) = place(frame) {
            live += 1;
            if live > places && place_text(&p).is_some_and(|t| !t.is_empty() && t.iter().all(|x| is_fi(x.tok))) {
                continue;
            }
        }
        out.push(frame.clone());
    }
    Input(out)
}

/// The text a token-list level has left to read.
fn place_text(p: &Place) -> Option<&[Token]> {
    match p {
        Place::Tokens(t, i) => Some(&t[*i..]),
        Place::Returned(t) => Some(&t[..]),
        _ => None,
    }
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
        (Place::Pseudo(_, a, i), Place::Pseudo(_, b, j)) => Rc::ptr_eq(a, b) && j > i,
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

/// The newest `n` of a drive's loop tables ([`crate::config::Limits::loop_heads`]).
fn keep_last<T>(table: &mut Vec<T>, n: u16) {
    let excess = table.len().saturating_sub(usize::from(n.max(1)));
    table.drain(..excess);
}

/// Whether two token lists are the same text.
fn same_text(a: &[Token], b: &[Token]) -> bool {
    a.iter().map(|t| t.tok).eq(b.iter().map(|t| t.tok))
}

/// The prefix of the names [`Machine::one_of`] makes.
pub(crate) const ONE_OF: &str = "one of:";

/// Whether two paths wait to run the same code at their groups' ends and
/// after the next assignment: else they stand at different points.
fn same_code(a: &Tag, b: &Tag) -> bool {
    (&a.after, a.after_assignment) == (&b.after, b.after_assignment)
}

/// The parked path `config` joins at its place: one with the same tag, or
/// one that waits to run the same code and either closed the same
/// conditional last (both arms of an `\if…` meet after its `\fi`) or,
/// past [`PARTITIONS`] tags at the place or between two lines of a file
/// (the statement is over), the first there.
fn partner(parked: &[Config], config: &Config) -> Option<usize> {
    let (key, tag) = (&config.key, &config.tag);
    let here: Vec<usize> = (0..parked.len()).filter(|&i| !parked[i].ended && parked[i].key == *key).collect();
    let merge = here.len() >= PARTITIONS || key.places.iter().all(|p| matches!(p, Place::File(..) | Place::Pseudo(..)));
    let fi = |i: usize| config.state.fi_mark.is_some() && parked[i].state.fi_mark == config.state.fi_mark;
    here.iter()
        .copied()
        .find(|&i| parked[i].tag == *tag)
        .or_else(|| here.iter().copied().find(|&i| same_code(&parked[i].tag, tag) && (merge || fi(i))))
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

/// Whether two paths differ only in data: what satex keeps under names of
/// its own (`slot`: a register's or a code table's entry) and macros whose
/// texts are numbers. What else differs is code the paths will run: a
/// macro's text, a meaning another command gave a name, or a name left
/// undefined or unknown.
fn data_apart(a: &Config, b: &Config, typed: &crate::env::Typed, slot: &dyn Fn(Sym) -> bool) -> bool {
    let number = |t: &Token| match t.tok {
        Tok::Chr(c, Catcode::Other) => c.is_ascii_digit() || "+-.,".contains(c) || crate::tex::is_unknown_digit(c),
        _ => typed.classify(t).is_some_and(|(c, _)| TextClass::NUMBER.contains(c)),
    };
    let code = |sym: Sym, binding: Option<&crate::env::Binding>| match binding.map(|b| &b.meaning) {
        None | Some(Meaning::Register(..)) => false,
        Some(Meaning::Macro(d)) => !(d.parameter_text.items.is_empty() && d.replacement_text.iter().all(number)),
        Some(_) => !slot(sym),
    };
    a.env.differing(&b.env).into_iter().all(|sym| {
        let (x, y) = (a.env.changed(sym), b.env.changed(sym));
        x.zip(y).is_some_and(|(x, y)| x.meaning == y.meaning) || !code(sym, x) && !code(sym, y)
    })
}

/// Where two stacks part when every frame above is the same one (whether
/// it stands for many aside): the frame and where one standing for any
/// number of it begins, below the part when the last shared frame is one.
/// `None` when they are equal or part otherwise.
fn many_from<T: PartialEq + Copy>(a: &[(T, bool)], b: &[(T, bool)]) -> Option<(usize, T)> {
    let n = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let t = a.get(n).or(b.get(n))?.0;
    if !a[n..].iter().chain(&b[n..]).all(|f| f.0 == t) {
        return None;
    }
    Some((if n > 0 && a[n - 1] == (t, true) { n - 1 } else { n }, t))
}
