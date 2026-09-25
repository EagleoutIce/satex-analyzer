use std::io::Write;
use std::path::Path;

use serde_json::{json, Value as Json};

use super::{parse_filter, Context, Output};
use crate::config::Config;
use crate::machine::Machine;
use crate::query::{self, Query, Record};

#[allow(clippy::too_many_arguments)]
pub fn run(
    context: &Context,
    name: Option<&str>,
    filter: Option<&str>,
    text: Option<&str>,
    prefix: Option<&str>,
    for_: Option<&str>,
    list: bool,
    _out: &mut impl Write,
) -> Result<Output, String> {
    if list {
        let names: Vec<Record> = Query::ALL
            .iter()
            .map(|(name, _)| {
                let mut record = Record::new();
                record.insert("name".into(), json!(*name));
                record
            })
            .collect();
        return Ok(Output::Records(names));
    }
    let name = name.ok_or("expected a query name; try --list")?;
    let query = Query::parse(name).ok_or_else(|| format!("unknown query `{name}`"))?;
    let filter = parse_filter(filter)?;
    if query == Query::Produces {
        let text = text.ok_or("the `produces` query needs the text to look for: --text TEXT")?;
        let found = query::produces(context.analysis, context.source, text);
        return Ok(Output::Records(found.into_iter().filter(|r| filter.accepts(r)).collect()));
    }
    if text.is_some() {
        return Err(format!("`--text` is what the `produces` query looks for, not `{name}`"));
    }
    if query == Query::Pgfkeys {
        let found = query::pgfkeys(context.analysis, prefix);
        return Ok(Output::Records(found.into_iter().filter(|r| filter.accepts(r)).collect()));
    }
    if query == Query::Options {
        let for_ = for_.ok_or("the `options` query needs a name: --for NAME")?;
        let found = query::options(context.analysis, for_);
        return Ok(Output::Records(found.into_iter().filter(|r| filter.accepts(r)).collect()));
    }
    if let Some(for_) = for_ {
        return Err(format!("`--for {for_}` is what the `options` query takes, not `{name}`"));
    }
    if prefix.is_some() {
        return Err(format!("`--prefix` narrows the `pgfkeys` query, not `{name}`"));
    }
    Ok(Output::Records(query::run(context.analysis, query, &filter)))
}

pub fn trace(context: &Context, filter: Option<&str>, interactive: bool, internal: bool, file: Option<&str>, out: &mut impl Write) -> Result<Output, String> {
    if interactive {
        step_through(context.analysis, file, internal, context.links, out);
        return Ok(Output::Done);
    }
    let analysis = context.analysis;
    let mut records = query::run(analysis, Query::Trace, &parse_filter(filter)?);
    if !internal {
        records.retain(|r| {
            let event = r.get("index").and_then(Json::as_u64).and_then(|i| analysis.trace.get(i as usize));
            event.is_some_and(|e| user_code(analysis, e))
        });
    }
    Ok(Output::Records(records))
}

/// Whether a trace event belongs to the document's own files, and is no
/// bookkeeping on an internal register (`\^^Bcount188`, named by a control
/// character) the kernel's allocators keep.
fn user_code(analysis: &crate::machine::Analysis, event: &crate::machine::Event) -> bool {
    query::origin(analysis, event.span.file) == "document"
        && !analysis.interner.name(event.name).starts_with(|c: char| c.is_control())
}

/// `trace --interactive`: one event at a time, with the macro's definition,
/// the arguments it took and the tokens the call put back in the stream.
fn step_through(analysis: &crate::machine::Analysis, file: Option<&str>, internal: bool, links: crate::render::Links, out: &mut impl Write) {
    use crate::facts::MeaningKind;
    use crate::machine::Step;
    use crate::tex::detokenize;
    let show = |toks: &[crate::tex::Token]| detokenize(toks, &analysis.interner);
    let total = analysis.trace.len();
    let ansi = matches!(anstream::stdout().current_choice(), anstream::ColorChoice::Always | anstream::ColorChoice::AlwaysAnsi);
    let bold = anstyle::Style::new().bold();
    let mut stdin = std::io::stdin().lock();
    let mut sources: std::collections::HashMap<String, Option<Vec<String>>> = Default::default();
    let mut run_on = false;
    // The events the default view hides since the last one shown.
    let mut hidden: Option<(u32, u32)> = None;
    let gray = anstyle::Style::new().dimmed();
    let flush = |hidden: &mut Option<(u32, u32)>, out: &mut dyn Write| {
        if let Some((from, to)) = hidden.take() {
            let (on, off) = if ansi { (format!("{gray}"), format!("{gray:#}")) } else { Default::default() };
            let _ = writeln!(
                out,
                "{on}[{}/{total}] -> [{}/{total}]  {} internal steps skipped (use --include-internal){off}",
                from + 1,
                to + 1,
                to - from + 1
            );
        }
    };
    // What the last command skips: the events inside a call, or up to a line
    // of the main file.
    enum Skip {
        Over(u16),
        NextLine(u32),
        ToLine(u32),
    }
    let mut skip: Option<Skip> = None;
    let mut here = 0;
    for event in &analysis.trace {
        let main = event.span.line > 0
            && match file {
                Some(name) => crate::config::names_file(analysis.file_name(event.span.file), name),
                None => event.span.file == analysis.main_file,
            };
        if !internal && !user_code(analysis, event) {
            hidden = Some(hidden.map_or((event.index, event.index), |(from, _)| (from, event.index)));
            continue;
        }
        let skipped = match skip {
            Some(Skip::Over(depth)) => event.depth > depth,
            Some(Skip::NextLine(line)) => !(main && event.span.line != line),
            Some(Skip::ToLine(line)) => !(main && event.span.line >= line),
            None => false,
        };
        if skipped {
            continue;
        }
        skip = None;
        if main {
            here = event.span.line;
        }
        let name = analysis.interner.cs(event.name);
        flush(&mut hidden, out);
        let file = analysis.short_name(event.span.file);
        let linked = crate::render::hyperlink(&name, &crate::render::file_url(analysis.file_name(event.span.file)), links);
        let shown = if ansi { format!("{bold}{linked}{bold:#}") } else { name.to_string() };
        let step = event.kind.as_str();
        let style = crate::render::step_style(event.kind);
        let step = if ansi { format!("{style}{step}{style:#}") } else { step.to_string() };
        let _ = writeln!(
            out,
            "[{}/{total}] {step} {shown}  {file}:{}:{}  depth {}",
            event.index + 1,
            event.span.line,
            event.span.col,
            event.depth
        );
        if let Some((head, tail)) = window(&mut sources, analysis.file_name(event.span.file), event.span.line, event.span.col, name.chars().count()) {
            let gray = anstyle::Style::new().dimmed();
            let (on, off) = if ansi { (format!("{gray}"), format!("{gray:#}")) } else { Default::default() };
            let lead = format!("l.{} ", event.span.line);
            let pad = " ".repeat(lead.chars().count() + head.chars().count());
            let _ = writeln!(out, "  {on}{lead}{head}{off}");
            let _ = writeln!(out, "  {on}{pad}{tail}{off}");
        }
        if let Some(detail) = &event.detail {
            let _ = writeln!(out, "  took: {detail}");
        }
        let defined = |node| analysis.facts.defs.iter().find(|d| d.node == node);
        match event.kind {
            Step::Expand => {
                let call = analysis.facts.expansions.iter().find(|e| e.span == event.span && e.name == event.name);
                if let Some(call) = call {
                    let def = match call.meaning {
                        MeaningKind::Macro(n) => defined(n),
                        _ => None,
                    };
                    if let Some(def) = def {
                        let _ = writeln!(
                            out,
                            "  because: {} defined by {} at {}:{}",
                            name,
                            analysis.interner.cs(def.by),
                            analysis.short_name(def.span.file),
                            def.span.line
                        );
                        if let Some(m) = &def.mac {
                            let _ = writeln!(out, "  macro:   {name}{} -> {}", m.parameter_text.render(&analysis.interner), show(&m.replacement_text));
                            let args: Vec<Vec<_>> = call.arguments.iter().map(|a| a.to_vec()).collect();
                            if !args.is_empty() {
                                let joined: Vec<String> = args.iter().map(|a| format!("{{{}}}", show(a))).collect();
                                let _ = writeln!(out, "  args:    {}", joined.join(" "));
                            }
                            if let Some(body) = crate::machine::substitute(m, &args, usize::MAX) {
                                let _ = writeln!(out, "  inserts: {}", show(&body));
                            }
                        }
                    } else {
                        let _ = writeln!(out, "  because: {name} is {}", call.meaning.as_str());
                    }
                }
            }
            Step::Define | Step::Assign => {
                if let Some(def) = analysis.facts.defs.iter().find(|d| d.span == event.span && d.name == event.name) {
                    let body = def.mac.as_ref().map(|m| show(&m.replacement_text)).unwrap_or_default();
                    let _ = writeln!(out, "  now:     {name} := {body}  ({} scope)", if def.global { "global" } else { "local" });
                }
            }
            Step::Branch => {
                let _ = writeln!(out, "  because: the test decided the {} arm", event.detail.as_deref().unwrap_or("undecided"));
            }
            _ => {}
        }
        if run_on {
            continue;
        }
        let _ = write!(out, "  [e]nter call, step [o]ver call, goto next [l]ine, [s]kip to <LINE>, [c]ontinue,  [q]uit\n> ");
        let _ = out.flush();
        let mut line = String::new();
        let read = std::io::BufRead::read_line(&mut stdin, &mut line);
        if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
            let _ = writeln!(out);
        } else if ansi {
            // Wipe the legend and the typed key: only the latest one shows.
            let _ = write!(out, "\x1b[2A\r\x1b[J");
        }
        match read {
            Ok(0) | Err(_) => run_on = true,
            Ok(_) => match line.trim() {
                "q" => return,
                "c" => run_on = true,
                "o" => skip = Some(Skip::Over(event.depth)),
                "l" => skip = Some(Skip::NextLine(here)),
                other => {
                    if let Some(line) = other.strip_prefix('s').and_then(|n| n.trim().parse().ok()) {
                        skip = Some(Skip::ToLine(line));
                    }
                }
            },
        }
    }
    flush(&mut hidden, out);
    let _ = writeln!(out, "-- end of the trace: {total} events --");
    let text = analysis.typeset.split_whitespace().collect::<Vec<_>>().join(" ");
    if !text.is_empty() {
        let (on, off) = if ansi { (format!("{bold}"), format!("{bold:#}")) } else { Default::default() };
        let _ = writeln!(out, "{on}output:{off}\n{text}");
    }
}

/// What TeX shows of an error's place: the source line up to the end of the
/// token at `col` (1-based, `width` characters), and the rest of the line.
/// Long lines are cut around the token.
fn window(
    cache: &mut std::collections::HashMap<String, Option<Vec<String>>>,
    path: &str,
    line: u32,
    col: u32,
    width: usize,
) -> Option<(String, String)> {
    let lines = cache
        .entry(path.to_string())
        .or_insert_with(|| std::fs::read_to_string(path).ok().map(|t| t.lines().map(str::to_string).collect()))
        .as_ref()?;
    let text: Vec<char> = lines.get(line.checked_sub(1)? as usize)?.chars().collect();
    let end = (col as usize).saturating_sub(1).saturating_add(width).min(text.len());
    let (from, to) = (end.saturating_sub(50), (end + 50).min(text.len()));
    let cut = |a: usize, b: usize| text[a..b].iter().collect::<String>();
    let head = format!("{}{}", if from > 0 { "…" } else { "" }, cut(from, end));
    let tail = format!("{}{}", cut(end, to), if to < text.len() { "…" } else { "" });
    Some((head, tail))
}


/// `satex query --request`: every query a JSON request names, run against
/// one analysis, printed as a JSON array of answers in the same order.  See
/// `doc/wiki/queries.md` for the request shape and examples.
pub fn run_request(
    request: &str,
    cfg: &Config,
    cli_file: Option<&Path>,
    out: &mut impl Write,
) -> Result<Output, String> {
    let text = read_request(request)?;
    let body: Json = serde_json::from_str(&text).map_err(|e| format!("{request}: {e}"))?;
    let answers = run_request_value(&body, cfg, cli_file)?;
    let _ = writeln!(out, "{}", serde_json::to_string_pretty(&answers).map_err(|e| e.to_string())?);
    Ok(Output::Done)
}

/// [`run_request`]'s body, taking the request already parsed rather than
/// read from a file: `satex lsp`'s `workspace/executeCommand` runs a
/// request an editor sent as an object, with no file of its own to read it
/// from.
pub fn run_request_value(
    body: &Json,
    cfg: &Config,
    cli_file: Option<&Path>,
) -> Result<Vec<Json>, String> {
    let mut cfg = cfg.clone();
    if let Some(overrides) = body.get("config").and_then(Json::as_object) {
        for (path, value) in overrides {
            cfg.set(path, &scalar(value))?;
        }
    }
    let file = body
        .get("file")
        .and_then(Json::as_str)
        .map(std::path::PathBuf::from)
        .or_else(|| cli_file.map(Path::to_path_buf))
        .ok_or("the request names no `file`, and none was given with -f")?;
    let source =
        std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let queries = body.get("queries").and_then(Json::as_array).cloned().unwrap_or_default();
    if queries.is_empty() {
        return Err("the request has no `queries` to run".into());
    }
    // The `options` query reads a key family out of a `\setkeys`-family
    // call's argument, which is only kept when this is on (see
    // `Command::wants_arguments`).
    if queries.iter().any(|q| q.get("type").and_then(Json::as_str) == Some("options")) {
        cfg.record_arguments = true;
    }
    let analysis = Machine::analyze(&source, Some(&file), &cfg);
    Ok(queries.iter().map(|entry| answer(&analysis, &source, entry)).collect())
}

fn read_request(request: &str) -> Result<String, String> {
    if request == "-" {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf).map_err(|e| e.to_string())?;
        return Ok(buf);
    }
    std::fs::read_to_string(request).map_err(|e| format!("{request}: {e}"))
}

/// A JSON value read the way `--set path=value` reads its own value: a
/// string used as written (no quotes to strip), anything else its JSON
/// spelling, which is valid YAML too — `Config::set` parses either.
fn scalar(value: &Json) -> String {
    match value {
        Json::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// One request query answered: `{"type": …, "records": […]}`, or
/// `{"type": …, "error": …}` when it named something wrong — a bad query
/// keeps the rest of the batch running rather than failing all of it.
fn answer(analysis: &crate::machine::Analysis, source: &str, entry: &Json) -> Json {
    let kind = entry.get("type").and_then(Json::as_str).unwrap_or_default();
    let error = |message: String| json!({ "type": kind, "error": message });
    let strings = |key: &str| -> Vec<String> {
        match entry.get(key) {
            Some(Json::Array(items)) => {
                items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()
            }
            Some(Json::String(s)) => vec![s.clone()],
            _ => Vec::new(),
        }
    };
    let filter = match entry.get("filter").and_then(Json::as_str).map(query::Filter::parse) {
        Some(Ok(filter)) => filter,
        Some(Err(message)) => return error(message),
        None => query::Filter::Always,
    };
    let flag = |key: &str| entry.get(key).and_then(Json::as_bool).unwrap_or(false);
    let records = match kind {
        "explain" => {
            let at = match entry.get("at").and_then(Json::as_str).map(query::parse_explain_at).transpose() {
                Ok(at) => at,
                Err(message) => return error(message),
            };
            return match query::explain_at(analysis, &strings("names"), flag("all"), at.as_ref()) {
                Ok((records, hints)) => json!({ "type": kind, "records": records, "hints": hints }),
                Err(message) => error(message),
            };
        }
        "controls" => {
            let names = strings("names");
            let mut out = if names.is_empty() {
                query::switches(analysis, flag("all"))
            } else {
                let mut out = Vec::new();
                for name in &names {
                    for mut record in query::controls(analysis, name) {
                        record.insert("for".into(), json!(name.trim_start_matches('\\')));
                        out.push(record);
                    }
                }
                out
            };
            query::in_document_order(analysis, &mut out);
            out
        }
        "slice" => {
            let at = match entry.get("at").and_then(Json::as_str).map(super::parse_place) {
                Some(Ok(at)) => Some(at),
                Some(Err(message)) => return error(message),
                None => None,
            };
            let names = strings("names");
            if names.is_empty() && at.is_none() {
                return error("`slice` needs `names` or `at`".into());
            }
            let direction =
                if flag("forward") { query::Direction::Forward } else { query::Direction::Backward };
            query::slice(analysis, &names, at.as_ref(), direction)
        }
        "options" => match entry.get("name").and_then(Json::as_str) {
            Some(name) => query::options(analysis, name),
            None => return error("`options` needs `name`".into()),
        },
        "produces" => match entry.get("text").and_then(Json::as_str) {
            Some(text) => query::produces(analysis, source, text),
            None => return error("`produces` needs `text`".into()),
        },
        "pgfkeys" => query::pgfkeys(analysis, entry.get("prefix").and_then(Json::as_str)),
        other => match Query::parse(other) {
            Some(query) => query::run(analysis, query, &query::Filter::Always),
            None => return error(format!("unknown query type `{other}`")),
        },
    };
    let records: Vec<Record> = records.into_iter().filter(|r| filter.accepts(r)).collect();
    json!({ "type": kind, "records": records })
}
