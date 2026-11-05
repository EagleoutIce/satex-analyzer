//! `satex lsp`: a thin, synchronous Language Server Protocol facade over the
//! queries and lints [`crate::cli`] already exposes on the command line —
//! no analysis logic of its own.
//!
//! One "engine" thread owns the interpreter state and every [`Analysis`] it
//! builds.  [`crate::analysis::machine::Machine`] uses `Rc` internally (the
//! interpreter is one machine, run on one thread — see [`crate::project::overlay`]),
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

mod argument_hover;
mod bib;
mod dialogs;
mod directive;
pub(crate) mod doccomment;
mod graphic;
mod handlers;
pub mod knowledge;
pub(crate) mod plugins;
pub(crate) mod primitives;
mod tabular;
mod type_at;
mod unreached;
#[cfg(target_os = "emscripten")]
mod wasm;

use crate::fixed::HashMap;
use crate::plugins::discovery;
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

use crate::analysis::machine::{Analysis, Machine};
use crate::linter::fix::Text;
use crate::project::config::Config;

/// The one `workspace/executeCommand` this server answers: its single
/// argument is a `satex query --request` body (`doc/wiki/queries.md`),
/// minus `file` when the request means the document that sent it.
const EXECUTE_COMMAND: &str = "satex/query";

/// The answer to a style prompt, `{code, answer}` with `answer` `off` or
/// `keep`, recorded in the project's `satex.yaml`.
const STYLE_COMMAND: &str = "satex/style";

/// The prompt's answers, as `window/showMessageRequest` offers them.
const STYLE_ANSWERS: [(&str, Option<crate::project::config::StyleChoice>); 3] = [
    ("Yes, turn it off", Some(crate::project::config::StyleChoice::Off)),
    ("No", None),
    ("Never ask", Some(crate::project::config::StyleChoice::Keep)),
];

/// The hovers of control sequences by document and name, `None` where there
/// is none: shared with the connection thread, which answers from them while
/// the engine analyzes. Filled on demand, and while the engine is idle for
/// the names the document uses most.
type Hovers = std::sync::Arc<Kept>;

#[derive(Default)]
struct Kept {
    hovers: std::sync::Mutex<HashMap<(Uri, String), Option<String>>>,
    /// Whether the engine is analyzing: only then does the connection thread
    /// answer from what was kept, without the part placed at the cursor.
    analyzing: std::sync::atomic::AtomicBool,
    /// Texts opened or changed: counted by the connection thread as they come, by the
    /// engine as it takes them. A run that began with fewer taken is stale once more came.
    edits: std::sync::atomic::AtomicU64,
}

impl std::ops::Deref for Kept {
    type Target = std::sync::Mutex<HashMap<(Uri, String), Option<String>>>;
    fn deref(&self) -> &Self::Target {
        &self.hovers
    }
}

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
    crate::util::progress::listen(Some(Box::new(move |now: &crate::util::progress::Status| {
        let note = Notification::new("satex/status".into(), now);
        let _ = status.send(Message::Notification(note));
    })));

    let (engine_tx, engine_rx) = crossbeam_channel::unbounded::<EngineMsg>();
    let (outbox, outbox_rx) = crossbeam_channel::unbounded::<Notification>();
    let debounce = Duration::from_millis(cfg.lsp.debounce_ms.max(1));
    let hovers = Hovers::default();
    let engine_hovers = hovers.clone();
    // A package's manual mined: its names' hovers are asked again.
    let weak = std::sync::Arc::downgrade(&hovers);
    crate::plugins::docs::index(&cfg.docs).subscribe(Box::new(move |names| {
        let names: std::collections::HashSet<&String> = names.iter().collect();
        if let Some(Ok(mut hovers)) = weak.upgrade().as_ref().map(|h| h.lock()) {
            hovers.retain(|(_, n), _| !names.contains(n));
        }
    }));
    // The stack the command line's main thread has: deep expansion nests that far.
    let engine = std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            handlers::VIEW.with_borrow_mut(|view| named.iter().for_each(|r| view.add(r)));
            let engine = Engine::new(cfg, root, main, outbox).with_hovers(engine_hovers);
            engine_loop(engine_rx.clone(), Engine { inbox: Some(engine_rx), ..engine }, debounce)
        })
        .map_err(|e| e.to_string())?;

    let result = main_loop(&connection, &engine_tx, &outbox_rx, &hovers);
    crate::analysis::probe::stop_all();
    let _ = engine_tx.send(EngineMsg::Shutdown);
    let _ = engine.join();
    crate::util::progress::listen(None);
    // The writer thread only stops once every `Sender` feeding it is gone
    // (`Connection::sender` among them), so it has to be dropped before
    // `io_threads.join()` can return.
    drop(connection);
    io_threads.join().map_err(|e| e.to_string())?;
    result
}

fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::INCREMENTAL)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        document_highlight_provider: Some(OneOf::Left(true)),
        color_provider: Some(lsp_types::ColorProviderCapability::Simple(true)),
        document_link_provider: Some(lsp_types::DocumentLinkOptions {
            resolve_provider: Some(false),
            work_done_progress_options: Default::default(),
        }),
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
            // a key list's next item opens with nothing typed: after `{`, `[`, `,` and `=`
            trigger_characters: Some(["\\", "{", "[", ",", "="].map(String::from).to_vec()),
            resolve_provider: Some(true),
            ..Default::default()
        }),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        code_lens_provider: Some(lsp_types::CodeLensOptions { resolve_provider: Some(false) }),
        inlay_hint_provider: Some(OneOf::Left(true)),
        execute_command_provider: Some(ExecuteCommandOptions {
            commands: vec![EXECUTE_COMMAND.to_string(), STYLE_COMMAND.to_string()],
            ..Default::default()
        }),
        ..Default::default()
    }
}

// --- connection thread: frames JSON-RPC, forwards to the engine --------

fn main_loop(
    connection: &Connection,
    engine_tx: &Sender<EngineMsg>,
    outbox: &Receiver<Notification>,
    hovers: &Hovers,
) -> Result<(), String> {
    // The open documents' text, to find the name under a hover here.
    let mut texts: HashMap<Uri, String> = HashMap::default();
    let mut prompts: HashMap<lsp_server::RequestId, String> = HashMap::default();
    let mut findings: Findings = HashMap::default();
    loop {
        select! {
            recv(connection.receiver) -> msg => {
                match msg {
                    Ok(Message::Request(req)) => {
                        if connection.handle_shutdown(&req).map_err(|e| e.to_string())? {
                            return Ok(());
                        }
                        let cards = rule_cards(&req, &findings);
                        let response = match kept_hover(&req, &texts, hovers).or_else(|| linked(&req, |uri| texts.get(uri))) {
                            Some(hover) => Response::new_ok(req.id, hover),
                            None => respond(&mut |m| to_engine(engine_tx, m), req),
                        };
                        let _ = connection.sender.send(Message::Response(with_cards(response, cards)));
                    }
                    Ok(Message::Notification(not)) => {
                        if let Some(m) = notification(not, |uri| texts.get(uri).map(String::as_str)) {
                            match &m {
                                EngineMsg::Open(uri, _, text, _) | EngineMsg::Change(uri, text, _) => {
                                    texts.insert(uri.clone(), text.clone());
                                    hovers.edits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                }
                                EngineMsg::Close(uri) => {
                                    texts.remove(uri);
                                }
                                _ => {}
                            }
                            to_engine(engine_tx, m);
                        }
                    }
                    Ok(Message::Response(answer)) => {
                        if let Some(code) = prompts.remove(&answer.id)
                            && let Some(choice) = style_choice(&answer)
                        {
                            to_engine(engine_tx, EngineMsg::Style(code, choice, crossbeam_channel::bounded(1).0));
                        }
                    }
                    Err(_) => return Ok(()),
                }
            }
            recv(outbox) -> note => match note {
                Ok(note) if note.method == STYLE_NOTIFICATION => {
                    for candidate in note.params["candidates"].as_array().into_iter().flatten() {
                        let id = lsp_server::RequestId::from(format!("satex-style-{}", prompts.len()));
                        prompts.insert(id.clone(), candidate["code"].as_str().unwrap_or_default().to_string());
                        let _ = connection.sender.send(Message::Request(style_prompt(id, candidate)));
                    }
                }
                Ok(note) => {
                    if let Ok(params) = serde_json::from_value::<PublishDiagnosticsParams>(note.params.clone()) {
                        let rules = params.diagnostics.iter().filter_map(|d| Some((d.range, d.data.as_ref()?.get("rule")?.as_str()?.to_string())));
                        findings.insert(params.uri.clone(), rules.collect());
                    }
                    let _ = connection.sender.send(Message::Notification(note));
                }
                Err(_) => {}
            }
        }
    }
}

/// The engine's notification naming the rules that may be the project's style.
const STYLE_NOTIFICATION: &str = "satex/styleCandidates";

/// One style candidate asked as `window/showMessageRequest`, which any
/// client can answer.
fn style_prompt(id: lsp_server::RequestId, candidate: &Json) -> Request {
    let actions: Vec<Json> = STYLE_ANSWERS.iter().map(|(title, _)| serde_json::json!({ "title": title })).collect();
    let params = serde_json::json!({ "type": 3, "message": candidate["question"], "actions": actions });
    Request::new(id, "window/showMessageRequest".into(), params)
}

/// What a client's answer to [`style_prompt`] records, `None` for no.
fn style_choice(answer: &Response) -> Option<crate::project::config::StyleChoice> {
    let title = answer.response_result.as_ref().ok()?.get("title")?.as_str()?;
    STYLE_ANSWERS.iter().find(|(t, _)| *t == title)?.1
}

/// Where the hovers of a document whose run read these files are kept
/// between sessions: one file per satex build and set of the installation's
/// files the run read (each by its path, size and time), so a hover of
/// another meaning is never read back.
fn hover_store(cfg: &Config, analysis: &Analysis, own: &Path) -> Option<std::path::PathBuf> {
    let dir = crate::caches::format::enabled_cache_dir(cfg)?;
    let mut hash = crate::caches::format::Fnv::new();
    hash.write(crate::caches::format::cache_key().as_bytes());
    for name in analysis.file_names().iter().filter(|f| !crate::project::paths::within(own, Path::new(f))) {
        let meta = std::fs::metadata(name).ok();
        hash.put(&(name, meta.as_ref().map(|m| m.len()), meta.and_then(|m| m.modified().ok())));
    }
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join(format!("hovers-{:016x}.json", hash.0)))
}

/// A hover answered from what the engine kept, without waiting for it.
fn kept_hover(req: &Request, texts: &HashMap<Uri, String>, hovers: &Hovers) -> Option<Json> {
    if req.method != "textDocument/hover" || !hovers.analyzing.load(std::sync::atomic::Ordering::Relaxed) {
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

/// The rule of each published finding, by where it stands.
type Findings = HashMap<Uri, Vec<(lsp_types::Range, String)>>;

/// For a hover over findings, their rules' cards ([`crate::linter::rule_markdown`]).
fn rule_cards(req: &Request, findings: &Findings) -> Option<String> {
    if req.method != "textDocument/hover" {
        return None;
    }
    let at: lsp_types::TextDocumentPositionParams = serde_json::from_value(req.params.clone()).ok()?;
    let mut codes: Vec<&str> = findings
        .get(&at.text_document.uri)?
        .iter()
        .filter(|(range, _)| range.start <= at.position && at.position <= range.end)
        .map(|(_, code)| code.as_str())
        .collect();
    codes.dedup();
    let cards: Vec<String> =
        codes.iter().filter_map(|c| crate::linter::explanation(c)).map(crate::linter::rule_markdown).collect();
    (!cards.is_empty()).then(|| cards.join("\n\n---\n\n"))
}

/// A hover's answer with the findings' `cards` first, then what the hover of the text says.
fn with_cards(mut response: Response, cards: Option<String>) -> Response {
    let (Some(cards), Ok(result)) = (cards, response.response_result.as_mut()) else { return response };
    match result.pointer_mut("/contents/value").and_then(|v| v.as_str().map(str::to_string).zip(Some(v))) {
        Some((text, value)) => *value = Json::String(format!("{cards}\n\n---\n\n{text}")),
        None => *result = serde_json::json!({ "contents": { "kind": "markdown", "value": cards } }),
    }
    response
}

/// Hands `msg` to the engine, which stops the probes it runs in the background for it.
fn to_engine(engine_tx: &Sender<EngineMsg>, msg: EngineMsg) -> bool {
    crate::analysis::probe::WANTED.store(true, std::sync::atomic::Ordering::Relaxed);
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
        // What the build wrote to each file, `null` where not known.
        "satex/written" => {
            let uri: Uri = serde_json::from_value(params["textDocument"]["uri"].clone()).map_err(|e| e.to_string())?;
            Ok(Json::Array(ask(engine_tx, |reply| EngineMsg::Written(uri, reply))?))
        }
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
        "completionItem/resolve" => {
            let item: lsp_types::CompletionItem = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let item = ask(engine_tx, |reply| EngineMsg::Resolve(Box::new(item), reply))?;
            Ok(serde_json::to_value(item).unwrap())
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
        "textDocument/documentColor" => {
            let p: lsp_types::DocumentColorParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let out = ask(engine_tx, |reply| EngineMsg::Colors(p.text_document.uri, reply))?;
            Ok(serde_json::to_value(out).unwrap())
        }
        "textDocument/documentLink" => {
            let p: lsp_types::DocumentLinkParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let out = ask(engine_tx, |reply| EngineMsg::Links(p.text_document.uri, reply))?;
            Ok(serde_json::to_value(out).unwrap())
        }
        "textDocument/colorPresentation" => {
            let p: lsp_types::ColorPresentationParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let out = ask(engine_tx, |reply| EngineMsg::Presentation(p.text_document.uri, p.range, p.color, reply))?;
            Ok(serde_json::to_value(out).unwrap())
        }
        "textDocument/inlayHint" => {
            let p: lsp_types::InlayHintParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let out = ask(engine_tx, |reply| EngineMsg::Inlays(p.text_document.uri, reply))?;
            Ok(serde_json::to_value(out).unwrap())
        }
        "textDocument/codeLens" => {
            let p: lsp_types::CodeLensParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let out = ask(engine_tx, |reply| EngineMsg::Lenses(p.text_document.uri, reply))?;
            Ok(serde_json::to_value(out).unwrap())
        }
        // A type plugin's dialogs at a position ([`plugins::views`]).
        "satex/view" => {
            // `plugin` without `position`: that plugin's manager, as a sidebar opens it
            #[derive(serde::Deserialize)]
            struct ViewParams {
                #[serde(rename = "textDocument")]
                doc: lsp_types::TextDocumentIdentifier,
                #[serde(default)]
                position: Option<Position>,
                #[serde(default)]
                plugin: Option<String>,
            }
            let p: ViewParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            Ok(Json::Array(ask(engine_tx, |reply| EngineMsg::View(p.doc.uri, p.position, p.plugin, reply))?))
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
            let uri = p.text_document.uri;
            let mut actions = ask(engine_tx, |reply| EngineMsg::CodeAction(uri.clone(), p.range.start, reply))?;
            let dialogs = ask(engine_tx, |reply| EngineMsg::Dialog(Ask::Actions(uri, p.range), reply))??;
            actions.extend(dialogs.as_array().cloned().unwrap_or_default());
            Ok(Json::Array(actions))
        }
        // The table and image dialogs (`dialogs.rs`): their state, the text they write.
        "satex/dialog" => {
            let uri: Uri = serde_json::from_value(params["textDocument"]["uri"].clone()).map_err(|e| e.to_string())?;
            let pos: Position = serde_json::from_value(params["position"].clone()).map_err(|e| e.to_string())?;
            ask(engine_tx, |reply| EngineMsg::Dialog(Ask::Open(uri, pos), reply))?
        }
        "satex/dialogApply" => {
            let uri: Uri = serde_json::from_value(params["textDocument"]["uri"].clone()).map_err(|e| e.to_string())?;
            ask(engine_tx, |reply| EngineMsg::Dialog(Ask::Apply(uri, params), reply))?
        }
        "workspace/executeCommand" => {
            let p: ExecuteCommandParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            if p.command == STYLE_COMMAND {
                let arg = p.arguments.first().cloned().unwrap_or_default();
                let code = arg["code"].as_str().ok_or("expected `{code, answer}`")?.to_string();
                let choice = match arg["answer"].as_str() {
                    Some("off") => crate::project::config::StyleChoice::Off,
                    Some("keep") => crate::project::config::StyleChoice::Keep,
                    _ => return Err("`answer` is `off` or `keep`".into()),
                };
                return ask(engine_tx, |reply| EngineMsg::Style(code, choice, reply))?;
            }
            if p.command != EXECUTE_COMMAND {
                return Err(format!("unknown command `{}`. This server only runs `{EXECUTE_COMMAND}`", p.command));
            }
            let body = p.arguments.into_iter().next().ok_or("expected one argument: a `satex query --request` body")?;
            ask(engine_tx, |reply| EngineMsg::ExecuteCommand(body, reply))?.map(Json::Array)
        }
        other => Err(format!("method not found: {other}")),
    }
}

/// The engine's message for a notification, `None` for one it ignores.
/// An edit arrives as ranges (incremental sync) applied to `text_of` the
/// document, and leaves as its whole new text.
fn notification<'a>(not: Notification, text_of: impl Fn(&Uri) -> Option<&'a str>) -> Option<EngineMsg> {
    match not.method.as_str() {
        "textDocument/didOpen" => serde_json::from_value::<DidOpenTextDocumentParams>(not.params).ok().and_then(|p| {
            let uri = p.text_document.uri;
            let path = handlers::uri_to_path(&uri)?;
            Some(EngineMsg::Open(uri, path, p.text_document.text, p.text_document.version))
        }),
        "textDocument/didChange" => serde_json::from_value::<DidChangeTextDocumentParams>(not.params).ok().map(|p| {
            let (uri, version) = (p.text_document.uri, p.text_document.version);
            let mut text = text_of(&uri).unwrap_or_default().to_string();
            for change in p.content_changes {
                match change.range {
                    Some(range) => {
                        let (start, end) = (offset(&text, range.start), offset(&text, range.end));
                        text.replace_range(start..end.max(start), &change.text);
                    }
                    None => text = change.text,
                }
            }
            EngineMsg::Change(uri, text, Some(version))
        }),
        "textDocument/didSave" => serde_json::from_value::<DidSaveTextDocumentParams>(not.params)
            .ok()
            .and_then(|p| p.text.map(|text| EngineMsg::Change(p.text_document.uri, text, None))),
        "textDocument/didClose" => serde_json::from_value::<DidCloseTextDocumentParams>(not.params)
            .ok()
            .map(|p| EngineMsg::Close(p.text_document.uri)),
        _ => None,
    }
}

/// The byte offset of an LSP position in `text`: lines split at `\n`,
/// columns in UTF-16 code units, clamped to the line's end.
fn offset(text: &str, at: Position) -> usize {
    let mut start = 0;
    for _ in 0..at.line {
        match text[start..].find('\n') {
            Some(i) => start += i + 1,
            None => return text.len(),
        }
    }
    let line = text[start..].split('\n').next().unwrap_or_default();
    let mut units = 0;
    for (i, c) in line.char_indices() {
        if units >= at.character as usize {
            return start + i;
        }
        units += c.len_utf16();
    }
    start + line.len()
}

// --- engine thread: owns every Analysis, never sends one out -----------

enum EngineMsg {
    /// A document opened, its text and version.
    Open(Uri, PathBuf, String, i32),
    /// A document's new text, with the version an edit gives it (a save gives none).
    Change(Uri, String, Option<i32>),
    Close(Uri),
    Hover(Uri, Position, Sender<Option<Hover>>),
    Definition(Uri, Position, Sender<Option<lsp_types::GotoDefinitionResponse>>),
    References(Uri, Position, bool, Sender<Option<Vec<Location>>>),
    Completion(Uri, Position, Sender<lsp_types::CompletionResponse>),
    /// A completion item's documentation, asked once it is selected.
    Resolve(Box<lsp_types::CompletionItem>, Sender<lsp_types::CompletionItem>),
    SignatureHelp(Uri, Position, Sender<Option<lsp_types::SignatureHelp>>),
    DocumentSymbol(Uri, Sender<Option<lsp_types::DocumentSymbolResponse>>),
    Highlight(Uri, Position, Sender<Option<Vec<lsp_types::DocumentHighlight>>>),
    Colors(Uri, Sender<Vec<lsp_types::ColorInformation>>),
    Links(Uri, Sender<Vec<lsp_types::DocumentLink>>),
    PrepareRename(Uri, Position, Sender<Option<lsp_types::PrepareRenameResponse>>),
    Rename(Uri, Position, String, Sender<Result<lsp_types::WorkspaceEdit, String>>),
    WorkspaceSymbol(String, Sender<Vec<lsp_types::SymbolInformation>>),
    /// The lint fixes of a document, and the type plugins' actions at a position.
    CodeAction(Uri, Position, Sender<Vec<Json>>),
    Dialog(Ask, Sender<Result<Json, String>>),
    View(Uri, Option<Position>, Option<String>, Sender<Vec<Json>>),
    Presentation(Uri, lsp_types::Range, lsp_types::Color, Sender<Vec<lsp_types::ColorPresentation>>),
    Inlays(Uri, Sender<Vec<lsp_types::InlayHint>>),
    Lenses(Uri, Sender<Vec<lsp_types::CodeLens>>),
    ExecuteCommand(Json, Sender<Result<Vec<Json>, String>>),
    /// A style prompt's answer for a rule: the path of the `satex.yaml` written.
    Style(String, crate::project::config::StyleChoice, Sender<Result<Json, String>>),
    Typeset(Uri, Sender<Vec<Json>>),
    Written(Uri, Sender<Vec<Json>>),
    Caches(Uri, Sender<Vec<String>>),
    Shutdown,
}

/// What a dialog request asks ([`dialogs`]).
enum Ask {
    Actions(Uri, lsp_types::Range),
    Open(Uri, Position),
    /// `{textDocument, kind, range, state}`.
    Apply(Uri, Json),
}

#[derive(Clone)]
struct Doc {
    text: String,
    /// The client's version of `text`: published diagnostics name the version they are of.
    version: i32,
    path: PathBuf,
    analysis: Option<Rc<Analysis>>,
    /// The text `analysis` read: an incremental plugin judges an edit against it.
    analyzed: Option<String>,
    /// How an edit the run was kept for moved the edited line: whole-document
    /// results are made from `analyzed` and moved by it.
    shift: Option<plugins::incremental::Shift>,
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
    /// The inputs no root's run was seen to read.
    orphans: HashSet<PathBuf>,
    /// Where the engine's notifications go: diagnostics, style candidates.
    outbox: Sender<Notification>,
    /// The rules a style prompt was sent for, asked once per session.
    asked: HashSet<String>,
    /// The rules turned off by a style prompt in this session.
    silenced: Vec<String>,
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
    checkpoints: HashMap<PathBuf, crate::analysis::machine::Checkpoints<'static>>,
    /// Where messages arrive while a run goes on, and those that wait for its end.
    inbox: Option<Receiver<EngineMsg>>,
    deferred: std::collections::VecDeque<EngineMsg>,
    /// Texts opened or changed that the engine took ([`Kept::edits`]).
    taken: u64,
    /// The last run stopped for a newer text.
    cancelled: bool,
    /// A run of no document: what a completion answers from before a
    /// document's first run ends, the installation's classes and packages.
    blank: Option<Rc<Analysis>>,
    /// The diagnostics last published per file, with the run they came from: a run reused for a
    /// newer version of the same texts publishes them again under that version.
    published: HashMap<Uri, (std::rc::Weak<Analysis>, PublishDiagnosticsParams)>,
    /// When each document was last edited: typing pauses after it.
    edited: HashMap<Uri, std::time::Instant>,
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
            | EngineMsg::Written(uri, ..)
            | EngineMsg::Caches(uri, ..) => Some(uri),
            EngineMsg::Dialog(Ask::Actions(uri, _) | Ask::Open(uri, _) | Ask::Apply(uri, _), _) => Some(uri),
            _ => None,
        }
    }
}

impl Engine {
    fn new(mut cfg: Config, root: PathBuf, main: Option<PathBuf>, outbox: Sender<Notification>) -> Self {
        // Completion reads the key family an argument feeds off the calls.
        cfg.record_arguments = true;
        // One spelling per file: a run cached under a canonical key keeps the
        // name it was started with, so a relative `-f` would leave the open
        // document's own file unmatched by every per-file feature.
        let absolute = |p: PathBuf| std::path::absolute(&p).unwrap_or(p);
        let (root, main) = (absolute(root), main.map(absolute));
        Engine {
            docs: HashMap::default(),
            actions: HashMap::default(),
            dirty: Vec::new(),
            cfg: Box::leak(Box::new(cfg)),
            root,
            main,
            runs: HashMap::default(),
            owners: HashMap::default(),
            orphans: HashSet::new(),
            outbox,
            asked: HashSet::new(),
            silenced: Vec::new(),
            hovers: Hovers::default(),
            prefill: Default::default(),
            preambles: HashMap::default(),
            preamble_texts: HashMap::default(),
            stores: HashMap::default(),
            checkpoints: HashMap::default(),
            inbox: None,
            deferred: Default::default(),
            taken: 0,
            cancelled: false,
            blank: None,
            published: HashMap::default(),
            edited: HashMap::default(),
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
        struct Analyzing(Hovers);
        impl Drop for Analyzing {
            fn drop(&mut self) {
                self.0.analyzing.store(false, std::sync::atomic::Ordering::Relaxed);
            }
        }
        if self.reuse(uri) {
            return;
        }
        self.hovers.analyzing.store(true, std::sync::atomic::Ordering::Relaxed);
        let _analyzing = Analyzing(self.hovers.clone());
        let Some(doc) = self.docs.get(uri) else { return };
        let (path, text) = (doc.path.clone(), doc.text.clone());
        // the versions the run reads: what its diagnostics are of
        let versions: Vec<(PathBuf, i32)> = self.docs.values().map(|d| (d.path.clone(), d.version)).collect();
        // Every open buffer is read for its file, so an `\input` of one sees
        // its unsaved text too (`crate::project::overlay`).
        for doc in self.docs.values() {
            crate::project::overlay::set(&doc.path, doc.text.clone());
        }
        self.cancelled = false;
        let (analysis, fresh) = self.run_for(&path, &text);
        // A newer text came while it ran: it stopped, and says nothing. The newer text's run follows.
        if self.cancelled {
            crate::util::progress::note!("satex: a run stopped for a newer text");
            if !self.dirty.contains(uri) {
                self.dirty.push(uri.clone());
            }
            return;
        }
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
        let prior = changed.then(|| doc.analysis.clone()).flatten();
        if fresh {
            // What a completion reads of the new run is built first, while the last run (or the
            // installation) answers: the format's names carried over from the last run. Its
            // diagnostics then say it answers. The playground's server answers only once a
            // message is done: there the first completion builds the rest.
            if let Some(prior) = &prior {
                self.serving(|_| crate::queries::query::carry_scope(prior, &analysis));
            }
            if !cfg!(target_os = "emscripten") {
                self.serving(|_| handlers::warm(&analysis));
            }
            self.install(uri, &analysis, &text, changed);
            self.publish_run(&analysis, &path, &versions, None);
        } else {
            self.install(uri, &analysis, &text, changed);
            // the same texts under newer versions: the same diagnostics, of these versions
            let again: Vec<PublishDiagnosticsParams> = (self.published.values_mut())
                .filter(|(run, _)| run.upgrade().is_some_and(|r| Rc::ptr_eq(&r, &analysis)))
                .filter_map(|(_, p)| {
                    let now = version_of(&versions, &p.uri);
                    (now != p.version).then(|| {
                        p.version = now;
                        p.clone()
                    })
                })
                .collect();
            self.send_diagnostics(again);
        }
        crate::util::progress::idle();
    }

    /// `analysis` of `text`, the one `uri`'s requests are answered from.
    fn install(&mut self, uri: &Uri, analysis: &Rc<Analysis>, text: &str, changed: bool) {
        let Some(doc) = self.docs.get_mut(uri) else { return };
        doc.analysis = Some(analysis.clone());
        (doc.analyzed, doc.shift) = (Some(text.to_string()), None);
        self.runs.retain(|_, run| Rc::strong_count(&run.analysis) > 1);
        if changed {
            self.refresh_hovers(uri);
        }
    }

    /// Lints `analysis` and publishes its diagnostics under `versions`, moved by `shift`
    /// in `path` when the run was kept across an edit there.
    fn publish_run(
        &mut self,
        analysis: &Rc<Analysis>,
        path: &Path,
        versions: &[(PathBuf, i32)],
        shift: Option<plugins::incremental::Shift>,
    ) {
        // Requests that only read are answered while the rules run.
        let open: Vec<PathBuf> = self.docs.values().map(|d| d.path.clone()).collect();
        let open: Vec<&Path> = open.iter().map(PathBuf::as_path).collect();
        let orphan = self.orphans.contains(&canonical(path));
        let (mut published, candidates) = self.serving(|engine| {
            let records = engine.unsilenced(handlers::findings(analysis));
            let candidates = crate::linter::style_candidates(&records, &analysis.settings.style_prompt);
            (publish(&mut engine.actions, analysis, &records, path, &open), candidates)
        });
        let own = handlers::path_uri(&path.display().to_string());
        if let Some(own) = published.iter_mut().find(|p| orphan && p.uri == own) {
            own.diagnostics.push(lsp_types::Diagnostic {
                severity: Some(lsp_types::DiagnosticSeverity::INFORMATION),
                code: Some(lsp_types::NumberOrString::String("unreachable-file".into())),
                source: Some("satex".into()),
                message: "No root document reads this file.".into(),
                ..Default::default()
            });
        }
        for p in &mut published {
            if let Some(shift) = shift.filter(|_| p.uri == own) {
                let mut moved = serde_json::to_value(&p.diagnostics).unwrap_or_default();
                shift.apply(&mut moved);
                p.diagnostics = serde_json::from_value(moved).unwrap_or_default();
                self.actions.get_mut(&own).into_iter().flatten().for_each(|a| shift.apply(a));
            }
            p.version = version_of(versions, &p.uri);
            self.published.insert(p.uri.clone(), (Rc::downgrade(analysis), p.clone()));
        }
        self.send_diagnostics(published);
        self.ask_style(candidates);
    }

    /// Keeps `uri`'s last run across the edits since, when an incremental plugin
    /// ([`plugins::incremental`]) takes them, and publishes its results for the new
    /// text; `false` when a run must follow.
    fn reuse(&mut self, uri: &Uri) -> bool {
        use plugins::incremental::{Edit, Reuse};
        // Another document's edit may change the same run.
        if self.dirty.iter().any(|d| d != uri) {
            return false;
        }
        let Some(doc) = self.docs.get(uri) else { return false };
        let (Some(analysis), Some(old)) = (doc.analysis.clone(), doc.analyzed.clone()) else { return false };
        let (path, new) = (doc.path.clone(), doc.text.clone());
        // What picks the run a text is read by must not change.
        let route = |t: &str| {
            let profile = crate::project::config::Profile::of_name(Some(&path), t);
            format!("{:?}", (discovery::is_document(t), discovery::declared_root(&path, t), profile))
        };
        if route(&old) != route(&new) {
            return false;
        }
        let name = path.display().to_string();
        let Some(edit) = Edit::new(&analysis, &name, &old, &new) else { return false };
        let Some((plugin, how)) = plugins::incremental::decide(&edit) else { return false };
        let shift = edit.shift();
        if how == Reuse::Provisional {
            // Only an engine with its own clock waits for the pause.
            let pause = Duration::from_millis(analysis.settings.type_plugins.typing.pause_ms);
            if self.inbox.is_none() || self.edited.get(uri).is_none_or(|t| t.elapsed() >= pause) {
                return false;
            }
            if doc.shift == Some(shift) {
                if !self.dirty.contains(uri) {
                    self.dirty.push(uri.clone());
                }
                return true;
            }
        }
        let versions: Vec<(PathBuf, i32)> = self.docs.values().map(|d| (d.path.clone(), d.version)).collect();
        // The run no longer stands for its texts: back at them, it runs again.
        self.runs.values_mut().filter(|r| Rc::ptr_eq(&r.analysis, &analysis)).for_each(|r| r.texts = 0);
        // The rules read the text the run's positions are of, then everything the new one.
        let read = if how == Reuse::Fresh { &new } else { &old };
        crate::project::overlay::set(&path, read.clone());
        self.publish_run(&analysis, &path, &versions, (how != Reuse::Fresh).then_some(shift));
        crate::project::overlay::set(&path, new.clone());
        if let Some(doc) = self.docs.get_mut(uri) {
            match how {
                Reuse::Fresh => (doc.analyzed, doc.shift) = (Some(new), None),
                _ => doc.shift = Some(shift),
            }
        }
        // A provisional result stays dirty: [`Engine::flush`] took it off the list.
        self.dirty.retain(|d| d != uri);
        if how == Reuse::Provisional {
            self.dirty.push(uri.clone());
        }
        plugins::incremental::count(plugin);
        let text = format!("{}: {plugin}", if how == Reuse::Provisional { "provisional" } else { "reused" });
        eprintln!("satex: {text}");
        crate::util::progress::tell(crate::util::progress::Status { text, ..Default::default() });
        true
    }

    fn send_diagnostics(&self, published: Vec<PublishDiagnosticsParams>) {
        for params in published {
            let _ = self.outbox.send(Notification::new("textDocument/publishDiagnostics".into(), params));
        }
    }

    fn unsilenced(&self, mut records: Vec<crate::queries::query::Record>) -> Vec<crate::queries::query::Record> {
        records.retain(|r| !self.silenced.iter().any(|c| r.get("code").and_then(Json::as_str) == Some(c)));
        records
    }

    /// Sends the style candidates not asked about yet in this session.
    fn ask_style(&mut self, candidates: Vec<crate::queries::query::Record>) {
        let asked = &mut self.asked;
        let new: Vec<_> = candidates
            .into_iter()
            .filter(|c| asked.insert(c.get("code").and_then(Json::as_str).unwrap_or_default().to_string()))
            .collect();
        if !new.is_empty() {
            let _ = self
                .outbox
                .send(Notification::new(STYLE_NOTIFICATION.into(), serde_json::json!({ "candidates": new })));
        }
    }

    /// Records a style prompt's answer. A rule turned off leaves the open
    /// documents' diagnostics at once, not only from the next server start.
    fn answer_style(&mut self, code: String, choice: crate::project::config::StyleChoice) -> Result<Json, String> {
        let path = crate::project::config::record_style_choice(&self.root, &code, choice)?;
        if choice == crate::project::config::StyleChoice::Off {
            self.silenced.push(code);
            let open: Vec<PathBuf> = self.docs.values().map(|d| d.path.clone()).collect();
            let open: Vec<&Path> = open.iter().map(PathBuf::as_path).collect();
            let analyzed: Vec<(PathBuf, Rc<Analysis>)> =
                self.docs.values().filter_map(|d| Some((d.path.clone(), d.analysis.clone()?))).collect();
            for (path, analysis) in analyzed {
                let records = self.unsilenced(handlers::findings(&analysis));
                let mut published = publish(&mut self.actions, &analysis, &records, &path, &open);
                let versions: Vec<(PathBuf, i32)> = self.docs.values().map(|d| (d.path.clone(), d.version)).collect();
                for p in &mut published {
                    p.version = version_of(&versions, &p.uri);
                }
                self.send_diagnostics(published);
            }
        }
        Ok(serde_json::json!(path.display().to_string()))
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
        let dir = if crate::project::paths::within(&self.root, path) {
            self.root.clone()
        } else {
            crate::project::paths::dir_of(path)
        };
        let searched = move || discovery::scan(&dir).candidate_roots(path);
        let candidates = known.into_iter().chain(input.then(searched).into_iter().flatten());
        for root in candidates {
            let root_key = canonical(&root);
            if root_key == key || tried.contains(&root_key) {
                continue;
            }
            let run = self.run(&root);
            if self.cancelled {
                return run;
            }
            if self.runs.get(&root_key).is_some_and(|r| r.reads.contains(&key)) {
                self.orphans.remove(&key);
                self.owners.insert(key, root);
                return run;
            }
            if preamble.is_none() && crate::linter::shared::document_start(&run.0).is_some() {
                preamble = Some(root.clone());
            }
            // A root that does not read the file is dropped.
            drop(run);
            self.runs.retain(|_, run| Rc::strong_count(&run.analysis) > 1);
            tried.push(root_key);
        }
        if input {
            self.orphans.insert(key);
        }
        // A part of a package the project provides is read where the
        // package puts it: after loading it, as its own code.
        if input && let Some(owner) = discovery::scan(&crate::project::paths::dir_of(path)).package_owners(path).first()
        {
            let stem = owner.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            let (load, class) = match owner.extension().is_some_and(|e| e == "cls") {
                true => (String::new(), stem.to_string()),
                false => (format!("\\usepackage{{{stem}}}"), "article".to_string()),
            };
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or_default();
            let root = crate::project::paths::dir_of(path).join(format!(".satex-part-{stem}.tex"));
            let text = format!(
                "\\documentclass{{{class}}}{load}\\makeatletter\\input{{{name}}}\\makeatother\\begin{{document}}\\end{{document}}\n"
            );
            crate::project::overlay::set(&root, text);
            return self.run(&root);
        }
        // A package or class no root reads is analyzed as itself, as `-f` does:
        // read into a document's body its `@` names and preamble commands fail.
        use crate::project::config::Profile;
        if matches!(Profile::of_name(Some(path), text), Some(Profile::Package | Profile::Class)) {
            return self.run(path);
        }
        // LaTeX's own hook reads it once the root's preamble is done.
        if input && let Some(root) = preamble {
            let dir = crate::project::paths::dir_of(&root);
            let name = |p: &Path| p.strip_prefix(&dir).unwrap_or(p).with_extension("").display().to_string();
            let stem = root.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            let part = dir.join(format!(".satex-part-{stem}.tex"));
            let text =
                format!("\\AddToHook{{begindocument/end}}{{\\input{{{}}}}}\\input{{{}}}\n", name(path), name(&root));
            crate::project::overlay::set(&part, text);
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
        let source = crate::project::overlay::read_to_string(root).unwrap_or_default();
        // A newer text, of any open file, makes this run stale: it stops, the next one starts.
        #[cfg(not(target_os = "emscripten"))]
        let stale: Box<dyn Fn() -> bool> = {
            let (kept, taken) = (self.hovers.clone(), self.taken);
            Box::new(move || kept.edits.load(std::sync::atomic::Ordering::Relaxed) > taken)
        };
        #[cfg(target_os = "emscripten")]
        let stale: Box<dyn Fn() -> bool> = Box::new(wasm::cancelled);
        let (analysis, cancelled) = crate::analysis::probe::cancellable(stale, || {
            self.serving(|engine| {
                let cfg = engine.cfg;
                match discovery::is_document(&source) {
                    // The build's files (`.aux`, the `.bbl`) stay for the next run.
                    true => Machine::build(
                        engine
                            .checkpoints
                            .entry(key.clone())
                            .or_insert_with(|| crate::analysis::machine::Checkpoints::new(cfg)),
                        &source,
                        root,
                        cfg,
                        true,
                    ),
                    false => Machine::analyze(&source, Some(root), cfg),
                }
            })
        });
        let analysis = Rc::new(analysis);
        if cancelled {
            self.cancelled = true;
            return (analysis, false);
        }
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
    /// from the analyses there are (nothing yet before a document's first one,
    /// never a wait), and the others wait for its end.
    fn serving<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let Some(inbox) = self.inbox.clone() else { return f(self) };
        let (mut docs, hovers, blank) = (self.docs.clone(), self.hovers.clone(), self.blank.clone());
        let deferred = Rc::new(std::cell::RefCell::new(Vec::new()));
        let waiting = deferred.clone();
        let pause = Box::new(move || {
            while let Ok(msg) = inbox.try_recv() {
                match msg {
                    EngineMsg::Change(ref uri, ref text, _) => {
                        if let Some(doc) = docs.get_mut(uri) {
                            doc.text = text.clone();
                        }
                        waiting.borrow_mut().push(msg);
                    }
                    msg => {
                        let answered = crate::analysis::probe::apart(|| answer(&docs, &hovers, blank.as_deref(), msg));
                        if let Some(msg) = answered {
                            waiting.borrow_mut().push(msg);
                        }
                    }
                }
            }
        });
        let out = crate::util::progress::pausing(pause, || f(self));
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
        let dir = crate::project::paths::dir_of(Path::new(analysis.file_name(analysis.main_file)));
        // The project's own names: defined by the document or in a file of the project, a
        // written `.aux` among them, whatever package's code read it there.
        let own: std::collections::HashSet<String> = analysis
            .facts
            .defs
            .iter()
            .filter(|d| {
                d.package.is_none() || crate::project::paths::within(&dir, Path::new(analysis.file_name(d.span.file)))
            })
            .map(|d| name(d.name))
            .collect();
        // The commands the document's own files call, not the kernel's inner ones.
        let mut owned: HashMap<crate::engine::tex::FileId, bool> = HashMap::default();
        let mut used: HashMap<crate::engine::tex::Sym, u32> = HashMap::default();
        for e in &analysis.facts.expansions {
            let file = e.span.file;
            if *owned
                .entry(file)
                .or_insert_with(|| crate::project::paths::within(&dir, Path::new(analysis.file_name(file))))
            {
                *used.entry(e.name).or_default() += e.count.max(1);
            }
        }
        let mut used: Vec<(u32, String)> = used.into_iter().map(|(sym, n)| (n, name(sym))).collect();
        // a called name's hover says what this document's calls typeset: asked again each run
        let called: std::collections::HashSet<String> = used.iter().map(|(_, n)| n.clone()).collect();
        used.sort_by(|a, b| b.cmp(a));
        let store = hover_store(self.cfg, analysis, &dir);
        let docs = crate::plugins::docs::index(&self.cfg.docs);
        let mut dirs = std::collections::HashSet::new();
        for file in analysis.file_names().iter().map(Path::new).filter(|_| self.cfg.plugins.docs) {
            if dirs.insert(file.parent()) {
                docs.request(file);
            }
        }
        let Ok(mut hovers) = self.hovers.lock() else { return };
        hovers.retain(|(u, n), _| u != uri || same && !own.contains(n) && !called.contains(n));
        // What an earlier session kept for these same loads.
        if !same && let Some(path) = &store {
            let kept: HashMap<String, Option<String>> =
                std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
            // A name a manual mined since documents may have been kept without it.
            for (name, markdown) in
                kept.into_iter().filter(|(name, _)| !own.contains(name) && !called.contains(name) && !docs.knows(name))
            {
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
        self.serving(Self::prefill_next);
    }

    fn prefill_next(&mut self) {
        let Some((uri, name)) = self.prefill.pop_front() else { return };
        let last = !self.prefill.iter().any(|(u, _)| *u == uri);
        let markdown = crate::analysis::probe::background(|| {
            crate::util::progress::because("hover computed ahead", || {
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

    /// Analyzes every document edited since the last call, the latest
    /// first. One the project takes as given (a submodule's, another
    /// repository's, `project.frozen`) waits until a request asks for it.
    fn flush(&mut self) {
        let frozen = crate::linter::Frozen::new(&self.cfg.project, &self.root);
        let dirty = std::mem::take(&mut self.dirty);
        self.cancelled = false;
        for uri in dirty.iter().rev() {
            // after a stopped run the rest waits for the newer text too
            if self.cancelled {
                if !self.dirty.contains(uri) {
                    self.dirty.push(uri.clone());
                }
                continue;
            }
            if self.docs.get(uri).is_some_and(|d| d.analysis.is_some() || !frozen.holds(&d.path)) {
                self.analyze(uri);
            }
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
        if matches!(msg, EngineMsg::Open(..) | EngineMsg::Change(..)) {
            self.taken += 1;
        }
        match msg {
            EngineMsg::Shutdown => {}
            EngineMsg::Open(uri, path, text, version) => {
                handlers::VIEW.with_borrow_mut(|view| view.add(&path));
                if self.blank.is_none() {
                    let mut cfg = self.cfg.clone();
                    cfg.load.format = false;
                    self.blank = Some(Rc::new(Machine::analyze("", None, &cfg)));
                }
                self.docs.insert(uri.clone(), Doc { text, version, path, analysis: None, analyzed: None, shift: None });
                if !self.dirty.contains(&uri) {
                    self.dirty.push(uri);
                }
            }
            EngineMsg::Change(uri, text, version) => {
                // The last analysis answers until the next one replaces it,
                // so a request never waits out a whole run.
                if let Some(doc) = self.docs.get_mut(&uri) {
                    self.edited.insert(uri.clone(), std::time::Instant::now());
                    doc.text = text;
                    doc.version = version.unwrap_or(doc.version);
                    if !self.dirty.contains(&uri) {
                        self.dirty.push(uri);
                    }
                }
            }
            EngineMsg::Close(uri) => {
                self.published.remove(&uri);
                self.edited.remove(&uri);
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
            | EngineMsg::Resolve(..)
            | EngineMsg::DocumentSymbol(..)
            | EngineMsg::Typeset(..)
            | EngineMsg::Written(..)
            | EngineMsg::Caches(..)
            | EngineMsg::Dialog(..)
            | EngineMsg::WorkspaceSymbol(..)) => {
                // A completion never waits for a run: it answers from the last one.
                if !matches!(msg, EngineMsg::Completion(..))
                    && let Some(uri) = msg.uri().or_else(|| self.docs.keys().next()).cloned()
                {
                    self.ensure(&uri);
                }
                answer(&self.docs, &self.hovers, self.blank.as_deref(), msg);
            }
            EngineMsg::Highlight(uri, pos, reply) => {
                self.fresh(&uri);
                let path = self.docs.get(&uri).map(|d| d.path.display().to_string()).unwrap_or_default();
                let result =
                    with_doc(&self.docs, &uri, |analysis, text| handlers::highlights(analysis, text, &path, pos));
                let _ = reply.send(result.flatten());
            }
            EngineMsg::Colors(uri, reply) => {
                let path = self.docs.get(&uri).map(|d| d.path.display().to_string()).unwrap_or_default();
                let result = with_doc_moved(&self.docs, &uri, |analysis, text| plugins::colors(analysis, text, &path));
                let _ = reply.send(result.unwrap_or_default());
            }
            EngineMsg::Inlays(uri, reply) => {
                let path = self.docs.get(&uri).map(|d| d.path.display().to_string()).unwrap_or_default();
                let result = with_doc_moved(&self.docs, &uri, |analysis, text| plugins::inlays(analysis, text, &path));
                let _ = reply.send(result.unwrap_or_default());
            }
            EngineMsg::Lenses(uri, reply) => {
                let path = self.docs.get(&uri).map(|d| d.path.display().to_string()).unwrap_or_default();
                let result = with_doc_moved(&self.docs, &uri, |analysis, text| plugins::lenses(analysis, text, &path));
                let _ = reply.send(result.unwrap_or_default());
            }
            EngineMsg::Presentation(uri, range, color, reply) => {
                let path = self.docs.get(&uri).map(|d| d.path.display().to_string()).unwrap_or_default();
                let result = with_doc(&self.docs, &uri, |analysis, text| {
                    plugins::color_presentations(analysis, text, &path, range, color)
                });
                let _ = reply.send(result.unwrap_or_default());
            }
            EngineMsg::View(uri, pos, plugin, reply) => {
                let path = self.docs.get(&uri).map(|d| d.path.display().to_string()).unwrap_or_default();
                let result = with_doc(&self.docs, &uri, |analysis, text| match (pos, plugin) {
                    (None, Some(name)) => plugins::manager(analysis, text, &path, &name).into_iter().collect(),
                    (pos, _) => plugins::views(analysis, text, &path, handlers::to_pos(text, pos.unwrap_or_default())),
                });
                let _ = reply.send(result.unwrap_or_default());
            }
            EngineMsg::Links(uri, reply) => {
                let path = self.docs.get(&uri).map(|d| d.path.display().to_string()).unwrap_or_default();
                let result =
                    with_doc(&self.docs, &uri, |analysis, text| handlers::document_links(analysis, text, &path));
                let _ = reply.send(result.unwrap_or_default());
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
            EngineMsg::CodeAction(uri, pos, reply) => {
                self.fresh(&uri);
                let mut out = self.actions.get(&uri).cloned().unwrap_or_default();
                let path = self.docs.get(&uri).map(|d| d.path.display().to_string()).unwrap_or_default();
                let typed = with_doc(&self.docs, &uri, |analysis, text| {
                    plugins::actions(analysis, text, &path, handlers::to_pos(text, pos))
                });
                out.extend(typed.into_iter().flatten().map(|a| serde_json::to_value(a).unwrap_or_default()));
                let _ = reply.send(out);
            }
            EngineMsg::Style(code, choice, reply) => {
                let _ = reply.send(self.answer_style(code, choice));
            }
            EngineMsg::ExecuteCommand(body, reply) => {
                // An open document's own analysis answers; another file is run.
                let file = body.get("file").and_then(Json::as_str).map(PathBuf::from);
                let open = self.docs.iter().find(|(_, d)| file.as_ref().is_none_or(|f| *f == d.path));
                let held = open.map(|(uri, _)| uri.clone()).and_then(|uri| {
                    self.fresh(&uri);
                    with_doc(&self.docs, &uri, |a, text| crate::cli::query::answer_request(&body, a, text.body()))?
                });
                let active = self.docs.values().next().map(|d| d.path.clone()).unwrap_or_else(|| self.root.clone());
                let result =
                    held.map_or_else(|| crate::cli::query::run_request_value(&body, self.cfg, Some(&active)), Ok);
                let _ = reply.send(result);
            }
        }
    }
}

/// Answers `msg` from the analyses in `docs` when it only reads one; any
/// other message comes back.
fn answer(docs: &HashMap<Uri, Doc>, hovers: &Hovers, blank: Option<&Analysis>, msg: EngineMsg) -> Option<EngineMsg> {
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
                            crate::util::progress::because("hover", || handlers::command_markdown(analysis, &name));
                        if let Ok(mut hovers) = hovers.lock() {
                            hovers.insert(key, markdown.clone());
                        }
                        markdown
                    });
                    // what a type plugin adds for the name's type (a length register's value in every unit)
                    let typed = plugins::command_hover(analysis, text, &path, at, &name);
                    let markdown = match (markdown, typed) {
                        (Some(m), Some(t)) => Some(format!("{m}\n\n{t}")),
                        (m, t) => m.or(t),
                    };
                    let link = dialogs::hover_link(analysis, text, &path, uri.as_str(), pos);
                    let markdown = markdown.map(|m| handlers::placed(analysis, m, &name, &path, at));
                    return with_link(markdown, link).map(handlers::markdown_hover);
                }
                let link = dialogs::hover_link(analysis, text, &path, uri.as_str(), pos);
                match (handlers::hover(analysis, text, &path, pos), link) {
                    (Some(mut h), Some(link)) => {
                        if let lsp_types::HoverContents::Markup(m) = &mut h.contents {
                            m.value = format!("{link}\n\n---\n\n{}", m.value);
                        }
                        Some(h)
                    }
                    (h, link) => h.or_else(|| link.map(handlers::markdown_hover)),
                }
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
            let result = crate::util::progress::because("signature help", || {
                with_doc(docs, &uri, |analysis, text| handlers::signature_help(analysis, text, pos))
            });
            let _ = reply.send(result.flatten());
        }
        EngineMsg::Completion(uri, pos, reply) => {
            let path = docs.get(&uri).map(|d| d.path.display().to_string()).unwrap_or_default();
            let result = crate::util::progress::because("completion", || {
                with_doc_or(docs, blank, &uri, |analysis, text| {
                    match bib::completion(analysis, text, &path, handlers::to_pos(text, pos)) {
                        Some(items) => lsp_types::CompletionResponse::Array(items),
                        None => handlers::completion(analysis, text, &path, pos),
                    }
                })
            });
            // Each item says which document it came from, for its resolve.
            let mut result = result.unwrap_or(lsp_types::CompletionResponse::Array(Vec::new()));
            let items = match &mut result {
                lsp_types::CompletionResponse::Array(items) => items,
                lsp_types::CompletionResponse::List(list) => &mut list.items,
            };
            for item in items.iter_mut().filter(|i| i.data.is_none()) {
                item.data = Some(serde_json::json!({ "uri": uri.as_str() }));
            }
            let _ = reply.send(result);
        }
        EngineMsg::Resolve(mut item, reply) => {
            // A command's documentation is its hover, kept or made from the last analysis.
            let uri = item.data.as_ref().and_then(|d| d["uri"].as_str()).and_then(|u| u.parse::<Uri>().ok());
            let name = format!("\\{}", item.label.trim_start_matches('\\'));
            let markdown = uri.and_then(|uri| {
                let key = (uri.clone(), name.clone());
                hovers.lock().ok().and_then(|h| h.get(&key).cloned()).or_else(|| {
                    with_doc(docs, &uri, |analysis, _| {
                        crate::util::progress::because("completion docs", || {
                            handlers::command_markdown(analysis, &name)
                        })
                    })
                })
            });
            if let Some(Some(markdown)) = markdown {
                item.documentation = Some(lsp_types::Documentation::MarkupContent(lsp_types::MarkupContent {
                    kind: lsp_types::MarkupKind::Markdown,
                    value: markdown,
                }));
            }
            let _ = reply.send(*item);
        }
        EngineMsg::DocumentSymbol(uri, reply) => {
            let path = docs.get(&uri).map(|d| d.path.display().to_string());
            let result = match path {
                Some(path) => with_doc_moved(docs, &uri, |analysis, text| {
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
        EngineMsg::Written(uri, reply) => {
            let or = format!("{}\n", crate::engine::tex::TEXT_OR);
            let files = |a: &Analysis| {
                let texts = |t: &String| t.split(or.as_str()).map(str::to_owned).collect::<Vec<_>>();
                a.written
                    .iter()
                    .map(|(name, text)| {
                        // each run's texts too: how the build got there
                        let runs: Vec<_> = (a.written_runs.iter())
                            .map(|run| run.iter().find(|(n, _)| n == name).and_then(|(_, t)| t.as_ref().map(texts)))
                            .collect();
                        serde_json::json!({"name": name, "texts": text.as_ref().map(texts), "runs": runs})
                    })
                    .collect()
            };
            let _ = reply.send(with_doc(docs, &uri, |analysis, _| files(analysis)).unwrap_or_default());
        }
        EngineMsg::Caches(uri, reply) => {
            // The run's size first, then what it reused: what the playground's timing pop-up shows.
            let report = |a: &Analysis| {
                let run = format!(
                    "run {} steps ({} loading), {} forks, {} peak paths, {} files, {} lines",
                    a.steps,
                    a.load_steps,
                    a.forks,
                    a.peak_paths,
                    a.files.len(),
                    a.lines
                );
                let files = a.timings.by_file();
                let total: f64 = files.iter().map(|(_, t, _)| t.as_secs_f64()).sum();
                let slow = files.into_iter().take(5).map(move |(id, t, _)| {
                    format!(
                        "slow {:.0} {:.0} {}",
                        t.as_secs_f64() * 1000.0,
                        100.0 * t.as_secs_f64() / total.max(1e-9),
                        a.file_name(id)
                    )
                });
                let limits = a.limits.iter().map(|l| format!("limit {} {} ({}×)", l.key, l.value, l.count));
                // the main file's tokens per line, and each lint rule's time (µs) and findings
                let heat = std::iter::once(format!("heat {}", a.file_name(a.main_file)))
                    .chain(a.timings.main_heat().map(|(line, n)| format!("line {line} {n}")));
                let rules =
                    a.timings.rules().into_iter().map(|(code, t, n)| format!("rule {code} {} {n}", t.as_micros()));
                std::iter::once(run)
                    .chain(slow)
                    .chain(limits)
                    .chain(heat)
                    .chain(rules)
                    .chain(a.cache_report())
                    .chain(plugins::incremental::counts())
                    .collect::<Vec<_>>()
            };
            let _ = reply.send(with_doc(docs, &uri, |analysis, _| report(analysis)).unwrap_or_default());
        }
        EngineMsg::Dialog(ask, reply) => {
            let uri = match &ask {
                Ask::Actions(u, _) | Ask::Open(u, _) | Ask::Apply(u, _) => u.clone(),
            };
            let path = docs.get(&uri).map(|d| d.path.display().to_string()).unwrap_or_default();
            let result = with_doc(docs, &uri, |analysis, text| match ask {
                Ask::Actions(_, range) => Ok(Json::Array(dialogs::actions(analysis, text, &path, uri.as_str(), range))),
                Ask::Open(_, pos) => Ok(dialogs::dialog(analysis, text, &path, pos).unwrap_or(Json::Null)),
                Ask::Apply(_, p) => {
                    let range = serde_json::from_value(p["range"].clone()).map_err(|e| e.to_string())?;
                    let kind = p["kind"].as_str().unwrap_or_default();
                    dialogs::apply(analysis, text, kind, range, &p["state"], p["op"].as_str())
                }
            });
            let _ = reply.send(result.unwrap_or(Ok(Json::Null)));
        }
        msg => return Some(msg),
    }
    None
}

/// A hover's text with the dialog's link first.
fn with_link(markdown: Option<String>, link: Option<String>) -> Option<String> {
    match (markdown, link) {
        (Some(m), Some(l)) => Some(format!("{l}\n\n---\n\n{m}")),
        (m, l) => m.or(l),
    }
}

fn engine_loop(rx: Receiver<EngineMsg>, mut engine: Engine, debounce: Duration) {
    loop {
        let received = if let Some(msg) = engine.deferred.pop_front() {
            Ok(msg)
        } else if !engine.dirty.is_empty() {
            rx.recv_timeout(debounce)
        } else if !engine.prefill.is_empty() {
            // Idle: one more hover, unless a message waits.
            crate::analysis::probe::WANTED.store(false, std::sync::atomic::Ordering::Relaxed);
            match rx.try_recv() {
                Ok(msg) => Ok(msg),
                Err(TryRecvError::Empty) => {
                    engine.prefill_one();
                    if engine.prefill.is_empty() {
                        crate::util::progress::idle();
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
        crate::util::progress::idle();
    }
}

/// The version of the open document at `uri` among `versions`.
fn version_of(versions: &[(PathBuf, i32)], uri: &Uri) -> Option<i32> {
    let path = handlers::uri_to_path(uri)?;
    versions.iter().find(|(p, _)| crate::project::paths::same_file(p, &path)).map(|(_, v)| *v)
}

/// The diagnostics of `analysis` to publish, by file, with an empty list
/// for `own` and each open file it reads that has none, clearing what an
/// earlier run found there; stashes their quick fixes for
/// `textDocument/codeAction`.
fn publish(
    actions: &mut HashMap<Uri, Vec<Json>>,
    analysis: &Analysis,
    records: &[crate::queries::query::Record],
    own: &Path,
    open: &[&Path],
) -> Vec<PublishDiagnosticsParams> {
    let names = analysis.file_names();
    let read = |p: &&Path| names.iter().any(|f| crate::project::paths::same_file(Path::new(f), p));
    // The findings of the file worked on and of the other open files it
    // reads: the rest of the run is context.
    let shown: Vec<String> = std::iter::once(own)
        .chain(open.iter().copied().filter(read))
        .map(|p| handlers::path_uri(&p.display().to_string()).to_string())
        .collect();
    let mut groups: Vec<Json> = handlers::diagnostics_json(records)
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
    with_doc_or(docs, None, uri, f)
}

/// [`with_doc`] for a result of the whole document: made from the text the run read
/// and moved as the edits since moved it ([`Doc::shift`]).
fn with_doc_moved<T: serde::Serialize + serde::de::DeserializeOwned>(
    docs: &HashMap<Uri, Doc>,
    uri: &Uri,
    f: impl FnOnce(&Analysis, &Text) -> T,
) -> Option<T> {
    let doc = docs.get(uri)?;
    let (Some(shift), Some(analyzed)) = (doc.shift, &doc.analyzed) else { return with_doc(docs, uri, f) };
    let analysis = doc.analysis.as_deref()?;
    let text = Text::new(analyzed.clone());
    let out = crate::engine::source::with(analysis.plugins.engine.encoding(), || f(analysis, &text));
    let mut value = serde_json::to_value(out).ok()?;
    shift.apply(&mut value);
    serde_json::from_value(value).ok()
}

/// [`with_doc`], with `blank` standing in for a document not analyzed yet.
fn with_doc_or<T>(
    docs: &HashMap<Uri, Doc>,
    blank: Option<&Analysis>,
    uri: &Uri,
    f: impl FnOnce(&Analysis, &Text) -> T,
) -> Option<T> {
    let doc = docs.get(uri)?;
    let analysis = doc.analysis.as_deref().or(blank)?;
    let text = Text::new(doc.text.clone());
    // Columns count what this document's engine reads, whatever was
    // analyzed last on this thread (`crate::engine::source`).
    Some(crate::engine::source::with(analysis.plugins.engine.encoding(), || f(analysis, &text)))
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
        if at != crate::analysis::probe::State::End
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
