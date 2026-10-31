//! Entries mined before, kept as a `.satex-docs` file in the package's
//! documentation directory: what an installation without the documented
//! sources (the browser's bundle) reads instead of them. `satex query`'s
//! `docs` writes them.

use std::path::Path;

use serde_json::{Value as Json, json};

use super::{Audience, Entry};

pub struct Mined;

/// The extension of a file of mined entries.
pub const EXTENSION: &str = "satex-docs";

impl super::DocPlugin for Mined {
    fn mine(&self, file: &Path, text: &str, out: &mut Vec<Entry>) {
        if file.extension().is_none_or(|e| e != EXTENSION) {
            return;
        }
        let Ok(Json::Array(rows)) = serde_json::from_str(text) else { return };
        out.extend(rows.iter().filter_map(entry));
    }
}

fn entry(row: &Json) -> Option<Entry> {
    let text = |k: &str| row.get(k).and_then(Json::as_str).map(str::to_string);
    let link = row
        .get("link")
        .and_then(Json::as_array)
        .and_then(|l| Some((l.first()?.as_str()?.into(), l.get(1)?.as_str()?.into())));
    Some(Entry {
        name: text("name")?,
        usage: text("usage"),
        text: text("text"),
        audience: if text("audience").as_deref() == Some("code") { Audience::Code } else { Audience::User },
        from: text("from").unwrap_or_default(),
        link,
    })
}

/// `entry` as a row of a `.satex-docs` file.
pub fn row(entry: &Entry) -> Json {
    json!({
        "name": entry.name,
        "usage": entry.usage,
        "text": entry.text,
        "audience": if entry.audience == Audience::Code { "code" } else { "user" },
        "from": entry.from,
        "link": entry.link.as_ref().map(|(label, url)| [label, url]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::docs::DocPlugin;

    #[test]
    fn a_mined_file_gives_back_its_entries() {
        let entry = Entry {
            name: "bar".into(),
            usage: Some("\\bar{⟨text⟩}".into()),
            text: Some("Draws a bar.".into()),
            audience: Audience::User,
            from: "pkg".into(),
            link: Some(("pkg manual".into(), "https://texdoc.org/pkg/pkg".into())),
        };
        let text = Json::Array(vec![row(&entry)]).to_string();
        let mut out = Vec::new();
        Mined.mine(Path::new("doc/latex/pkg/pkg.satex-docs"), &text, &mut out);
        assert_eq!(out, vec![entry]);
        let mut none = Vec::new();
        Mined.mine(Path::new("doc/latex/pkg/pkg.tex"), &text, &mut none);
        assert!(none.is_empty());
    }
}
