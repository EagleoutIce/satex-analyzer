//! expl3's naming conventions (interface3, "Naming functions and
//! variables"), checked against what the run did: the module function that
//! made or set each variable ([`crate::facts::Facts::vars`]) and whether
//! the assignment it made was global. A name alone says what the author
//! meant; the run says what happened.

use crate::lint::{Category, Ref, Report, Rule, Severity, Source};
use crate::roles::VarKind;

pub static RULES: &[Rule] = &[
    Rule {
        code: "expl3-scope",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "an expl3 variable's scope prefix disagrees with how the run assigned it",
        explanation: "\
A variable named `\\l_…` is local and `\\g_…` global (interface3). The run set
this one the other way: a `\\l_` variable by a global function (`\\tl_gset:Nn`),
or a `\\g_` variable by a local one. Declarations (`\\…_new:N`) are global for
every variable and do not count.

Fix by renaming the variable or using the function of the other scope.",
        references: &[Ref { source: Source::Interface3, at: "Naming functions and variables" }],
        fixes: &[],
        run: scope,
    },
    Rule {
        code: "expl3-type",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "an expl3 variable's type suffix disagrees with the functions that set it",
        explanation: "\
A variable's name ends in its type (`_tl`, `_seq`, `_int`, …; interface3). The
run made or set this one with another module's function, as `\\seq_set_split:Nnn`
on a `_tl` variable. A string and a comma list are token lists, so token list
functions on them are fine.

Fix by renaming the variable or using the functions of its type.",
        references: &[Ref { source: Source::Interface3, at: "Naming functions and variables" }],
        fixes: &[],
        run: types,
    },
    Rule {
        code: "expl3-constant-set",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "an expl3 constant is set after it was declared",
        explanation: "\
A `\\c_…` variable is a constant (interface3): it is made once, by `\\…_const:N…`
or `\\…_new:N`, and never set. The run set this one again.

Fix by making it a variable (`\\l_…` or `\\g_…`) or by not changing it.",
        references: &[Ref { source: Source::Interface3, at: "Naming functions and variables" }],
        fixes: &[],
        run: constants,
    },
];

/// What a function does to its variable, by its name: declare it, set it
/// globally, or set it locally (interface3's `new`/`const`, `g…` and the rest).
#[derive(PartialEq)]
enum Act {
    Declare,
    Global,
    Local,
}

fn act(function: &str) -> Act {
    let action = function.trim_start_matches('\\').split(':').next().unwrap_or_default();
    let verb = action.split_once('_').map_or("", |(_, v)| v);
    match verb {
        v if v == "new" || v.starts_with("const") || v.starts_with("new_") => Act::Declare,
        v if v.starts_with('g') && !v.starts_with("get") => Act::Global,
        _ => Act::Local,
    }
}

/// The variables the user's files passed, each with its name and the function.
fn uses(report: &Report) -> Vec<(crate::tex::Span, String, VarKind, String)> {
    let a = report.analysis();
    a.facts.vars.iter().map(|v| (v.span, a.interner.cs(v.name), v.kind, a.interner.cs(v.by))).collect()
}

fn scope(report: &mut Report) {
    for (span, name, _, by) in uses(report) {
        let bare = name.trim_start_matches('\\');
        let wrong = match act(&by) {
            Act::Global => bare.starts_with("l_"),
            Act::Local => bare.starts_with("g_"),
            Act::Declare => false,
        };
        if wrong {
            let other = if bare.starts_with("l_") { "local" } else { "global" };
            let message = format!("`{name}` is {other} by its name, but `{by}` sets it the other way");
            report.add(span, &name, message, None);
        }
    }
}

fn types(report: &mut Report) {
    for (span, name, kind, by) in uses(report) {
        let Some(suffix) = name.rsplit('_').next() else { continue };
        let named = VarKind::ALL.into_iter().find(|k| k.as_str() == suffix);
        let register = matches!(suffix, "int" | "dim" | "skip" | "muskip" | "box" | "ior" | "iow");
        let compatible = |a: VarKind, b: VarKind| {
            let up = |k: VarKind| if matches!(k, VarKind::Str | VarKind::Clist) { VarKind::Tl } else { k };
            a == b || up(a) == b || up(b) == a
        };
        let disagrees = match named {
            Some(named) => !compatible(named, kind),
            None => register,
        };
        if disagrees {
            let message =
                format!("`{name}` is a `{suffix}` variable by its name, but `{by}` treats it as `{}`", kind.as_str());
            report.add(span, &name, message, None);
        }
    }
}

fn constants(report: &mut Report) {
    for (span, name, _, by) in uses(report) {
        if name.trim_start_matches('\\').starts_with("c_") && act(&by) != Act::Declare {
            let message = format!("`{name}` is a constant by its name, but `{by}` sets it");
            report.add(span, &name, message, None);
        }
    }
}
