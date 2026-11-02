//! What a command's arguments are: a label it defines, a key it references,
//! a file it reads, and the [`Ty`] TeX's scanners read it as. Completion and
//! signatures ask this.
//!
//! Three sources, in this order:
//! - the run's evidence: the calls that made an occurrence or a load,
//!   matched to the braced argument that held its key or name, and the
//!   tokens a scanner read, matched to the braced argument they stand in;
//! - the flow through the project's macro bodies: an argument handed on,
//!   behind a literal prefix or not, to an argument with a role has it too;
//! - a seed of the kernel's and common packages' conventions, for commands
//!   the project does not redefine.

use crate::builtins::{LoadKind, OccKind};
use crate::machine::Analysis;
use crate::tex::{Catcode, MacroDef, ParamItem, RegKind, Sym, Tok, Token};
use std::rc::Rc;

/// What one argument is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoleKind {
    Occ(OccKind),
    Load(LoadKind),
    Type(Ty),
}

impl RoleKind {
    /// The type the role gives its argument: a key is a key, what a tool
    /// or load reads as a file is a file of the kind it reads.
    pub fn ty(self) -> Ty {
        use FileKind as F;
        match self {
            RoleKind::Occ(OccKind::Bibliography) => Ty::File(F::Bib),
            RoleKind::Occ(OccKind::BibStyle) => Ty::File(F::Bst),
            RoleKind::Occ(OccKind::Graphics) => Ty::File(F::Graphic),
            RoleKind::Occ(OccKind::Ref | OccKind::Label) => Ty::Key(KeyKind::Label),
            RoleKind::Occ(OccKind::Cite | OccKind::BibItem) => Ty::Key(KeyKind::Cite),
            RoleKind::Occ(OccKind::BeginEnvironment | OccKind::EndEnvironment) => Ty::Key(KeyKind::Environment),
            RoleKind::Occ(OccKind::Key | OccKind::KeyUse) => Ty::Key(KeyKind::Option),
            // What a tool's input line carries says nothing of the argument's kind.
            RoleKind::Occ(OccKind::Entry | OccKind::Write) => Ty::Bottom,
            RoleKind::Occ(_) => Ty::Key(KeyKind::Any),
            RoleKind::Load(LoadKind::Class) => Ty::File(F::Cls),
            RoleKind::Load(LoadKind::Package) => Ty::File(F::Sty),
            RoleKind::Load(LoadKind::Input | LoadKind::Include) => Ty::File(F::Tex),
            RoleKind::Load(_) => Ty::File(F::Any),
            RoleKind::Type(t) => t,
        }
    }
}

/// What a key names. The kind is what conventions and lints match on:
/// a label's prefix, a citation's database entry, a key list's option.
/// `Any` is a key of no known kind, the join of two kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyKind {
    Label,
    Cite,
    Option,
    /// A key pgfkeys stores under its family's path (`/tikz/thick`), below `Option`.
    Pgf,
    Environment,
    Any,
}

/// What an expl3 variable holds, by the module of the function that made or
/// set it (`\tl_set:Nn`, `\seq_new:N`; interface3, "Naming functions and
/// variables"). Register-backed kinds (`int`, `dim`, `skip`, `muskip`,
/// `box`, `ior`/`iow`) are registers of their kind instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VarKind {
    Tl,
    Str,
    Clist,
    Seq,
    Prop,
    Fp,
    Bool,
    Regex,
    Coffin,
    Flag,
    Intarray,
    Cctab,
}

impl VarKind {
    pub const ALL: [VarKind; 12] = [
        VarKind::Tl,
        VarKind::Str,
        VarKind::Clist,
        VarKind::Seq,
        VarKind::Prop,
        VarKind::Fp,
        VarKind::Bool,
        VarKind::Regex,
        VarKind::Coffin,
        VarKind::Flag,
        VarKind::Intarray,
        VarKind::Cctab,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            VarKind::Tl => "tl",
            VarKind::Str => "str",
            VarKind::Clist => "clist",
            VarKind::Seq => "seq",
            VarKind::Prop => "prop",
            VarKind::Fp => "fp",
            VarKind::Bool => "bool",
            VarKind::Regex => "regex",
            VarKind::Coffin => "coffin",
            VarKind::Flag => "flag",
            VarKind::Intarray => "intarray",
            VarKind::Cctab => "cctab",
        }
    }

    /// The kind whose variable an expl3 function takes as its first
    /// argument: `\tl_set:Nn`, `\seq_gput_right:cn`. None for another
    /// module or a function of no variable (`\tl_if_empty:nTF`).
    pub fn of_function(name: &str) -> Option<VarKind> {
        let (base, sig) = name.trim_start_matches('\\').split_once(':')?;
        let module = base.split('_').next()?;
        if !sig.starts_with(['N', 'c']) {
            return None;
        }
        VarKind::ALL.into_iter().find(|k| k.as_str() == module)
    }
}

/// What a file argument is read as: the extension its program adds and the
/// installation tree (kpathsea's `TEXINPUTS`, `BIBINPUTS`, `BSTINPUTS`) it
/// searches. `Any` is a file of no known kind, the join of two kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FileKind {
    Tex,
    Sty,
    Cls,
    Bib,
    Bst,
    Bbx,
    Cbx,
    /// A figure: the extensions come from the run's own graphics.
    Graphic,
    Any,
}

/// Each kind's extension, label (as signatures and hovers say it) and the
/// installation's tree of such files, empty where none: the run decides a
/// graphic's extension, and the `.tex` files of the tree are documentation.
const FILE_KINDS: [(FileKind, &str, &str, &str); 9] = [
    (FileKind::Tex, "tex", "TeX file", ""),
    (FileKind::Sty, "sty", "package", "tex"),
    (FileKind::Cls, "cls", "class", "tex"),
    (FileKind::Bib, "bib", "bibliography database", "bibtex/bib"),
    (FileKind::Bst, "bst", "bibliography style", "bibtex/bst"),
    (FileKind::Bbx, "bbx", "biblatex bibliography style", "tex"),
    (FileKind::Cbx, "cbx", "biblatex citation style", "tex"),
    (FileKind::Graphic, "", "graphic file", ""),
    (FileKind::Any, "", "file", ""),
];

impl FileKind {
    fn row(self) -> (&'static str, &'static str, &'static str) {
        FILE_KINDS.iter().find(|r| r.0 == self).map_or(("", "file", ""), |r| (r.1, r.2, r.3))
    }

    pub fn ext(self) -> Option<&'static str> {
        Some(self.row().0).filter(|e| !e.is_empty())
    }

    pub fn label(self) -> &'static str {
        self.row().1
    }

    pub fn tree(self) -> Option<&'static str> {
        Some(self.row().2).filter(|t| !t.is_empty())
    }

    /// The kind whose extension is `ext`.
    pub fn of_ext(ext: &str) -> Option<FileKind> {
        FILE_KINDS.iter().find(|r| !ext.is_empty() && r.1 == ext).map(|r| r.0)
    }
}

impl std::fmt::Display for RoleKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoleKind::Occ(k) => f.write_str(k.as_str()),
            RoleKind::Load(k) => f.write_str(k.as_str()),
            RoleKind::Type(t) => t.fmt(f),
        }
    }
}

/// What text an argument holds, as TeX reads it. Ordered by the texts each
/// admits, so a join goes up to the nearest common one: a register's name
/// is text of its value's kind, and a dimension is glue without stretch
/// (tex.web §§ 440, 448, 461, 1224).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    Bottom,
    /// ⟨number⟩ (`scan_int`, § 440).
    Number,
    /// ⟨dimen⟩ (`scan_dimen`, § 448).
    Dimen,
    /// ⟨glue⟩ (`scan_glue`, § 461).
    Glue,
    /// ⟨mudimen⟩ or ⟨muglue⟩ (§ 461 with `mu`).
    MuGlue,
    /// Any of the four.
    Quantity,
    /// A register an assignment names (§ 1224).
    Reg(RegKind),
    /// A register of a kind the text does not fix: what a body assigns
    /// `#i` to (`#1#2\relax`).
    Register,
    /// A box register's number (`\box`, `\copy`, `\setbox`, § 1079).
    Box,
    /// The characters of a register's name a `\csname` forms (LaTeX's
    /// `c@⟨counter⟩`).
    Named(RegKind),
    /// A key of a kind the role fixes.
    Key(KeyKind),
    /// A file of a kind the program reading it fixes.
    File(FileKind),
    /// Characters forming a name.
    Name,
    /// A control sequence an assignment defines (`get_r_token`, § 1215).
    Cs,
    /// A conditional `\newif` defines (`\if⟨name⟩`).
    Switch,
    /// An expl3 variable of a kind ([`VarKind`]): a control sequence the
    /// module's functions made or set.
    Var(VarKind),
    /// The key list a loading command takes in its optional argument.
    Options,
    /// A release date a body compares with the one a file provides
    /// (`2020/01/01`, read as a number): the file must be at least that new.
    Date,
    /// A definition's replacement text (tex.web § 473): code.
    Body,
    /// Balanced text of any kind, names and code alike.
    Text,
    Top,
}

impl Ty {
    /// Every type, from `Top` down, children after their parents.
    pub fn all() -> Vec<Ty> {
        use Ty::*;
        let regs = RegKind::ALL
            .iter()
            .copied()
            .filter(|k| !matches!(k, RegKind::Char | RegKind::MathChar | RegKind::UMathChar));
        let keys = [KeyKind::Any, KeyKind::Label, KeyKind::Cite, KeyKind::Option, KeyKind::Pgf, KeyKind::Environment];
        let files = FILE_KINDS.iter().map(|r| r.0).filter(|k| *k != FileKind::Any);
        let mut out = vec![
            Top, Quantity, Number, Dimen, Glue, MuGlue, Box, Register, Cs, Switch, Options, Date, Text, Name, Body,
        ];
        out.extend(VarKind::ALL.map(Var));
        out.extend(regs.clone().map(Reg));
        out.extend(regs.map(Named));
        out.extend(keys.map(Key));
        out.push(File(FileKind::Any));
        out.extend(files.map(File));
        out.push(Bottom);
        out
    }

    /// The next type up.
    /// The family charts and chips colour the type by.
    pub fn family(self) -> &'static str {
        match self {
            Ty::Top | Ty::Bottom => "top",
            Ty::Number | Ty::Dimen | Ty::Glue | Ty::MuGlue | Ty::Quantity | Ty::Box => "quantity",
            Ty::Reg(_) | Ty::Register | Ty::Named(_) => "register",
            Ty::Key(_) | Ty::Options => "key",
            Ty::File(_) => "file",
            Ty::Var(_) => "expl3",
            Ty::Cs | Ty::Switch => "cs",
            _ => "text",
        }
    }

    pub fn parent(self) -> Ty {
        use Ty::*;
        match self {
            Bottom => Bottom,
            Dimen | Reg(RegKind::Skip) => Glue,
            Number | Glue | MuGlue => Quantity,
            Reg(RegKind::Count) | Box => Number,
            Reg(RegKind::Dimen) => Dimen,
            Reg(RegKind::MuSkip) => MuGlue,
            Named(_) | Key(KeyKind::Any) | File(FileKind::Any) => Name,
            Name | Body | Date => Text,
            Key(_) => Key(KeyKind::Any),
            File(_) => File(FileKind::Any),
            Switch
            | Var(
                VarKind::Tl
                | VarKind::Seq
                | VarKind::Prop
                | VarKind::Fp
                | VarKind::Bool
                | VarKind::Regex
                | VarKind::Coffin
                | VarKind::Flag
                | VarKind::Intarray
                | VarKind::Cctab,
            ) => Cs,
            // A string and a comma list are token lists (interface3).
            Var(VarKind::Str | VarKind::Clist) => Var(VarKind::Tl),
            Reg(_) | Register | Quantity | Text | Cs | Options | Top => Top,
        }
    }

    /// The least type above both.
    pub fn join(self, other: Ty) -> Ty {
        let up = |t: Ty| std::iter::successors(Some(t), |t| (*t != Ty::Top).then(|| t.parent()));
        match (self, other) {
            (Ty::Bottom, t) | (t, Ty::Bottom) => t,
            _ => up(other).find(|t| up(self).any(|s| s == *t)).unwrap_or(Ty::Top),
        }
    }

    pub fn leq(self, other: Ty) -> bool {
        self.join(other) == other
    }

    /// The type of a name's own text: a register or parameter is text of
    /// its kind, a `\chardef` constant a number.
    pub fn of_meaning(meaning: &crate::tex::Meaning) -> Option<Ty> {
        use crate::builtins::Primitive as P;
        Some(match meaning {
            crate::tex::Meaning::Register(RegKind::Char | RegKind::MathChar | RegKind::UMathChar, _) => Ty::Number,
            crate::tex::Meaning::Register(k, _) => Ty::Reg(*k),
            m => match m.prim()? {
                P::IntegerParameter => Ty::Reg(RegKind::Count),
                P::DimenParameter => Ty::Reg(RegKind::Dimen),
                P::GlueParameter { mu } => Ty::Reg(if mu { RegKind::MuSkip } else { RegKind::Skip }),
                _ => return None,
            },
        })
    }

    /// What a scanner calls the quantity it reads ([`crate::machine::Machine::probe_scans`]).
    pub fn scanned(what: &str) -> Option<Ty> {
        Some(match what {
            "number" => Ty::Number,
            "dimen" => Ty::Dimen,
            "glue" => Ty::Glue,
            "mudimen" | "muglue" => Ty::MuGlue,
            _ => return None,
        })
    }
}

impl std::fmt::Display for Ty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ty::Reg(k) => write!(f, "{} register", k.as_str()),
            Ty::Named(RegKind::Count) => f.write_str("counter"),
            Ty::Named(k) => write!(f, "{} name", k.as_str()),
            Ty::Var(k) => write!(f, "{} variable", k.as_str()),
            Ty::File(k) if *k != FileKind::Any => match k.ext() {
                Some(ext) => write!(f, "{} (.{ext})", k.label()),
                None => f.write_str(k.label()),
            },
            t => f.write_str(match t {
                Ty::Bottom => "nothing",
                Ty::Number => "number",
                Ty::Dimen => "dimen",
                Ty::Glue => "glue",
                Ty::MuGlue => "muglue",
                Ty::Quantity => "quantity",
                Ty::Box => "box",
                Ty::Register => "register",
                Ty::Key(KeyKind::Label) => "label",
                Ty::Key(KeyKind::Cite) => "citation key",
                Ty::Key(KeyKind::Option) => "option key",
                Ty::Key(KeyKind::Pgf) => "pgf key",
                Ty::Key(KeyKind::Environment) => "environment name",
                Ty::Key(KeyKind::Any) => "key",
                Ty::File(_) => "file",
                Ty::Name => "name",
                Ty::Cs => "control sequence",
                Ty::Switch => "conditional",
                Ty::Var(_) => unreachable!("written above"),
                Ty::Options => "options",
                Ty::Date => "date, at least",
                Ty::Body => "definition body",
                Ty::Text => "text",
                _ => "any",
            }),
        }
    }
}

impl std::fmt::Display for Role {
    /// `#1 ref`, or with the text a wrapper puts before it, `#1 ref after sec:`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{} {}", self.arg + 1, self.kind)?;
        if !self.prefix.is_empty() {
            write!(f, " after {}", self.prefix)?;
        }
        if !self.suffix.is_empty() {
            write!(f, " before {}", self.suffix)?;
        }
        Ok(())
    }
}

/// Argument `arg` (counting braced arguments from 0) of a command is a
/// `kind`; its key is `prefix`, then what the argument holds (an item of
/// it, for a list), then `suffix`: `\usetikzlibrary{calc}` loads
/// `tikzlibrarycalc.code.tex`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Role {
    pub arg: usize,
    pub kind: RoleKind,
    pub prefix: String,
    pub suffix: String,
}

/// How deep the flow follows macros calling macros.
const DEPTH: usize = 6;

/// The roles of `cmd`'s arguments.
pub fn roles(analysis: &Analysis, cmd: &str) -> Vec<Role> {
    roles_typed(analysis, &sources(analysis).typed, cmd)
}

/// The types the run read in `cmd`'s `[…]` arguments, by their place
/// among those, joined per place.
pub fn optional_types(analysis: &Analysis, cmd: &str) -> Vec<(usize, Ty)> {
    let mut out: Vec<(usize, Ty)> = Vec::new();
    for (_, arg, ty, _) in sources(analysis).typed.iter().filter(|(c, .., optional)| c == cmd && *optional) {
        match out.iter_mut().find(|(a, _)| a == arg) {
            Some((_, t)) => *t = t.join(*ty),
            None => out.push((*arg, *ty)),
        }
    }
    out
}

/// The roles of each of `cmds`, the run's typed tokens read once.
pub fn roles_many(analysis: &Analysis, cmds: &[&str]) -> Vec<Vec<Role>> {
    let typed = &sources(analysis).typed;
    cmds.iter().map(|c| roles_typed(analysis, typed, c)).collect()
}

fn roles_typed(analysis: &Analysis, typed: &Typed, cmd: &str) -> Vec<Role> {
    let mut out = roles_at(analysis, typed, cmd, 0);
    // An argument that defines a key is not also a use of it: the run
    // reading its own label back is no reference.
    let defines = |arg: usize, out: &[Role]| {
        out.iter()
            .any(|r| r.arg == arg && matches!(r.kind, RoleKind::Occ(OccKind::Label | OccKind::BibItem | OccKind::Key)))
    };
    let all = out.clone();
    out.retain(|r| !(matches!(r.kind, RoleKind::Occ(OccKind::Ref | OccKind::KeyUse)) && defines(r.arg, &all)));
    // Types an argument was seen to take join to one.
    let mut joined: Vec<Role> = Vec::new();
    for r in out.iter().filter(|r| matches!(r.kind, RoleKind::Type(_))) {
        match joined.iter_mut().find(|j| j.arg == r.arg) {
            Some(j) => j.kind = RoleKind::Type(j.kind.ty().join(r.kind.ty())),
            None => joined.push(r.clone()),
        }
    }
    out.retain(|r| !matches!(r.kind, RoleKind::Type(_)));
    // What the body does with an argument no use typed.
    let body = analysis.interner.lookup(cmd).and_then(|s| analysis.env.meaning(s).as_macro().cloned());
    let (statics, pairs) = body.map(|m| assigned(analysis, &m.replacement_text)).unwrap_or_default();
    // The value of a register a use named is of that register's kind.
    for (reg, value) in pairs {
        let named = joined.iter().find(|j| j.arg == reg).map(|j| j.kind.ty());
        if let Some(Ty::Reg(kind)) = named
            && !joined.iter().any(|j| j.arg == value)
            && Ty::Reg(kind).parent().leq(Ty::Quantity)
        {
            joined.push(Role {
                arg: value,
                kind: RoleKind::Type(Ty::Reg(kind).parent()),
                prefix: String::new(),
                suffix: String::new(),
            });
        }
    }
    for (arg, ty) in statics {
        if !joined.iter().any(|j| j.arg == arg) {
            joined.push(Role { arg, kind: RoleKind::Type(ty), prefix: String::new(), suffix: String::new() });
        }
    }
    out.extend(joined);
    out.sort_by_key(|r| (r.arg, r.prefix.clone(), r.kind.to_string()));
    out.dedup();
    out
}

/// The type of each argument `roles` names: what TeX read it as, else
/// what its other roles make it (a key, a file). Sorted by argument.
pub fn arg_types(roles: &[Role]) -> Vec<(usize, Ty)> {
    // A bare name is refined by what the argument is used as, and a key
    // the program reading it as a file is a file.
    let read = |r: &&Role| {
        let others = |f: &dyn Fn(&Role) -> bool| roles.iter().any(|o| o.arg == r.arg && f(o));
        let file = |o: &Role| !matches!(o.kind, RoleKind::Type(_)) && matches!(o.kind.ty(), Ty::File(_));
        match r.kind {
            RoleKind::Type(t) => t != Ty::Name || !others(&|o| !matches!(o.kind, RoleKind::Type(_))),
            _ => !others(&|o| matches!(o.kind, RoleKind::Type(t) if t != Ty::Name)) && (file(r) || !others(&file)),
        }
    };
    let mut out: Vec<(usize, Ty)> = Vec::new();
    for r in roles.iter().filter(read) {
        match out.iter_mut().find(|(arg, _)| *arg == r.arg) {
            Some((_, t)) => *t = t.join(r.kind.ty()),
            None => out.push((r.arg, r.kind.ty())),
        }
    }
    out.sort_by_key(|(arg, _)| *arg);
    out
}

/// Each parameter with its type.
type ParamTypes = Vec<(usize, Ty)>;

/// The command and argument each typed token stands in: its place among
/// the braced ones, or among the `[…]` ones when the last is `true`.
type Typed = Vec<(String, usize, Ty, bool)>;

/// The run's typed tokens ([`crate::facts::Facts::types`]), each matched to
/// the braced argument of the command its source text stands in. A
/// definition's body is no argument (`\def\x{…}`), and a name the run
/// left a register or constant is no command (`\setlength\x{…}`).
fn typed(analysis: &Analysis) -> Typed {
    // Each file's text and where its lines start.
    let mut texts: crate::fixed::HashMap<crate::tex::FileId, Option<(String, Vec<usize>)>> = Default::default();
    let defs = analysis.facts.defs.iter().filter(|d| analysis.project_file(d.span.file));
    let bodies: Vec<crate::tex::Span> =
        defs.filter_map(|d| d.mac.as_ref()?.replacement_text.first().map(|t| t.span)).collect();
    for at in bodies.iter().chain(analysis.facts.types.keys()) {
        texts.entry(at.file).or_insert_with(|| {
            let path = &analysis.files.get(usize::from(at.file))?.path;
            let text = crate::overlay::read_to_string(std::path::Path::new(path)).ok()?;
            let lines = std::iter::once(0).chain(text.match_indices('\n').map(|(i, _)| i + 1)).collect();
            Some((text, lines))
        });
    }
    let source = |at: crate::tex::Span| -> Option<(&str, usize)> {
        let (text, lines) = texts.get(&at.file)?.as_ref()?;
        let start = *lines.get(usize::try_from(at.line).ok()?.checked_sub(1)?)?;
        Some((text, start + crate::source::byte_of_col(&text[start..], at.col)))
    };
    let bodies: crate::fixed::HashSet<(crate::tex::FileId, usize)> =
        bodies.iter().filter_map(|at| Some((at.file, source(*at)?.1))).collect();
    let mut out = Vec::new();
    // A body that stands in another command's argument makes that argument
    // a body (`\newcommand{\x}{…}`); `\def\x{…}` names no other command.
    for d in analysis.facts.defs.iter().filter(|d| analysis.project_file(d.span.file)) {
        let Some(at) = d.mac.as_ref().and_then(|m| m.replacement_text.first()).map(|t| t.span) else { continue };
        let Some((text, byte)) = source(at) else { continue };
        let Some((cmd, arg, _, optional)) = enclosing_call(text, byte) else { continue };
        if cmd != analysis.interner.name(d.name) && analysis.interner.lookup(cmd).is_some() {
            out.push((cmd.to_string(), arg, Ty::Body, optional));
        }
    }
    for (at, ty) in &analysis.facts.types {
        let Some((text, byte)) = source(*at) else { continue };
        let Some((cmd, arg, open, optional)) = enclosing_call(text, byte) else { continue };
        let first = text.len() - text[open + 1..].trim_start().len();
        let Some(sym) = analysis.interner.lookup(cmd) else { continue };
        let meaning = analysis.env.meaning(sym);
        let command = !matches!(meaning, crate::tex::Meaning::Register(..))
            && !matches!(meaning.prim(), Some(crate::builtins::Primitive::Register(_)));
        if command && !bodies.contains(&(at.file, first)) {
            out.push((cmd.to_string(), arg, *ty, optional));
        }
    }
    out
}

/// The command whose argument holds byte `at` of `text`: its name, the
/// argument's place among those of its kind (braced, or `[…]` when the
/// last is `true`) and where it opens. It looks back no further than the
/// paragraph: an argument that spans one is rare, and so is not worth a
/// scan of the whole file per token.
fn enclosing_call(text: &str, at: usize) -> Option<(&str, usize, usize, bool)> {
    let b = text.as_bytes();
    let back = |i: usize| i.checked_sub(1).filter(|j| !(b[*j] == b'\n' && b.get(j + 1) == Some(&b'\n')));
    // Out to the group `at` stands in: braces nest, a `[` counts outside them.
    let (mut i, mut depth, mut bracket) = (at, 0, 0);
    loop {
        i = back(i)?;
        match b[i] {
            b'}' => depth += 1,
            b'{' if depth == 0 => break,
            b'{' => depth -= 1,
            b']' if depth == 0 => bracket += 1,
            b'[' if depth == 0 && bracket == 0 => break,
            b'[' if depth == 0 => bracket -= 1,
            _ => {}
        }
    }
    let (open, optional) = (i, b[i] == b'[');
    // The groups before it, back to the command's name.
    let mut arg = 0;
    loop {
        while back(i).is_some_and(|j| b[j].is_ascii_whitespace()) {
            i -= 1;
        }
        let (open, close) = match b.get(back(i)?)? {
            b'}' => (b'{', b'}'),
            b']' => (b'[', b']'),
            _ => break,
        };
        arg += usize::from((open == b'[') == optional);
        let mut depth = 0;
        loop {
            i = back(i)?;
            depth += i32::from(b[i] == close) - i32::from(b[i] == open);
            if depth == 0 {
                break;
            }
        }
    }
    if back(i).is_some_and(|j| b[j] == b'*') {
        i -= 1;
    }
    let end = i;
    while back(i).is_some_and(|j| b[j].is_ascii_alphabetic() || b[j] == b'@') {
        i -= 1;
    }
    (back(i).is_some_and(|j| b[j] == b'\\') && i < end).then(|| (&text[i..end], arg, open, optional))
}

fn roles_at(analysis: &Analysis, typed: &Typed, cmd: &str, depth: usize) -> Vec<Role> {
    let mut out = evidence(analysis, cmd);
    out.extend(typed.iter().filter(|(c, .., optional)| c == cmd && !optional).map(|(_, arg, ty, _)| Role {
        arg: *arg,
        kind: RoleKind::Type(*ty),
        prefix: String::new(),
        suffix: String::new(),
    }));
    let body = project_body(analysis, cmd);
    if let Some(body) = &body
        && depth < DEPTH
    {
        out.extend(flow(analysis, typed, body, depth));
    }
    // A command the project defines anew is what its body says; the
    // conventions stand in only where the run saw nothing of it.
    if body.is_none() && !out.iter().any(|r| matches!(r.kind, RoleKind::Occ(_) | RoleKind::Load(_))) {
        out.extend(seed(cmd));
    }
    out
}

/// What the roles read of the run's sources, once per analysis: the typed
/// tokens, and the calls that made an occurrence or a load.
#[derive(Clone, Default)]
pub struct Sources {
    typed: Typed,
    calls: Vec<Call>,
}

/// An occurrence or a load: its key and kind, and the line from the `\` of
/// the call that made it on, then from its own place on.
#[derive(Clone)]
struct Call {
    lines: Vec<String>,
    key: String,
    kind: RoleKind,
}

fn sources(analysis: &Analysis) -> &Sources {
    analysis.role_sources.get_or_init(|| Sources { typed: typed(analysis), calls: calls(analysis) })
}

/// Each occurrence and load, with the rest of the lines where a call may
/// stand: each file read once.
fn calls(analysis: &Analysis) -> Vec<Call> {
    let occurrences = analysis
        .facts
        .occurrences
        .iter()
        .map(|o| (o.call.map_or(o.span, |c| c.1), Some(o.span), o.key.as_str(), RoleKind::Occ(o.kind)));
    let loads =
        analysis.facts.loads.iter().map(|l| (l.call.unwrap_or(l.span), None, l.name.as_str(), RoleKind::Load(l.kind)));
    let mut texts: crate::fixed::HashMap<crate::tex::FileId, Option<(String, Vec<usize>)>> = Default::default();
    occurrences
        .chain(loads)
        .filter_map(|(at, own, key, kind)| {
            let (text, starts) = texts
                .entry(at.file)
                .or_insert_with(|| {
                    let path = &analysis.files.get(usize::from(at.file))?.path;
                    let text = crate::overlay::read_to_string(std::path::Path::new(path)).ok()?;
                    let starts = std::iter::once(0).chain(text.match_indices('\n').map(|(i, _)| i + 1)).collect();
                    Some((text, starts))
                })
                .as_ref()?;
            // A value written later (biblatex's `.bcf`, at the start of the
            // document) stands where the document's call gave it.
            let rest = |at: crate::tex::Span| {
                let start = *starts.get(usize::try_from(at.line).ok()?.checked_sub(1)?)?;
                let line = text[start..].lines().next()?;
                Some(line.get(crate::source::byte_of_col(line, at.col)..)?).filter(|r| r.starts_with('\\'))
            };
            let lines: Vec<String> = [Some(at), own.filter(|o| o.file == at.file)]
                .into_iter()
                .flatten()
                .filter_map(rest)
                .map(String::from)
                .collect();
            (!lines.is_empty()).then(|| Call { lines, key: key.to_string(), kind })
        })
        .collect()
}

/// The roles the run saw `cmd`'s calls take: each occurrence or load made
/// by a call of `cmd`, at the braced argument that held its key.
fn evidence(analysis: &Analysis, cmd: &str) -> Vec<Role> {
    sources(analysis)
        .calls
        .iter()
        .filter_map(|Call { lines, key, kind }| {
            let held = lines.iter().find_map(|line| {
                call_arguments(line, cmd)?.iter().enumerate().find_map(|(i, held)| {
                    if let Some(prefix) = key.strip_suffix(held.as_str()).filter(|_| !held.is_empty()) {
                        return Some((i, prefix.to_string(), String::new()));
                    }
                    // A load per item of a list (`\usetikzlibrary{calc,arrows}`), its
                    // name the item between a fixed prefix and suffix.
                    let item = held.split(',').map(str::trim).filter(|t| !t.is_empty()).find(|t| key.contains(*t))?;
                    let (prefix, suffix) = key.rsplit_once(item)?;
                    (matches!(kind, RoleKind::Load(_)) && !(prefix.is_empty() && suffix.is_empty()))
                        .then(|| (i, prefix.to_string(), suffix.to_string()))
                })
            });
            let (arg, prefix, suffix) = held?;
            Some(Role { arg, kind: *kind, prefix, suffix })
        })
        .collect()
}

/// The braced arguments of a call of `cmd` that `line` starts with; `None`
/// where it starts with none.
fn call_arguments(line: &str, cmd: &str) -> Option<Vec<String>> {
    let rest = line.strip_prefix('\\')?.strip_prefix(cmd)?;
    if rest.starts_with(|c: char| c.is_ascii_alphabetic() || c == '@') {
        return None;
    }
    Some(braced_groups(rest.trim_start_matches('*')))
}

/// The `{…}` groups at the start of `text`, `[…]` ones skipped.
pub fn braced_groups(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text.trim_start();
    while let Some(open) = rest.chars().next().filter(|c| matches!(c, '{' | '[')) {
        let close = if open == '{' { '}' } else { ']' };
        let mut depth = 0;
        let end = rest.char_indices().find_map(|(i, c)| {
            depth += i32::from(c == open) - i32::from(c == close);
            (c == close && depth == 0).then_some(i)
        });
        let Some(end) = end else { break };
        if open == '{' {
            out.push(rest[1..end].trim().to_string());
        }
        rest = rest[end + 1..].trim_start();
    }
    out
}

/// The body of the project's own definition of `cmd`, if it has one.
fn project_body(analysis: &Analysis, cmd: &str) -> Option<std::rc::Rc<[Token]>> {
    analysis
        .facts
        .defs
        .iter()
        .rev()
        .filter(|d| analysis.interner.name(d.name) == cmd && analysis.project_file(d.span.file))
        .find_map(|d| d.mac.as_ref().map(|m| m.replacement_text.clone()))
}

/// The roles a body gives its parameters: `#i` in a braced argument of a
/// command with a role there, after literal characters or not.
fn flow(analysis: &Analysis, typed: &Typed, body: &[Token], depth: usize) -> Vec<Role> {
    let mut out = Vec::new();
    for (at, t) in body.iter().enumerate() {
        let Tok::Cs(sym) = t.tok else { continue };
        let callee = analysis.interner.name(sym);
        let groups = body_groups(&body[at + 1..]);
        if groups.iter().all(|g| param_after(analysis, g).is_none()) {
            continue;
        }
        let known = roles_at(analysis, typed, callee, depth + 1);
        for (j, group) in groups.iter().enumerate() {
            let Some((i, prefix)) = param_after(analysis, group) else { continue };
            // A type is the whole argument's, not of what follows a prefix.
            let whole = |r: &&Role| !matches!(r.kind, RoleKind::Type(_)) || prefix.is_empty() && r.prefix.is_empty();
            for r in known.iter().filter(|r| r.arg == j).filter(whole) {
                out.push(Role {
                    arg: i,
                    kind: r.kind,
                    prefix: format!("{}{prefix}", r.prefix),
                    suffix: r.suffix.clone(),
                });
            }
        }
    }
    out
}

/// The types a body's own assignments and scanners give its parameters
/// (tex.web §§ 1224, 1057): `#i` after a register heading an assignment
/// is its value, after `\penalty`, `\kern` or `\hskip` a number, dimen or
/// glue; `#i` heading an assignment (after `\advance`, or with `\relax`
/// ending the value) names a register and the `#j` after it is a
/// quantity; `#i` in `\csname…\endcsname` is a name, and the `#j` after
/// such a name heading an assignment a quantity.
/// Also each register parameter with the parameter of its value.
fn assigned(analysis: &Analysis, body: &[Token]) -> (ParamTypes, Vec<(usize, usize)>) {
    use crate::builtins::{Primitive as P, Typeset};
    let toks: Vec<&Token> =
        body.iter().filter(|t| !matches!(t.tok, Tok::Chr(_, Catcode::Space) | Tok::Chr('=', Catcode::Other))).collect();
    let prim = |k: usize| match toks.get(k)?.tok {
        Tok::Cs(s) => analysis.env.meaning(s).prim(),
        _ => None,
    };
    let starts = |k: usize| {
        k == 0
            || matches!(toks[k - 1].tok, Tok::Chr(_, Catcode::Begin | Catcode::End))
            || matches!(prim(k - 1), Some(P::Relax | P::Prefix(_) | P::Arith(_)))
    };
    let param = |k: usize| match toks.get(k)?.tok {
        Tok::Param(n) => Some(usize::from(n).saturating_sub(1)),
        _ => None,
    };
    let mut out = Vec::new();
    let mut pairs = Vec::new();
    let mut csname = None;
    for k in 0..toks.len() {
        match prim(k) {
            Some(P::Csname) => csname = Some(k),
            Some(P::Endcsname) => csname = None,
            _ => {}
        }
        let Some(i) = param(k) else { continue };
        if csname.is_some() {
            out.push((i, Ty::Name));
            continue;
        }
        let Some(before) = k.checked_sub(1) else { continue };
        // The operand before `#i`: a register, a parameter or a formed name.
        let head = match prim(before) {
            Some(P::Endcsname) => (0..before).rev().find(|&h| prim(h) == Some(P::Csname)),
            _ => Some(before),
        };
        let reg = match toks[before].tok {
            Tok::Cs(s) => Ty::of_meaning(&analysis.env.meaning(s)),
            _ => None,
        };
        let scans = prim(before).and_then(crate::builtins::pdf_string_skip);
        let ty = match (prim(before), reg, param(before)) {
            _ if scans == Some(Typeset::Number) => Some(Ty::Number),
            _ if scans == Some(Typeset::Dimen) => Some(Ty::Dimen),
            _ if scans == Some(Typeset::Glue) => Some(Ty::Glue),
            _ if !head.is_some_and(starts) => None,
            (_, Some(Ty::Reg(kind)), _) => Some(Ty::Reg(kind).parent()).filter(|t| t.leq(Ty::Quantity)),
            // Text then text (`#1#2`) is an assignment only where `\advance`
            // heads it or `\relax` ends the value.
            _ if !(head.is_some_and(|h| h > 0 && matches!(prim(h - 1), Some(P::Arith(_))))
                || prim(k + 1) == Some(P::Relax)) =>
            {
                None
            }
            (_, _, Some(j)) if j != i => {
                out.push((j, Ty::Register));
                pairs.push((j, i));
                Some(Ty::Quantity)
            }
            (Some(P::Endcsname), ..) => Some(Ty::Quantity),
            _ => None,
        };
        out.extend(ty.map(|t| (i, t)));
    }
    let mut joined: Vec<(usize, Ty)> = Vec::new();
    for (i, t) in out {
        match joined.iter_mut().find(|(j, _)| *j == i) {
            Some((_, u)) => *u = u.join(t),
            None => joined.push((i, t)),
        }
    }
    (joined, pairs)
}

/// The braced groups that follow a command in a body, `[…]` ones skipped.
fn body_groups(tokens: &[Token]) -> Vec<&[Token]> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(t) = tokens.get(i) {
        match t.tok {
            Tok::Chr(_, Catcode::Space) => i += 1,
            Tok::Chr('[', Catcode::Other) => {
                let Some(end) = tokens[i..].iter().position(|t| t.tok == Tok::Chr(']', Catcode::Other)) else { break };
                i += end + 1;
            }
            Tok::Chr(_, Catcode::Begin) => {
                let mut depth = 0;
                let Some(end) = tokens[i..].iter().position(|t| {
                    match t.tok {
                        Tok::Chr(_, Catcode::Begin) => depth += 1,
                        Tok::Chr(_, Catcode::End) => depth -= 1,
                        _ => {}
                    }
                    depth == 0
                }) else {
                    break;
                };
                out.push(&tokens[i + 1..i + end]);
                i += end + 1;
            }
            _ => break,
        }
    }
    out
}

/// A group that is literal characters and then one parameter `#i`: the
/// parameter (from 0) and the characters before it.
fn param_after(analysis: &Analysis, group: &[Token]) -> Option<(usize, String)> {
    let (last, before) = group.split_last()?;
    let Tok::Param(n) = last.tok else { return None };
    let prefix = crate::tex::text_of(before, &analysis.interner);
    before.iter().all(|t| matches!(t.tok, Tok::Chr(..))).then(|| (usize::from(n).saturating_sub(1), prefix))
}

/// The conventions of the kernel and common packages, for a command whose
/// meaning is theirs: where its first braced argument is a key or a name.
fn seed(cmd: &str) -> Vec<Role> {
    use LoadKind as L;
    use OccKind as O;
    let first = |kind| vec![Role { arg: 0, kind, prefix: String::new(), suffix: String::new() }];
    match cmd {
        "documentclass" | "LoadClass" => first(RoleKind::Load(L::Class)),
        "usepackage" | "RequirePackage" => first(RoleKind::Load(L::Package)),
        "input" | "subfile" => first(RoleKind::Load(L::Input)),
        "include" | "includeonly" => first(RoleKind::Load(L::Include)),
        "begin" => first(RoleKind::Occ(O::BeginEnvironment)),
        "end" => first(RoleKind::Occ(O::EndEnvironment)),
        "includegraphics" => first(RoleKind::Occ(O::Graphics)),
        "label" => first(RoleKind::Occ(O::Label)),
        "crefrange" | "Crefrange" => (0..2)
            .map(|arg| Role { arg, kind: RoleKind::Occ(O::Ref), prefix: String::new(), suffix: String::new() })
            .collect(),
        "ref" | "eqref" | "pageref" | "autoref" | "nameref" | "cref" | "Cref" | "cpageref" | "Cpageref" | "vref"
        | "Vref" | "labelcref" | "subref" => first(RoleKind::Occ(O::Ref)),
        c if c.contains("cite") => first(RoleKind::Occ(O::Cite)),
        _ => Vec::new(),
    }
}

/// Whether `cmd` reads a release date from the text after it: its
/// arguments flow, through the calls that read on past their own body, to a
/// macro that splits them at slashes, as `\@ifl@t@r` does with
/// `\@parse@version`. Found from the bodies alone, so the same holds for
/// every command that reads one.
pub fn takes_release(analysis: &Analysis, cmd: &str) -> bool {
    let Some(sym) = analysis.interner.lookup(cmd) else { return false };
    let bodies = Bodies(analysis);
    // where the document writes it, any of its bodies may read the call
    bodies.all(sym).iter().any(|m| !body_date_slots(&bodies, m, DATE_DEPTH, &mut Default::default()).is_empty())
}

/// The macros as the preamble sees them: the format's meaning where it has
/// one, since the run replaces what only the preamble may use
/// (`\@onlypreamble`) with an error, else the run's.
struct Bodies<'a>(&'a Analysis);

impl Bodies<'_> {
    fn get(&self, sym: Sym) -> Option<Rc<MacroDef>> {
        let kernel = self.0.kernel.as_ref().and_then(|k| k.meaning(sym).as_macro().cloned());
        kernel.or_else(|| self.0.env.meaning(sym).as_macro().cloned())
    }

    /// Every body the run gave `sym` besides: `\documentclass` lets
    /// `\usepackage` be `\RequirePackage` for the preamble alone.
    fn all(&self, sym: Sym) -> Vec<Rc<MacroDef>> {
        let mut out: Vec<Rc<MacroDef>> = Vec::new();
        let made = self.0.facts.defs.iter().filter(|d| d.name == sym).filter_map(|d| d.mac.clone());
        for m in self.get(sym).into_iter().chain(made) {
            if !out.iter().any(|o| Rc::ptr_eq(o, &m)) {
                out.push(m);
            }
        }
        out
    }

    fn is(&self, t: &Token, p: crate::builtins::Primitive) -> bool {
        matches!(t.tok, Tok::Cs(s) if self.0.env.meaning(s).prim() == Some(p))
    }
}

/// The date slots found per macro, with the depth left when found.
type Known = crate::fixed::HashMap<Sym, (usize, Vec<usize>)>;

/// How deep [`date_slots`] follows a body's calls.
const DATE_DEPTH: usize = 9;

/// The places among the arguments of `sym` its body reads as a date.
fn date_slots(bodies: &Bodies<'_>, sym: Sym, depth: usize, known: &mut Known) -> Vec<usize> {
    // a result found with less depth left may miss what this one finds
    if let Some((_, done)) = known.get(&sym).filter(|(d, _)| *d >= depth) {
        return done.clone();
    }
    let Some(mac) = bodies.get(sym) else { return Vec::new() };
    known.insert(sym, (depth, Vec::new()));
    let out = body_date_slots(bodies, &mac, depth, known);
    known.insert(sym, (depth, out.clone()));
    out
}

/// The places among the arguments of `mac` (counting those a tail call
/// reads past its own parameters) its body reads as a date: one whose text
/// a macro with a `/`-delimited parameter splits, or one passed to a place
/// that is itself such a date. A `\csname` forms a name, so no date.
fn body_date_slots(bodies: &Bodies<'_>, mac: &MacroDef, depth: usize, known: &mut Known) -> Vec<usize> {
    use crate::builtins::Primitive as P;
    if mac.parameter_text.items.iter().any(|i| matches!(i, ParamItem::Lit(Tok::Chr('/', _)))) {
        return vec![0];
    }
    let mut out: Vec<usize> = Vec::new();
    if depth == 0 {
        return out;
    }
    let toks: Vec<&Token> =
        mac.replacement_text.iter().filter(|t| !matches!(t.tok, Tok::Chr(_, Catcode::Space))).collect();
    let param = |t: &&Token| match t.tok {
        Tok::Param(n) => Some(usize::from(n).saturating_sub(1)),
        _ => None,
    };
    for (k, t) in toks.iter().enumerate() {
        let Tok::Cs(c) = t.tok else { continue };
        let slots = date_slots(bodies, c, depth - 1, known);
        let Some(&last) = slots.iter().max() else { continue };
        if let Some(callee) = bodies.get(c) {
            let (args, _) = call_args(bodies, &toks[k + 1..], &callee, last + 1);
            for arg in slots.iter().filter_map(|&p| args.get(p)) {
                if !arg.iter().any(|t| bodies.is(t, P::Csname)) {
                    out.extend(arg.iter().filter_map(param));
                }
            }
        }
    }
    // a call that ends the body reads the rest of its arguments from after this macro's own
    let arity = usize::from(mac.arity());
    let branch = toks.last().is_some_and(|t| bodies.is(t, P::Fi));
    for (c, supplied) in tails(bodies, &toks, branch) {
        let slots = date_slots(bodies, c, depth - 1, known);
        out.extend(slots.into_iter().filter(|q| *q >= supplied).map(|q| arity + q - supplied));
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// The calls in `toks` that read on past its end, each with how many
/// arguments `toks` gives it: the last call, and the calls that end the
/// arguments it is handed (`\@ifnextchar[{\a#1}{\a#1[]}` continues in
/// `\a`). With `branch`, a call that ends a conditional's arm counts too.
fn tails(bodies: &Bodies<'_>, toks: &[&Token], branch: bool) -> Vec<(Sym, usize)> {
    use crate::builtins::Primitive as P;
    let mut out = Vec::new();
    let mut depth = 0i32;
    for (k, t) in toks.iter().enumerate() {
        match t.tok {
            Tok::Chr(_, Catcode::Begin) => depth += 1,
            Tok::Chr(_, Catcode::End) => depth -= 1,
            Tok::Cs(c) if depth == 0 => {
                let rest = &toks[k + 1..];
                if let Some(callee) = bodies.get(c) {
                    let (args, end) = call_args(bodies, rest, &callee, usize::from(callee.arity()));
                    let ends_arm = rest.get(end).is_some_and(|t| bodies.is(t, P::Else) || bodies.is(t, P::Fi));
                    if end == rest.len() || branch && ends_arm {
                        out.push((c, args.len()));
                        for arg in args.iter().filter(|a| !a.is_empty()) {
                            out.extend(tails(bodies, arg, false));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// The first `n` arguments written after a call of `callee`, and where they
/// end, matched against its parameter text (tex.web § 392): a delimited one
/// runs to its delimiter, an undelimited one past the parameters is a group
/// without its braces, a `\csname…\endcsname` or a single token. Fewer than
/// `n` where the text ends first: the call reads on past it.
fn call_args<'a>(bodies: &Bodies<'_>, toks: &[&'a Token], callee: &MacroDef, n: usize) -> (Vec<Vec<&'a Token>>, usize) {
    use crate::builtins::Primitive as P;
    let items = &callee.parameter_text.items;
    let mut out = Vec::new();
    let mut i = 0;
    let mut j = 0;
    // one balanced item from `i`: a group, a `\csname…\endcsname` or a token
    let item = |i: usize| -> usize {
        let mut e = i;
        let mut depth = 0i32;
        let csname = bodies.is(toks[i], P::Csname);
        while let Some(t) = toks.get(e) {
            e += 1;
            match t.tok {
                Tok::Chr(_, Catcode::Begin) => depth += 1,
                Tok::Chr(_, Catcode::End) => depth -= 1,
                _ if csname && bodies.is(t, P::Endcsname) => break,
                _ => {}
            }
            if depth <= 0 && !csname {
                break;
            }
        }
        e
    };
    let unbrace =
        |i: usize, e: usize| match e >= i + 2 && matches!(toks[i].tok, Tok::Chr(_, Catcode::Begin)) && item(i) == e {
            true => toks[i + 1..e - 1].to_vec(),
            false => toks[i..e].to_vec(),
        };
    while out.len() < n && i < toks.len() {
        match items.get(j) {
            Some(ParamItem::Lit(lit)) => {
                if toks[i].tok != *lit {
                    break;
                }
                i += 1;
                j += 1;
            }
            Some(ParamItem::Param(_)) => {
                let delim: Vec<&Tok> = items[j + 1..]
                    .iter()
                    .map_while(|d| match d {
                        ParamItem::Lit(t) => Some(t),
                        ParamItem::Param(_) => None,
                    })
                    .collect();
                j += 1 + delim.len();
                if delim.is_empty() {
                    let e = item(i);
                    out.push(unbrace(i, e));
                    i = e;
                    continue;
                }
                let mut e = i;
                while e < toks.len() && !toks[e..].iter().map(|t| &t.tok).take(delim.len()).eq(delim.iter().copied()) {
                    e = item(e);
                }
                if e >= toks.len() {
                    return (out, toks.len());
                }
                out.push(unbrace(i, e));
                i = e + delim.len();
            }
            None => {
                let e = item(i);
                out.push(unbrace(i, e));
                i = e;
            }
        }
    }
    (out, i)
}
