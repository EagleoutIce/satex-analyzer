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

pub fn trace(context: &Context, filter: Option<&str>) -> Result<Output, String> {
    Ok(Output::Records(query::run(context.analysis, Query::Trace, &parse_filter(filter)?)))
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
