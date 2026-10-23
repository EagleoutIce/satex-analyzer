//! The SaTeX knowledge base (`knowledge.md`): what the types
//! of arguments and the BibTeX styles are, for hover.

/// `## alias, alias` headings, each with the Markdown that follows it.
fn entries() -> impl Iterator<Item = (Vec<&'static str>, &'static str)> {
    include_str!("knowledge.md").split("\n## ").map(|e| e.trim_start_matches("## ")).filter_map(|e| {
        let (head, body) = e.split_once('\n')?;
        Some((head.split(',').map(str::trim).collect(), body.trim()))
    })
}

/// The entry named `name`, as its first name and text.
pub fn topic(name: &str) -> Option<(&'static str, &'static str)> {
    entries().find(|(names, _)| names.contains(&name)).map(|(names, body)| (names[0], body))
}

/// The hover section for each of `names` that has an entry, once each.
pub fn about(names: &[String]) -> String {
    let mut seen = Vec::new();
    let mut out = String::new();
    for (title, body) in names.iter().filter_map(|n| topic(n)) {
        if !seen.contains(&title) {
            seen.push(title);
            out.push_str(&format!("#### About {title}\n\n{body}\n\n"));
        }
    }
    out
}
