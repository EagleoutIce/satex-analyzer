use std::io::Write;

use super::{Context, Format, Output, parse_filter};
use crate::lint::apply::{self, Outcome};
use crate::query::Record;
use crate::{lint, render};

/// What `satex lint` does with the fixes: `--fix` writes them, `--diff`
/// prints them, `--unsafe-fixes` lets either take the unsafe ones too.
#[derive(Clone, Copy, Default)]
pub struct Fixing {
    pub fix: bool,
    pub unsafe_fixes: bool,
    pub diff: bool,
}

pub fn run(
    context: &Context,
    filter: Option<&str>,
    all: bool,
    rules: bool,
    explain: Option<&str>,
    fixing: Fixing,
    out: &mut impl Write,
) -> Result<Output, String> {
    if rules {
        return Ok(Output::Records(lint::rules()));
    }
    if let Some(code) = explain {
        let rule = lint::explanation(code).ok_or_else(|| format!("no rule called `{code}`"))?;
        let text = match context.format {
            Format::Json => serde_json::to_string_pretty(&serde_json::json!({
                "code": rule.code,
                "category": rule.category.as_str(),
                "severity": rule.severity.as_str(),
                "summary": rule.summary,
                "explanation": rule.explanation,
            }))
            .map_err(|e| e.to_string())?,
            _ => format!(
                "{}  {}\n{}\n\n{}",
                render::heading(rule.code),
                render::label(&format!("{} / {}", rule.category.as_str(), rule.severity.as_str())),
                rule.summary,
                rule.explanation
            ),
        };
        let _ = writeln!(out, "{text}");
        return Ok(Output::Done);
    }
    let filter = parse_filter(filter)?;
    let select = |record: &Record| (all || record["origin"] == "document") && filter.accepts(record);
    let records: Vec<Record> = lint::lint(context.analysis).into_iter().filter(|r| select(r)).collect();
    if !fixing.fix && !fixing.diff {
        return Ok(Output::Lint(records));
    }
    let analysis = context.analysis;
    let main = analysis.file_name(analysis.main_file).to_string();
    if !std::path::Path::new(&main).is_file() {
        return Err("`--fix` and `--diff` change files: name the document with -f".into());
    }
    let outcome = apply::fixpoint(&main, &analysis.settings, records, &select, fixing.unsafe_fixes);
    let summary = summary(&outcome, fixing);
    if fixing.diff {
        for (path, (old, new)) in &outcome.changed {
            let _ = write!(out, "{}", apply::unified(path, old, new));
        }
        anstream::eprintln!("{summary}");
        return Ok(Output::Done);
    }
    apply::write(&outcome)?;
    if context.format.is_text() {
        let _ = write!(out, "{}", render::lint(&outcome.remaining, context.links));
        let _ = writeln!(out, "{}", render::label(&summary));
        return Ok(Output::Done);
    }
    anstream::eprintln!("{summary}");
    Ok(Output::Lint(outcome.remaining))
}

/// `fixed 3 in 2 rounds, 4 remaining (1 more with --unsafe-fixes)`, and a
/// line for every fix that was put off because it overlapped another.
fn summary(outcome: &Outcome, fixing: Fixing) -> String {
    let verb = if fixing.diff { "would fix" } else { "fixed" };
    let mut text = format!(
        "{verb} {} in {} {}, {} remaining",
        outcome.applied,
        outcome.rounds,
        if outcome.rounds == 1 { "round" } else { "rounds" },
        outcome.remaining.len()
    );
    let fixable = |unsafe_fixes| outcome.remaining.iter().filter(|r| apply::applicable(r, unsafe_fixes)).count();
    let safe = fixable(false);
    let more = fixable(true) - safe;
    if safe > 0 {
        text.push_str(&format!(", {safe} still fixable after {} rounds", apply::MAX_ROUNDS));
    }
    if more > 0 && !fixing.unsafe_fixes {
        text.push_str(&format!(" ({more} more with --unsafe-fixes)"));
    }
    for skipped in &outcome.skipped {
        text.push_str(&format!(
            "\n{} at {} overlaps {} and was left for the next round",
            skipped.code, skipped.place, skipped.by
        ));
    }
    text
}
