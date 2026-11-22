//! The Typeset view's rows as `satex/typesetRows`, sent while a run goes on and once it ends.
//! A run's rows go out in parts of `lsp.rows_per_message` (`part`, `more`), each built, sent and
//! dropped before the next: the server never holds the document's rows. Final rows have a
//! `serial`. The first rows a run sets as the last one did (the text before an edit) are not sent
//! again: `keep` says how many of the rows `base` stand. A paragraph preview is a `patch` of the
//! final rows it names (`base`).

use super::{handlers, notify_to};
use crate::analysis::machine::{Analysis, RunHooks};
use crossbeam_channel::Sender;
use lsp_server::Notification;
use lsp_types::Uri;
use serde_json::Value as Json;
use std::rc::Rc;

#[derive(Default)]
pub(super) struct TypesetStream {
    /// The document whose typeset rows go out as soon as its run ends, before its lints (`satex/analyze` `typeset`).
    pub(super) early: Option<Uri>,
    /// The final rows last sent, of this document, as a preview's patch needs them: their `serial`, where each stands.
    sent: Option<(Uri, u64, Rc<Vec<handlers::RowAt>>)>,
    /// How many final sets went out: each one's `serial`.
    serial: u64,
    /// `lsp.rows_per_message`.
    pub(super) part: usize,
}

impl TypesetStream {
    pub(super) fn new(part: usize) -> Self {
        TypesetStream { part, ..Default::default() }
    }

    /// The hooks a run of `uri` sends its early rows through. An edit inside a paragraph sends
    /// its rows first, from a run to the paragraph's end. Every row after the edit follows as
    /// soon as the run meets the last one apart from the page being built: the last run's rows,
    /// its page breaks pending until this run ends.
    pub(super) fn hooks(&self, uri: &Uri, version: Option<i32>, out: &Sender<Notification>) -> RunHooks {
        if self.early.as_ref() != Some(uri) {
            return RunHooks::default();
        }
        let sent = self.sent.as_ref().filter(|(u, ..)| u == uri).map(|(_, serial, rows)| (*serial, rows.clone()));
        let preview = sent.map(|(base, rows)| {
            let (uri, out) = (uri.clone(), out.clone());
            Box::new(move |run: Analysis, path: &str, lo, hi| {
                if let Some(Json::Object(mut patch)) = handlers::typeset_patch(&rows, &run, path, lo, hi) {
                    let fresh = patch.remove("rows").unwrap_or_default();
                    patch.insert("base".into(), base.into());
                    let mut head = head(&uri, version, true);
                    head.insert("patch".into(), patch.into());
                    head.insert("rows".into(), fresh);
                    note(&out, head);
                }
            }) as crate::analysis::machine::Preview
        });
        let (uri, out, part) = (uri.clone(), out.clone(), self.part);
        let provisional: crate::analysis::machine::Lax =
            Box::new(move |run| drop(send(&out, head(&uri, version, true), &run, part, None)));
        RunHooks { preview, provisional: Some(provisional) }
    }

    /// Sends the rows of `analysis`, a new run of `uri`, and keeps where each stands for the next
    /// preview and its hash for the next run.
    pub(super) fn finished(
        &mut self,
        uri: &Uri,
        version: Option<i32>,
        analysis: &Analysis,
        out: &Sender<Notification>,
    ) {
        if self.early.as_ref() != Some(uri) {
            return;
        }
        // A serial no other server's run has: a page that outlives its server (a restart, `satex gui`
        // reloaded) never takes these rows' base for one it holds.
        let fresh =
            || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_micros() as u64);
        self.serial = if self.serial == 0 { (fresh() & ((1 << 52) - 1)).max(1) } else { self.serial + 1 };
        let mut head = head(uri, version, false);
        head.insert("serial".into(), self.serial.into());
        let base = self.sent.take().filter(|(u, ..)| u == uri).map(|(_, serial, rows)| (serial, rows));
        let rows = send(out, head, analysis, self.part, base.as_ref().map(|(serial, rows)| (*serial, &rows[..])));
        self.sent = Some((uri.clone(), self.serial, Rc::new(rows)));
    }
}

/// What every `satex/typesetRows` of `uri`'s text `version` says; `preview` marks rows a later one replaces.
fn head(uri: &Uri, version: Option<i32>, preview: bool) -> serde_json::Map<String, Json> {
    let mut head = serde_json::Map::new();
    head.insert("uri".into(), serde_json::json!(uri));
    head.insert("version".into(), serde_json::json!(version));
    if preview {
        head.insert("preview".into(), Json::Bool(true));
    }
    head
}

/// The rows of `analysis` under `head`, in parts of `part` rows (0: one part); where each stands.
/// With the rows last sent (`base`: their serial, where each stood), the first rows equal to those
/// are counted (`keep`, `base`), not sent.
fn send(
    out: &Sender<Notification>,
    mut head: serde_json::Map<String, Json>,
    analysis: &Analysis,
    part: usize,
    base: Option<(u64, &[handlers::RowAt])>,
) -> Vec<handlers::RowAt> {
    let (mut rows, mut k, mut at) = (Vec::new(), 0u64, Vec::new());
    // how many rows stand so far, while every row has
    let mut keep = base.map(|_| 0usize);
    let mut flush = |rows: &mut Vec<Json>, keep: &mut Option<usize>, more: bool| {
        if let (Some(keep), Some((serial, _))) = (keep.take(), base) {
            head.insert("keep".into(), keep.into());
            head.insert("base".into(), serial.into());
        }
        let mut params = head.clone();
        params.insert("rows".into(), Json::Array(std::mem::take(rows)));
        params.insert("part".into(), k.into());
        params.insert("more".into(), more.into());
        note(out, params);
        k += 1;
    };
    handlers::typeset_each(analysis, |row| {
        at.push(handlers::RowAt::of(&row));
        if let (Some(kept), Some((_, base))) = (keep, base)
            && rows.is_empty()
            && base.get(kept).is_some_and(|was| was.hash == at[kept].hash)
        {
            keep = Some(kept + 1);
            return;
        }
        // the last part says so: a full one waits for the next row
        if part > 0 && rows.len() >= part {
            flush(&mut rows, &mut keep, true);
        }
        rows.push(row);
    });
    flush(&mut rows, &mut keep, false);
    at
}

/// One `satex/typesetRows` notification: its rows move into it, no copy of them is made.
fn note(out: &Sender<Notification>, params: serde_json::Map<String, Json>) {
    notify_to(out, Notification { method: "satex/typesetRows".into(), params: Json::Object(params) });
}
