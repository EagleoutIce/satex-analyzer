//! `satex install`: install the packages the document needs and this
//! machine does not have.
//!
//! Installing is not a text edit, so it is never part of `satex lint
//! --fix`: the lint only names the command.  Here it is the request itself,
//! which is why the query runs whatever `plugins.packages` says.  satex
//! never elevates privileges — when the installation's tree cannot be
//! written it says so and gives the command to run.

use std::io::Write;

use super::{Context, Format, Output};
use crate::plugin::packages::{Manager, Provided, shell, tree_writable, unresolved};
use crate::render;

/// What `satex install` was asked to do.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub yes: bool,
    pub dry_run: bool,
    pub usermode: bool,
}

/// One package to install: the name the document loads, and the package of
/// the distribution that ships it.
struct Target {
    wanted: String,
    package: String,
}

pub fn run(context: &Context, packages: &[String], options: Options, out: &mut impl Write) -> Result<Output, String> {
    let analysis = context.analysis;
    let json = context.format == Format::Json;
    let manager = Manager::requested(&analysis.settings, analysis.plugins.provider)
        .map_err(|why| format!("nothing to install: {}", why.as_str()))?;

    let targets = match packages.is_empty() {
        true => missing(context, &manager),
        false => named(&manager, packages, json, out),
    };
    if targets.is_empty() {
        let _ = writeln!(out, "every package the document loads is installed");
        return Ok(Output::Done);
    }

    let names: Vec<String> = targets.iter().map(|t| t.package.clone()).collect();
    let usermode = options.usermode;
    let blocked = tree_writable(&analysis.distribution.roots) == Some(false) && !usermode;
    let plan: Vec<Vec<String>> =
        names.iter().map(|name| manager.install_command(std::slice::from_ref(name), usermode)).collect();
    if json && options.dry_run {
        return print_json(out, &targets, &plan, None);
    }
    if !json {
        let _ = writeln!(out, "{}", render::heading("install"));
        for (target, command) in targets.iter().zip(&plan) {
            let note = match target.wanted == target.package {
                true => String::new(),
                false => format!(" (for {})", target.wanted),
            };
            let _ = writeln!(out, "  {} {}{note}", render::label(&target.package), shell(command));
        }
    }
    // satex never elevates, so a tree it cannot write is the user's to
    // install into, either in user mode or with their own elevated run.
    if blocked && !json {
        let _ = writeln!(out, "  {} {} cannot be written by this user", render::label("tree"), main_root(analysis));
        let _ = writeln!(
            out,
            "  {} `satex install --usermode`, or run\n    {}",
            render::label("instead"),
            manager.elevated(&names, analysis.plugins.platform)
        );
    }
    if options.dry_run {
        if !json {
            let _ = writeln!(out, "  {} nothing was run", render::label("dry run"));
        }
        return Ok(Output::Done);
    }
    if blocked {
        return Err("the installation tree is not writable, and satex does not elevate".into());
    }
    if !options.yes && !confirm(out)? {
        let _ = writeln!(out, "nothing was installed");
        return Ok(Output::Done);
    }

    let mut results = Vec::new();
    let mut failed = 0;
    for name in &names {
        let done = manager.install(std::slice::from_ref(name), usermode);
        if !done.ok {
            failed += 1;
        }
        if !json {
            let state = if done.ok { render::good("installed") } else { "failed".to_string() };
            let _ = writeln!(out, "  {} {name}", render::label(&state));
            if !done.message.is_empty() && !done.ok {
                for line in done.message.lines() {
                    let _ = writeln!(out, "    {line}");
                }
            }
        }
        results.push((name.clone(), done));
    }
    if json {
        return print_json(out, &targets, &plan, Some(&results));
    }
    match failed {
        0 => Ok(Output::Done),
        n => Err(format!("{n} of {} package(s) could not be installed", names.len())),
    }
}

/// The tree an install writes into, for the message that says it cannot be.
fn main_root(analysis: &crate::machine::Analysis) -> String {
    analysis
        .distribution
        .roots
        .iter()
        .find(|root| root.file_name().is_some_and(|name| name == "texmf-dist"))
        .or_else(|| analysis.distribution.roots.first())
        .map(|root| root.display().to_string())
        .unwrap_or_else(|| "the installation".into())
}

/// The packages the user named, less those the distribution has no package
/// for: a name that is no package of this distribution is said so, not
/// handed to the manager.
fn named(manager: &Manager, packages: &[String], json: bool, out: &mut impl Write) -> Vec<Target> {
    let mut targets = Vec::new();
    for name in packages {
        if !manager.knows(name) {
            if !json {
                let _ = writeln!(out, "{} `{name}` is no package of this distribution", render::label("unknown"));
            }
            continue;
        }
        targets.push(Target { wanted: name.clone(), package: name.clone() });
    }
    targets
}

/// The packages the document loads, that the search did not find, and that
/// the distribution could install.
fn missing(context: &Context, manager: &Manager) -> Vec<Target> {
    let mut targets: Vec<Target> = Vec::new();
    for missing in unresolved(context.analysis) {
        let Provided::By(package) = manager.provides(&missing.file) else { continue };
        if targets.iter().any(|t| t.package == package) {
            continue;
        }
        targets.push(Target { wanted: missing.name, package });
    }
    targets
}

/// Ask before changing the machine.  Without a terminal there is nobody to
/// ask, so `--yes` has to say it.
fn confirm(out: &mut impl Write) -> Result<bool, String> {
    if !crate::progress::terminal(std::io::stdin()) {
        return Err("not a terminal: pass --yes to install without asking".into());
    }
    let _ = write!(out, "install these? [y/N] ");
    let _ = out.flush();
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer).map_err(|e| e.to_string())?;
    Ok(matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes"))
}

type Results = [(String, crate::plugin::packages::Installed)];

fn print_json(
    out: &mut impl Write,
    targets: &[Target],
    plan: &[Vec<String>],
    results: Option<&Results>,
) -> Result<Output, String> {
    let packages: Vec<serde_json::Value> = targets
        .iter()
        .zip(plan)
        .map(|(target, command)| {
            let done = results.and_then(|r| r.iter().find(|(name, _)| *name == target.package));
            serde_json::json!({
                "wanted": target.wanted,
                "package": target.package,
                "command": shell(command),
                "installed": done.map(|(_, d)| d.ok),
                "status": done.and_then(|(_, d)| d.status),
                "message": done.map(|(_, d)| d.message.clone()),
            })
        })
        .collect();
    let text = serde_json::to_string_pretty(&serde_json::json!({ "packages": packages })).map_err(|e| e.to_string())?;
    let _ = writeln!(out, "{text}");
    Ok(Output::Done)
}
