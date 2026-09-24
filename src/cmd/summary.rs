use std::io::Write;

use super::{Context, Format, Output};
use crate::{query, render};

pub fn run(
    context: &Context,
    all: bool,
    depth: Option<usize>,
    out: &mut impl Write,
) -> Result<Output, String> {
    let mut detail = if all { render::Detail::full() } else { render::Detail::brief() };
    if let Some(depth) = depth {
        detail.depth = depth;
    }
    let text = match context.format {
        Format::Json => serde_json::to_string_pretty(&query::summary_json(context.analysis))
            .map_err(|e| e.to_string())?,
        _ => render::summary(context.analysis, &query::summary(context.analysis), detail, context.links),
    };
    let _ = write!(out, "{text}");
    Ok(Output::Done)
}
