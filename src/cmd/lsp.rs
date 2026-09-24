//! `satex lsp`: resolves the transport and config overrides from the CLI,
//! then hands off to [`crate::lsp::serve`], which is the actual server.

use std::path::Path;

use crate::config::Config;

pub fn run(cfg: &Config, file: Option<&Path>, port: Option<u16>, host: Option<&str>) -> Result<(), String> {
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
    crate::lsp::serve(cfg, root)
}
