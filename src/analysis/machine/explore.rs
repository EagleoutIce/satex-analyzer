//! Undecided choices (SEMANTICS.md § 14).
//!
//! Every choice the run cannot decide goes through [`Machine::branch`]. A
//! main-loop command is rerun from its start once for each combination of
//! arms its choices allow, so a choice inside a scan is as exact as one in
//! the main loop. Each run is a path. Paths wait on a worklist and are joined
//! when they reach the same place. No path is dropped. A command that
//! branches is a loop head: its joined state grows to a fixpoint and is
//! widened after `widen_after` passes.

use super::*;
use crate::engine::env::{Fork, PathEnv, Reach};
use crate::engine::value::Num;

/// Maximum reruns of one command. Further choices are left to the caller,
/// which widens them, as it does past `limits.choices`.
const RUNS: usize = 64;
/// How many paths at the same place inside an expansion are kept apart when
/// they differ in what a join would lose (a [`Tag`]). Running on usually
/// removes such a difference (`\leavevmode` on one path, the next character
/// on the other).
const PARTITIONS: usize = 2;

/// A choice the running command met: its number of arms and the arm this run takes.
#[derive(Clone, Copy)]
pub(super) struct Choice {
    pub(super) span: Span,
    pub(super) by: Sym,
    pub(super) arms: usize,
    pub(super) arm: usize,
    /// The choice picks one of the meanings or texts a join left. This arm
    /// is valid only where the join's sources agree with the path.
    pub(super) joined: bool,
}

/// A path parked between two commands.
#[derive(Clone)]
pub(super) struct Config {
    state: PathState,
    env: PathEnv,
    key: Key,
    tag: Tag,
    /// Whether it reached `\end` or the end of its input.
    ended: bool,
    /// Whether [`Machine::crowd`] already compared it with every other waiting path.
    crowded: bool,
}

/// Where a path stands, used to decide which paths join: its input stack
/// (bottom first, without exhausted levels), its conditionals and its groups.
#[derive(PartialEq, Clone)]
struct Key {
    progress: u64,
    places: Vec<Place>,
    /// Each conditional's limit and opener, and whether it stands for many.
    conds: Vec<((CondLimit, Span), bool)>,
    /// Each group's kind, and whether it stands for many.
    groups: Vec<(GroupKind, bool)>,
    scans: Vec<(usize, bool)>,
}

/// What a join of paths at one place loses: which mode, switches and
/// waiting tokens go together (trace partitioning).
#[derive(PartialEq, Clone)]
struct Tag {
    mode: crate::engine::mode::Modes,
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

/// What a join added to the state it grew from: names, and other parts of the state.
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

/// A command that branched, with the join of every state a path started it in.
#[derive(Clone)]
struct Head {
    span: Span,
    progress: u64,
    config: Config,
    passes: u32,
    /// The bound the last pass here started with ([`Machine::pass_bound`]).
    bound: Option<u64>,
}

/// Where a loop was entered: the input levels, conditionals and groups at entry.
#[derive(Clone)]
struct Loop {
    id: LoopId,
    depth: (usize, usize, usize),
    passes: u32,
    /// Number of output routines fired when last met ([`Machine::pages`]).
    pages: u64,
    /// Number of names assigned before the entry.
    recent: usize,
    /// The input levels below the loop body, by identity and by place. A pass
    /// reaches its next call without consuming any of them.
    levels: Vec<Level>,
    places: Vec<Place>,
    /// The top input level at entry and its place: the list the entry command was read from.
    top: Option<(Level, Place)>,
    /// The join of the states the passes started in.
    joined: Config,
    /// The bindings the first pass started with. A summary widens against
    /// these, since the join already contains the last pass.
    entry: PathEnv,
    /// After the summary, the body ran once more from the widened state. Writes
    /// to names the widening made unknown (`\csname a\the\n`) are only found by running it.
    rerun: bool,
    /// The assignment count at the summary. Later writes on the path after the
    /// summary come after every pass ([`Machine::end_covered`]).
    summed: u64,
    /// How much the drive's text scan had produced at entry.
    made: Option<usize>,
}

/// One input level, by identity rather than by position. Not by a bare
/// address, since a freed address is reused and addresses depend on
/// allocation order. The list is held, so its address stays unique.
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
    /// Where the paths split. Their bindings are stored as changes relative to it.
    fork: Option<Fork>,
    parked: Vec<Config>,
    /// Paths that finished the drive's command, waiting for the others.
    done: Vec<Config>,
    heads: Vec<Head>,
    /// Number of waiting paths after the last [`Machine::crowd`] that could not join them all.
    crowded: usize,
    /// Loop entries by an identity that ignores content, with their pass counts.
    /// Kept across rewinds and when their heads are retired.
    loops: Vec<Loop>,
    /// Loops summarized in this drive. A path that meets one again stops there.
    stopped: Vec<Loop>,
    /// Recursions widened past `recursion_depth`, each with the join of every
    /// path cut there. A path that adds nothing to the join stops.
    cut: Vec<(Span, Config)>,
    /// Whether this is the drive of the whole run, where join points and dropped paths apply.
    top: bool,
    /// Paths at a join point ([`Machine::join_arrived`]), waiting for the rest.
    arrived: Vec<Config>,
    /// Whether one of them waits at the end of an output routine.
    at_output: bool,
    /// Paths dropped for a certain error (`analysis.assume_compiles`), with the
    /// place. Kept until the others finish. If every other path also ends in an
    /// error, no path compiles and these continue ([`Machine::settle`]).
    shelved: Vec<Shelved>,
    /// Whether a path continued with a certain error because no other path compiled.
    failed: bool,
    /// Paths that ran `\end`, in their final state. The run ends in their state
    /// rather than in that of a path that ran off the end of its input.
    job_ends: Vec<Config>,
    /// Paths that ran off the end of their input, in their final state.
    input_ends: Vec<Config>,
}

/// A dropped path with its error and place, and the diagnostic counts its
/// command made (rolled back when dropped, restored if it continues).
#[derive(Clone)]
struct Shelved {
    config: Config,
    error: String,
    at: Span,
    counts: Counts,
    /// The diagnostics its run raised ([`Machine::retract`]), removed if it stays dropped.
    rows: std::ops::Range<usize>,
}

/// Diagnostic counts, as (counter, how many).
type Counts = Vec<(usize, u32)>;

/// The paths one command left.
enum Next {
    /// One path, still running.
    One,
    /// One path, which has ended.
    Ended,
    /// Several paths, all parked.
    Many(Vec<Config>),
    /// No path: every arm was impossible, or a loop head already covers the state.
    None,
}

/// The state of a [`Machine::drive`] outside the machine: the enclosing
/// command's state and the paths. Saved in checkpoints.
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
    /// Every input stack held by its paths, and whether it belongs to a path
    /// that may still run (rather than a state kept for joins). A resumed run
    /// points each of them at the current file contents.
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
    /// The arm to take at an undecided choice. Every one of `arms` is taken,
    /// each on its own path, by rerunning the command. `None` when the command
    /// may not make more choices, and the caller must handle it another way.
    pub(crate) fn branch(&mut self, by: Sym, arms: usize, span: Span) -> Option<usize> {
        self.branch_at(by, arms, span, false)
    }

    /// [`Machine::branch`]. `needed` means no other handling is sound (the
    /// paths cannot be merged), so the fork backstop of a hot site, which only
    /// bounds work, does not refuse it.
    fn branch_at(&mut self, by: Sym, arms: usize, span: Span, needed: bool) -> Option<usize> {
        if arms < 2 {
            return (arms == 1).then_some(0);
        }
        if !needed
            && self.script.len() <= self.choices.len()
            && let Some(arm) = self.page_arm(span)
        {
            return Some(arm);
        }
        if !self.cutting() && self.learned_cut() {
            return None;
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
        // Replays count toward the site's work too. Only new choices are
        // refused, so a replay still follows the run it repeats.
        if !needed && self.hot_site(span) && !scripted {
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

    /// Under `analysis.assume_page_one_arm`, once the run made
    /// `limits.output_forks` such choices, a choice during page building takes
    /// its first arm without forking: a float fits where it is tried, and a loop
    /// around a page clear ends. Float placement decides little else, and
    /// following every arm would run the output routine once per arm of every
    /// float and widen everything it wrote. The other arms are not followed.
    /// This is a named assumption, reported once per site.
    fn page_arm(&mut self, span: Span) -> Option<usize> {
        if !self.cfg.analysis.assume_page_one_arm {
            return None;
        }
        if !self.output_active() && !self.clearing_page() {
            return None;
        }
        if self.out.output_forks < self.limits.output_forks {
            return None;
        }
        self.diagnose(
            Severity::Assumption,
            "page-one-arm",
            span,
            "an undecided choice while a page is made takes its first arm: floats fit, loops around a page end"
                .to_string(),
        );
        Some(0)
    }

    /// Whether the text of `\\clearpage` is still being read. Checks the input
    /// stack, since `within` also holds calls whose text is used up.
    fn clearing_page(&self) -> bool {
        let Some(clear) = self.out.interner.lookup("clearpage") else { return false };
        self.input.iter().any(|f| matches!(f, Frame::Tokens { expanding: Some(s), .. } if *s == clear))
    }

    /// The arm [`Machine::branch`] returned does not exist, nor does any later
    /// one. The path is not a possible run of the document.
    pub(crate) fn no_arm(&mut self) {
        self.infeasible = true;
        self.halted = true;
    }

    /// The path ends here as a run of its own (TeX loops forever, or an
    /// assumption fails). No other path covers it, but it is not refuted, so
    /// its diagnostics stay and later arms are still explored.
    pub(crate) fn end_path(&mut self) {
        self.no_arm();
        self.ended_run = true;
    }

    /// Whether the running path took an arm over unknown text or over a
    /// joined meaning or text. What follows holds only where that text has
    /// the arm's shape.
    pub(crate) fn took_unknown_arm(&self) -> bool {
        self.choices.iter().any(|c| c.joined || c.by == self.unknown || self.is_unknown_name(c.by))
    }

    /// The choice just made picks one of the meanings or texts a join left.
    pub(crate) fn joined_arm(&mut self) {
        if let Some(c) = self.choices.last_mut() {
            c.joined = true;
        }
    }

    /// Whether the run may be one of several paths, so what it does now need
    /// not happen in every run.
    pub(crate) fn on_paths(&self) -> bool {
        self.parted || self.env.forking() || !self.choices.is_empty()
    }

    /// Runs commands on every path until `done` holds for all of them or they
    /// end. Paths that reach `done` are joined and continue as one.
    pub(super) fn drive(&mut self, unit: Unit, done: impl Fn(&Self) -> bool) {
        // A run resumed from a checkpoint continues with the saved drive.
        let Drive { mut outer, outer_unit, nested, mut paths, keep_errors, mut mark, mut commands } =
            self.checkpoints.resume().unwrap_or_else(|| {
                let outer =
                    (std::mem::take(&mut self.script), std::mem::take(&mut self.choices), self.runs, self.replayable);
                let outer_unit = (std::mem::replace(&mut self.unit, unit), self.lone);
                // Nested inside a command, whose rerun undoes what this drive counts.
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
            // Between two top-level commands, let the language server answer requests.
            if !ended && paths.top {
                crate::util::progress::pause();
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
            // Once no path compiles, the dropped paths continue right away. They
            // are behind the running path, so they catch up and join it.
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
                (self.approximated, self.blind) = (false, false);
            }
            let (live_before, steps_before) = (paths.parked.len(), self.out.steps);
            let next = if finished { None } else { Some(self.explore(&mut paths, true)) };
            if paths.top
                && let Some(hot) = &mut self.out.hot
            {
                hot.by_paths[live_before.min(7)] += self.out.steps - steps_before;
            }
            self.waiting = paths.parked.len() + paths.done.len();
            let peak = match self.output_active() {
                true => &mut self.out.peak_output_paths,
                false => &mut self.out.peak_paths,
            };
            if paths.parked.len() + 1 > *peak {
                *peak = paths.parked.len() + 1;
                if self.cfg.verbose >= 4 {
                    crate::util::progress::note!("satex: {} paths at {:?}", self.out.peak_paths, self.position_span());
                }
                if std::env::var_os("SATEX_WHY_APART").is_some() {
                    eprintln!("{}", why_apart(&paths, self.position_span(), &self.out));
                }
            }
            if self.cfg.trace_state && self.apart.as_ref().map(|a| a.0) != Some(paths.parked.len()) {
                let why = (paths.parked.len() > 1).then(|| why_apart(&paths, self.position_span(), &self.out));
                self.apart = Some((paths.parked.len(), why.unwrap_or_default().into()));
            }
            self.report_progress();
            self.report_paths(&paths, commands);
            self.report_memory(&paths);
            commands += 1;
            let live = match next {
                Some(Next::One) if self.at_join.is_some() && paths.top && paths.fork.is_some() => {
                    paths.at_output |= self.at_join.take() == Some(JoinAt::Output);
                    let env = self.env.path(paths.fork.as_ref().expect("checked"));
                    paths.arrived.push(Config::new(self.save_path(), env, false));
                    false
                }
                Some(Next::One) => {
                    self.at_join = None;
                    true
                }
                Some(Next::Ended) => {
                    if self.cfg.verbose >= 4 && paths.top {
                        let at = self.position_span();
                        let within: Vec<&str> = self.within.iter().rev().take(6).map(|(s, _)| self.name(*s)).collect();
                        crate::util::progress::note!(
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
                        match outcome.state.at_join.take().filter(|_| paths.top) {
                            Some(at) => {
                                paths.at_output |= at == JoinAt::Output;
                                paths.arrived.push(outcome);
                            }
                            None => self.park(&mut paths, &fork, outcome),
                        }
                    }
                    false
                }
                Some(Next::None) => false,
                // The last path is infeasible. The analysis never ends there
                // while input is left. Inside a command, the enclosing path is
                // the infeasible one, and its drive drops it.
                None if paths.fork.is_none() => match self.infeasible && self.halted && self.input_left() && !nested {
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
            // Paths wait at a join point until the running path has passed it,
            // i.e. it is further on, between two commands, in the same mode,
            // where their texts meet. A path waiting at the end of an output
            // routine inside a command keeps waiting, since the command may fire
            // the routine again. Once joined, they continue behind the running path.
            if live && !paths.arrived.is_empty() && self.passed(&paths) {
                let first = self.join_arrived(&mut paths, &fork);
                paths.parked.push(first);
            }
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
                    // An ended path waits for the end of the run and joins there.
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
                let first = self.join_arrived(&mut paths, &fork);
                ended = false;
                self.resume_config(&fork, first);
            } else if !live {
                let configs = std::mem::take(&mut paths.done);
                if configs.is_empty() {
                    if !ended && self.input_left() {
                        // Only paths dropped for an error are left, so they
                        // continue ([`Machine::settle`]).
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
                    // Out of input at the drive's own boundary (`run_tokens`):
                    // the last path finished the drive, not the job.
                    let drive_end = done(self) && matches!(self.input.last(), Some(Frame::Boundary));
                    if ended && !drive_end && !self.out.exhausted {
                        self.end_with_the_job(&mut paths, &fork);
                    }
                    self.halted |= !drive_end;
                    self.env.unfork(fork);
                    break;
                }
                let joined = self.join_done(&fork, &configs, true);
                // Paths that cannot be joined (different open conditionals, or
                // input that cannot be merged) become a choice of the enclosing
                // command. Once it may make no more choices they are joined anyway,
                // with the deepest conditional and group stacks standing for all.
                let joined = match joined {
                    Some(joined) => joined,
                    None => {
                        let span = self.position_span();
                        self.swap_choices(&mut outer);
                        let arm = self.branch_at(self.unknown, configs.len(), span, true);
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
                                    "paths a look ahead parted into end apart past the choices one command may make. They are joined, as deep in conditionals and groups as the deepest".into(),
                                );
                                joined
                            }
                            None => {
                                // During a cut the region is summarized whichever path it follows.
                                if arm.is_none() && !self.cutting() {
                                    self.diagnose(
                                        Severity::Unsupported,
                                        "unjoined-lookahead",
                                        span,
                                        "paths a look ahead parted into end apart in different files past the choices one command may make. The first is followed".into(),
                                    );
                                }
                                // A replayed arm beyond the paths this run ended with is infeasible.
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
            // Ended paths wait for the end of the run, so their fork stays.
            if paths.parked.is_empty() && paths.done.is_empty() && paths.arrived.is_empty() {
                self.settle_closed(&mut paths);
            }
            let idle = paths.parked.is_empty()
                && paths.done.is_empty()
                && paths.arrived.is_empty()
                && paths.job_ends.is_empty()
                && paths.input_ends.is_empty();
            if idle && paths.shelved.is_empty() {
                self.env.unfork(fork);
                paths.heads.clear();
            } else if idle {
                // Only paths dropped for an error are waiting. Rebase them and the
                // recursion summaries onto the running path's current state, so
                // later forks start from here and not from the error's command.
                // Otherwise a join would compare everything the document did since.
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

    /// The run ends in the join of all paths that ended the same way as the
    /// last one (by `\end`, or by running out of input), never in just one of
    /// them. If the last path ran out of input without `\end` while others ran
    /// `\end`, it is a fatal error ("*** (job aborted, no legal \end found)",
    /// tex.web § 360). A document that compiles takes no such path, so the run
    /// ends as the `\end` paths did.
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
        // Join the effects even when the pending code or input differs: the
        // run is over, so what they would do or read next does not matter.
        let last = match self.join_done(fork, &ends, false) {
            Some(joined) => joined,
            None => {
                let (last, rest) = ends.split_last().expect("not empty");
                rest.iter().fold(last.clone_config(), |a, b| self.join_configs(fork, &a, b, None, true).0)
            }
        };
        self.resume_config(fork, last);
    }

    /// Whether a file is still being read and the budget is not spent.
    pub(super) fn input_left(&self) -> bool {
        !self.out.exhausted && self.input.iter().any(|f| matches!(f, Frame::File { mouth, .. } if !mouth.ending()))
    }

    /// Every path was dropped before the input ended, so an approximation on
    /// the way went wrong. The run never ends there. It continues from the
    /// last path the way the guard continues after a cut: what that path wrote
    /// since the fork (`written`) and what the region assigned become unknown,
    /// the remaining token lists down to the file are dropped, and an unknown
    /// token stands for what they would have done.
    fn read_on_dropped(&mut self, written: Vec<Sym>) {
        let span = self.position_span();
        if self.cfg.verbose >= 4 {
            crate::util::progress::note!(
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
            "every path was dropped here before the input ended. The run goes on after the expansion they were in, and what it assigned is unknown".into(),
        );
        let syms = self.typed_syms();
        self.env.widen(&written, None, &crate::engine::env::typed!(self, syms));
        self.mint();
        (self.halted, self.infeasible) = (false, false);
        self.taint_after_cut();
        self.resync_now();
    }

    /// Called when no other path waits and the running one finished or
    /// continued with a certain error. If it failed, nothing compiles, so the
    /// dropped paths continue with their errors kept. Otherwise the dropped
    /// paths were not runs of the document, and their errors are reported as drops.
    fn settle(&mut self, paths: &mut Paths, ended: bool) {
        let shelved = std::mem::take(&mut paths.shelved);
        if !paths.failed {
            for s in shelved {
                self.retract(s.rows);
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

    /// How much the running command raised the counts of existing diagnostics,
    /// read from `undo[mark..]`.
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

    /// Swaps the running drive's choices with the enclosing command's.
    fn swap_choices(&mut self, outer: &mut (Vec<usize>, Vec<Choice>, usize, bool)) {
        std::mem::swap(&mut self.script, &mut outer.0);
        std::mem::swap(&mut self.choices, &mut outer.1);
        std::mem::swap(&mut self.runs, &mut outer.2);
        std::mem::swap(&mut self.replayable, &mut outer.3);
    }

    /// Joins the paths that finished the drive's command. Their inputs join as
    /// one text ([`Machine::widen_input`]), so a path that stopped elsewhere
    /// still reads what it has left. A number scan ([`Unit::Number`]) and a
    /// definition body also need equal conditionals, groups and scans. `None`
    /// when they cannot be joined.
    fn join_done(&mut self, fork: &Fork, configs: &[Config], strict: bool) -> Option<Config> {
        // A forced join starts from the path deepest in groups.
        let start = match strict {
            true => 0,
            false => {
                (0..configs.len()).max_by_key(|&k| (configs[k].env.groups().len(), configs[k].state.conds.len()))?
            }
        };
        let mut order: Vec<Config> = vec![configs.get(start)?.clone_config()];
        order.extend(configs.iter().enumerate().filter(|&(k, _)| k != start).map(|(_, c)| c.clone_config()));
        let stacks = strict && matches!(self.unit, Unit::Number | Unit::Body { .. });
        let (mut joined, _) =
            self.fold(fork, order, 1, |x, y| same_code(&x.tag, &y.tag) && (!stacks || same_stacks(x, y)));
        (joined.len() == 1).then(|| joined.pop()).flatten()
    }

    /// Runs one command on the running path, once for every combination of
    /// arms its choices allow. Without `head` it is not checked against the
    /// loop heads, because it starts from one.
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
        let mut dropped: Vec<(Config, String, Counts, std::ops::Range<usize>)> = Vec::new();
        // Each rerun and each re-entry after a cut is charged, as in the drive.
        let mut mark = if head { self.out.steps.wrapping_sub(1) } else { self.out.steps };
        loop {
            self.charge(&mut mark);
            self.choices.clear();
            if self.runs > 0 {
                self.path_ids += 1;
                self.path_id = self.path_ids;
            }
            (self.infeasible, self.ended_run) = (false, false);
            self.at_join = None;
            // A top-level run starts with no pending error. A nested one keeps its path's.
            (self.dropping, self.dropping_possible, self.dropping_breaks, self.dropping_nested) = match paths.top {
                true => (None, false, false, false),
                false => start.dropping.clone(),
            };
            let rows_from = self.raised_mark();
            let run_from = self.out.steps;
            let going = match self.unit {
                Unit::Command => self.command(),
                unit => self.scan_step(unit),
            };
            if self.runs > 0
                && paths.top
                && let Some(hot) = &mut self.out.hot
            {
                hot.replayed += self.out.steps - run_from;
            }
            self.runs += 1;
            first = first.or(self.choices.first().copied());
            // A refuted run ends its combination of arms: the other arms
            // of its last choice are tried next (see `next_script`).
            let refuted = self.infeasible && !self.ended_run;
            let next = next_script(&self.choices, refuted);
            let rows = rows_from..self.raised_mark();
            // A refuted run raised no diagnostics.
            if refuted {
                self.retract(rows.clone());
            }
            // In a nested drive, a path keeps an error only if TeX's recovery
            // would break library code's brackets. The enclosing command's run
            // then drops it if another path is certainly feasible. From any
            // other error, TeX recovers where it stands.
            if !paths.top {
                match self.dropping_breaks {
                    true => self.dropping_nested = true,
                    false => (self.dropping, self.dropping_possible) = (None, false),
                }
            }
            let error = match paths.top {
                true => self.dropping.take().filter(|_| !self.infeasible),
                false => None,
            };
            // An error that may be an artifact of approximation drops its path
            // only if another path is certainly feasible (approximated nothing
            // since the fork). Otherwise the path continues as TeX recovers,
            // unless the recovery would break library code's brackets: then
            // the fault is contained.
            let error = match error {
                Some(e) if self.dropping_possible && !Self::exact_other(paths, &outcomes) => {
                    if going && self.dropping_breaks && !self.dropping_nested {
                        self.contain_fault(&e, file);
                    }
                    None
                }
                e => e,
            };
            if outcomes.is_empty() && dropped.is_empty() && next.is_none() {
                self.choices.clear();
                // If other paths wait, this one is shelved with its certain error.
                let others = !paths.parked.is_empty() || !paths.arrived.is_empty();
                paths.failed |= error.is_some() && !others;
                if let Some(error) = error.filter(|_| others) {
                    let base = paths.fork.as_ref().expect("others wait");
                    let (state, env, at) = (self.save_path(), self.env.path(base), self.position_span());
                    let counts = self.made_counts(counters.0);
                    paths.shelved.push(Shelved { config: Config::new(state, env, !going), error, at, counts, rows });
                    self.restore_path(start);
                    self.env.rewind(&fork);
                    // If this run created `paths.fork`, the shelved path still needs it.
                    if !own || paths.fork.is_none() {
                        self.env.unfork(fork);
                    }
                    self.undo_counters(counters);
                    return Next::None;
                }
                // An earlier refuted run set `paths.fork` to this fork. Unfork it only once.
                if own {
                    paths.fork = None;
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
                    Some(error) => dropped.push((config, error, self.made_counts(counters.0), rows)),
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
        // Keep the last path even with its error: the document has that error.
        if outcomes.is_empty() && paths.parked.is_empty() && paths.arrived.is_empty() {
            paths.failed |= !dropped.is_empty();
            outcomes.extend(dropped.drain(..).map(|d| d.0));
        }
        for (config, error, counts, rows) in dropped {
            let at = self.position_span();
            paths.shelved.push(Shelved { config, error, at, counts, rows });
        }
        if outcomes.is_empty() {
            return Next::None;
        }
        match first {
            Some(choice) if head => self.loop_head(paths, choice, start, outcomes),
            _ => Next::Many(outcomes),
        }
    }

    /// Whether the loop site at `span` may still fork. Once its budget is spent,
    /// the whole head is widened instead of forking at a conditional in the next pass.
    fn site_affords(&self, span: Span) -> bool {
        let limit = self.limits.site_forks;
        limit == 0 || self.site_forked(span) < limit
    }

    /// Whether the loop of choices by `by` at `span` was entered more than
    /// `limits.loop_entries` times here. Every further entry counts as a re-entry.
    pub(crate) fn loop_capped(&self, span: Span, by: Sym) -> bool {
        let id = (span, by, self.source_progress - self.lines_read);
        self.scratch.loop_summaries.get(&id).is_some_and(|c| c.1 > u32::from(self.cfg.limits.loop_entries))
    }

    /// Whether some path other than the running one is exact (approximated
    /// nothing since the fork).
    fn exact_other(paths: &Paths, outcomes: &[Config]) -> bool {
        let mut others = paths.parked.iter().chain(&paths.arrived).chain(&paths.done).chain(outcomes);
        others.any(|c| !c.state.approximated)
    }

    /// A command that branched, started in `start`, is a loop head. When met
    /// again at the same place with nothing read in between, its state is
    /// joined into the head's, and paths continue from the join only if the
    /// join grew. When met deeper in the input, it is a recursion, cut after
    /// `widen_after` levels.
    fn loop_head(&mut self, paths: &mut Paths, choice: Choice, start: PathState, mut outcomes: Vec<Config>) -> Next {
        let Choice { span, by, .. } = choice;
        self.out.heat(|h| &mut h.heads, span);
        let fork = paths.fork.clone().expect("paths parted");
        let progress = start.source_progress;
        let here = Config::new(start, self.env.path(&fork), false);
        let same = |h: &Head| h.span == span && h.progress == progress;
        let widen_after = u32::from(self.limits.widen_after);
        // Forced convergence: every re-entry counts, whatever its state.
        let id = (span, by, progress);
        let depth = (here.key.places.len(), here.key.conds.len(), here.key.groups.len());
        // A re-entry (as opposed to a later, separate call) is at least as deep
        // in conditionals and groups, and its call was read from input above
        // the level the entry's call came from, i.e. input the pass produced.
        // A sibling call from that level, or from a level created after the
        // entry ended, is a new entry.
        let levels: Vec<Level> = here.state.input.iter().filter_map(level).collect();
        // Per level: whether the rest of the list has no unknown text, so
        // reading it ends after at most as many commands as it has tokens.
        let known: Vec<bool> = live_frames(&here.state.input)
            .into_iter()
            .map(|f| {
                pending(&[f]).is_some_and(|t| !t.iter().any(|t| t.cs().is_some_and(|s| self.is_unknown_marker(s))))
            })
            .collect();
        let reentry = |l: &Loop| {
            l.id == id && l.depth.1 <= depth.1 && l.depth.2 <= depth.2 && {
                // Walk up from the bottom while the entry's levels are unchanged
                // (same list at the same position). If a level was read further
                // since, the call came after the entry ended.
                let (mut m, mut read) = (0, false);
                while m < l.levels.len().min(levels.len()) {
                    let equal = here.key.places.get(m) == Some(&l.places[m]);
                    if !equal {
                        read = l.levels[m] == levels[m];
                        break;
                    }
                    m += 1;
                }
                // Read further in the entry's own list, at the same depth: a
                // sibling call (tex.web §§ 321-322). From a list the entry
                // called: a recursion.
                let sibling = !read
                    && m == l.levels.len()
                    && levels.len() == m + 1
                    && known.get(m) == Some(&true)
                    && (depth.1, depth.2) == (l.depth.1, l.depth.2)
                    && l.top.as_ref().is_some_and(|(top, at)| {
                        levels.get(m) == Some(top) && here.key.places.get(m).is_some_and(|p| p != at)
                    });
                // If a level below the entry's top was read further, every
                // level above it ended first (tex.web § 325), including the
                // entry's call, however deep this call runs.
                m > 0 && !sibling && !read
            }
        };
        // Met again after an enclosing loop passed a page: this is a call from
        // that loop's next pass, not a pass of this loop. Page building repeats
        // per page, and only the outermost loop counts.
        let pages = here.state.pages;
        let paged = |l: &Loop| {
            pages > l.pages
                && paths.loops.iter().any(|o| {
                    o.id != l.id
                        && o.pages > l.pages
                        && o.places.len() < l.places.len()
                        && l.places.starts_with(&o.places)
                })
        };
        let paged: Vec<LoopId> = paths.loops.iter().filter(|l| paged(l)).map(|l| l.id).collect();
        let paged_meet = paths.loops.iter().any(|l| l.id == id && reentry(l) && paged.contains(&l.id));
        let reentry = |l: &Loop| reentry(l) && !paged.contains(&l.id);
        // A summarized loop met again inside its own levels stops the path.
        // If the summary's own path meets the loop again, it is not dropped but
        // summarized again right away: the first summary already widened what
        // the loop writes. Each further meeting unwinds one more token level
        // below the entry, never a file.
        if self.cfg.verbose >= 4 && !here.state.summary_of.is_empty() {
            crate::util::progress::note!(
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
            // Unwind to the entry, never below it: the callers' frames hold
            // their own closers (`\fi`, `\endgroup`) and the code after the loop.
            let progress = here.key.progress;
            self.resume_config(&fork, here);
            if self.exits_carry(paths, &fork, l, progress, span) {
                return Next::None;
            }
            let before = (self.input.iter().filter_map(place).count(), self.conds.len(), self.env.depth());
            self.unwind(entry, span);
            // Nothing was unwound (the source is on top). Reading one more
            // unknown would reach this head again with nothing read, so the
            // branch runs like any other.
            if before == (self.input.iter().filter_map(place).count(), self.conds.len(), self.env.depth()) {
                // Already at the entry. If a path after the summary carries
                // the continuation, this pass is covered and ends. After
                // `widen_after` meetings this path is the continuation itself,
                // so it is widened and cut ([`Machine::end_covered`]).
                if exits || paths.stopped[l].passes > widen_after {
                    let here = Config::new(self.save_path(), self.env.path(&fork), false);
                    return self.end_covered(paths, &fork, l, here, span, depth);
                }
                return Next::Many(outcomes);
            }
            self.approximated = true;
            self.taint_after_cut();
            self.read_unknown(span);
            return self.explore(paths, false);
        }
        if let Some(l) = paths.stopped.iter().position(reentry) {
            // A pass of a loop that is already summarized, whose summary path
            // went past it. What this pass wrote becomes unknown on every
            // waiting path (which carry the summary's continuation), then it ends.
            return self.end_covered(paths, &fork, l, here, span, depth);
        }
        // A fresh entry (a later call, not a pass) restarts the pass count, but
        // only `limits.loop_entries` times per point. After that every entry is
        // a re-entry, so every point is summarized after finitely many passes.
        let fresh = !paths.loops.iter().any(|l| l.id == id && reentry(l));
        let counts = self.scratch.loop_summaries.entry(id).or_default();
        counts.1 += u32::from(fresh && !paged_meet);
        let (summaries, capped) = (counts.0, counts.1 > u32::from(self.cfg.limits.loop_entries));
        // Forget entries whose call ended: they are never unwound to.
        paths.loops.retain(|l| l.id != id || reentry(l));
        if !capped {
            paths.stopped.retain(|l| l.id != id);
        }
        // Already summarized in another drive: cut at the first re-entry.
        let k = match summaries > 0 {
            true => 1,
            false => widen_after.max(1),
        };
        // The bound this pass starts with, passed on to its arms.
        let bound = self.pass_bound(&here.state.input, &here.state.streams);
        for c in outcomes.iter_mut() {
            let bounds = Rc::make_mut(&mut c.state.loop_bounds);
            bounds.retain(|(l, _)| *l != id);
            bounds.push((id, bound));
        }
        let entry = paths.loops.iter().find(|l| l.id == id).map(|l| (l.depth.0, l.depth.1, l.passes));
        let forced = match paths.loops.iter().position(|l| l.id == id) {
            Some(l) => {
                let (joined, _) = self.join_passes(&fork, &paths.loops[l].joined, &here, None, true);
                // A pass with strictly less left to read (stream lines, then
                // bounded text) is not counted: such loops end by construction,
                // with every state exact.
                let before = here.state.loop_bounds.iter().find(|(l, _)| *l == id).and_then(|(_, b)| *b);
                let shrinks = matches!((before, bound), (Some(a), Some(b)) if b < a);
                let l = &mut paths.loops[l];
                l.pages = pages;
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
                    pages,
                    recent: self.env.recent_len(),
                    levels,
                    places,
                    top,
                    entry: here.env.clone(),
                    joined,
                    rerun: false,
                    summed: 0,
                    made: here.state.scans.last().filter(|a| a.name.is_none() && a.number.is_none()).map(Accum::len),
                });
                keep_last(&mut paths.loops, self.limits.loop_heads);
                capped
            }
        };
        let syms = self.typed_syms();
        // A path that differs from a head in code it will still run (pgfmath
        // with or without a unary minus on its stack) is a separate head.
        // Past `limits.heads_apart` such heads at one place, they are joined.
        // On a new page the enclosing loop's head joins the passes, so heads
        // from earlier pages are dropped.
        if paged_meet {
            paths.heads.retain(|h| !same(h));
        }
        let crowded = paths.heads.iter().filter(|h| same(h)).count() >= usize::from(self.limits.heads_apart);
        let mut fits: Vec<bool> = {
            let typed = crate::engine::env::typed!(self, syms);
            let slot = |sym: Sym| self.name(sym).starts_with(|c: char| c < ' ');
            // Only compare heads at this place. Comparing bindings is costly.
            paths.heads.iter().map(|h| same(h) && (crowded || data_apart(&h.config, &here, &typed, &slot))).collect()
        };
        // When forced and no separate head matches exactly, join all heads at
        // this place. A join that converges by widening needs no summary.
        if forced && !paths.heads.iter().zip(&fits).any(|(h, f)| *f && h.config.key == here.key) {
            fits = paths.heads.iter().map(same).collect();
        }
        // When forced and a head here differs only in being fewer groups of one
        // kind deep, widen both to "any number of them" from where they differ
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
        // A tail call from an `\else` arm re-enters under frames that hold only
        // `\fi`, so keys are also compared without them and their conditionals.
        let heads = || paths.heads.iter().enumerate().filter(|(i, _)| fits[*i]);
        let exact = heads().find(|(_, h)| same(h) && h.config.key == here.key).map(|(i, _)| i).or_else(|| {
            let b = here_bare.as_ref()?;
            heads().find(|(_, h)| same(h) && bare(&h.config.key) == *b).map(|(i, _)| i)
        });
        let folded_exact = exact.is_some_and(|h| paths.heads[h].config.key != here.key);
        // Heads that differ only in digits (a counter in the argument, as in
        // `\__flag_height_loop:wN`) are one head with the digits made unknown
        // ([`widen_input`]). Other differences make new heads.
        let typed = crate::engine::env::typed!(self, syms);
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
        // Alike after dropping the `\fi` tail means the fixpoint is reached, so force now.
        let forced = forced || (folded && entry.is_some_and(|e| e.2 >= 1));
        let (forced_new, forced_join) = (forced, forced);
        // The input the head is compared with: this path's, without its `\fi` tail.
        let here_input = match folded {
            true => fi_frames_dropped(&here.state.input, entry.map_or(0, |e| e.0), &is_fi),
            false => here.state.input.clone(),
        };
        // Heads at one place whose token lists differ in more than digits (an
        // argument growing every pass): after `widen_after` of them, the newest
        // is treated as the same head with its input widened. A head whose input
        // held more bounded text is an earlier pass of a loop that ends by
        // construction, so it is neither kin nor a deeper level.
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
        // Whether widening grew the head's input. If so, what it reads next is
        // not covered by what it already ran with.
        let mut input_grew = false;
        let (h, here) = match (exact, alike) {
            (Some(h), _) => (h, here),
            (None, Some(h)) => {
                let widen = paths.heads[h].passes >= widen_after || !self.site_affords(span);
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
        // A summary runs the body once more from the widened state, unless a
        // scan is in progress: that drive would end and summarize again.
        let cut = !here.key.scans.is_empty();
        if forced_new && (h == usize::MAX || (!folded && paths.heads[h].config.key != here.key)) {
            return self.summarize(paths, &fork, here, id, span, by, cut);
        }
        if h == usize::MAX {
            // Met again inside its own input, it is a recursion: every frame
            // below the one it ran from is still there.
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
                // Widen: join the deepest level's state into this one, and let
                // the call it made, with every level below, expand to unknown text.
                self.diagnose(
                    Severity::Imprecision,
                    "recursion-widened",
                    span,
                    format!(
                        "\\{} decides a recursion and could not be decided. Its deeper levels are widened",
                        self.name(by)
                    ),
                );
                if self.cfg.verbose >= 4 {
                    let places: Vec<String> = here.key.places.iter().map(|p| self.describe_place(p)).collect();
                    crate::util::progress::note!("satex: recursion widened at {span:?}, input: {places:#?}");
                }
                let depth = paths.heads[level].config.key.places.len();
                let (joined, _) = self.join_passes(&fork, &paths.heads[level].config, &here, Some(Reach::Havoc), false);
                self.resume_config(&fork, Config::new(here.state, joined.env, false));
                while self.input.iter().filter_map(place).count() > depth {
                    self.pop_frame();
                }
                self.scratch.summarized.insert(span);
                self.read_unknown(span);
                // Every path cut here joins one widened summary, so the cuts
                // reach a fixpoint instead of each continuing on its own.
                let cut = Config::new(self.save_path(), self.env.path(&fork), false);
                let typed = crate::engine::env::typed!(self, syms);
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
                let (joined, growth) = self.join_passes(&fork, &paths.cut[i].1, &cut, Some(Reach::Havoc), false);
                let growth =
                    Growth { parts: [growth.parts, if grew { vec!["input"] } else { vec![] }].concat(), ..growth };
                if !growth.any() {
                    return Next::None;
                }
                paths.cut[i].1 = Config::new(joined.state.clone(), joined.env.clone(), false);
                self.resume_config(&fork, joined);
                return self.explore(paths, false);
            }
            // Drop heads that every path has read past: nothing meets them again.
            // Source progress alone never retires a head, since hook code
            // runs without reading source.
            let typed = crate::engine::env::typed!(self, syms);
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
        // A pass forced by the pass count is first joined with widening. If that
        // adds nothing, the loop is at its fixpoint; one more widened pass may
        // reach it. Only a loop that keeps growing is summarized.
        let widen = paths.heads[h].passes >= widen_after || forced_join || !self.site_affords(span);
        // After a join at the head, the next pass runs from the join ([`Reach::Joined`]).
        let reach = widen.then_some(Reach::Joined);
        let (joined, mut growth) = self.join_passes(&fork, &paths.heads[h].config, &here, reach, !widen);
        if input_grew {
            growth.parts.push("input");
        }
        // The head ran on its join's arms ([`crate::engine::env::Corr`]). A pass
        // that matches none of them is not covered, even if the join contains it.
        if !self.env.covers(&fork, &paths.heads[h].config.env, &here.env) {
            growth.parts.push("arms");
        }
        if !growth.any() {
            return Next::None;
        }
        let forced_join = forced_join && paths.heads[h].passes > widen_after;
        // Still growing pass after pass despite widening: the widening does not
        // converge, so the loop is summarized (forced convergence).
        let diverges = widen && paths.heads[h].passes >= widen_after.saturating_mul(2);
        if forced_join || (diverges && paths.loops.iter().any(|l| l.id == id)) {
            return self.summarize(paths, &fork, here, id, span, by, cut);
        }
        if self.cfg.verbose >= 4 && widen {
            let names: Vec<&str> = growth.slots.iter().take(8).map(|&sym| self.name(sym)).collect();
            crate::util::progress::note!(
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
                "\\{} decides a loop and could not be decided. Every number of passes is joined{widened}",
                self.name(by)
            ),
        );
        self.resume_config(&fork, joined);
        self.explore(paths, false)
    }

    /// Two paths at the same place that differ only in how many groups or
    /// conditionals of one kind they have open above a common base. Both are
    /// widened to "any number of that kind" from there (stacks as intervals,
    /// [`crate::engine::env::PathEnv::many_from`]). Only for groups opened since
    /// the fork, plain ones with the same tokens waiting.
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

    /// Summarizes a loop met `widen_after` times. What it wrote becomes
    /// unknown, and its body runs once more from there (with `cut`, it is cut
    /// right away). Then its input unwinds to the entry and reads one unknown
    /// token, and the conditionals and groups it opened are assumed closed.
    /// If met again in this drive, the path stops.
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
            crate::util::progress::note!(
                "satex: loop \\{} at {span:?} summarized, entered at {depth:?}",
                self.name(by)
            );
        }
        self.scratch.loop_summaries.entry(id).or_default().0 += 1;
        // Join and widen the states the passes started in. A name the passes
        // agree on keeps its value; one they differ in may hold any of their
        // values or more. `here` goes first, since the summary continues in
        // its groups. Without `cut`, the body runs once more from this join,
        // and what that pass writes is widened when it meets the loop again
        // ([`Machine::end_covered`], [`Reach::Joined`]). With `cut`, no pass runs.
        let reach = if cut { Reach::Havoc } else { Reach::Joined };
        let (joined, _) = self.join_passes(fork, &here, &summarized.joined, Some(reach), true);
        // Names on which the first pass and this one disagree are unknown after
        // the skipped passes. The passes may not have met a head to widen at
        // (each deeper in conditionals or groups), so their join only lists
        // the values seen so far.
        summarized.joined = here.clone_config();
        summarized.summed = self.env.assignments;
        let began = summarized.entry.clone();
        let written = here.env.differing_over(&began, |s| Some(self.env.at_fork(fork, s)));
        paths.stopped.push(summarized);
        keep_last(&mut paths.stopped, self.limits.loop_heads);
        self.resume_config(fork, Config::new(here.state, joined.env, false));
        // A bound no pass changed stays. One that changed is widened to its end.
        let syms = self.typed_syms();
        self.env.widen(&written, Some((&began, fork)), &crate::engine::env::typed!(self, syms));
        self.mint();
        let here = Config::new(self.save_path(), self.env.path(fork), false);
        let env = here.env.clone();
        self.cut_loop(paths, fork, here, env, id, depth, span, by, cut)
    }

    /// Called when a pass of an already summarized loop is met again, after the
    /// summary path went past it. What this pass wrote becomes unknown on every
    /// waiting path (they carry the summary's continuation). Then the pass ends.
    fn end_covered(
        &mut self,
        paths: &mut Paths,
        fork: &Fork,
        l: usize,
        here: Config,
        span: Span,
        depth: (usize, usize, usize),
    ) -> Next {
        // A name the pass wrote but certainly restored to the summary's value
        // (a nesting depth up and down again) keeps it. An unknown or joined
        // value equal to the summary's does not count: it may stand for
        // something else together with the names it correlates with.
        let summary = paths.stopped[l].joined.env.clone();
        let differ: std::collections::HashSet<Sym> =
            self.env.path(fork).differing_over(&summary, |s| Some(self.env.at_fork(fork, s))).into_iter().collect();
        let mut written = self.env.recent_since(paths.stopped[l].recent);
        written.retain(|s| differ.contains(s) || !self.env.slot(*s).is_some_and(|b| b.certain));
        let syms = self.typed_syms();
        if self.cfg.verbose >= 4 {
            crate::util::progress::note!("satex: loop at {span:?} met again at {depth:?}, covered by its summary");
        }
        // No waiting path continued past the summary (it ended, or the waiting
        // paths are other arms, not this loop's continuation). So this pass is
        // the only continuation: it is cut, widened, and continues with an
        // unknown token.
        let id = paths.stopped[l].id;
        let carried =
            paths.parked.iter().chain(&paths.arrived).chain(&paths.done).any(|p| p.state.summary_of.contains(&id));
        if !carried {
            let mut entry = paths.stopped[l].depth;
            // Met again with no source read since the last cut: the cut left
            // the text that leads back here. Each such meeting drops one more
            // input level. When only source is left, the line is skipped.
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
            self.env.widen(&written, Some((&summary, fork)), &crate::engine::env::typed!(self, syms));
            self.mint();
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
            let carries = c.state.summary_of.contains(&id) && c.key.groups.len() <= here.key.groups.len();
            self.resume_config(fork, c.clone_config());
            // The summary's path comes after every pass, so code it wrote since
            // (a dispatching `\next`) stays, not the pass's version. In a group
            // opened since, the saved code is older and is widened.
            let after = paths.stopped[l].summed;
            let data: Vec<Sym> = (written.iter().copied())
                .filter(|&s| {
                    !carries
                        || self.env.stamp_of(s) <= after
                        || !self.env.slot(s).is_some_and(crate::engine::env::is_code)
                })
                .collect();
            self.env.widen(&data, Some((&summary, fork)), &crate::engine::env::typed!(self, syms));
            self.mint();
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

    /// Cuts a loop the verification pass did not close. If a pass already
    /// left it, continue there ([`Machine::exits_carry`]). Otherwise its
    /// input unwinds to the entry and reads one unknown token.
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
        let progress = here.key.progress;
        self.resume_config(fork, Config::new(here.state, env, false));
        let l = paths.stopped.iter().rposition(|l| l.id == id);
        let carried = cut && l.is_some_and(|l| self.exits_carry(paths, fork, l, progress, span));
        if cut && !carried {
            self.unwind(depth, span);
        }
        self.diagnose(
            Severity::Imprecision,
            "recursion-widened",
            span,
            format!(
                "\\{} decides a loop met {} times. It is summarized, so what it writes may hold anything",
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
        if carried {
            return Next::None;
        }
        if cut {
            self.read_unknown(span);
        }
        self.explore(paths, false)
    }

    /// Finds paths that left stopped loop `l` by ending the text scan it ran in
    /// (its `}` came from `\prg_break_point: }` or from the loop's own code).
    /// The summary continues there: what the loop writes, and what the scan
    /// produced since the entry, become unknown on them. Returns whether any did.
    fn exits_carry(&mut self, paths: &mut Paths, fork: &Fork, l: usize, progress: u64, span: Span) -> bool {
        let Some(made) = paths.stopped[l].made else { return false };
        let id = paths.stopped[l].id;
        let made_toks = |a: &Accum| a.to_vec().into_iter().take(made).map(|t| t.tok).collect::<Vec<_>>();
        let before = self.scans.last().map(made_toks);
        let exits: Vec<usize> = (0..paths.done.len())
            .filter(|&i| {
                let c = &paths.done[i];
                c.key.progress == progress
                    && c.state.loop_bounds.iter().any(|b| b.0 == id)
                    && c.state.scans.last().map(made_toks) == before
            })
            .collect();
        if exits.is_empty() {
            return false;
        }
        let entry = paths.stopped[l].entry.clone();
        let summary = Config::new(self.save_path(), self.env.path(fork), false);
        // What the passes wrote: as the summary widens ([`Machine::summarize`]).
        let written = summary.env.differing_over(&entry, |s| Some(self.env.at_fork(fork, s)));
        let unknown = self.unknown_text(Unk::Any, span);
        let syms = self.typed_syms();
        for i in exits {
            self.resume_config(fork, paths.done[i].clone_config());
            self.env.widen(&written, Some((&entry, fork)), &crate::engine::env::typed!(self, syms));
            self.mint();
            self.wild.union(&summary.state.wild);
            if let Some(acc) = self.scans.last_mut() {
                acc.widen_from(made, unknown);
            }
            if !self.scratch.summary_of.contains(&id) {
                Rc::make_mut(&mut self.scratch.summary_of).push(id);
            }
            paths.done[i] = Config::new(self.save_path(), self.env.path(fork), false);
        }
        self.resume_config(fork, summary);
        true
    }

    /// How many conditionals and groups `t` closes, less those it opens.
    pub(super) fn closing(&self, t: &Token) -> (i64, i64) {
        self.closing_in(t, &mut Vec::new())
    }

    /// [`Machine::closing`], looking through parameterless macros (`\close`
    /// for `\endgroup`) that are not already in `seen`.
    fn closing_in(&self, t: &Token, seen: &mut Vec<Sym>) -> (i64, i64) {
        use crate::engine::builtins::Primitive as P;
        let char_group = |cat: crate::engine::tex::Catcode| match cat {
            crate::engine::tex::Catcode::Begin => (0, -1),
            crate::engine::tex::Catcode::End => (0, 1),
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

    /// Drops the token lists above `depth.0` input levels (never source text)
    /// and closes what they opened: the conditionals and groups opened since
    /// the lowest dropped list was pushed ([`Machine::close_since`]). If no
    /// list was dropped, those above `depth.1` conditionals and `depth.2` groups.
    pub(super) fn unwind(&mut self, depth: (usize, usize, usize), span: Span) {
        self.leak_call_recs();
        let mut at = None;
        // Count what the dropped text would close itself. Closing that much is
        // exactly what running it would do, so it is no assumption.
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
        // What the dropped text leaves open becomes "any number" of what the
        // passes opened, if it is all of one kind (stacks as intervals).
        let from = self.env.depth_upto(at.1);
        let open = (self.conds.len().saturating_sub(at.0) as i64, (self.env.depth() - from) as i64);
        if (open.0 > closes.0 || open.1 > closes.1) && self.many_since(at.0, from) {
            return;
        }
        let (groups, conds) = self.close_since(at, false);
        if (groups, conds) != (0, 0) && (conds as i64 > closes.0 || groups as i64 > closes.1) {
            self.diagnose(
                Severity::Assumption,
                "unbalanced-path",
                span,
                "a loop opened conditionals or groups it was summarized in. They are assumed closed".into(),
            );
        }
    }

    /// Collapses the conditionals above `conds` and the groups from `groups`
    /// up into one entry each standing for any number, if each is of one
    /// kind. Returns whether it did.
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
        let mut open = crate::analysis::machine::CondDepth::default();
        let strays = (0..text.len()).filter(|&i| open.feed(self.cond_prim(text[i])).is_some()).collect();
        (strays, open.0)
    }

    /// Whether `text` can be replaced by one token: its braces and
    /// conditionals all close inside it.
    fn whole(&self, text: &[Token]) -> bool {
        self.strays(text) == (Vec::new(), 0) && balanced(text)
    }

    /// Per length `k`, whether `text[..k]` is [`Self::whole`], in one pass.
    fn whole_prefixes(&self, text: &[Token]) -> Vec<bool> {
        let (mut open, mut depth, mut broken) = (crate::analysis::machine::CondDepth::default(), 0i64, false);
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

    /// Settles paths dropped for an error in a file the running path has since
    /// closed. The running path got through that file without the error, so the
    /// dropped paths are settled as at the drive's end ([`Machine::settle`]).
    /// Keeping them would hold the fork open, and every later load would run
    /// as if on several paths.
    fn settle_closed(&mut self, paths: &mut Paths) {
        if paths.failed || paths.shelved.is_empty() {
            return;
        }
        let open: Vec<FileId> = self
            .input
            .iter()
            .filter_map(|f| match f {
                Frame::File { mouth, .. } => Some(mouth.file),
                _ => None,
            })
            .collect();
        let closed = |s: &Shelved| {
            s.config
                .key
                .places
                .iter()
                .rev()
                .find_map(|p| match p {
                    Place::File(file, ..) => Some(*file),
                    _ => None,
                })
                .is_some_and(|file| !open.contains(&file))
        };
        let (gone, kept): (Vec<Shelved>, Vec<Shelved>) =
            std::mem::take(&mut paths.shelved).into_iter().partition(closed);
        paths.shelved = kept;
        for s in gone {
            self.retract(s.rows);
            self.report_dropped(&s.error, s.at);
        }
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

    /// Called when every path is at a join point (`analysis.join_paths_at`).
    /// Paths with the same conditionals, groups and scans whose inputs can be
    /// merged are joined. The first continues, the rest keep waiting.
    fn join_arrived(&mut self, paths: &mut Paths, fork: &Fork) -> Config {
        let arrived = std::mem::take(&mut paths.arrived);
        paths.at_output = false;
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
        let kept = if left == 0 { String::new() } else { format!(", {} stay apart", left + 1) };
        if count > 1 {
            self.diagnose(
                Severity::Imprecision,
                "paths-joined-at",
                span,
                format!("{count} paths arrived at a join point and were joined{kept}. They differed in {apart}"),
            );
        }
        first
    }

    /// Joins each of `configs` into the first earlier one that `same` accepts
    /// and whose input can be merged with it ([`Machine::join_two`]), until at
    /// most `limit` are left. Also returns what the joins added.
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

    /// Joins two paths ([`Machine::join_configs`]) including their input: kept
    /// as is where both read the same, else merged into one text
    /// ([`Machine::widen_input`]). `None` when they cannot be joined.
    fn join_two(&mut self, fork: &Fork, a: &Config, b: &Config) -> Option<(Config, Growth)> {
        let input = match a.key.places == b.key.places {
            true => None,
            false => Some(self.widen_input(&a.state.input, &b.state.input, self.position_span(), false, false)?),
        };
        let (joined, mut growth) = self.join_configs(fork, a, b, None, true);
        if a.tag.mode != b.tag.mode {
            growth.parts.push("mode");
        }
        let Some(input) = input else { return Some((joined, growth)) };
        growth.parts.push("input");
        Some((Config::new(PathState { input, ..joined.state }, joined.env, joined.ended), growth))
    }

    /// Whether the running path passed every path waiting at a join point
    /// ([`Paths::arrived`]).
    fn passed(&self, paths: &Paths) -> bool {
        let file = |p: &Place| matches!(p, Place::File(..) | Place::Pseudo(..));
        self.input.iter().filter_map(place).all(|p| file(&p))
            && paths.arrived.iter().all(|a| {
                a.key.progress < self.place_progress()
                    && a.tag.mode == self.mode
                    && (!paths.at_output || a.key.places.iter().all(file))
            })
    }

    /// The running path's source position, as [`Key`] counts it.
    fn place_progress(&self) -> u64 {
        self.source_progress - self.lines_read
    }

    /// The largest number of arms a digit split made among the names two paths
    /// hold differently, 0 if none ([`partner`]). If their macros hold
    /// different digits of a split number (a lookahead `\def`d from it), the
    /// paths are at different stages of reading it and stay apart until the
    /// statement ends (`usize::MAX`).
    fn digits_apart(&self, a: &Config, b: &Config) -> usize {
        if self.scratch.split_digits.is_empty() {
            return 0;
        }
        let differing = a.env.differing(&b.env);
        let texts = |c: &Config, sym: Sym| {
            let b = c.env.changed(sym);
            let may = b.and_then(|b| b.may.as_deref()).unwrap_or_default();
            let text = b.into_iter().map(|b| &b.meaning).chain(may).filter_map(|m| m.as_macro());
            self.digits_read(text.flat_map(|m| m.replacement_text.iter()))
        };
        if differing.iter().any(|&sym| texts(a, sym) != texts(b, sym)) {
            return usize::MAX;
        }
        let arms = differing.into_iter().filter_map(|sym| self.scratch.split_digits.get(&sym));
        arms.copied().max().unwrap_or(0)
    }

    /// Joins the running path with a parked one at the same place ([`partner`]).
    fn meet(&mut self, paths: &mut Paths, fork: &Fork) {
        // Cheap check first: building the full key copies the path's state, on every step.
        let (progress, conds, groups) = (self.place_progress(), self.conds.len(), self.env.groups().len());
        if paths
            .parked
            .iter()
            .any(|p| p.key.progress == progress && p.key.conds.len() == conds && p.key.groups.len() == groups)
        {
            let live = Config::new(self.save_path(), self.env.path(fork), false);
            if partner(&paths.parked, &live, &|a, b| self.digits_apart(a, b)).is_some() {
                let joined = self.absorb(paths, fork, live, true);
                self.resume_config(fork, joined);
            }
        }
    }

    /// Between two lines of a file, if a parked path is at the same place and
    /// in the same conditionals but in more groups: in the document's own text,
    /// both are widened to "any number" of the extra groups
    /// ([`Machine::widen_stacks`]) so they can join. In a package, with
    /// `analysis.assume_packages_close_groups`, the extra groups are assumed
    /// closed, since a package closes what it opens.
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
            // The waiting path is the deeper one, so it continues from here.
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
            "a path stood here in more groups than another. The groups are assumed closed, as a package closes what it opens".into(),
        );
    }

    /// How many paths a drive keeps apart before it joins those that are
    /// alike (see [`Machine::crowd`]). At most [`Machine::path_bound`].
    fn alike_bound(&self) -> usize {
        usize::from(self.limits.alike).min(self.path_bound())
    }

    /// How many paths a drive keeps apart: `limits.paths` between commands,
    /// `limits.choices` inside a scan, where the arms of a choice should join
    /// as soon as they can.
    fn path_bound(&self) -> usize {
        match self.unit {
            Unit::Command => usize::from(self.limits.paths),
            _ => usize::from(self.limits.choices),
        }
    }

    /// Called when more than `alike_bound` paths wait. Joins pairs that are
    /// `alike` (same stacks, texts may differ) with their input widened
    /// ([`widen_input`]), until the count is in bounds or no pair can be
    /// joined. Paths that are not alike keep waiting; this is reported once.
    fn crowd(&mut self, paths: &mut Paths, fork: &Fork) {
        let bound = self.alike_bound();
        let span = self.position_span();
        let (mut i, before) = (0, paths.parked.len());
        while paths.parked.len() > bound && i < paths.parked.len() {
            let joined = (i + 1..paths.parked.len()).find_map(|j| {
                let (a, b) = (&paths.parked[i], &paths.parked[j]);
                if (a.crowded && b.crowded) || !alike(a, b) || !self.reads_alike(a, b) {
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
            let (joined, _) = self.join_configs(fork, &widen(a), &widen(b), None, true);
            paths.parked.push(joined);
        }
        // The rest cannot be joined now. Try again only when more paths wait.
        paths.parked.iter_mut().for_each(|c| c.crowded = true);
        paths.crowded = paths.parked.len();
        if paths.parked.len() == before {
            return;
        }
        self.diagnose(
            Severity::Imprecision,
            "paths-widened",
            span,
            format!("more than {bound} paths waited. Those at one point were joined"),
        );
    }

    /// Whether the code two paths share above where their inputs differ means
    /// the same on both. If a name in it is bound differently (a parser's
    /// `\next` in two of its states), the paths read what follows at different
    /// points of one run and cannot be joined.
    fn reads_alike(&self, a: &Config, b: &Config) -> bool {
        let (x, y) = (live_frames(&a.state.input), live_frames(&b.state.input));
        let ahead: Vec<&Frame> =
            x.iter().rev().zip(y.iter().rev()).take_while(|(p, q)| place(p) == place(q)).map(|(p, _)| *p).collect();
        if ahead.len() == x.len() && x.len() == y.len() {
            return true;
        }
        frames_text(&ahead).all(|t| {
            t.cs().is_none_or(|s| {
                self.typed_text_of(*t).is_some() || self.one_of_members(*t).is_some() || a.env.binds_alike(&b.env, s)
            })
        })
    }

    /// Joins two texts by keeping their common prefix and suffix and replacing
    /// the differing middle with one token. If the middle has unbalanced
    /// braces or conditionals, the prefix and suffix shrink until it is whole.
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
    /// whether it leaves one open.
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

    /// Joins the input stacks of two alike paths. When they are equally deep,
    /// go level by level: equal levels are kept, and two token lists become
    /// one token standing for either beginning ([`Machine::one_of`]) followed
    /// by their common end. Otherwise keep the shared bottom levels and merge
    /// everything above them into one such text. `None` when a file or a
    /// boundary differs, or when code differs between two paths that are not
    /// two passes of one loop (`pass`): they are at different points.
    fn widen_input(&mut self, a: &Input, b: &Input, span: Span, widen: bool, pass: bool) -> Option<Input> {
        let (a, b) = (live_frames(a), live_frames(b));
        // Paths at different digits of one number are at different stages of
        // reading it, and join only once it is read. Levels at the same place
        // hold the same text.
        let same = a.iter().zip(&b).take_while(|(x, y)| place(x) == place(y)).count();
        if self.digits_read(frames_text(&a[same..])) != self.digits_read(frames_text(&b[same..])) {
            return None;
        }
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

    /// The digits of split numbers ([`Machine::digit_arms`]) that `text`
    /// holds: each number and which slice of its spelling (the whole number
    /// as `0, 0, 0`). Paths that differ here are at different stages of
    /// reading the number.
    fn digits_read<'t>(&self, text: impl Iterator<Item = &'t Token>) -> Vec<(Sym, u32, u32, u32)> {
        let split = &self.scratch.split_digits;
        if split.is_empty() {
            return Vec::new();
        }
        let mut read: Vec<_> = text
            .filter_map(|t| t.cs())
            .filter_map(|s| match self.typed_slice(s) {
                Some(slice) => split.contains_key(&slice.0).then_some(slice),
                None => split.contains_key(&s).then_some((s, 0, 0, 0)),
            })
            .collect();
        read.sort_unstable_by_key(|&(s, k, i, j)| (s.0, k, i, j));
        read.dedup();
        read
    }

    /// Joins two texts still to be read: one token standing for either
    /// beginning, followed by their common end. `None` when a beginning is not
    /// brace balanced, since one token cannot stand for a `}` that closes a
    /// group opened before it.
    fn widen_text(&mut self, p: &[Token], q: &[Token], span: Span, widen: bool, pass: bool) -> Option<Vec<Token>> {
        let mut common = p.iter().rev().zip(q.iter().rev()).take_while(|(x, y)| x.tok == y.tok).count();
        if common == p.len() && common == q.len() {
            return Some(p.to_vec());
        }
        // Shorten the common end until the differing beginnings are balanced,
        // so an argument that grows inside its braces becomes one text.
        let (wp, wq) = (self.whole_prefixes(p), self.whole_prefixes(q));
        while common > 0 && !(wp[p.len() - common] && wq[q.len() - common]) {
            common -= 1;
        }
        let (head, tail) = (&p[..p.len() - common], &p[p.len() - common..]);
        let other = &q[..q.len() - common];
        if !self.whole(head) || !self.whole(other) {
            return self.widen_middle(p, q, span, widen, pass);
        }
        // Passes that differ only in digits (a loop counter) become the text
        // with unknown digits, which the next pass meets unchanged. Between
        // different paths, digits in code are code: one of either
        // ([`Machine::join_texts`]), and unknown text once widened.
        let syms = self.typed_syms();
        let typed = crate::engine::env::typed!(self, syms);
        let (p, q) = (crate::engine::env::unknown_runs(head, &typed), crate::engine::env::unknown_runs(other, &typed));
        if pass && same_text(&p, &q) {
            let mut text = p.to_vec();
            text.extend_from_slice(tail);
            return Some(text);
        }
        // Also keep the common beginning when the differing rest is balanced.
        // A recursion whose passes differ in a letter at the end of their
        // input (`\__int_to_Base:nnnN`) then joins at that letter, and its
        // calls stay text a scan can read.
        let shared = head.iter().zip(other).take_while(|(x, y)| x.tok == y.tok).count();
        let shared = if self.whole(&head[shared..]) && self.whole(&other[shared..]) { shared } else { 0 };
        // At the end of a number scan, texts that differ only in numbers (one
        // typed with an interval on some path) become the digits of each hull.
        if matches!(self.unit, Unit::Number)
            && let Some(mut text) = self.join_numbers_in(head, other, widen, span)
        {
            text.extend_from_slice(tail);
            return Some(text);
        }
        // Data that code still to run will read is part of that code, so two
        // paths differing there differ in code. Data read first (a value
        // being scanned) is just data.
        if !pass && self.holds_code(&head[..shared]) {
            return None;
        }
        let one = self.join_texts(&head[shared..], &other[shared..], widen, pass, span)?;
        let mut text = head[..shared].to_vec();
        text.push(one);
        text.extend_from_slice(tail);
        Some(text)
    }

    /// Reads `\\fi`s that are next in token lists now, as TeX's main loop
    /// would. Arms that leave a recursion at different depths then end up at
    /// one place and join when parked.
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
            // Stop when the read is refused (cut or budget) or the `\fi` cannot
            // be closed here (output routine): no progress.
            let open = self.conds.len();
            if self.next_token().is_none() {
                break;
            }
            self.conditional_fi(fi.span);
            if self.conds.len() >= open {
                break;
            }
        }
    }

    /// Parks `config`, joined with any parked path at its place ([`partner`]).
    fn park(&mut self, paths: &mut Paths, fork: &Fork, config: Config) {
        let config = self.absorb(paths, fork, config, false);
        paths.parked.push(config);
    }

    /// Joins `config` with every parked path at its place ([`partner`]) and
    /// removes those from the parked list. With `first`, `config`'s state is
    /// the one kept.
    fn absorb(&mut self, paths: &mut Paths, fork: &Fork, mut config: Config, first: bool) -> Config {
        while !config.ended
            && let Some(i) = partner(&paths.parked, &config, &|a, b| self.digits_apart(a, b))
        {
            let other = paths.parked.swap_remove(i);
            let (a, b) = if first { (&config, &other) } else { (&other, &config) };
            config = self.join_configs(fork, a, b, None, true).0;
        }
        config
    }

    /// The parked path to run next, if the running one is ahead of it: the one
    /// furthest back, by source position and then input. Paths ahead thus
    /// wait where the others will reach them and join.
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

    /// Restores the counters that decide when the run gives up to their
    /// values at the start of the running command.
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

    /// A token that stands for one of `texts`. Each text is one of its
    /// meanings, as a parameterless macro. A path that executes it splits per
    /// text; anything else sees unknown text. A member that is itself such a
    /// token contributes its own texts. The name encodes the set, so every
    /// path and pass that builds the same set gets the same token with the
    /// same meaning. With more than `meaning_set` texts it is unknown text.
    pub(crate) fn one_of(&mut self, texts: Vec<Rc<[Token]>>, span: Span) -> Token {
        let prefix = format!("{UNKNOWN_NAME}{ONE_OF}");
        let mut members: Vec<Rc<[Token]>> = Vec::new();
        for text in texts {
            // Unknown text already contains every text.
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
        // Texts that are all numbers become the typed digits of their hull,
        // so the bounds survive the join.
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
        // Hex digits (letters or others) with an unknown part become typed
        // text of their class and longest length, like numbers become the
        // digits of their hull.
        let hex = TextClass::new(TextClass::DIGIT.0 | TextClass::LETTER.0 | TextClass::OTHER.0 | TextClass::HEX.0);
        let data = |m: &Self, t: &Token| matches!(t.tok, Tok::Chr(..)) || m.typed_text_of(*t).is_some();
        if members.len() > 1
            && members.iter().any(|t| self.has_unknown(t))
            && members.iter().all(|t| t.iter().all(|x| data(self, x)))
        {
            let texts: Vec<&[Token]> = members.iter().map(|t| &**t).collect();
            if self.cover(&texts).is_some_and(|(class, ..)| hex.contains(class) && class.0 & TextClass::HEX.0 != 0) {
                return self.unknown_text(Unk::Of(&texts), span);
            }
        }
        if members.len() > usize::from(self.limits.meaning_set) {
            let texts: Vec<&[Token]> = members.iter().map(|t| &**t).collect();
            return self.unknown_text(Unk::Of(&texts), span);
        }
        let (sym, may) = self.one_of_sym(members);
        let now = self.carried.passes_joined;
        self.carried.made.entry(sym).and_modify(|m| m.1 = true).or_insert((now, false));
        match self.env.slot(sym).and_then(|b| b.may.clone()) {
            // Rebuilt on a path that had narrowed it: restore all its texts.
            Some(had) if had.len() != may.len() => self.env.refine_may(sym, &may, &[]),
            Some(_) => {}
            None => {
                let mut binding = Binding::builtin(Meaning::Unknown);
                binding.may = Some(may.into());
                self.env.constant(sym, binding);
            }
        }
        Token::new(Tok::Cs(sym), span)
    }

    /// The name of the one-of token for `members`, and its meanings.
    pub(crate) fn one_of_sym(&mut self, members: Vec<Rc<[Token]>>) -> (Sym, Vec<Meaning>) {
        let mut named: Vec<(u128, Rc<[Token]>)> =
            members.into_iter().map(|t| (text_hash(&t, |s| self.name(s).to_owned()), t)).collect();
        named.sort_by_key(|n| n.0);
        let class = self.class_name(named.iter().map(|n| &n.1[..]));
        let ids = named.iter().map(|n| n.0).collect::<Vec<_>>();
        let sym = self.intern(&format!("{UNKNOWN_NAME}{ONE_OF}{ids:x?}{class}"));
        (sym, named.into_iter().map(|(_, text)| text_meaning(text)).collect())
    }

    /// One token for either of two balanced texts. Data with unknown parts
    /// becomes typed text, so a loop over it ends. Other data becomes a
    /// one-of token. Differing code is only joined between two passes of one
    /// loop (`pass`), as unknown text that may hold either. Two paths that
    /// differ in code are at different points and never join (`None`).
    fn join_texts(&mut self, x: &[Token], y: &[Token], widen: bool, pass: bool, span: Span) -> Option<Token> {
        // The token gets the span of the text it replaces, so its gaps are
        // distinguished from those of other joins in the same call.
        let span = x.first().or(y.first()).map_or(span, |t| t.span);
        if self.holds_code(x) || self.holds_code(y) {
            return pass.then(|| self.unknown_text(Unk::Of(&[x, y]), span));
        }
        // Data, typed or not: a one-of token, exact until widened.
        let marked = |t: &[Token]| {
            t.iter().any(|t| t.cs().is_some() && self.typed_text_of(*t).is_none() && self.one_of_members(*t).is_none())
        };
        if !widen && !marked(x) && !marked(y) {
            return Some(self.one_of(vec![Rc::from(x), Rc::from(y)], span));
        }
        // Flatten one-of tokens into their texts. The class is the union,
        // and the result may be empty if a member is.
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
            // If every token is typed text that may be empty, the text may be empty.
            if flat.iter().all(|t| self.typed_shape(*t).is_some_and(|(_, one, _)| !one)) {
                texts.push(Vec::new());
            }
            texts.push(flat);
        }
        let texts: Vec<&[Token]> = texts.iter().map(Vec::as_slice).collect();
        Some(self.unknown_text(Unk::Of(&texts), span))
    }

    /// Joins `x` and `y` token by token, replacing places where both spell
    /// numbers ([`Machine::number_text`]) with the digits of their hull. `None`
    /// when they differ in anything else, or no number is typed digits.
    fn join_numbers_in(&mut self, x: &[Token], y: &[Token], widen: bool, span: Span) -> Option<Vec<Token>> {
        let typed = |m: &Self, t: &Token| t.cs().is_some_and(|s| m.is_unknown_digits(s));
        // Length of the number at the start: one typed token, or a run of digits.
        let run = |m: &Self, t: &[Token]| match t.first() {
            Some(f) if typed(m, f) => 1,
            _ => t.iter().take_while(|t| matches!(t.tok, Tok::Chr('0'..='9', Catcode::Other))).count(),
        };
        let (mut out, mut i, mut j, mut any) = (Vec::new(), 0, 0, false);
        while i < x.len() || j < y.len() {
            if let (Some(a), Some(b)) = (x.get(i), y.get(j))
                && a.tok == b.tok
            {
                out.push(*a);
                (i, j) = (i + 1, j + 1);
                continue;
            }
            let (m, n) = (run(self, &x[i..]), run(self, &y[j..]));
            let (p, q) = (&x[i..i + m], &y[j..j + n]);
            let (a, b) = (self.number_text(p)?, self.number_text(q)?);
            // When widening, a hull that grew jumps to the end of its range.
            let hull = if widen { a.widen(&a.join(&b)) } else { a.join(&b) };
            any |= typed(self, &p[0]) || typed(self, &q[0]);
            // If a typed side already equals the hull, reuse it so the join settles.
            match [(p, a), (q, b)].into_iter().find(|&(t, n)| n == hull && typed(self, &t[0])) {
                Some((t, _)) => out.push(t[0]),
                None => out.extend(self.digits_in(hull, span)),
            }
            (i, j) = (i + m, j + n);
        }
        any.then_some(out)
    }

    /// Whether `text` holds code still to run: a control sequence or an active
    /// character, except typed data or a one-of token of data texts.
    fn holds_code(&self, text: &[Token]) -> bool {
        let code = |t: &Token| match t.tok {
            Tok::Chr(_, Catcode::Active) => true,
            Tok::Cs(_) => self.typed_text_of(*t).is_none(),
            _ => false,
        };
        // One level is enough, since `one_of` keeps its members flat.
        text.iter().any(|t| code(t) && self.one_of_members(*t).is_none_or(|ms| ms.iter().any(|m| m.iter().any(code))))
    }

    /// Widens a text that grows at its end (an `\\aftergroup` per pass, an
    /// append) to the common part followed by "any number of repetitions" of
    /// what one side adds ([`Machine::repeat_token`]). An existing repetition
    /// absorbs one more. Any other difference becomes unknown text.
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
    /// ([`crate::engine::env::Typed::one_of_class`]): the mark, the class, `+` when no
    /// text is empty.
    fn class_name<'t>(&mut self, texts: impl Iterator<Item = &'t [Token]>) -> String {
        let syms = self.typed_syms();
        let typed = crate::engine::env::typed!(self, syms);
        let texts: Vec<&[Token]> = texts.collect();
        let (mut class, mut one) = (crate::engine::tex::TextClass::EMPTY, true);
        for text in &texts {
            one &= typed.holds_one(text);
            for t in *text {
                class =
                    class.union(typed.class_of(t).map_or_else(|| crate::engine::tex::TextClass::of(t.tok), |c| c.0));
            }
        }
        let mut name = format!("{}{}{}", crate::engine::env::CLASS_MARK, class.spell(), if one { '+' } else { '_' });
        // For lists of names, the name also lists which names they may hold.
        for n in typed.names_of(&texts, self.env.sets.text_names).map(|(n, _)| n).unwrap_or_default() {
            name.push(UNKNOWN_NAME_GAP);
            name.push_str(&n);
        }
        name
    }

    /// A token for any number of repetitions of `text` (`text*`): either the
    /// empty text, or `text` followed by the token itself. When executed it
    /// splits like any [`Machine::one_of`] token, and the resulting loop
    /// converges by fixpoint.
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

    /// The number a text spells: digits, or one typed-digits token with an
    /// interval. `None` for any other text.
    fn number_text(&self, text: &[Token]) -> Option<Num> {
        // Use the interval typed digits were made with: it covers every path's
        // narrowing of them. A slice may start with 0.
        if let [t] = text
            && let Some(name) = t.cs().and_then(|sym| self.name(sym).strip_prefix(crate::engine::scan::TYPED_DIGITS))
        {
            let (lo, hi) = name.split_once('#')?.0.split_once("..")?;
            return Some(Num::range(lo.parse().ok()?, hi.parse().ok()?));
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
        // Only as the number prints: `00` does not spell 0.
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

    /// At `-vvvv`, prints the sizes of the main tables at each new 256 MiB high.
    fn report_memory(&mut self, paths: &Paths) {
        if self.cfg.verbose < 4 {
            return;
        }
        let mb = crate::analysis::budget::in_use() >> 28;
        if mb <= self.out.memory_reported {
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
        crate::util::progress::note!(
            "satex: memory {} MiB at {:?}: paths {:?} | env trail/journal/recent/forks {:?} | defs {} exps {} occs {} diags {} | graph {} {} | names {} | calls {} {} | sites {} {} {} diag_seen {} | undo {} copies {} written {} reads {} keys {} | summaries {} {} gaps {}",
            crate::analysis::budget::in_use() >> 20,
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

    /// At `-vvvv`, prints where the waiting paths are, most crowded places
    /// first, to show why they do not join.
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
        // The openers of groups that some paths are in but the shallowest is not.
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
            crate::util::progress::note!(
                "satex: groups open on some paths only, by opener: {:?}",
                &openers[..openers.len().min(SHOWN)]
            );
        }
        let mut places: Vec<(String, Vec<usize>)> = places.into_iter().collect();
        places.sort_by_key(|p| std::cmp::Reverse(p.1.len()));
        crate::util::progress::note!("satex: {} paths waiting, at {} places:", paths.parked.len(), places.len());
        for (at, which) in places.into_iter().take(SHOWN) {
            // What keeps the first two paths apart.
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
            crate::util::progress::note!("  {}x {at}{apart}", which.len());
        }
    }

    fn describe_place(&self, place: &Place) -> String {
        /// How many tokens of a list a report shows.
        const TOKENS: usize = 8;
        match place {
            Place::File(file, offset, ..) => format!("{}@{offset}", self.out.short_name(*file)),
            Place::Tokens(toks, pos) => {
                format!(
                    "[{}]",
                    crate::engine::tex::detokenize(&toks[*pos..toks.len().min(pos + TOKENS)], &self.out.interner)
                )
            }
            Place::Returned(toks) => {
                format!("back[{}]", crate::engine::tex::detokenize(&toks[..toks.len().min(TOKENS)], &self.out.interner))
            }
            Place::Pseudo(id, _, at) => format!("scantokens {}:{}:{}@{at}", id.file, id.line, id.col),
            Place::Boundary => "boundary".into(),
        }
    }

    /// Continues as `config`.
    fn resume_config(&mut self, fork: &Fork, config: Config) {
        self.restore_path(config.state);
        self.env.resume(fork, &config.env);
    }

    /// [`Machine::join_configs`] for passes of one loop or recursion. After
    /// such a join, an earlier one-of token mixes several passes' choices, so
    /// later splits must no longer narrow it ([`Machine::split_one_of_groups`]).
    fn join_passes(
        &mut self,
        fork: &Fork,
        a: &Config,
        b: &Config,
        widen: Option<Reach>,
        paired: bool,
    ) -> (Config, Growth) {
        self.carried.passes_joined = self.carried.passes_joined.wrapping_add(1);
        self.join_configs(fork, a, b, widen, paired)
    }

    /// The least upper bound of two paths at one place, and what it adds over
    /// `a`. The bindings stay relative to `fork`.
    fn join_configs(
        &mut self,
        fork: &Fork,
        a: &Config,
        b: &Config,
        reach: Option<Reach>,
        paired: bool,
    ) -> (Config, Growth) {
        let widen = reach.is_some();
        let span = self.position_span();
        self.out.heat(|h| &mut h.joins, span);
        // Many joins can run in one step, so check `limits.memory` here too.
        if crate::analysis::budget::over() {
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
                    // Paths that differ in this code never join ([`same_code`]),
                    // so only passes of one loop get here.
                    false => self.join_texts(x, y, false, true, span).into_iter().collect(),
                })
            })
            .collect();
        let syms = self.typed_syms();
        let typed = crate::engine::env::typed!(self, syms);
        self.out.joined_slots += (a.env.len() + b.env.len()) as u64;
        let (env, slots, waiting) = self.env.join_paths(fork, &a.env, &b.env, &typed, (reach, paired), &after);
        self.mint();
        // When widening, a name the loop builds from a number (`\flag test 12`,
        // `\csname f\the\i`) belongs to a family that may be defined ([`Wild`]).
        // Its members stay defined, and one more member is no growth. Without
        // this, a loop that defines one more such name per pass never converges.
        let slots = match widen {
            false => slots,
            true => slots
                .into_iter()
                .filter(|&sym| {
                    let name = self.name(sym).to_string();
                    if name.contains(crate::analysis::machine::NAME_NUMBER) {
                        return false;
                    }
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
        // Keep the longer dependency list whole, with the conditionals that
        // pushed it, so their `\fi`s pop it in step.
        if y.cds.len() > x.cds.len() {
            (x.cds, x.conds) = (y.cds.clone(), y.conds.clone());
        }
        if x.parted_by != y.parted_by {
            x.parted_by = None;
        }
        x.material |= y.material;
        x.approximated |= y.approximated;
        x.blind |= y.blind;
        x.ran_off = x.ran_off.or(y.ran_off);
        // The join keeps a pending error only if every joined path has one.
        // A path without one is a feasible run that covers the other's recovery.
        x.dropping = match (x.dropping.0.take(), &y.dropping) {
            (Some(e), (Some(_), p, b, n)) => (Some(e), x.dropping.1 || *p, x.dropping.2 || *b, x.dropping.3 || *n),
            _ => (None, false, false, false),
        };
        for (p, q) in x.scan_starts.iter_mut().zip(&y.scan_starts) {
            p.1 |= q.1;
        }
        // For each loop, keep the larger bound its last pass started with, so a
        // pass counts as shrinking only if it is below both. Use an index: every
        // loop keeps its entry (thousands in a thesis), and a linear search per
        // entry made each join quadratic.
        if y.loop_bounds != x.loop_bounds {
            let bounds = Rc::make_mut(&mut x.loop_bounds);
            let mut at: crate::fixed::LookupMap<_, usize> =
                bounds.iter().enumerate().map(|(i, (l, _))| (*l, i)).collect();
            for &(id, b) in y.loop_bounds.iter() {
                match at.get(&id) {
                    Some(&i) => bounds[i].1 = bounds[i].1.max(b),
                    None => {
                        at.insert(id, bounds.len());
                        bounds.push((id, b));
                    }
                }
            }
        }
        x.cut_taint = match (x.cut_taint, y.cut_taint) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        x.page_touched |= y.page_touched;
        x.page_grew |= y.page_grew;
        // The files may hold what either path wrote (tex.web § 1370).
        x.outputs = super::join_outputs(&x.outputs, &y.outputs, usize::from(self.limits.meaning_set));
        x.pending = super::join_pending(&x.pending, &y.pending, usize::from(self.limits.meaning_set));
        // A word still being set on one path and finished on the other: settle
        // the pending characters on both, so the sizes join instead of widening.
        let (p, q) = match (x.natural, y.natural) {
            (Some(p), Some(q)) if p.pending != q.pending => {
                (Some(self.settled_for_join(p)), Some(self.settled_for_join(q)))
            }
            pair => pair,
        };
        x.natural = crate::engine::mode::join_natural(p, q);
        // A pass that changed the list's length may run any number of times.
        if widen && let (Some(p), Some(q), Some(n)) = (p, q, x.natural.as_mut()) {
            if p.along != q.along {
                n.along = crate::engine::mode::Dim::ANY;
            }
            if p.para.along != q.para.along {
                n.para.along = crate::engine::mode::Dim::ANY;
            }
        }
        x.main_tail = x.main_tail.merge(y.main_tail);
        x.main_depth =
            crate::engine::mode::Dim::range(x.main_depth.lo.min(y.main_depth.lo), x.main_depth.hi.max(y.main_depth.hi));
        // The length of the list a pass built is state like a binding. A join
        // that grew it (a page with one more rule) runs another pass, so the
        // skipped passes are covered by the widened one.
        let length = |n: Option<crate::engine::mode::Natural>| n.map(|n| (n.along, n.para.along));
        if length(x.natural) != length(a.state.natural) {
            growth.parts.push("list");
        }
        // A new family of names defined by a pass is growth, since the join's
        // bindings do not contain it.
        if x.wild.lacks(&y.wild) {
            growth.parts.push("names");
        }
        x.wild.union(&y.wild);
        // Join what the scans in progress produced: the common beginning and
        // end, with one token for either middle.
        for (p, q) in x.scans.iter_mut().zip(&y.scans) {
            p.paired.retain(|l| q.paired.contains(l));
            if let Some(joined) = self.join_accum(p, q, widen, span) {
                *p = joined;
                growth.parts.push("scan");
            }
        }
        // Category codes (tex.web § 232), the line end (§ 240), a waiting token
        // (§ 1269) and `\openin` streams (§ 480) that differ become unknown.
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
        let streams = join_streams(&x.streams, &y.streams, widen, usize::from(self.limits.value_set));
        if !same_streams(&x.streams, &streams) {
            growth.parts.push("streams");
        }
        x.streams = streams;
        // A waiting `\afterassignment` token (tex.web § 1269) joins like the
        // `\aftergroup` ones: one token for either, or none.
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
    /// Joins what two paths produced in one scan. `None` when the texts are
    /// equal. When widening, or when the differing middle is not brace balanced
    /// (one token cannot stand for a `}`), the middle becomes unknown text. An
    /// unknown token already there absorbs what follows, so the join settles.
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
        // Two typed-digit numbers count as alike, and their hull replaces them.
        let typed =
            |m: &Self, t: &Token| t.cs().filter(|&s| m.typed_slice(s).is_none()).and_then(|s| m.typed_digits_of(s));
        let alike = |m: &Self, x: &Token, y: &Token| x.tok == y.tok || (typed(m, x).is_some() && typed(m, y).is_some());
        let head = p.iter().zip(&q).take_while(|(x, y)| alike(self, x, y)).count();
        let tail = p[head..].iter().rev().zip(q[head..].iter().rev()).take_while(|(x, y)| alike(self, x, y)).count();
        let (orig, mut p, q) = (p.clone(), p, q);
        let aligned: Vec<usize> = (0..head).chain(p.len() - tail..p.len()).collect();
        for i in aligned {
            let j = if i < head { i } else { q.len() - (p.len() - i) };
            // When widening, a hull that grew jumps to the end of its range.
            if p[i].tok != q[j].tok
                && let (Some(x), Some(y)) = (typed(self, &p[i]), typed(self, &q[j]))
                && let hull = if widen { x.widen(&x.join(&y)) } else { x.join(&y) }
                && hull != x
                && let [t] = self.digits_in(hull, p[i].span)[..]
            {
                p[i] = t;
            }
        }
        // When widening, unknown and typed text next to the change is included
        // in it, so repeated growth is absorbed by one token.
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
        } else if widen && mp.iter().chain(mq).any(|t| t.tok == unknown) {
            text.push(self.unknown_text(Unk::Any, span));
        } else if !balanced(mp) || !balanced(mq) {
            // One token cannot stand for a brace. It covers the characters of
            // both sides, with braces counted as other characters.
            let brace = |t: &&Token| matches!(t.tok, Tok::Chr(_, Catcode::Begin | Catcode::End));
            let bare = |t: &[Token]| -> Vec<Token> { t.iter().filter(|t| !brace(t)).copied().collect() };
            let braces = mp.iter().chain(mq).any(|t| brace(&t));
            let token = match self.cover(&[&bare(mp), &bare(mq)]) {
                Some((class, one, _, cs)) => {
                    let class = if braces { class.union(TextClass::OTHER) } else { class };
                    self.unknown_text(Unk::Class(class, one, None, &cs), span)
                }
                None => self.unknown_text(Unk::Any, span),
            };
            text.push(token);
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

/// Joins two [`Unit::Number`] results: the value if equal, else their hull,
/// else unknown.
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

/// Joins two names being built. `None` when they are equal. Otherwise the
/// set of names either built, up to `meaning_set` of them, and beyond that
/// one name with an unknown part ([`join_name_parts`]).
fn join_names(a: &Accum, b: &Accum, limit: usize) -> Option<Accum> {
    let mut n = a.name.as_deref().cloned().unwrap_or_default();
    let other = b.name.as_deref().cloned().unwrap_or_default();
    let mut all = n.names.clone();
    let mut grew = false;
    for (name, took) in other.names.into_iter().zip(other.took) {
        match all.iter().position(|x| *x == name) {
            Some(i) => {
                let joined = super::join_took(&n.took[i], &took);
                grew |= joined.len() != n.took[i].len();
                n.took[i] = joined;
            }
            None => {
                all.push(name);
                n.took.push(took);
                grew = true;
            }
        }
    }
    if !grew && n.files == other.files && n.closed == other.closed && n.gap.contains(other.gap) {
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
    n.gap = n.gap.union(other.gap);
    if n.number != other.number {
        n.number = None;
    }
    n.names = all;
    if n.names.len() > limit {
        n.collapse();
    }
    Some(Accum { name: Some(Box::new(n)), ..a.with(Vec::new()) })
}

/// Joins two names, each `(prefix, name)` (the prefix is set once an unknown
/// part was met), into the common beginning, an unknown part, and the common
/// end after it.
pub(crate) fn join_name_parts(x: (&Option<String>, &String), y: (&Option<String>, &String)) -> (String, String) {
    let (px, sx) = (x.0.as_deref().unwrap_or(x.1), x.1.as_str());
    let (py, sy) = (y.0.as_deref().unwrap_or(y.1), y.1.as_str());
    let prefix: String = px.chars().zip(py.chars()).take_while(|(c, d)| c == d).map(|(c, _)| c).collect();
    // Compare the ends only after the shared beginning, so the two do not overlap.
    let rest = |whole: bool, s: &str| if whole { s[prefix.len().min(s.len())..].to_string() } else { s.to_string() };
    let (sx, sy) = (rest(x.0.is_none(), sx), rest(y.0.is_none(), sy));
    let mut suffix: Vec<char> =
        sx.chars().rev().zip(sy.chars().rev()).take_while(|(c, d)| c == d).map(|(c, _)| c).collect();
    suffix.reverse();
    (prefix, suffix.into_iter().collect())
}

/// Whether two parked paths are alike except for the text they still read
/// before reaching their common input: same source place, same conditionals
/// and groups, same code waiting.
fn alike(a: &Config, b: &Config) -> bool {
    same_code(&a.tag, &b.tag) && !a.ended && !b.ended && a.key.progress == b.key.progress && same_stacks(a, b)
}

/// Whether two paths are in the same conditionals, groups and scans.
fn same_stacks(a: &Config, b: &Config) -> bool {
    let (x, y) = (&a.key, &b.key);
    x.conds == y.conds && x.groups == y.groups && x.scans == y.scans
}

/// The text the input levels hold, in reading order. `None` when one of
/// them is a file, which is not a token list.
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

/// Which of two input stacks (bottom first) is further back. At the first
/// level where they differ: the earlier position in the same file or list, or
/// the longer remaining text of two lists. If one is a prefix of the other,
/// the deeper one is further back, since it has more to read first.
fn place_order(a: &[Place], b: &[Place]) -> std::cmp::Ordering {
    // One key per level, compared in order, giving the total order `sort_by`
    // needs. Text further back (more left to read) comes first.
    let key = |p: &Place| match p {
        Place::File(f, i, ..) => (0, *f as usize, (*i as u64, 0)),
        Place::Pseudo(at, _, i) => (1, at.file as usize, (u64::from(at.line) << 32 | u64::from(at.col), *i)),
        Place::Tokens(t, i) => (2, usize::MAX - (t.len() - i), (0, 0)),
        Place::Returned(t) => (2, usize::MAX - t.len(), (0, 0)),
        Place::Boundary => (3, 0, (0, 0)),
    };
    let n = a.len().min(b.len());
    a[..n].iter().map(key).cmp(b[..n].iter().map(key)).then_with(|| b.len().cmp(&a.len()))
}

/// The componentwise minimum of `at` and the stamp of `frame`, if it is a token list.
pub(super) fn lower(at: Option<(usize, u64, u64)>, frame: Option<&Frame>) -> Option<(usize, u64, u64)> {
    let Some(Frame::Tokens { at: b, .. }) = frame else { return at };
    Some(at.map_or(*b, |a| (a.0.min(b.0), a.1.min(b.1), a.2.min(b.2))))
}

/// The stamp for a list joined from `frames`: the lowest of theirs, or `now`
/// for put-back text (dropping it loses no earlier writes).
fn stamp(frames: &[&Frame], now: (usize, u64, u64)) -> (usize, u64, u64) {
    frames.iter().fold(None, |at, f| lower(at, Some(f))).unwrap_or(now)
}

/// The input levels that still have something to read.
fn live_frames(input: &Input) -> Vec<&Frame> {
    input.iter().filter(|frame| place(frame).is_some()).collect()
}

/// The tokens the token lists of `frames` still have to read.
fn frames_text<'f>(frames: &'f [&Frame]) -> impl Iterator<Item = &'f Token> {
    frames.iter().flat_map(|f| match f {
        Frame::Tokens { toks, pos, .. } => &toks[*pos..],
        Frame::Returned(tokens) => &tokens[..],
        _ => &[],
    })
}

/// The position of one input level, `None` once it is used up.
pub(super) fn place(frame: &Frame) -> Option<Place> {
    match frame {
        Frame::File { mouth, .. } => {
            Some(Place::File(mouth.file, mouth.offset(), mouth.ending(), mouth.source().clone()))
        }
        Frame::Tokens { toks, pos, .. } => (*pos < toks.len()).then(|| Place::Tokens(toks.clone(), *pos)),
        Frame::Pseudo { mouth, at, pos, .. } => Some(Place::Pseudo(*at, mouth.text().clone(), mouth.offset() + pos)),
        Frame::Returned(tokens) => (!tokens.is_empty()).then(|| Place::Returned(tokens.clone())),
        Frame::Boundary => Some(Place::Boundary),
    }
}

/// A 128-bit hash of a text, used to name it (collisions are not expected in
/// practice). Control sequences hash by name, not symbol number, so names and
/// set order do not depend on what the run interned before.
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

/// Splits a name that ends in digits (a loop counter, `l_test_flag12`)
/// before the digits: the prefix, and an empty suffix.
fn digit_run(name: &str) -> Option<(String, String)> {
    // satex's own names (`\count10`'s slot, markers) are not a generated family.
    if name.starts_with('\u{2}') {
        return None;
    }
    let prefix = name.trim_end_matches(|c: char| c.is_ascii_digit());
    (!prefix.is_empty() && prefix.len() < name.len()).then(|| (prefix.to_string(), String::new()))
}

/// Whether two keys are at the same place except for digits in their token
/// lists: same number of levels, same conditionals and groups, and every
/// level equal or the same text once runs of digits are collapsed.
fn alike_places(a: &Key, b: &Key, typed: &crate::engine::env::Typed) -> bool {
    a.progress == b.progress
        && a.conds == b.conds
        && a.groups == b.groups
        && a.scans == b.scans
        && a.places.len() == b.places.len()
        && a.places.iter().zip(&b.places).all(|(x, y)| same_place(x, y, typed))
}

/// `key` without the token lists above the entry's `places` levels that hold
/// only `\fi`, and without the conditionals they close. `None` unless they
/// close every conditional opened after the entry's `conds`.
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

/// `input` without the `\fi`-only token lists above `places` live levels.
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

/// The text a token-list level still has to read.
fn place_text(p: &Place) -> Option<&[Token]> {
    match p {
        Place::Tokens(t, i) => Some(&t[*i..]),
        Place::Returned(t) => Some(&t[..]),
        _ => None,
    }
}

/// Whether `x` and `y` read the same text, ignoring unknown digits.
fn same_place(x: &Place, y: &Place, typed: &crate::engine::env::Typed) -> bool {
    let text = |p: &Place| match p {
        Place::Tokens(toks, pos) => Some(crate::engine::env::unknown_runs(&toks[*pos..], typed)),
        Place::Returned(toks) => Some(crate::engine::env::unknown_runs(toks, typed)),
        _ => None,
    };
    x == y || matches!((text(x), text(y)), (Some(p), Some(q)) if same_text(&p, &q))
}

/// Whether `key` has moved past a frame below the head `head`: at the first
/// level where they differ, it is further on in the same file or token list.
/// Only that is final. A frame popped and pushed again (a tail call,
/// `\unspace`'s loop) may bring the head back, so it stays.
fn left(head: &Key, key: &Key, typed: &crate::engine::env::Typed) -> bool {
    let below = &head.places[..head.places.len().saturating_sub(1)];
    let Some((x, y)) = below.iter().zip(&key.places).find(|(x, y)| !same_place(x, y, typed)) else {
        return false;
    };
    match (x, y) {
        (Place::File(f, i, ..), Place::File(g, j, ..)) => f == g && j > i,
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

/// Keeps only the newest `n` entries of a drive's loop table ([`crate::project::config::Limits::loop_heads`]).
fn keep_last<T>(table: &mut Vec<T>, n: u16) {
    let excess = table.len().saturating_sub(usize::from(n.max(1)));
    table.drain(..excess);
}

/// Whether two token lists are the same text.
pub(crate) fn same_text(a: &[Token], b: &[Token]) -> bool {
    a.iter().map(|t| t.tok).eq(b.iter().map(|t| t.tok))
}

/// The prefix of the names [`Machine::one_of`] makes.
pub(crate) const ONE_OF: &str = "one of:";

/// The meaning a one-of token has for one of its texts: a parameterless
/// macro ([`Machine::one_of`]).
pub(crate) fn text_meaning(text: Rc<[Token]>) -> Meaning {
    Meaning::Macro(Rc::new(MacroDef {
        parameter_text: Default::default(),
        arg_spec: None,
        replacement_text: text,
        long: false,
        outer: false,
        protected: false,
    }))
}

/// Whether two paths will run the same code at their group ends and after
/// the next assignment. If not, they are at different points.
fn same_code(a: &Tag, b: &Tag) -> bool {
    (&a.after, a.after_assignment) == (&b.after, b.after_assignment)
}

/// The parked path that `config` joins at its place: one with the same tag,
/// or one that will run the same code and either closed the same conditional
/// last (both arms of an `\if…` meet after its `\fi`), or is the first there
/// once [`PARTITIONS`] tags are at the place or the paths are between two lines
/// of a file (the statement is over). Paths that took different arms of a
/// digit split join only once as many paths wait as the split made (`apart`),
/// or the statement is over. Joined earlier, a 1-digit arm would pair with a
/// 2-digit arm that is a pass behind.
fn partner(parked: &[Config], config: &Config, apart: &dyn Fn(&Config, &Config) -> usize) -> Option<usize> {
    let (key, tag) = (&config.key, &config.tag);
    let here: Vec<usize> = (0..parked.len()).filter(|&i| !parked[i].ended && parked[i].key == *key).collect();
    let fi = |i: usize| {
        config.state.fi_mark.is_some()
            && parked[i].state.fi_mark == config.state.fi_mark
            && (config.state.fi_mark.is_none_or(|m| !m.2) || parked[i].state.mode == config.state.mode)
    };
    // Arms of one mode split join once they are in the same mode (see `Machine::mode_split`).
    let arms = |i: usize| {
        config.state.mode_split.is_some()
            && parked[i].state.mode_split == config.state.mode_split
            && parked[i].state.mode == config.state.mode
    };
    let file = key.places.iter().all(|p| matches!(p, Place::File(..) | Place::Pseudo(..)));
    let merge = here.len() >= PARTITIONS || file;
    // Arms of a digit split wait for as many paths as the split made.
    let own = |i: usize| file || here.len() >= apart(&parked[i], config);
    here.iter().copied().find(|&i| parked[i].tag == *tag && own(i)).or_else(|| {
        here.iter().copied().find(|&i| same_code(&parked[i].tag, tag) && own(i) && (merge || fi(i) || arms(i)))
    })
}

/// The arms for the next run of a command: the last choice with an arm left
/// advances to it, and the choices after it start over. With `absent`, the
/// last choice's arm did not exist, nor does any later one.
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

/// Whether two paths differ only in data: satex's internal names (`slot`: a
/// register's or a code table's entry) and macros whose texts are numbers.
/// Any other difference is code the paths will run: a macro's text, a
/// meaning given to a name, or a name left undefined or unknown.
fn data_apart(a: &Config, b: &Config, typed: &crate::engine::env::Typed, slot: &dyn Fn(Sym) -> bool) -> bool {
    let number = |t: &Token| match t.tok {
        Tok::Chr(c, Catcode::Other) => {
            c.is_ascii_digit() || "+-.,".contains(c) || crate::engine::tex::is_unknown_digit(c)
        }
        _ => typed.classify(t).is_some_and(|(c, _)| TextClass::NUMBER.contains(c)),
    };
    let code = |sym: Sym, binding: Option<&crate::engine::env::Binding>| match binding.map(|b| &b.meaning) {
        None | Some(Meaning::Register(..)) => false,
        Some(Meaning::Macro(d)) => !(d.parameter_text.items.is_empty() && d.replacement_text.iter().all(number)),
        Some(_) => !slot(sym),
    };
    a.env.differing(&b.env).into_iter().all(|sym| {
        let (x, y) = (a.env.changed(sym), b.env.changed(sym));
        x.zip(y).is_some_and(|(x, y)| x.meaning == y.meaning) || !code(sym, x) && !code(sym, y)
    })
}

/// Where two stacks differ, if every frame above that point is the same kind
/// (ignoring whether it stands for many). Returns the kind and the index where
/// "any number of it" starts, one lower if the last shared frame already
/// stands for many. `None` when the stacks are equal or differ otherwise.
fn many_from<T: PartialEq + Copy>(a: &[(T, bool)], b: &[(T, bool)]) -> Option<(usize, T)> {
    let n = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let t = a.get(n).or(b.get(n))?.0;
    if !a[n..].iter().chain(&b[n..]).all(|f| f.0 == t) {
        return None;
    }
    Some((if n > 0 && a[n - 1] == (t, true) { n - 1 } else { n }, t))
}

/// What keeps the waiting paths from joining. `SATEX_WHY_APART` prints it,
/// and dev mode (`trace_state`) shows it per trace step.
fn why_apart(paths: &Paths, at: Span, out: &Analysis) -> String {
    let s = |s: Span| format!("{}:{}", out.short_name(s.file), s.line);
    let p = &paths.parked;
    let (mut progress, mut stacks, mut tags, mut ended) = (0, 0, 0, 0);
    let (mut conds, mut groups, mut scans, mut first) = (0, 0, 0, None::<String>);
    let mut sample = None::<String>;
    let pairs = p.len() * p.len().saturating_sub(1) / 2;
    for (i, a) in p.iter().enumerate() {
        for b in &p[i + 1..] {
            if a.ended || b.ended {
                ended += 1;
            } else if a.key.progress != b.key.progress {
                progress += 1;
            } else if !same_stacks(a, b) {
                stacks += 1;
                let (x, y) = (&a.key, &b.key);
                if x.conds != y.conds {
                    conds += 1;
                }
                if x.groups != y.groups {
                    groups += 1;
                }
                if x.scans != y.scans {
                    scans += 1;
                }
                if first.is_none() {
                    first = Some(format!(
                        "conds {:?} vs {:?} | groups {:?} vs {:?}",
                        x.conds.iter().map(|c| s(c.0.1)).collect::<Vec<_>>(),
                        y.conds.iter().map(|c| s(c.0.1)).collect::<Vec<_>>(),
                        x.groups,
                        y.groups
                    ));
                }
            } else if !same_code(&a.tag, &b.tag) {
                tags += 1;
            } else if sample.is_none() {
                let text = |c: &Config| {
                    let frames = live_frames(&c.state.input);
                    match pending(&frames) {
                        Some(t) => crate::engine::tex::detokenize(&t[..t.len().min(24)], &out.interner),
                        None => format!("<{} frames, a file below>", frames.len()),
                    }
                };
                sample = Some(format!("A: {} || B: {}", text(a), text(b)));
            }
        }
    }
    let at_progress: std::collections::BTreeSet<u64> = p.iter().map(|c| c.key.progress).collect();
    format!(
        "why-apart {}: {} waiting, {pairs} pairs: progress {progress} ({} places), stacks {stacks}, tags {tags}, ended {ended}, rest (alike) {}",
        s(at),
        p.len(),
        at_progress.len(),
        pairs - progress - stacks - tags - ended
    ) + &format!(
        "\nwhy-apart   alike pair input: {}\nwhy-apart   stacks differ in: conds {conds}, groups {groups}, scans {scans}, e.g. {}",
        sample.unwrap_or_default(),
        first.unwrap_or_default()
    )
}
