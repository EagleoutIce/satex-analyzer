use std::io::Write;

use super::{Context, Format, Output};

pub fn run(context: &Context, out: &mut impl Write) -> Result<Output, String> {
    let analysis = context.analysis;
    let files = analysis.file_names();
    let text = match context.format {
        Format::Json => serde_json::to_string_pretty(&analysis.graph.to_json(&analysis.interner, &files))
            .map_err(|e| e.to_string())?,
        _ => analysis.graph.to_dot(&analysis.interner, &files),
    };
    let _ = writeln!(out, "{text}");
    Ok(Output::Done)
}
