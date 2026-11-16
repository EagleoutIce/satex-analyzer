//! `satex lsp`: resolves the transport and config overrides from the CLI,
//! then hands off to [`crate::lsp::serve`], which is the actual server.

use std::path::Path;

use crate::project::config::Config;

#[cfg_attr(target_os = "emscripten", allow(unused_variables))]
pub fn run(
    cfg: &Config,
    file: Option<&Path>,
    port: Option<u16>,
    host: Option<&str>,
    lines: bool,
) -> Result<(), String> {
    let mut cfg = cfg.clone();
    if let Some(port) = port {
        cfg.lsp.port = Some(port);
    }
    if let Some(host) = host {
        cfg.lsp.host = host.to_string();
    }
    let root = file
        .and_then(Path::parent)
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    #[cfg(not(target_os = "emscripten"))]
    if lines {
        crate::lsp::lines(cfg, root, file.map(Path::to_path_buf));
        return Ok(());
    }
    // stdin closed before `initialize`: say so, not the channel's own words
    crate::lsp::serve(cfg, root, file.map(Path::to_path_buf)).map_err(|e| match e.as_str() {
        "disconnected channel" => "the language client closed the connection before `initialize`".to_string(),
        _ => e,
    })
}
