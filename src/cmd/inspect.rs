use std::io::Write;
use std::path::Path;

use super::{parse_filter, parse_place, parse_position, Context, Format, Output};
use crate::query;

pub fn scope(
    context: &Context,
    at: Option<&str>,
    filter: Option<&str>,
    all: bool,
) -> Result<Output, String> {
    let position = at.map(parse_position).transpose()?;
    let filter = parse_filter(filter)?;
    Ok(Output::Records(
        query::scope(context.analysis, position, all)
            .into_iter()
            .filter(|record| filter.accepts(record))
            .collect(),
    ))
}

pub fn explain(context: &Context, names: &[String], all: bool, at: Option<&str>) -> Result<Output, String> {
    let at = at.map(query::parse_explain_at).transpose()?;
    let (records, hints) = query::explain_at(context.analysis, names, all, at.as_ref())?;
    Ok(match hints.is_empty() {
        true => Output::Detail(records),
        false => Output::DetailPartial(records, hints.join("; ")),
    })
}

/// Query knobs for [`slice`], bundled to keep the function's argument count down.
pub struct SliceQuery<'a> {
    pub at: Option<&'a str>,
    pub where_: Option<&'a str>,
    pub forward: bool,
    pub list: bool,
}

pub fn slice(
    context: &Context,
    names: &[String],
    query: SliceQuery,
    out_dir: Option<&Path>,
    out: &mut impl Write,
) -> Result<Output, String> {
    let position = query.at.map(parse_place).transpose()?;
    let filter = query.where_.map(query::Filter::parse).transpose()?;
    if names.is_empty() && position.is_none() && filter.is_none() {
        return Err("give a name to slice on, --at [FILE:]LINE:COL, or --where EXPR".into());
    }
    let direction =
        if query.forward { query::Direction::Forward } else { query::Direction::Backward };
    let records = match &filter {
        Some(filter) => query::slice_matching(context.analysis, filter, direction),
        None => query::slice(context.analysis, names, position.as_ref(), direction),
    };
    if records.is_empty() {
        let what = match (names.is_empty(), &position, &filter) {
            (false, ..) => format!("`{}`", names.join("`, `")),
            (true, Some(position), _) => format!("position {position}"),
            (true, None, Some(_)) => format!("`--where {}`", query.where_.unwrap_or_default()),
            (true, None, None) => "the criterion".into(),
        };
        return Ok(Output::Partial(
            records,
            format!("{what} names nothing the run reached, so the slice is empty"),
        ));
    }
    if let Some(dir) = out_dir {
        return write_project(context, &records, dir, out);
    }
    if !query.list && context.format == Format::Text {
        let _ = write!(out, "{}", query::reconstruct(context.analysis, &records, context.source));
        return Ok(Output::Done);
    }
    Ok(Output::Records(records))
}

/// `slice --out DIR`: the reconstruction as the sliced main file, plus every
/// local package, class, graphics or bibliography file it names, with paths
/// preserved relative to the main file's own directory.
fn write_project(
    context: &Context,
    records: &[query::Record],
    dir: &Path,
    out: &mut impl Write,
) -> Result<Output, String> {
    let text = query::reconstruct(context.analysis, records, context.source);
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let main_path = Path::new(context.analysis.file_name(context.analysis.main_file));
    let main_name = main_path.file_name().map(std::path::PathBuf::from).unwrap_or_else(|| "main.tex".into());
    std::fs::write(dir.join(&main_name), &text)
        .map_err(|e| format!("cannot write {}: {e}", dir.join(&main_name).display()))?;
    let files = query::project_files(context.analysis, &text);
    for (source, relative) in &files {
        let dest = dir.join(relative);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        std::fs::copy(source, &dest)
            .map_err(|e| format!("cannot copy {} to {}: {e}", source.display(), dest.display()))?;
    }
    let _ = writeln!(
        out,
        "wrote {} and {} project file{} to {}",
        main_name.display(),
        files.len(),
        if files.len() == 1 { "" } else { "s" },
        dir.display()
    );
    Ok(Output::Done)
}

pub fn controls(context: &Context, names: &[String], all: bool) -> Result<Output, String> {
    let mut records = if names.is_empty() {
        query::switches(context.analysis, all)
    } else {
        // Several names answer in one table: each row keeps the switch or
        // option it belongs to in `for`, so one answer is still traceable
        // to the name that asked for it.
        let mut out = Vec::new();
        for name in names {
            for mut record in query::controls(context.analysis, name) {
                record.insert("for".into(), serde_json::json!(name.trim_start_matches('\\')));
                out.push(record);
            }
        }
        out
    };
    query::in_document_order(context.analysis, &mut records);
    Ok(match (names.is_empty(), all) {
        (true, false) => Output::Partial(
            records,
            "`--all` adds the switches and options the kernel and the packages bring".into(),
        ),
        _ => Output::Records(records),
    })
}
