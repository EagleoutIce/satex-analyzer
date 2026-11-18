//! The Typeset view's rows as `satex/typesetRows`, sent while a run goes on and once it ends.

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
    /// The rows last sent, of this document: what a paragraph preview patches.
    sent: Option<(Uri, Rc<Vec<Json>>)>,
}

impl TypesetStream {
    /// The hooks a run of `uri` sends its early rows through. An edit inside a paragraph sends
    /// its rows first, from a run to the paragraph's end. Every row after the edit follows as
    /// soon as the run meets the last one apart from the page being built: the last run's rows,
    /// its page breaks pending until this run ends.
    pub(super) fn hooks(&self, uri: &Uri, version: Option<i32>, out: &Sender<Notification>) -> RunHooks {
        if self.early.as_ref() != Some(uri) {
            return RunHooks::default();
        }
        let sent = self.sent.as_ref().filter(|(u, _)| u == uri).map(|(_, rows)| rows.clone());
        let preview = sent.map(|rows| {
            let (uri, out) = (uri.clone(), out.clone());
            Box::new(move |run: Analysis, path: &str, lo, hi| {
                notify(&out, &uri, version, &handlers::typeset_preview(&rows, &run, path, lo, hi), true)
            }) as crate::analysis::machine::Preview
        });
        let (uri, out) = (uri.clone(), out.clone());
        let provisional: crate::analysis::machine::Lax =
            Box::new(move |run| notify(&out, &uri, version, &handlers::typeset(&run), true));
        RunHooks { preview, provisional: Some(provisional) }
    }

    /// Sends the rows of `analysis`, a new run of `uri`, and keeps them for the next preview.
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
        let rows = Rc::new(handlers::typeset(analysis));
        self.sent = Some((uri.clone(), rows.clone()));
        notify(out, uri, version, &rows, false);
    }
}

/// One `satex/typesetRows` notification; `preview` marks rows a later one replaces.
fn notify(out: &Sender<Notification>, uri: &Uri, version: Option<i32>, rows: &[Json], preview: bool) {
    let mut params = serde_json::json!({"uri": uri, "version": version, "rows": rows});
    if preview {
        params["preview"] = Json::Bool(true);
    }
    notify_to(out, Notification::new("satex/typesetRows".into(), params));
}
