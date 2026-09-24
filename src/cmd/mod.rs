//! The command line: one module per group of commands.

pub mod cache;
mod graph;
mod inspect;
mod lint;
pub mod lsp;
pub mod query;
mod summary;
mod tokens;

use std::io::Write;

use clap::Subcommand;

use crate::machine::Analysis;
use crate::query::Record;
pub use crate::plugin::Format;
use crate::render::Links;

/// What a [`Command`] hands back to the CLI driver to print: a finished list
/// of [`Record`]s, a partial one with a note about what was left out, or
/// nothing further to print.
pub enum Output {
    Records(Vec<Record>),
    /// One block per record rather than a table: `explain` and the other
    /// detail views, whose fields are too wide for a row.
    Detail(Vec<Record>),
    /// `lint`'s findings: grouped by severity and kept apart from
    /// performance and precision notes rather than one flat table.
    Lint(Vec<Record>),
    /// Records plus a line naming what was left out, and the flag that shows it.
    Partial(Vec<Record>, String),
    /// [`Output::Detail`] plus a footer line, the way [`Output::Partial`] adds
    /// one to [`Output::Records`]: `explain`, when a name had more
    /// definitions than the default view shows.
    DetailPartial(Vec<Record>, String),
    Done,
}

/// Everything a [`Command`] needs to run: the finished [`Analysis`], the
/// source text, and the output [`Format`].
pub struct Context<'a> {
    pub analysis: &'a Analysis,
    pub source: &'a str,
    pub format: Format,
    pub links: Links,
}

/// One `satex` subcommand and its arguments, parsed by clap.
/// [`Command::run`] dispatches it against a [`Context`].
#[derive(Subcommand)]
pub enum Command {
    /// Run a query.  `satex query --list` names them all.
    Query {
        /// The query to run; see `satex query --list`.
        #[arg(value_name = "NAME")]
        name: Option<String>,
        /// Filter expression, for example `tag=switch and package~^my`.
        #[arg(long, value_name = "EXPR")]
        filter: Option<String>,
        /// The text the `produces` query looks for, as a literal substring.
        #[arg(long, value_name = "TEXT")]
        text: Option<String>,
        /// The key path the `pgfkeys` query lists keys under, as `/tikz/pingu`.
        #[arg(long, value_name = "PATH")]
        prefix: Option<String>,
        /// The environment or command the `options` query asks about, with
        /// or without the leading backslash.
        #[arg(long = "for", value_name = "NAME")]
        for_: Option<String>,
        /// List the query names instead of running one.
        #[arg(long)]
        list: bool,
        /// Run every query a JSON request names, against one analysis, and
        /// print one answer per query in the same order; `-` for stdin.
        /// See `doc/wiki/queries.md`.
        #[arg(long, value_name = "FILE", conflicts_with_all = ["name", "filter", "text", "prefix", "for_", "list"])]
        request: Option<String>,
    },
    /// Context-sensitive lints for the input file; `--all` includes libraries.
    Lint {
        /// Restrict the findings to a filter expression, for example
        /// `tag=switch and package~^my`.
        #[arg(long, value_name = "EXPR", help_heading = "Selection")]
        filter: Option<String>,
        /// Include the packages and the kernel, not just the input file.
        #[arg(long, help_heading = "Selection")]
        all: bool,
        /// List the rules instead of running them.
        #[arg(long, help_heading = "Rules")]
        rules: bool,
        /// Explain one rule and how to fix what it reports.
        #[arg(long, value_name = "CODE", help_heading = "Rules")]
        explain: Option<String>,
        /// Apply the safe fixes to the project's files, lint again until
        /// nothing fixable is left, and report what remains.
        #[arg(long, help_heading = "Fixes")]
        fix: bool,
        /// Apply the fixes that need a review too; with `--fix` or `--diff`.
        #[arg(long, help_heading = "Fixes")]
        unsafe_fixes: bool,
        /// Print the fixes as a unified diff instead of writing them.
        #[arg(long, help_heading = "Fixes")]
        diff: bool,
    },
    /// What the document and every file it loads contribute.
    Summary {
        /// Every file, and the names each one defines.
        #[arg(long)]
        all: bool,
        /// How deep the load tree goes.
        #[arg(long, value_name = "N")]
        depth: Option<usize>,
    },
    /// Control sequences visible at a position, for completion.
    Scope {
        /// Position in the main file as LINE:COL; omit for end of document.
        #[arg(long, value_name = "LINE:COL")]
        at: Option<String>,
        /// Filter expression, for example `tag=switch and package~^my`.
        #[arg(long, value_name = "EXPR")]
        filter: Option<String>,
        /// Include package and kernel internals, and every register and
        /// character meaning — hidden by default, so a completion sees the
        /// hundred names worth offering rather than the kernel's own
        /// bookkeeping.
        #[arg(long)]
        all: bool,
    },
    /// What a control sequence means and where it comes from.
    Explain {
        /// Control sequence names to explain, with or without the leading
        /// backslash.
        #[arg(value_name = "NAME", required = true)]
        names: Vec<String>,
        /// List every recorded definition instead of the preamble and
        /// document ones.
        #[arg(long)]
        all: bool,
        /// Ask at one place instead: `preamble`, `document`, `LINE`,
        /// `FILE:LINE` or `after:PACKAGE`.
        #[arg(long, value_name = "WHERE")]
        at: Option<String>,
    },
    /// Slice the document around a control sequence, environment or position:
    /// a compilable reconstruction of the main file by default.
    Slice {
        /// Control sequences, environment names or label keys to slice on.
        #[arg(value_name = "NAME")]
        names: Vec<String>,
        /// Slice on whatever is at this position instead; the main file
        /// unless a file the run read is named.
        #[arg(long, value_name = "[FILE:]LINE:COL")]
        at: Option<String>,
        /// Slice on every occurrence or definition a query accepts instead
        /// of a name or a position, e.g. `kind=begin-environment and
        /// key=figure` to slice to every figure.
        #[arg(long = "where", value_name = "EXPR")]
        where_: Option<String>,
        /// Follow dependencies forwards — what the criterion affects.
        #[arg(long)]
        forward: bool,
        /// Print the matching records instead of the reconstructed document;
        /// implied by `--format json/csv/markdown`.
        #[arg(long)]
        list: bool,
        /// Write a new project into DIR: the sliced main file, plus every
        /// local package, class, graphics or bibliography file it needs,
        /// with paths preserved.
        #[arg(long, value_name = "DIR")]
        out: Option<std::path::PathBuf>,
    },
    /// Everything a switch, option or conditional governs.  Without a name,
    /// the switches and options in effect.  Several names answer in one
    /// table, each row's `for` column saying which one it answers.
    #[command(alias = "options")]
    Controls {
        /// A switch (`\ifdraft`), a package option, or any conditional;
        /// repeatable.
        #[arg(value_name = "NAME")]
        names: Vec<String>,
        /// Include the switches and options the kernel and the packages bring.
        #[arg(long)]
        all: bool,
    },
    /// The program dependence graph — data dependencies plus control
    /// dependencies — as DOT or JSON.
    #[command(alias = "deps")]
    Dependencies,
    /// The execution trace, step by step.
    Trace {
        /// Filter expression, for example `tag=switch and package~^my`.
        #[arg(long, value_name = "EXPR")]
        filter: Option<String>,
        /// Only these lines of the main file, with everything their calls do.
        #[arg(long, value_name = "FROM:TO")]
        lines: Option<String>,
        /// Only these steps of the run.
        #[arg(long, value_name = "FROM:TO")]
        steps: Option<String>,
    },
    /// The interpreted LaTeX2e kernel and the package caches.  Without a
    /// further command, prints their status.
    Cache {
        #[command(subcommand)]
        action: Option<CacheAction>,
    },
    /// The token stream the mouth produces.
    Tokens,
    /// A Language Server Protocol server: a thin facade over the queries and
    /// lints above, driven by `initialize`/`didOpen`/`didChange` instead of
    /// one-shot arguments.  Stdio by default; `--port` switches to TCP.
    Lsp {
        /// Listen on this TCP port instead of stdio.
        #[arg(long, value_name = "N")]
        port: Option<u16>,
        /// Interface to bind when `--port` is given.
        #[arg(long, value_name = "HOST")]
        host: Option<String>,
    },
}

/// `satex cache`'s further commands.
#[derive(Subcommand)]
pub enum CacheAction {
    /// Interpret the kernel, replacing its cache, and the given packages —
    /// or `cache_index`'s configured ones when none are given.  Builds only
    /// what is missing or stale; `--refresh` rebuilds everything.
    Build {
        /// Packages to prepare instead of the configured ones.
        #[arg(value_name = "PACKAGE")]
        packages: Vec<String>,
        /// Rebuild every package cache too, not just what is missing or stale.
        #[arg(long)]
        refresh: bool,
        /// Prepare for this engine instead of the configured ones (repeatable).
        #[arg(long = "engine", value_name = "ENGINE")]
        engines: Vec<String>,
        /// Interpret this preamble on top of the kernel and cache the
        /// result, the way `mylatexformat` dumps a format from one, instead
        /// of building any packages.
        #[arg(long, value_name = "PATH")]
        preamble: Option<std::path::PathBuf>,
    },
    /// Delete every cache: the interpreted kernel and the package caches.
    Clear,
    /// Apply the size and staleness pruning now.
    Prune,
}

impl Command {
    pub fn needs_source(&self) -> bool {
        !matches!(
            self,
            Command::Query { list: true, .. }
                | Command::Query { request: Some(_), .. }
                | Command::Lint { rules: true, .. }
                | Command::Lint { explain: Some(_), .. }
                | Command::Cache { action: Some(_) }
                | Command::Lsp { .. }
        )
    }

    pub fn needs_analysis(&self) -> bool {
        self.needs_source() && !matches!(self, Command::Tokens)
    }

    pub fn wants_trace(&self) -> bool {
        matches!(self, Command::Trace { .. })
    }

    /// Whether the run needs to keep what it read for each argument: the
    /// `options` query reads a key family's name out of the argument a
    /// `\setkeys`-family call was made with, which is only recorded when
    /// this is on (`record_arguments` in `satex.yaml`, off by default —
    /// keeping every argument costs real memory on a large document).
    pub fn wants_arguments(&self) -> bool {
        matches!(self, Command::Query { name: Some(n), request: None, .. } if n == "options")
    }

    /// `lint --fix` or `--diff`: the output is a diff or the findings left
    /// after fixing, which say themselves what the analysis could not follow.
    pub fn fixes(&self) -> bool {
        matches!(self, Command::Lint { fix: true, .. } | Command::Lint { diff: true, .. })
    }

    /// Whether `format` is something this command can print, matching the
    /// precedence its own `run` gives its flags (`--rules` before
    /// `--explain` before the findings themselves) — and a hint for what to
    /// try instead when it is not.  `github` and `sarif` are for diagnostics
    /// (`lint`); `dot` is for the dependency graph; everything else
    /// prints whatever rows it has as text, json, csv or markdown.
    pub fn check_format(&self, format: Format) -> Result<(), String> {
        let generic = matches!(format, Format::Text | Format::Json | Format::Csv | Format::Markdown);
        let (ok, hint): (bool, &str) = match self {
            Command::Lint { rules: true, .. } => (generic, "text, json, csv or markdown"),
            Command::Lint { explain: Some(_), .. } => {
                (matches!(format, Format::Text | Format::Json), "text or json")
            }
            Command::Lint { .. } => (
                generic || matches!(format, Format::Github | Format::Sarif | Format::Lsp),
                "text, json, csv, markdown, github, sarif or lsp",
            ),
            Command::Summary { .. } | Command::Cache { action: None } => {
                (matches!(format, Format::Text | Format::Json), "text or json")
            }
            Command::Dependencies => {
                (matches!(format, Format::Text | Format::Json | Format::Dot), "text, json or dot")
            }
            _ => (generic, "text, json, csv or markdown"),
        };
        if ok {
            Ok(())
        } else {
            Err(format!("`--format {}` does not apply here; try {hint}", format.as_str()))
        }
    }

    pub fn opaque_conditionals(&self) -> Vec<String> {
        match self {
            // Without a name the document's own switches are analyzed as
            // undecidable, so the summary can say what each one governs.
            Command::Controls { names, .. } if names.is_empty() => {
                vec![crate::config::EVERY_SWITCH.to_string()]
            }
            Command::Controls { names, .. } => {
                names.iter().map(|n| n.trim_start_matches('\\').to_string()).collect()
            }
            _ => Vec::new(),
        }
    }

    pub fn run(&self, context: &Context, out: &mut impl Write) -> Result<Output, String> {
        match self {
            Command::Query { name, filter, text, prefix, for_, list, request: _ } => query::run(
                context,
                name.as_deref(),
                filter.as_deref(),
                text.as_deref(),
                prefix.as_deref(),
                for_.as_deref(),
                *list,
                out,
            ),
            Command::Trace { filter, .. } => query::trace(context, filter.as_deref()),
            Command::Lint { filter, all, rules, explain, fix, unsafe_fixes, diff } => {
                let fixing = lint::Fixing { fix: *fix, unsafe_fixes: *unsafe_fixes, diff: *diff };
                lint::run(context, filter.as_deref(), *all, *rules, explain.as_deref(), fixing, out)
            }
            Command::Summary { all, depth } => summary::run(context, *all, *depth, out),
            Command::Scope { at, filter, all } => {
                inspect::scope(context, at.as_deref(), filter.as_deref(), *all)
            }
            Command::Explain { names, all, at } => inspect::explain(context, names, *all, at.as_deref()),
            Command::Slice { names, at, where_, forward, list, out: out_dir } => {
                let query = inspect::SliceQuery {
                    at: at.as_deref(),
                    where_: where_.as_deref(),
                    forward: *forward,
                    list: *list,
                };
                inspect::slice(context, names, query, out_dir.as_deref(), out)
            }
            Command::Controls { names, all } => inspect::controls(context, names, *all),
            Command::Dependencies => graph::run(context, out),
            Command::Cache { action: None } => cache::status(context, out),
            // `main.rs` special-cases every `Cache { action: Some(_) }` and
            // `Lsp` before a `Context` exists, the same way it does
            // `Query { request: Some(_) }`.
            Command::Cache { action: Some(_) } => Ok(Output::Done),
            Command::Lsp { .. } => Ok(Output::Done),
            Command::Tokens => tokens::run(context),
        }
    }
}

pub fn parse_filter(text: Option<&str>) -> Result<crate::query::Filter, String> {
    match text {
        None => Ok(crate::query::Filter::Always),
        Some(text) => crate::query::Filter::parse(text),
    }
}

pub fn parse_position(text: &str) -> Result<(u32, u32), String> {
    let (line, col) = text.split_once(':').ok_or("expected LINE:COL")?;
    Ok((
        line.trim().parse().map_err(|_| "line must be a number")?,
        col.trim().parse().map_err(|_| "column must be a number")?,
    ))
}

/// `LINE:COL`, or `FILE:LINE:COL` for a position in a file the run read
/// rather than in the main one.
pub fn parse_place(text: &str) -> Result<crate::query::At, String> {
    let (file, rest) = match text.rsplit_once(':').and_then(|(head, _)| head.rsplit_once(':')) {
        Some((file, _)) if !file.is_empty() => (Some(file.to_string()), &text[file.len() + 1..]),
        _ => (None, text),
    };
    let (line, col) = parse_position(rest)?;
    Ok(crate::query::At { file, line, col })
}
