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
//!
//! An input file is analyzed through its root: the one its `% !TeX root`
//! line names, else the first candidate root (`Discovery::candidate_roots`)
//! whose run reads it. A root's run is reused while the open buffers' texts
//! are those it read, so the same text sent again runs nothing.

// lsp_types::Uri's interior mutability is a cache with no bearing on Eq/Hash; safe as a map key.
#![allow(clippy::mutable_key_type)]

mod doccomment;
mod handlers;
mod knowledge;
pub(crate) mod primitives;
mod unreached;
#[cfg(target_os = "emscripten")]
mod wasm;

use crate::fixed::HashMap;
use crate::plugin::discovery;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, TryRecvError, select};
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, Response};
use lsp_types::{
    CodeActionParams, CodeActionProviderCapability, CompletionOptions, CompletionParams, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams, DocumentHighlightParams,
    DocumentSymbolParams, ExecuteCommandOptions, ExecuteCommandParams, GotoDefinitionParams, Hover, HoverParams,
    HoverProviderCapability, Location, OneOf, Position, PublishDiagnosticsParams, ReferenceParams, RenameOptions,
    RenameParams, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};
use serde_json::Value as Json;

use crate::config::Config;
use crate::lint::fix::Text;
use crate::machine::{Analysis, Machine};

/// The one `workspace/executeCommand` this server answers: its single
/// argument is a `satex query --request` body (`doc/wiki/queries.md`),
/// minus `file` when the request means the document that sent it.
const EXECUTE_COMMAND: &str = "satex/query";

/// The hovers of control sequences by document and name, `None` where there
/// is none: shared with the connection thread, which answers from them while
/// the engine analyzes. Filled on demand, and while the engine is idle for
/// the names the document uses most.
type Hovers = std::sync::Arc<std::sync::Mutex<HashMap<(Uri, String), Option<String>>>>;

/// How many of the names a document uses most get their hover while the
/// engine is idle.
const PREFILL: usize = 400;

#[cfg_attr(target_os = "emscripten", allow(unreachable_code))]
pub fn serve(cfg: Config, root: PathBuf, main: Option<PathBuf>) -> Result<(), String> {
    #[cfg(target_os = "emscripten")]
    wasm::serve(cfg, root, main);
    let (connection, io_threads) = match cfg.lsp.port {
        Some(port) => Connection::listen((cfg.lsp.host.as_str(), port)).map_err(|e| e.to_string())?,
        None => Connection::stdio(),
    };
    let params =
        connection.initialize(serde_json::to_value(server_capabilities()).unwrap()).map_err(|e| e.to_string())?;
    // The folder the client opened, in its own spelling: the working
    // directory is the resolved one, and a folder opened through a symlink
    // must be answered under the symlink (#17).
    let named = client_roots(&params);
    let root = named.first().cloned().unwrap_or(root);
    // What the engine is doing (`caching pdftex kernel`), for the editor's
    // status bar: `satex/status` with a `progress::Status`, `text` empty when idle.
    let status = connection.sender.clone();
    crate::progress::listen(Some(Box::new(move |now: &crate::progress::Status| {
        let note = Notification::new("satex/status".into(), now);
        let _ = status.send(Message::Notification(note));
    })));

    let (engine_tx, engine_rx) = crossbeam_channel::unbounded::<EngineMsg>();
    let (diagnostics_tx, diagnostics_rx) = crossbeam_channel::unbounded::<PublishDiagnosticsParams>();
    let debounce = Duration::from_millis(cfg.lsp.debounce_ms.max(1));
    let hovers = Hovers::default();
    let engine_hovers = hovers.clone();
    // The stack the command line's main thread has: deep expansion nests that far.
    let engine = std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            handlers::VIEW.with_borrow_mut(|view| named.iter().for_each(|r| view.add(r)));
            let engine = Engine::new(cfg, root, main, diagnostics_tx).with_hovers(engine_hovers);
            engine_loop(engine_rx.clone(), Engine { inbox: Some(engine_rx), ..engine }, debounce)
        })
        .map_err(|e| e.to_string())?;

    let result = main_loop(&connection, &engine_tx, &diagnostics_rx, &hovers);
    let _ = engine_tx.send(EngineMsg::Shutdown);
    let _ = engine.join();
    crate::progress::listen(None);
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
        linked_editing_range_provider: Some(lsp_types::LinkedEditingRangeServerCapabilities::Simple(true)),
        workspace_symbol_provider: Some(OneOf::Left(true)),
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: Default::default(),
        })),
        signature_help_provider: Some(lsp_types::SignatureHelpOptions {
            trigger_characters: Some(vec!["{".into(), "[".into()]),
            retrigger_characters: Some(vec!["}".into(), "]".into()]),
            work_done_progress_options: Default::default(),
        }),
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
    hovers: &Hovers,
) -> Result<(), String> {
    // The open documents' text, to find the name under a hover here.
    let mut texts: HashMap<Uri, String> = HashMap::default();
    loop {
        select! {
            recv(connection.receiver) -> msg => {
                match msg {
                    Ok(Message::Request(req)) => {
                        if connection.handle_shutdown(&req).map_err(|e| e.to_string())? {
                            return Ok(());
                        }
                        match kept_hover(&req, &texts, hovers).or_else(|| linked(&req, |uri| texts.get(uri))) {
                            Some(hover) => {
                                let _ = connection.sender.send(Message::Response(Response::new_ok(req.id, hover)));
                            }
                            None => handle_request(connection, engine_tx, req),
                        }
                    }
                    Ok(Message::Notification(not)) => handle_notification(
                        &mut |m| {
                            match &m {
                                EngineMsg::Open(uri, _, text) | EngineMsg::Change(uri, text) => {
                                    texts.insert(uri.clone(), text.clone());
                                }
                                EngineMsg::Close(uri) => {
                                    texts.remove(uri);
                                }
                                _ => {}
                            }
                            to_engine(engine_tx, m)
                        },
                        not,
                    ),
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

/// Where the hovers of a document whose run read these files are kept
/// between sessions: one file per satex build and set of the installation's
/// files the run read (each by its path, size and time), so a hover of
/// another meaning is never read back.
fn hover_store(cfg: &Config, analysis: &Analysis, own: &Path) -> Option<std::path::PathBuf> {
    let dir = crate::format::enabled_cache_dir(cfg)?;
    let mut hash = crate::format::Fnv::new();
    hash.write(crate::format::cache_key().as_bytes());
    for name in analysis.file_names().iter().filter(|f| !crate::paths::within(own, Path::new(f))) {
        let meta = std::fs::metadata(name).ok();
        hash.put(&(name, meta.as_ref().map(|m| m.len()), meta.and_then(|m| m.modified().ok())));
    }
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join(format!("hovers-{:016x}.json", hash.0)))
}

/// A hover answered from what the engine kept, without waiting for it.
fn kept_hover(req: &Request, texts: &HashMap<Uri, String>, hovers: &Hovers) -> Option<Json> {
    if req.method != "textDocument/hover" {
        return None;
    }
    let p: HoverParams = serde_json::from_value(req.params.clone()).ok()?;
    let uri = p.text_document_position_params.text_document.uri;
    let text = Text::new(texts.get(&uri)?.clone());
    let pos = handlers::to_pos(&text, p.text_document_position_params.position);
    let (name, true) = handlers::word_at(text.line(pos.line), pos.col)? else { return None };
    // A primitive's name shows its syntax where it still means it: the
    // engine adds that to what it kept.
    if primitives::positional(&name) {
        return None;
    }
    let kept = hovers.lock().ok()?.get(&(uri, name))?.clone();
    Some(serde_json::to_value(kept.map(handlers::markdown_hover)).unwrap_or(Json::Null))
}

/// Linked editing, answered from the text alone: it follows the typing.
fn linked<'a>(req: &Request, text: impl FnOnce(&Uri) -> Option<&'a String>) -> Option<Json> {
    if req.method != "textDocument/linkedEditingRange" {
        return None;
    }
    let p: lsp_types::LinkedEditingRangeParams = serde_json::from_value(req.params.clone()).ok()?;
    let at = p.text_document_position_params;
    let ranges = text(&at.text_document.uri).and_then(|text| handlers::linked_environment(text, at.position));
    Some(serde_json::to_value(ranges).unwrap_or(Json::Null))
}

fn handle_request(connection: &Connection, engine_tx: &Sender<EngineMsg>, req: Request) {
    let _ = connection.sender.send(Message::Response(respond(&mut |m| to_engine(engine_tx, m), req)));
}

/// Hands `msg` to the engine, which stops the probes it runs in the background for it.
fn to_engine(engine_tx: &Sender<EngineMsg>, msg: EngineMsg) -> bool {
    crate::probe::WANTED.store(true, std::sync::atomic::Ordering::Relaxed);
    engine_tx.send(msg).is_ok()
}

/// `req` answered through `send`, which hands a message to the engine.
fn respond(send: Send<'_>, req: Request) -> Response {
    let id = req.id.clone();
    let method = req.method.clone();
    match dispatch(send, req) {
        Ok(value) => Response::new_ok(id, value),
        Err(message) => Response::new_err(id, ErrorCode::InternalError as i32, format!("{method}: {message}")),
    }
}

/// Hands one message to the engine: a channel to its thread, or the engine itself.
type Send<'a> = &'a mut dyn FnMut(EngineMsg) -> bool;

/// Sends `msg` to the engine and blocks on its one-shot reply — the engine
/// answers every read from its own thread rather than handing out its
/// `Analysis`, so this is the only way to ask it anything.
fn ask<T>(engine_tx: Send<'_>, build: impl FnOnce(Sender<T>) -> EngineMsg) -> Result<T, String> {
    let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
    let stopped = || "the analysis engine has stopped".to_string();
    if !engine_tx(build(reply_tx)) {
        return Err(stopped());
    }
    reply_rx.recv().map_err(|_| stopped())
}

fn dispatch(engine_tx: Send<'_>, req: Request) -> Result<Json, String> {
    let params = req.params;
    match req.method.as_str() {
        // What the document's paragraphs set, as `satex query typeset` has it.
        "satex/typeset" => {
            let uri: Uri = serde_json::from_value(params["textDocument"]["uri"].clone()).map_err(|e| e.to_string())?;
            Ok(Json::Array(ask(engine_tx, |reply| EngineMsg::Typeset(uri, reply))?))
        }
        // The cache report of the last analysis: which kernel, class and package caches it reused or rebuilt.
        "satex/caches" => {
            let uri: Uri = serde_json::from_value(params["textDocument"]["uri"].clone()).map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(ask(engine_tx, |reply| EngineMsg::Caches(uri, reply))?).unwrap())
        }
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
        "textDocument/signatureHelp" => {
            let p: lsp_types::SignatureHelpParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let uri = p.text_document_position_params.text_document.uri;
            let pos = p.text_document_position_params.position;
            let help = ask(engine_tx, |reply| EngineMsg::SignatureHelp(uri, pos, reply))?;
            Ok(serde_json::to_value(help).unwrap())
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
            let body = p.arguments.into_iter().next().ok_or("expected one argument: a `satex query --request` body")?;
            ask(engine_tx, |reply| EngineMsg::ExecuteCommand(body, reply))?.map(Json::Array)
        }
        other => Err(format!("method not found: {other}")),
    }
}

fn handle_notification(engine_tx: Send<'_>, not: Notification) {
    let sent = match not.method.as_str() {
        "textDocument/didOpen" => serde_json::from_value::<DidOpenTextDocumentParams>(not.params).ok().and_then(|p| {
            let uri = p.text_document.uri;
            let path = handlers::uri_to_path(&uri)?;
            Some(EngineMsg::Open(uri, path, p.text_document.text))
        }),
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
        engine_tx(msg);
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
    SignatureHelp(Uri, Position, Sender<Option<lsp_types::SignatureHelp>>),
    DocumentSymbol(Uri, Sender<Option<lsp_types::DocumentSymbolResponse>>),
    Highlight(Uri, Position, Sender<Option<Vec<lsp_types::DocumentHighlight>>>),
    PrepareRename(Uri, Position, Sender<Option<lsp_types::PrepareRenameResponse>>),
    Rename(Uri, Position, String, Sender<Result<lsp_types::WorkspaceEdit, String>>),
    WorkspaceSymbol(String, Sender<Vec<lsp_types::SymbolInformation>>),
    CodeAction(Uri, Sender<Vec<Json>>),
    ExecuteCommand(Json, Sender<Result<Vec<Json>, String>>),
    Typeset(Uri, Sender<Vec<Json>>),
    Caches(Uri, Sender<Vec<String>>),
    Shutdown,
}

#[derive(Clone)]
struct Doc {
    text: String,
    path: PathBuf,
    analysis: Option<Rc<Analysis>>,
}

/// One run of a root: the open buffers' texts it read, by hash, and the
/// files it read.
struct Run {
    texts: u64,
    analysis: Rc<Analysis>,
    reads: HashSet<PathBuf>,
}

/// The workspace folders the client named, else its root.
fn client_roots(params: &Json) -> Vec<PathBuf> {
    let uri = |v: &Json| v.as_str().and_then(|s| s.parse::<Uri>().ok()).and_then(|u| handlers::uri_to_path(&u));
    let folders: Vec<PathBuf> = params
        .get("workspaceFolders")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .filter_map(|f| f.get("uri").and_then(uri))
        .collect();
    if !folders.is_empty() {
        return folders;
    }
    params
        .get("rootUri")
        .and_then(uri)
        .or_else(|| params.get("rootPath").and_then(Json::as_str).map(PathBuf::from))
        .into_iter()
        .collect()
}

/// The analysis state: open documents, their analyses and quick fixes.
struct Engine {
    docs: HashMap<Uri, Doc>,
    actions: HashMap<Uri, Vec<Json>>,
    dirty: Vec<Uri>,
    /// Leaked once per server: the checkpoints of a run hold it.
    cfg: &'static Config,
    root: PathBuf,
    /// The root `satex lsp -f` names, tried first for an input file.
    main: Option<PathBuf>,
    /// The last run of each root an open document is answered from.
    runs: HashMap<PathBuf, Run>,
    /// The root a run was seen to read each file from.
    owners: HashMap<PathBuf, PathBuf>,
    diagnostics_tx: Sender<PublishDiagnosticsParams>,
    hovers: Hovers,
    /// The names whose hover the engine computes while it is idle.
    prefill: std::collections::VecDeque<(Uri, String)>,
    /// Each document's loads, when its hovers were last kept: the kernel's
    /// and the packages' names mean the same while these do.
    preambles: HashMap<Uri, Vec<String>>,
    /// Each document's text before `\begin{document}` at its last run.
    preamble_texts: HashMap<Uri, String>,
    /// Where each document's hovers are kept between sessions, and the
    /// names it defines itself, which are never kept there.
    stores: HashMap<Uri, (std::path::PathBuf, std::collections::HashSet<String>)>,
    /// Each document root's checkpoints, from which its next run resumes.
    checkpoints: HashMap<PathBuf, crate::machine::Checkpoints<'static>>,
    /// Where messages arrive while a run goes on, and those that wait for its end.
    inbox: Option<Receiver<EngineMsg>>,
    deferred: std::collections::VecDeque<EngineMsg>,
}

impl EngineMsg {
    /// The document a request asks about.
    fn uri(&self) -> Option<&Uri> {
        match self {
            EngineMsg::Hover(uri, ..)
            | EngineMsg::Definition(uri, ..)
            | EngineMsg::References(uri, ..)
            | EngineMsg::SignatureHelp(uri, ..)
            | EngineMsg::Completion(uri, ..)
            | EngineMsg::DocumentSymbol(uri, ..)
            | EngineMsg::Typeset(uri, ..)
            | EngineMsg::Caches(uri, ..) => Some(uri),
            _ => None,
        }
    }
}

impl Engine {
    fn new(
        mut cfg: Config,
        root: PathBuf,
        main: Option<PathBuf>,
        diagnostics_tx: Sender<PublishDiagnosticsParams>,
    ) -> Self {
        // Completion reads the key family an argument feeds off the calls.
        cfg.record_arguments = true;
        Engine {
            docs: HashMap::default(),
            actions: HashMap::default(),
            dirty: Vec::new(),
            cfg: Box::leak(Box::new(cfg)),
            root,
            main,
            runs: HashMap::default(),
            owners: HashMap::default(),
            diagnostics_tx,
            hovers: Hovers::default(),
            prefill: Default::default(),
            preambles: HashMap::default(),
            preamble_texts: HashMap::default(),
            stores: HashMap::default(),
            checkpoints: HashMap::default(),
            inbox: None,
            deferred: Default::default(),
        }
    }

    /// Keeps the hovers in `hovers`, which another thread reads.
    fn with_hovers(self, hovers: Hovers) -> Self {
        Engine { hovers, ..self }
    }

    /// Analyzes `uri` through its root, then publishes the diagnostics of a
    /// new run once its hovers are the run's: a hover asked after them
    /// never gets an older one. A run of the same texts is reused.
    fn analyze(&mut self, uri: &Uri) {
        let Some(doc) = self.docs.get(uri) else { return };
        let (path, text) = (doc.path.clone(), doc.text.clone());
        // Every open buffer is read for its file, so an `\input` of one sees
        // its unsaved text too (`crate::overlay`).
        for doc in self.docs.values() {
            crate::overlay::set(&doc.path, doc.text.clone());
        }
        let (analysis, fresh) = self.run_for(&path, &text);
        self.dirty.retain(|d| d != uri);
        let Some(doc) = self.docs.get_mut(uri) else { return };
        let changed = !doc.analysis.as_ref().is_some_and(|a| Rc::ptr_eq(a, &analysis));
        let preamble = text.split("\\begin{document}").next().unwrap_or_default().to_string();
        let same = self.preamble_texts.insert(uri.clone(), preamble.clone()).is_some_and(|p| p == preamble);
        if changed
            && same
            && let Some(old) = &doc.analysis
        {
            carry_probes(old, &analysis);
        }
        doc.analysis = Some(analysis.clone());
        self.runs.retain(|_, run| Rc::strong_count(&run.analysis) > 1);
        if changed {
            self.refresh_hovers(uri);
        }
        if fresh {
            let open: Vec<&Path> = self.docs.values().map(|d| d.path.as_path()).collect();
            for params in publish(&mut self.actions, &analysis, &path, &open) {
                let _ = self.diagnostics_tx.send(params);
            }
        }
        crate::progress::idle();
    }

    /// The analysis answering for `path`: that of the root its `% !TeX root`
    /// line names, else, for an input file, of the first candidate root a
    /// run sees reading it, else its own. Whether it is a new run is the second.
    fn run_for(&mut self, path: &Path, text: &str) -> (Rc<Analysis>, bool) {
        let key = canonical(path);
        // Any file that is no document (an input, a package's part) is read
        // through the document that reads it; literate sources are their own.
        let literate = path.extension().is_some_and(|e| matches!(e.to_str(), Some("dtx" | "ins" | "fdd")));
        let input = !literate && !discovery::is_document(text);
        let declared = discovery::declared_root(path, text);
        let known: Vec<PathBuf> = match (declared, input) {
            (Some(root), _) => vec![root],
            (None, true) => self.owners.get(&key).into_iter().chain(&self.main).cloned().collect(),
            (None, false) => Vec::new(),
        };
        let mut tried = Vec::new();
        // The first root whose run began the document: an input no root
        // reads is read after its preamble, as that root would read it.
        let mut preamble: Option<PathBuf> = None;
        let dir = if crate::paths::within(&self.root, path) { self.root.clone() } else { crate::paths::dir_of(path) };
        let searched = move || discovery::scan(&dir).candidate_roots(path);
        let candidates = known.into_iter().chain(input.then(searched).into_iter().flatten());
        for root in candidates {
            let root_key = canonical(&root);
            if root_key == key || tried.contains(&root_key) {
                continue;
            }
            let run = self.run(&root);
            if self.runs.get(&root_key).is_some_and(|r| r.reads.contains(&key)) {
                self.owners.insert(key, root);
                return run;
            }
            if preamble.is_none() && crate::lint::shared::document_start(&run.0).is_some() {
                preamble = Some(root.clone());
            }
            // A root that does not read the file is dropped.
            drop(run);
            self.runs.retain(|_, run| Rc::strong_count(&run.analysis) > 1);
            tried.push(root_key);
        }
        // A part of a package the project provides is read where the
        // package puts it: after loading it, as its own code.
        if input && let Some(owner) = discovery::scan(&crate::paths::dir_of(path)).package_owners(path).first() {
            let stem = owner.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            let (load, class) = match owner.extension().is_some_and(|e| e == "cls") {
                true => (String::new(), stem.to_string()),
                false => (format!("\\usepackage{{{stem}}}"), "article".to_string()),
            };
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or_default();
            let root = crate::paths::dir_of(path).join(format!(".satex-part-{stem}.tex"));
            let text = format!(
                "\\documentclass{{{class}}}{load}\\makeatletter\\input{{{name}}}\\makeatother\\begin{{document}}\\end{{document}}\n"
            );
            crate::overlay::set(&root, text);
            return self.run(&root);
        }
        // LaTeX's own hook reads it once the root's preamble is done.
        if input && let Some(root) = preamble {
            let dir = crate::paths::dir_of(&root);
            let name = |p: &Path| p.strip_prefix(&dir).unwrap_or(p).with_extension("").display().to_string();
            let stem = root.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            let part = dir.join(format!(".satex-part-{stem}.tex"));
            let text =
                format!("\\AddToHook{{begindocument/end}}{{\\input{{{}}}}}\\input{{{}}}\n", name(path), name(&root));
            crate::overlay::set(&part, text);
            return self.run(&part);
        }
        self.run(path)
    }

    /// `root` analyzed for the open buffers' texts, reused while they are
    /// those of its last run.
    fn run(&mut self, root: &Path) -> (Rc<Analysis>, bool) {
        let texts = {
            let mut docs: Vec<(&PathBuf, &String)> = self.docs.values().map(|d| (&d.path, &d.text)).collect();
            docs.sort();
            let mut hasher = std::hash::DefaultHasher::new();
            docs.hash(&mut hasher);
            hasher.finish()
        };
        let key = canonical(root);
        if let Some(run) = self.runs.get(&key).filter(|run| run.texts == texts) {
            return (run.analysis.clone(), false);
        }
        let source = crate::overlay::read_to_string(root).unwrap_or_default();
        let analysis = Rc::new(self.serving(|engine| {
            let cfg = engine.cfg;
            match discovery::is_document(&source) {
                // The build's files (`.aux`, the `.bbl`) stay for the next run.
                true => Machine::build(
                    engine.checkpoints.entry(key.clone()).or_insert_with(|| crate::machine::Checkpoints::new(cfg)),
                    &source,
                    root,
                    cfg,
                    true,
                ),
                false => Machine::analyze(&source, Some(root), cfg),
            }
        }));
        // The server's stderr is the client's log.
        for note in analysis.limit_notes() {
            eprintln!("satex: {note}");
        }
        let reads: HashSet<PathBuf> = analysis.file_names().iter().map(|f| canonical(Path::new(f))).collect();
        // A document's run says which root each file it reads belongs to.
        if discovery::is_document(&source) {
            for file in &reads {
                self.owners.entry(file.clone()).or_insert_with(|| root.to_path_buf());
            }
        }
        self.runs.insert(key, Run { texts, analysis: analysis.clone(), reads });
        let runs = &self.runs;
        self.checkpoints.retain(|root, _| runs.contains_key(root));
        (analysis, true)
    }

    /// `f`'s run, while which the requests that only read are answered
    /// from the analyses there are, and the others wait for its end.
    fn serving<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let Some(inbox) = self.inbox.clone() else { return f(self) };
        let (mut docs, hovers) = (self.docs.clone(), self.hovers.clone());
        let deferred = Rc::new(std::cell::RefCell::new(Vec::new()));
        let waiting = deferred.clone();
        let pause = Box::new(move || {
            while let Ok(msg) = inbox.try_recv() {
                match msg {
                    EngineMsg::Change(ref uri, ref text) => {
                        if let Some(doc) = docs.get_mut(uri) {
                            doc.text = text.clone();
                        }
                        waiting.borrow_mut().push(msg);
                    }
                    // A document's first analysis is waited for.
                    msg if msg.uri().is_some_and(|uri| docs.get(uri).is_none_or(|d| d.analysis.is_none())) => {
                        waiting.borrow_mut().push(msg)
                    }
                    msg => {
                        if let Some(msg) = answer(&docs, &hovers, msg) {
                            waiting.borrow_mut().push(msg);
                        }
                    }
                }
            }
        });
        let out = crate::progress::pausing(pause, || f(self));
        self.deferred.extend(deferred.take());
        out
    }

    /// After `uri` was analyzed: its hovers of names the document defines
    /// go, and all of them when its loads changed; the names it uses most
    /// are queued for the idle engine.
    fn refresh_hovers(&mut self, uri: &Uri) {
        let Some(analysis) = self.docs.get(uri).and_then(|d| d.analysis.as_ref()) else { return };
        let name = |sym| analysis.interner.name(sym).to_string();
        let loads: Vec<String> = analysis.facts.loads.iter().map(|l| l.name.clone()).collect();
        let same = self.preambles.insert(uri.clone(), loads.clone()).is_some_and(|before| before == loads);
        let own: std::collections::HashSet<String> =
            analysis.facts.defs.iter().filter(|d| d.package.is_none()).map(|d| name(d.name)).collect();
        // The commands the document's own files call, not the kernel's inner ones.
        let dir = crate::paths::dir_of(Path::new(analysis.file_name(analysis.main_file)));
        let mut owned: HashMap<crate::tex::FileId, bool> = HashMap::default();
        let mut used: HashMap<crate::tex::Sym, u32> = HashMap::default();
        for e in &analysis.facts.expansions {
            let file = e.span.file;
            if *owned.entry(file).or_insert_with(|| crate::paths::within(&dir, Path::new(analysis.file_name(file)))) {
                *used.entry(e.name).or_default() += e.count.max(1);
            }
        }
        let mut used: Vec<(u32, String)> = used.into_iter().map(|(sym, n)| (n, name(sym))).collect();
        used.sort_by(|a, b| b.cmp(a));
        let store = hover_store(self.cfg, analysis, &dir);
        let Ok(mut hovers) = self.hovers.lock() else { return };
        hovers.retain(|(u, n), _| u != uri || same && !own.contains(n));
        // What an earlier session kept for these same loads.
        if !same && let Some(path) = &store {
            let kept: HashMap<String, Option<String>> =
                std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
            for (name, markdown) in kept.into_iter().filter(|(name, _)| !own.contains(name)) {
                hovers.entry((uri.clone(), name)).or_insert(markdown);
            }
        }
        match store {
            Some(path) => self.stores.insert(uri.clone(), (path, own.clone())),
            None => self.stores.remove(uri),
        };
        self.prefill.retain(|(u, _)| u != uri);
        for (_, n) in used.into_iter().take(PREFILL) {
            if !hovers.contains_key(&(uri.clone(), n.clone())) {
                self.prefill.push_back((uri.clone(), n));
            }
        }
    }

    /// Writes `uri`'s hovers of the kernel's and the packages' names where
    /// the next session reads them.
    fn store_hovers(&self, uri: &Uri) {
        let Some((path, own)) = self.stores.get(uri) else { return };
        let Ok(hovers) = self.hovers.lock() else { return };
        let kept: std::collections::BTreeMap<&String, &Option<String>> =
            hovers.iter().filter(|((u, n), _)| u == uri && !own.contains(n)).map(|((_, n), m)| (n, m)).collect();
        if let Ok(bytes) = serde_json::to_vec(&kept) {
            let temporary = path.with_extension("tmp");
            if std::fs::write(&temporary, bytes).is_ok() {
                let _ = std::fs::rename(&temporary, path);
            }
        }
    }

    /// One queued hover, computed and kept; once the queue of a document is
    /// done, its hovers are stored for the next session.
    fn prefill_one(&mut self) {
        let Some((uri, name)) = self.prefill.pop_front() else { return };
        let last = !self.prefill.iter().any(|(u, _)| *u == uri);
        let markdown = crate::probe::background(|| {
            crate::progress::because("hover computed ahead", || {
                with_doc(&self.docs, &uri, |analysis, _| handlers::command_markdown(analysis, &name))
            })
        });
        let Some(markdown) = markdown else {
            // A request came first: asked again once the engine is idle.
            self.prefill.push_front((uri, name));
            return;
        };
        if let Some(markdown) = markdown
            && let Ok(mut hovers) = self.hovers.lock()
        {
            hovers.insert((uri.clone(), name), markdown);
        }
        if last {
            self.store_hovers(&uri);
        }
    }

    /// `uri` analyzed for what it is now, where an answer edits its text.
    fn fresh(&mut self, uri: &Uri) {
        if self.dirty.contains(uri) || self.docs.get(uri).is_some_and(|d| d.analysis.is_none()) {
            self.analyze(uri);
        }
    }

    /// Analyzes every document edited since the last call.
    fn flush(&mut self) {
        for uri in std::mem::take(&mut self.dirty) {
            self.analyze(&uri);
        }
    }

    /// Analyzes `uri` right away when it has no [`Analysis`] yet, so the first
    /// request after `didOpen` does not have to wait out the debounce.
    fn ensure(&mut self, uri: &Uri) {
        if self.docs.get(uri).is_some_and(|d| d.analysis.is_some()) {
            return;
        }
        self.analyze(uri);
    }

    fn step(&mut self, msg: EngineMsg) {
        match msg {
            EngineMsg::Shutdown => {}
            EngineMsg::Open(uri, path, text) => {
                handlers::VIEW.with_borrow_mut(|view| view.add(&path));
                self.docs.insert(uri.clone(), Doc { text, path, analysis: None });
                if !self.dirty.contains(&uri) {
                    self.dirty.push(uri);
                }
            }
            EngineMsg::Change(uri, text) => {
                // The last analysis answers until the next one replaces it,
                // so a request never waits out a whole run.
                if let Some(doc) = self.docs.get_mut(&uri) {
                    doc.text = text;
                    if !self.dirty.contains(&uri) {
                        self.dirty.push(uri);
                    }
                }
            }
            EngineMsg::Close(uri) => {
                self.store_hovers(&uri);
                self.stores.remove(&uri);
                self.docs.remove(&uri);
                self.actions.remove(&uri);
                self.dirty.retain(|d| d != &uri);
                self.preambles.remove(&uri);
                self.preamble_texts.remove(&uri);
                self.prefill.retain(|(u, _)| u != &uri);
                if let Ok(mut hovers) = self.hovers.lock() {
                    hovers.retain(|(u, _), _| u != &uri);
                }
            }
            msg @ (EngineMsg::Hover(..)
            | EngineMsg::Definition(..)
            | EngineMsg::References(..)
            | EngineMsg::SignatureHelp(..)
            | EngineMsg::Completion(..)
            | EngineMsg::DocumentSymbol(..)
            | EngineMsg::Typeset(..)
            | EngineMsg::Caches(..)
            | EngineMsg::WorkspaceSymbol(..)) => {
                if let Some(uri) = msg.uri().or_else(|| self.docs.keys().next()).cloned() {
                    self.ensure(&uri);
                }
                answer(&self.docs, &self.hovers, msg);
            }
            EngineMsg::Highlight(uri, pos, reply) => {
                self.fresh(&uri);
                let path = self.docs.get(&uri).map(|d| d.path.display().to_string()).unwrap_or_default();
                let result =
                    with_doc(&self.docs, &uri, |analysis, text| handlers::highlights(analysis, text, &path, pos));
                let _ = reply.send(result.flatten());
            }
            EngineMsg::PrepareRename(uri, pos, reply) => {
                self.fresh(&uri);
                let path = self.docs.get(&uri).map(|d| d.path.display().to_string()).unwrap_or_default();
                let result =
                    with_doc(&self.docs, &uri, |analysis, text| handlers::prepare_rename(analysis, text, &path, pos));
                let _ = reply.send(result.flatten());
            }
            EngineMsg::Rename(uri, pos, new_name, reply) => {
                self.fresh(&uri);
                let path = self.docs.get(&uri).map(|d| d.path.display().to_string()).unwrap_or_default();
                let result = with_doc(&self.docs, &uri, |analysis, text| {
                    handlers::rename(analysis, text, &path, pos, &new_name)
                })
                .unwrap_or_else(|| Err("document is not open".to_string()));
                let _ = reply.send(result);
            }
            EngineMsg::CodeAction(uri, reply) => {
                self.fresh(&uri);
                let _ = reply.send(self.actions.get(&uri).cloned().unwrap_or_default());
            }
            EngineMsg::ExecuteCommand(body, reply) => {
                // An open document's own analysis answers; another file is run.
                let file = body.get("file").and_then(Json::as_str).map(PathBuf::from);
                let open = self.docs.iter().find(|(_, d)| file.as_ref().is_none_or(|f| *f == d.path));
                let held = open.map(|(uri, _)| uri.clone()).and_then(|uri| {
                    self.fresh(&uri);
                    with_doc(&self.docs, &uri, |a, text| crate::cmd::query::answer_request(&body, a, text.body()))?
                });
                let active = self.docs.values().next().map(|d| d.path.clone()).unwrap_or_else(|| self.root.clone());
                let result =
                    held.map_or_else(|| crate::cmd::query::run_request_value(&body, self.cfg, Some(&active)), Ok);
                let _ = reply.send(result);
            }
        }
    }
}

/// Answers `msg` from the analyses in `docs` when it only reads one; any
/// other message comes back.
fn answer(docs: &HashMap<Uri, Doc>, hovers: &Hovers, msg: EngineMsg) -> Option<EngineMsg> {
    match msg {
        EngineMsg::Hover(uri, pos, reply) => {
            let path = docs.get(&uri).map(|d| d.path.display().to_string()).unwrap_or_default();
            let result = with_doc(docs, &uri, |analysis, text| {
                let at = handlers::to_pos(text, pos);
                // A control sequence's hover is kept for the next time,
                // and placed where it is asked.
                if let Some((name, true)) = handlers::word_at(text.line(at.line), at.col) {
                    let key = (uri.clone(), name.clone());
                    let kept = hovers.lock().ok().and_then(|h| h.get(&key).cloned());
                    let markdown = kept.unwrap_or_else(|| {
                        let markdown =
                            crate::progress::because("hover", || handlers::command_markdown(analysis, &name));
                        if let Ok(mut hovers) = hovers.lock() {
                            hovers.insert(key, markdown.clone());
                        }
                        markdown
                    });
                    return markdown.map(|m| handlers::markdown_hover(handlers::placed(analysis, m, &name, &path, at)));
                }
                handlers::hover(analysis, text, &path, pos)
            });
            let _ = reply.send(result.flatten());
        }
        EngineMsg::Definition(uri, pos, reply) => {
            let path = docs.get(&uri).map(|d| d.path.display().to_string()).unwrap_or_default();
            let result = with_doc(docs, &uri, |analysis, text| handlers::definition(analysis, text, &path, pos));
            let _ = reply.send(result.flatten());
        }
        EngineMsg::References(uri, pos, decls, reply) => {
            let path = docs.get(&uri).map(|d| d.path.display().to_string()).unwrap_or_default();
            let result = with_doc(docs, &uri, |analysis, text| handlers::references(analysis, text, &path, pos, decls));
            let _ = reply.send(result.flatten());
        }
        EngineMsg::SignatureHelp(uri, pos, reply) => {
            let result = crate::progress::because("signature help", || {
                with_doc(docs, &uri, |analysis, text| handlers::signature_help(analysis, text, pos))
            });
            let _ = reply.send(result.flatten());
        }
        EngineMsg::Completion(uri, pos, reply) => {
            let result = crate::progress::because("completion", || {
                with_doc(docs, &uri, |analysis, text| handlers::completion(analysis, text, pos))
            });
            let _ = reply.send(result.unwrap_or(lsp_types::CompletionResponse::Array(Vec::new())));
        }
        EngineMsg::DocumentSymbol(uri, reply) => {
            let path = docs.get(&uri).map(|d| d.path.display().to_string());
            let result = match path {
                Some(path) => with_doc(docs, &uri, |analysis, text| {
                    handlers::document_symbols(analysis, &path, text).map(lsp_types::DocumentSymbolResponse::Flat)
                })
                .flatten(),
                None => None,
            };
            let _ = reply.send(result);
        }
        EngineMsg::WorkspaceSymbol(query, reply) => {
            let result = docs
                .keys()
                .next()
                .cloned()
                .and_then(|u| with_doc(docs, &u, |analysis, _| handlers::workspace_symbols(analysis, &query)))
                .unwrap_or_default();
            let _ = reply.send(result);
        }
        EngineMsg::Typeset(uri, reply) => {
            let result = with_doc(docs, &uri, |analysis, _| handlers::typeset(analysis));
            let _ = reply.send(result.unwrap_or_default());
        }
        EngineMsg::Caches(uri, reply) => {
            let _ = reply.send(with_doc(docs, &uri, |analysis, _| analysis.cache_report()).unwrap_or_default());
        }
        msg => return Some(msg),
    }
    None
}

fn engine_loop(rx: Receiver<EngineMsg>, mut engine: Engine, debounce: Duration) {
    loop {
        let received = if let Some(msg) = engine.deferred.pop_front() {
            Ok(msg)
        } else if !engine.dirty.is_empty() {
            rx.recv_timeout(debounce)
        } else if !engine.prefill.is_empty() {
            // Idle: one more hover, unless a message waits.
            crate::probe::WANTED.store(false, std::sync::atomic::Ordering::Relaxed);
            match rx.try_recv() {
                Ok(msg) => Ok(msg),
                Err(TryRecvError::Empty) => {
                    engine.prefill_one();
                    if engine.prefill.is_empty() {
                        crate::progress::idle();
                    }
                    continue;
                }
                Err(TryRecvError::Disconnected) => Err(RecvTimeoutError::Disconnected),
            }
        } else {
            rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
        };
        match received {
            Ok(EngineMsg::Shutdown) | Err(RecvTimeoutError::Disconnected) => return,
            Ok(msg) => engine.step(msg),
            Err(RecvTimeoutError::Timeout) => engine.flush(),
        }
        crate::progress::idle();
    }
}

/// The diagnostics of `analysis` to publish, by file, with an empty list
/// for `own` and each open file it reads that has none, clearing what an
/// earlier run found there; stashes their quick fixes for
/// `textDocument/codeAction`.
fn publish(
    actions: &mut HashMap<Uri, Vec<Json>>,
    analysis: &Analysis,
    own: &Path,
    open: &[&Path],
) -> Vec<PublishDiagnosticsParams> {
    let names = analysis.file_names();
    let read = |p: &&Path| names.iter().any(|f| crate::paths::same_file(Path::new(f), p));
    // The findings of the file worked on and of the other open files it
    // reads: the rest of the run is context.
    let shown: Vec<String> = std::iter::once(own)
        .chain(open.iter().copied().filter(read))
        .map(|p| handlers::path_uri(&p.display().to_string()).to_string())
        .collect();
    let mut groups: Vec<Json> = handlers::diagnostics_json(analysis)
        .into_iter()
        .filter(|g| g.get("uri").and_then(Json::as_str).is_some_and(|u| shown.iter().any(|s| s == u)))
        .collect();
    for path in std::iter::once(own).chain(open.iter().copied().filter(read)) {
        let uri = handlers::path_uri(&path.display().to_string());
        if !groups.iter().any(|g| g.get("uri").and_then(Json::as_str) == Some(uri.as_str())) {
            groups.push(serde_json::json!({ "uri": uri, "diagnostics": [], "codeActions": [] }));
        }
    }
    let mut published = Vec::new();
    for group in groups {
        let Some(group_uri) = group.get("uri").and_then(Json::as_str).and_then(|s| s.parse::<Uri>().ok()) else {
            continue;
        };
        let diagnostics =
            group.get("diagnostics").cloned().and_then(|d| serde_json::from_value(d).ok()).unwrap_or_default();
        let code_actions = group.get("codeActions").and_then(|a| a.as_array()).cloned().unwrap_or_default();
        actions.insert(group_uri.clone(), code_actions);
        published.push(PublishDiagnosticsParams { uri: group_uri, diagnostics, version: None });
    }
    published
}

/// Runs `f` against `uri`'s [`Analysis`] and a [`Text`] of its current
/// buffer, or `None` when the document is not open or has no analysis yet.
fn with_doc<T>(docs: &HashMap<Uri, Doc>, uri: &Uri, f: impl FnOnce(&Analysis, &Text) -> T) -> Option<T> {
    let doc = docs.get(uri)?;
    let analysis = doc.analysis.as_deref()?;
    let text = Text::new(doc.text.clone());
    // Columns count what this document's engine reads, whatever was
    // analyzed last on this thread (`crate::source`).
    Some(crate::source::with(analysis.plugins.engine.encoding(), || f(analysis, &text)))
}

/// What `old` probed of names the package code defines, by name, in
/// `new`, made of the same preamble and loads: such a name means at the
/// preamble's end and at the start what it meant there. Probes at the end,
/// and of names the document defines, are its own.
fn carry_probes(old: &Analysis, new: &Analysis) {
    let loads = |a: &Analysis| a.facts.loads.iter().map(|l| l.name.clone()).collect::<Vec<_>>();
    if loads(old) != loads(new) {
        return;
    }
    let own: HashSet<&str> =
        new.facts.defs.iter().filter(|d| d.package.is_none()).map(|d| new.interner.name(d.name)).collect();
    let (Ok(from), Ok(mut to)) = (old.probes.lock(), new.probes.lock()) else { return };
    for (&(sym, at), probed) in from.iter() {
        let name = old.interner.name(sym);
        if at != crate::probe::State::End
            && !own.contains(name)
            && let Some(sym) = new.interner.lookup(name)
        {
            to.entry((sym, at)).or_insert_with(|| probed.clone());
        }
    }
}

/// `path` with symlinks resolved, where it exists: the key of a file.
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
