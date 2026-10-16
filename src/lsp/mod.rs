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

mod doccomment;
mod handlers;
mod primitives;
mod unreached;
#[cfg(target_os = "emscripten")]
mod wasm;

use crate::fixed::HashMap;
use std::path::{Path, PathBuf};
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
    // status bar: `satex/status` with `{ "text": … }`, empty when idle.
    let status = connection.sender.clone();
    crate::progress::listen(Some(Box::new(move |text: &str| {
        let note = Notification::new("satex/status".into(), serde_json::json!({ "text": text }));
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
            engine_loop(engine_rx, Engine::new(cfg, root, main, diagnostics_tx).with_hovers(engine_hovers), debounce)
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
                        match kept_hover(&req, &texts, hovers) {
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
                            engine_tx.send(m).is_ok()
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

fn handle_request(connection: &Connection, engine_tx: &Sender<EngineMsg>, req: Request) {
    let _ = connection.sender.send(Message::Response(respond(&mut |m| engine_tx.send(m).is_ok(), req)));
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
    Shutdown,
}

struct Doc {
    text: String,
    path: PathBuf,
    analysis: Option<Analysis>,
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
    cfg: Config,
    root: PathBuf,
    main: Option<PathBuf>,
    diagnostics_tx: Sender<PublishDiagnosticsParams>,
    hovers: Hovers,
    /// The names whose hover the engine computes while it is idle.
    prefill: std::collections::VecDeque<(Uri, String)>,
    /// Each document's loads, when its hovers were last kept: the kernel's
    /// and the packages' names mean the same while these do.
    preambles: HashMap<Uri, Vec<String>>,
    /// Where each document's hovers are kept between sessions, and the
    /// names it defines itself, which are never kept there.
    stores: HashMap<Uri, (std::path::PathBuf, std::collections::HashSet<String>)>,
}

impl Engine {
    fn new(
        mut cfg: Config,
        root: PathBuf,
        main: Option<PathBuf>,
        diagnostics_tx: Sender<PublishDiagnosticsParams>,
    ) -> Self {
        // The document an input file is read as part of (`satex lsp -f`, else
        // the one discovery would start from).
        let main = main.or_else(|| crate::plugin::discovery::scan(&root).root_document());
        // Completion reads the key family an argument feeds off the calls.
        cfg.record_arguments = true;
        Engine {
            docs: HashMap::default(),
            actions: HashMap::default(),
            dirty: Vec::new(),
            cfg,
            root,
            main,
            diagnostics_tx,
            hovers: Hovers::default(),
            prefill: Default::default(),
            preambles: HashMap::default(),
            stores: HashMap::default(),
        }
    }

    /// Keeps the hovers in `hovers`, which another thread reads.
    fn with_hovers(self, hovers: Hovers) -> Self {
        Engine { hovers, ..self }
    }

    /// Analyzes `uri`, then publishes its diagnostics once its hovers are
    /// the new analysis's: a hover asked after them never gets an older one.
    fn analyze(&mut self, uri: &Uri) {
        let published = analyze(&mut self.docs, &mut self.actions, uri, &self.cfg, self.main.as_deref());
        self.dirty.retain(|d| d != uri);
        self.refresh_hovers(uri);
        for params in published {
            let _ = self.diagnostics_tx.send(params);
        }
        crate::progress::idle();
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
        let store = hover_store(&self.cfg, analysis, &dir);
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
        if let Some(markdown) = with_doc(&self.docs, &uri, |analysis, _| handlers::command_markdown(analysis, &name))
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
                self.prefill.retain(|(u, _)| u != &uri);
                if let Ok(mut hovers) = self.hovers.lock() {
                    hovers.retain(|(u, _), _| u != &uri);
                }
            }
            EngineMsg::Hover(uri, pos, reply) => {
                self.ensure(&uri);
                let path = self.docs.get(&uri).map(|d| d.path.display().to_string()).unwrap_or_default();
                let result = with_doc(&self.docs, &uri, |analysis, text| {
                    let at = handlers::to_pos(text, pos);
                    // A control sequence's hover is kept for the next time,
                    // and placed where it is asked.
                    if let Some((name, true)) = handlers::word_at(text.line(at.line), at.col) {
                        let key = (uri.clone(), name.clone());
                        let kept = self.hovers.lock().ok().and_then(|h| h.get(&key).cloned());
                        let markdown = kept.unwrap_or_else(|| {
                            let markdown = handlers::command_markdown(analysis, &name);
                            if let Ok(mut hovers) = self.hovers.lock() {
                                hovers.insert(key, markdown.clone());
                            }
                            markdown
                        });
                        return markdown
                            .map(|m| handlers::markdown_hover(handlers::placed(analysis, m, &name, &path, at)));
                    }
                    handlers::hover(analysis, text, &path, pos)
                });
                let _ = reply.send(result.flatten());
            }
            EngineMsg::Definition(uri, pos, reply) => {
                self.ensure(&uri);
                let path = self.docs.get(&uri).map(|d| d.path.display().to_string()).unwrap_or_default();
                let result =
                    with_doc(&self.docs, &uri, |analysis, text| handlers::definition(analysis, text, &path, pos));
                let _ = reply.send(result.flatten());
            }
            EngineMsg::References(uri, pos, decls, reply) => {
                self.ensure(&uri);
                let path = self.docs.get(&uri).map(|d| d.path.display().to_string()).unwrap_or_default();
                let result = with_doc(&self.docs, &uri, |analysis, text| {
                    handlers::references(analysis, text, &path, pos, decls)
                });
                let _ = reply.send(result.flatten());
            }
            EngineMsg::SignatureHelp(uri, pos, reply) => {
                self.ensure(&uri);
                let result = with_doc(&self.docs, &uri, |analysis, text| handlers::signature_help(analysis, text, pos));
                let _ = reply.send(result.flatten());
            }
            EngineMsg::Completion(uri, pos, reply) => {
                self.ensure(&uri);
                let result = with_doc(&self.docs, &uri, |analysis, text| handlers::completion(analysis, text, pos));
                let _ = reply.send(result.unwrap_or(lsp_types::CompletionResponse::Array(Vec::new())));
            }
            EngineMsg::DocumentSymbol(uri, reply) => {
                self.ensure(&uri);
                let path = self.docs.get(&uri).map(|d| d.path.display().to_string());
                let result = match path {
                    Some(path) => with_doc(&self.docs, &uri, |analysis, text| {
                        handlers::document_symbols(analysis, &path, text).map(lsp_types::DocumentSymbolResponse::Flat)
                    })
                    .flatten(),
                    None => None,
                };
                let _ = reply.send(result);
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
            EngineMsg::WorkspaceSymbol(query, reply) => {
                let uri = self.docs.keys().next().cloned();
                if let Some(uri) = &uri {
                    self.ensure(uri);
                }
                let result = uri
                    .and_then(|u| with_doc(&self.docs, &u, |analysis, _| handlers::workspace_symbols(analysis, &query)))
                    .unwrap_or_default();
                let _ = reply.send(result);
            }
            EngineMsg::CodeAction(uri, reply) => {
                self.fresh(&uri);
                let _ = reply.send(self.actions.get(&uri).cloned().unwrap_or_default());
            }
            EngineMsg::Typeset(uri, reply) => {
                self.ensure(&uri);
                let result = with_doc(&self.docs, &uri, |analysis, _| handlers::typeset(analysis));
                let _ = reply.send(result.unwrap_or_default());
            }
            EngineMsg::ExecuteCommand(body, reply) => {
                let active = self.docs.values().next().map(|d| d.path.clone()).unwrap_or_else(|| self.root.clone());
                let result = crate::cmd::query::run_request_value(&body, &self.cfg, Some(&active));
                let _ = reply.send(result);
            }
        }
    }
}

fn engine_loop(rx: Receiver<EngineMsg>, mut engine: Engine, debounce: Duration) {
    loop {
        let received = if !engine.dirty.is_empty() {
            rx.recv_timeout(debounce)
        } else if !engine.prefill.is_empty() {
            // Idle: one more hover, unless a message waits.
            match rx.try_recv() {
                Ok(msg) => Ok(msg),
                Err(TryRecvError::Empty) => {
                    engine.prefill_one();
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
    }
}

/// Re-analyzes `uri`'s current text, stashes the quick-fix code actions of
/// its diagnostics for later `textDocument/codeAction` calls, and returns
/// the diagnostics to publish.
/// Every other open document is set in the overlay first, so an `\input` of
/// one sees its unsaved text too (`crate::overlay`).  An input file (no
/// `\documentclass`, as discovery classifies it) is analyzed as part of
/// `main`, so its names are seen with every file that uses them.
fn analyze(
    docs: &mut HashMap<Uri, Doc>,
    actions: &mut HashMap<Uri, Vec<Json>>,
    uri: &Uri,
    cfg: &Config,
    main: Option<&Path>,
) -> Vec<PublishDiagnosticsParams> {
    let Some((text, path)) = docs.get(uri).map(|d| (d.text.clone(), d.path.clone())) else { return Vec::new() };
    // Rules and handlers read the buffer, not the file on disk.
    crate::overlay::set(&path, text.clone());
    for (other_uri, other) in docs.iter() {
        if other_uri != uri {
            crate::overlay::set(&other.path, other.text.clone());
        }
    }
    let input = path.extension().is_some_and(|e| e == "tex") && !text.contains("\\documentclass");
    let whole = main.filter(|main| input && !same_file(main, &path)).and_then(|main| {
        let source = crate::overlay::read_to_string(main).ok()?;
        let analysis = Machine::analyze(&source, Some(main), cfg);
        // Only a document that reads the file speaks for it.
        analysis.file_names().iter().any(|f| same_file(Path::new(f), &path)).then_some(analysis)
    });
    let analysis = whole.unwrap_or_else(|| Machine::analyze(&text, Some(&path), cfg));
    // The server's stderr is the client's log.
    for note in analysis.limit_notes() {
        eprintln!("satex: {note}");
    }
    let mut groups = handlers::diagnostics_json(&analysis);
    let own_uri = handlers::path_uri(&path.display().to_string());
    if !groups.iter().any(|g| g.get("uri").and_then(Json::as_str) == Some(own_uri.as_str())) {
        groups.push(serde_json::json!({ "uri": own_uri, "diagnostics": [], "codeActions": [] }));
    }
    if let Some(doc) = docs.get_mut(uri) {
        doc.analysis = Some(analysis);
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
    let analysis = doc.analysis.as_ref()?;
    let text = Text::new(doc.text.clone());
    // Columns count what this document's engine reads, whatever was
    // analyzed last on this thread (`crate::source`).
    Some(crate::source::with(analysis.plugins.engine.encoding(), || f(analysis, &text)))
}

fn same_file(a: &Path, b: &Path) -> bool {
    a == b || std::fs::canonicalize(a).ok().zip(std::fs::canonicalize(b).ok()).is_some_and(|(a, b)| a == b)
}
