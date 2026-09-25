//! `satex lsp`: a thin, synchronous Language Server Protocol facade over the
//! queries and lints [`crate::cmd`] already exposes on the command line —
//! no analysis logic of its own.
//!
//! One "engine" thread owns the interpreter state and every [`Analysis`] it
//! builds.  [`crate::machine::Machine`] uses `Rc` internally (the
//! interpreter is one machine, run on one thread — see [`crate::overlay`]),
//! so an `Analysis` cannot cross threads; instead the engine thread answers
//! every request itself and only ever sends back plain, `Send` values
//! (`lsp_types` responses, JSON).  The connection thread (stdio or TCP) only
//! frames JSON-RPC messages and forwards them to the engine.
//!
//! `didOpen`/`didChange`/`didSave` mark a document dirty; the engine
//! re-analyzes it after `lsp.debounce_ms` of quiet (so fast typing
//! coalesces into one run) rather than on every keystroke.  An edit that
//! lands while a document is still waiting out its debounce replaces the
//! pending text, which is this facade's form of "cancelling" a stale run:
//! nothing has started yet, so there is nothing to interrupt.

// lsp_types::Uri's interior mutability is a cache with no bearing on Eq/Hash; safe as a map key.
#![allow(clippy::mutable_key_type)]

mod handlers;

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use crossbeam_channel::{select, Receiver, RecvTimeoutError, Sender};
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, Response};
use lsp_types::{
    CodeActionParams, CodeActionProviderCapability, CompletionOptions, CompletionParams,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, DocumentSymbolParams, ExecuteCommandOptions, ExecuteCommandParams,
    DocumentHighlightParams, GotoDefinitionParams, Hover, HoverParams, HoverProviderCapability, Location, OneOf, Position,
    PublishDiagnosticsParams, ReferenceParams, RenameOptions, RenameParams, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, Uri,
};
use serde_json::Value as Json;

use crate::config::Config;
use crate::lint::fix::Text;
use crate::machine::{Analysis, Machine};

/// The one `workspace/executeCommand` this server answers: its single
/// argument is a `satex query --request` body (`doc/wiki/queries.md`),
/// minus `file` when the request means the document that sent it.
const EXECUTE_COMMAND: &str = "satex/query";

pub fn serve(cfg: Config, root: PathBuf) -> Result<(), String> {
    let (connection, io_threads) = match cfg.lsp.port {
        Some(port) => Connection::listen((cfg.lsp.host.as_str(), port)).map_err(|e| e.to_string())?,
        None => Connection::stdio(),
    };
    connection
        .initialize(serde_json::to_value(server_capabilities()).unwrap())
        .map_err(|e| e.to_string())?;

    let (engine_tx, engine_rx) = crossbeam_channel::unbounded::<EngineMsg>();
    let (diagnostics_tx, diagnostics_rx) = crossbeam_channel::unbounded::<PublishDiagnosticsParams>();
    let debounce = Duration::from_millis(cfg.lsp.debounce_ms.max(1));
    let engine = std::thread::spawn(move || engine_loop(engine_rx, diagnostics_tx, cfg, root, debounce));

    let result = main_loop(&connection, &engine_tx, &diagnostics_rx);
    let _ = engine_tx.send(EngineMsg::Shutdown);
    let _ = engine.join();
    // The writer thread only stops once every `Sender` feeding it is gone
    // (`Connection::sender` among them), so it has to be dropped before
    // `io_threads.join()` can return.
    drop(connection);
    io_threads.join().map_err(|e| e.to_string())?;
    result
}

fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        document_highlight_provider: Some(OneOf::Left(true)),
        workspace_symbol_provider: Some(OneOf::Left(true)),
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: Default::default(),
        })),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec!["\\".into(), "{".into()]),
            ..Default::default()
        }),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        execute_command_provider: Some(ExecuteCommandOptions {
            commands: vec![EXECUTE_COMMAND.to_string()],
            ..Default::default()
        }),
        ..Default::default()
    }
}

// --- connection thread: frames JSON-RPC, forwards to the engine --------

fn main_loop(
    connection: &Connection,
    engine_tx: &Sender<EngineMsg>,
    diagnostics_rx: &Receiver<PublishDiagnosticsParams>,
) -> Result<(), String> {
    loop {
        select! {
            recv(connection.receiver) -> msg => {
                match msg {
                    Ok(Message::Request(req)) => {
                        if connection.handle_shutdown(&req).map_err(|e| e.to_string())? {
                            return Ok(());
                        }
                        handle_request(connection, engine_tx, req);
                    }
                    Ok(Message::Notification(not)) => handle_notification(engine_tx, not),
                    Ok(Message::Response(_)) => {}
                    Err(_) => return Ok(()),
                }
            }
            recv(diagnostics_rx) -> params => {
                if let Ok(params) = params {
                    let note = Notification::new("textDocument/publishDiagnostics".into(), params);
                    let _ = connection.sender.send(Message::Notification(note));
                }
            }
        }
    }
}

fn handle_request(connection: &Connection, engine_tx: &Sender<EngineMsg>, req: Request) {
    let id = req.id.clone();
    let method = req.method.clone();
    let outcome = dispatch(engine_tx, req);
    let response = match outcome {
        Ok(value) => Response::new_ok(id, value),
        Err(message) => {
            Response::new_err(id, ErrorCode::InternalError as i32, format!("{method}: {message}"))
        }
    };
    let _ = connection.sender.send(Message::Response(response));
}

/// Sends `msg` to the engine and blocks on its one-shot reply — the engine
/// answers every read from its own thread rather than handing out its
/// `Analysis`, so this is the only way to ask it anything.
fn ask<T>(engine_tx: &Sender<EngineMsg>, build: impl FnOnce(Sender<T>) -> EngineMsg) -> Result<T, String> {
    let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
    engine_tx.send(build(reply_tx)).map_err(|_| "the analysis engine has stopped".to_string())?;
    reply_rx.recv().map_err(|_| "the analysis engine has stopped".to_string())
}

fn dispatch(engine_tx: &Sender<EngineMsg>, req: Request) -> Result<Json, String> {
    let params = req.params;
    match req.method.as_str() {
        "textDocument/hover" => {
            let p: HoverParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let uri = p.text_document_position_params.text_document.uri;
            let pos = p.text_document_position_params.position;
            let hover = ask(engine_tx, |reply| EngineMsg::Hover(uri, pos, reply))?;
            Ok(serde_json::to_value(hover).unwrap())
        }
        "textDocument/definition" => {
            let p: GotoDefinitionParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let uri = p.text_document_position_params.text_document.uri;
            let pos = p.text_document_position_params.position;
            let def = ask(engine_tx, |reply| EngineMsg::Definition(uri, pos, reply))?;
            Ok(serde_json::to_value(def).unwrap())
        }
        "textDocument/references" => {
            let p: ReferenceParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let uri = p.text_document_position.text_document.uri;
            let pos = p.text_document_position.position;
            let declarations = p.context.include_declaration;
            let refs = ask(engine_tx, |reply| EngineMsg::References(uri, pos, declarations, reply))?;
            Ok(serde_json::to_value(refs).unwrap())
        }
        "textDocument/completion" => {
            let p: CompletionParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let uri = p.text_document_position.text_document.uri;
            let pos = p.text_document_position.position;
            let items = ask(engine_tx, |reply| EngineMsg::Completion(uri, pos, reply))?;
            Ok(serde_json::to_value(items).unwrap())
        }
        "textDocument/documentSymbol" => {
            let p: DocumentSymbolParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let symbols = ask(engine_tx, |reply| EngineMsg::DocumentSymbol(p.text_document.uri, reply))?;
            Ok(serde_json::to_value(symbols).unwrap())
        }
        "textDocument/documentHighlight" => {
            let p: DocumentHighlightParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let uri = p.text_document_position_params.text_document.uri;
            let pos = p.text_document_position_params.position;
            let out = ask(engine_tx, |reply| EngineMsg::Highlight(uri, pos, reply))?;
            Ok(serde_json::to_value(out).unwrap())
        }
        "textDocument/prepareRename" => {
            let p: lsp_types::TextDocumentPositionParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let out = ask(engine_tx, |reply| EngineMsg::PrepareRename(p.text_document.uri, p.position, reply))?;
            Ok(serde_json::to_value(out).unwrap())
        }
        "textDocument/rename" => {
            let p: RenameParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let uri = p.text_document_position.text_document.uri;
            let pos = p.text_document_position.position;
            let edit = ask(engine_tx, |reply| EngineMsg::Rename(uri, pos, p.new_name, reply))??;
            Ok(serde_json::to_value(edit).unwrap())
        }
        "workspace/symbol" => {
            let p: lsp_types::WorkspaceSymbolParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let symbols = ask(engine_tx, |reply| EngineMsg::WorkspaceSymbol(p.query, reply))?;
            Ok(serde_json::to_value(symbols).unwrap())
        }
        "textDocument/codeAction" => {
            let p: CodeActionParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let actions = ask(engine_tx, |reply| EngineMsg::CodeAction(p.text_document.uri, reply))?;
            Ok(Json::Array(actions))
        }
        "workspace/executeCommand" => {
            let p: ExecuteCommandParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            if p.command != EXECUTE_COMMAND {
                return Err(format!("unknown command `{}`; this server only runs `{EXECUTE_COMMAND}`", p.command));
            }
            let body = p
                .arguments
                .into_iter()
                .next()
                .ok_or("expected one argument: a `satex query --request` body")?;
            ask(engine_tx, |reply| EngineMsg::ExecuteCommand(body, reply))?.map(Json::Array)
        }
        other => Err(format!("method not found: {other}")),
    }
}

fn handle_notification(engine_tx: &Sender<EngineMsg>, not: Notification) {
    let sent = match not.method.as_str() {
        "textDocument/didOpen" => serde_json::from_value::<DidOpenTextDocumentParams>(not.params).ok().and_then(
            |p| {
                let uri = p.text_document.uri;
                let path = handlers::uri_to_path(&uri)?;
                Some(EngineMsg::Open(uri, path, p.text_document.text))
            },
        ),
        // Full sync only (see `server_capabilities`): the whole text
        // arrives in the one change, so the latest one wins.
        "textDocument/didChange" => {
            serde_json::from_value::<DidChangeTextDocumentParams>(not.params).ok().and_then(|p| {
                p.content_changes.into_iter().next_back().map(|c| EngineMsg::Change(p.text_document.uri, c.text))
            })
        }
        "textDocument/didSave" => serde_json::from_value::<DidSaveTextDocumentParams>(not.params)
            .ok()
            .and_then(|p| p.text.map(|text| EngineMsg::Change(p.text_document.uri, text))),
        "textDocument/didClose" => serde_json::from_value::<DidCloseTextDocumentParams>(not.params)
            .ok()
            .map(|p| EngineMsg::Close(p.text_document.uri)),
        _ => None,
    };
    if let Some(msg) = sent {
        let _ = engine_tx.send(msg);
    }
}

// --- engine thread: owns every Analysis, never sends one out -----------

enum EngineMsg {
    Open(Uri, PathBuf, String),
    Change(Uri, String),
    Close(Uri),
    Hover(Uri, Position, Sender<Option<Hover>>),
    Definition(Uri, Position, Sender<Option<lsp_types::GotoDefinitionResponse>>),
    References(Uri, Position, bool, Sender<Option<Vec<Location>>>),
    Completion(Uri, Position, Sender<lsp_types::CompletionResponse>),
    DocumentSymbol(Uri, Sender<Option<lsp_types::DocumentSymbolResponse>>),
    Highlight(Uri, Position, Sender<Option<Vec<lsp_types::DocumentHighlight>>>),
    PrepareRename(Uri, Position, Sender<Option<lsp_types::PrepareRenameResponse>>),
    Rename(Uri, Position, String, Sender<Result<lsp_types::WorkspaceEdit, String>>),
    WorkspaceSymbol(String, Sender<Vec<lsp_types::SymbolInformation>>),
    CodeAction(Uri, Sender<Vec<Json>>),
    ExecuteCommand(Json, Sender<Result<Vec<Json>, String>>),
    Shutdown,
}

struct Doc {
    text: String,
    path: PathBuf,
    analysis: Option<Analysis>,
}

fn engine_loop(
    rx: Receiver<EngineMsg>,
    diagnostics_tx: Sender<PublishDiagnosticsParams>,
    cfg: Config,
    root: PathBuf,
    debounce: Duration,
) {
    let mut docs: HashMap<Uri, Doc> = HashMap::new();
    let mut actions: HashMap<Uri, Vec<Json>> = HashMap::new();
    let mut dirty: Vec<Uri> = Vec::new();
    loop {
        let received = if dirty.is_empty() {
            rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
        } else {
            rx.recv_timeout(debounce)
        };
        let msg = match received {
            Ok(msg) => msg,
            Err(RecvTimeoutError::Timeout) => {
                for uri in dirty.drain(..) {
                    analyze(&mut docs, &mut actions, &uri, &cfg, &diagnostics_tx);
                }
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => return,
        };
        match msg {
            EngineMsg::Shutdown => return,
            EngineMsg::Open(uri, path, text) => {
                docs.insert(uri.clone(), Doc { text, path, analysis: None });
                if !dirty.contains(&uri) {
                    dirty.push(uri);
                }
            }
            EngineMsg::Change(uri, text) => {
                if let Some(doc) = docs.get_mut(&uri) {
                    doc.text = text;
                    doc.analysis = None;
                    if !dirty.contains(&uri) {
                        dirty.push(uri);
                    }
                }
            }
            EngineMsg::Close(uri) => {
                docs.remove(&uri);
                actions.remove(&uri);
                dirty.retain(|d| d != &uri);
            }
            EngineMsg::Hover(uri, pos, reply) => {
                ensure_analyzed(&mut docs, &mut actions, &uri, &cfg, &diagnostics_tx, &mut dirty);
                let result = with_doc(&docs, &uri, |analysis, text| handlers::hover(analysis, text, pos));
                let _ = reply.send(result.flatten());
            }
            EngineMsg::Definition(uri, pos, reply) => {
                ensure_analyzed(&mut docs, &mut actions, &uri, &cfg, &diagnostics_tx, &mut dirty);
                let result = with_doc(&docs, &uri, |analysis, text| handlers::definition(analysis, text, pos));
                let _ = reply.send(result.flatten());
            }
            EngineMsg::References(uri, pos, decls, reply) => {
                ensure_analyzed(&mut docs, &mut actions, &uri, &cfg, &diagnostics_tx, &mut dirty);
                let path = docs.get(&uri).map(|d| d.path.display().to_string()).unwrap_or_default();
                let result =
                    with_doc(&docs, &uri, |analysis, text| handlers::references(analysis, text, &path, pos, decls));
                let _ = reply.send(result.flatten());
            }
            EngineMsg::Completion(uri, pos, reply) => {
                ensure_analyzed(&mut docs, &mut actions, &uri, &cfg, &diagnostics_tx, &mut dirty);
                let result = with_doc(&docs, &uri, |analysis, text| handlers::completion(analysis, text, pos));
                let _ = reply.send(result.unwrap_or(lsp_types::CompletionResponse::Array(Vec::new())));
            }
            EngineMsg::DocumentSymbol(uri, reply) => {
                ensure_analyzed(&mut docs, &mut actions, &uri, &cfg, &diagnostics_tx, &mut dirty);
                let path = docs.get(&uri).map(|d| d.path.display().to_string());
                let result = match path {
                    Some(path) => with_doc(&docs, &uri, |analysis, text| {
                        handlers::document_symbols(analysis, &path, text)
                            .map(lsp_types::DocumentSymbolResponse::Flat)
                    })
                    .flatten(),
                    None => None,
                };
                let _ = reply.send(result);
            }
            EngineMsg::Highlight(uri, pos, reply) => {
                ensure_analyzed(&mut docs, &mut actions, &uri, &cfg, &diagnostics_tx, &mut dirty);
                let path = docs.get(&uri).map(|d| d.path.display().to_string()).unwrap_or_default();
                let result = with_doc(&docs, &uri, |analysis, text| handlers::highlights(analysis, text, &path, pos));
                let _ = reply.send(result.flatten());
            }
            EngineMsg::PrepareRename(uri, pos, reply) => {
                ensure_analyzed(&mut docs, &mut actions, &uri, &cfg, &diagnostics_tx, &mut dirty);
                let path = docs.get(&uri).map(|d| d.path.display().to_string()).unwrap_or_default();
                let result = with_doc(&docs, &uri, |analysis, text| handlers::prepare_rename(analysis, text, &path, pos));
                let _ = reply.send(result.flatten());
            }
            EngineMsg::Rename(uri, pos, new_name, reply) => {
                ensure_analyzed(&mut docs, &mut actions, &uri, &cfg, &diagnostics_tx, &mut dirty);
                let path = docs.get(&uri).map(|d| d.path.display().to_string()).unwrap_or_default();
                let result = with_doc(&docs, &uri, |analysis, text| handlers::rename(analysis, text, &path, pos, &new_name))
                    .unwrap_or_else(|| Err("document is not open".to_string()));
                let _ = reply.send(result);
            }
            EngineMsg::WorkspaceSymbol(query, reply) => {
                let uri = docs.keys().next().cloned();
                if let Some(uri) = &uri {
                    ensure_analyzed(&mut docs, &mut actions, uri, &cfg, &diagnostics_tx, &mut dirty);
                }
                let result = uri
                    .and_then(|u| with_doc(&docs, &u, |analysis, _| handlers::workspace_symbols(analysis, &query)))
                    .unwrap_or_default();
                let _ = reply.send(result);
            }
            EngineMsg::CodeAction(uri, reply) => {
                ensure_analyzed(&mut docs, &mut actions, &uri, &cfg, &diagnostics_tx, &mut dirty);
                let _ = reply.send(actions.get(&uri).cloned().unwrap_or_default());
            }
            EngineMsg::ExecuteCommand(body, reply) => {
                let active = docs.values().next().map(|d| d.path.clone()).unwrap_or_else(|| root.clone());
                let result = crate::cmd::query::run_request_value(&body, &cfg, Some(&active));
                let _ = reply.send(result);
            }
        }
    }
}

/// Analyzes `uri` right away when it has no [`Analysis`] yet, so the first
/// request after `didOpen` does not have to wait out the debounce.
fn ensure_analyzed(
    docs: &mut HashMap<Uri, Doc>,
    actions: &mut HashMap<Uri, Vec<Json>>,
    uri: &Uri,
    cfg: &Config,
    diagnostics_tx: &Sender<PublishDiagnosticsParams>,
    dirty: &mut Vec<Uri>,
) {
    if docs.get(uri).is_some_and(|d| d.analysis.is_some()) {
        return;
    }
    analyze(docs, actions, uri, cfg, diagnostics_tx);
    dirty.retain(|d| d != uri);
}

/// Re-analyzes `uri`'s current text, publishes its diagnostics, and stashes
/// their quick-fix code actions for later `textDocument/codeAction` calls.
/// Every other open document is set in the overlay first, so an `\input` of
/// one sees its unsaved text too (`crate::overlay`).
fn analyze(
    docs: &mut HashMap<Uri, Doc>,
    actions: &mut HashMap<Uri, Vec<Json>>,
    uri: &Uri,
    cfg: &Config,
    diagnostics_tx: &Sender<PublishDiagnosticsParams>,
) {
    let Some((text, path)) = docs.get(uri).map(|d| (d.text.clone(), d.path.clone())) else { return };
    for (other_uri, other) in docs.iter() {
        if other_uri != uri {
            crate::overlay::set(&other.path, other.text.clone());
        }
    }
    let analysis = Machine::analyze(&text, Some(&path), cfg);
    let mut groups = handlers::diagnostics_json(&analysis);
    let own_uri = handlers::path_uri(&path.display().to_string());
    if !groups.iter().any(|g| g.get("uri").and_then(Json::as_str) == Some(own_uri.as_str())) {
        groups.push(serde_json::json!({ "uri": own_uri, "diagnostics": [], "codeActions": [] }));
    }
    if let Some(doc) = docs.get_mut(uri) {
        doc.analysis = Some(analysis);
    }
    for group in groups {
        let Some(group_uri) = group.get("uri").and_then(Json::as_str).and_then(|s| s.parse::<Uri>().ok()) else {
            continue;
        };
        let diagnostics = group
            .get("diagnostics")
            .cloned()
            .and_then(|d| serde_json::from_value(d).ok())
            .unwrap_or_default();
        let code_actions = group.get("codeActions").and_then(|a| a.as_array()).cloned().unwrap_or_default();
        actions.insert(group_uri.clone(), code_actions);
        let _ = diagnostics_tx.send(PublishDiagnosticsParams { uri: group_uri, diagnostics, version: None });
    }
}

/// Runs `f` against `uri`'s [`Analysis`] and a [`Text`] of its current
/// buffer, or `None` when the document is not open or has no analysis yet.
fn with_doc<T>(docs: &HashMap<Uri, Doc>, uri: &Uri, f: impl FnOnce(&Analysis, &Text) -> T) -> Option<T> {
    let doc = docs.get(uri)?;
    let analysis = doc.analysis.as_ref()?;
    let text = Text::new(doc.text.clone());
    Some(f(analysis, &text))
}
