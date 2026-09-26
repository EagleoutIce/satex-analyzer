//! Carrying out quick-fixes: `satex lint --fix` and `--diff`.  Fixes are
//! applied in memory, the document is analyzed again on the result, and
//! this repeats until nothing fixable is left, since one fix can make
//! another possible.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value as Json;

use crate::config::Config;
use crate::lint::fix::{Applicability, Edit, Text};
use crate::machine::Machine;
use crate::query::Record;

/// How often the document is fixed and linted again at most.  Fixes settle
/// in two or three rounds (a clashing option moved to the first load leaves
/// a duplicate load, which the next round deletes); a fix that keeps coming
/// back stops here.
pub const MAX_ROUNDS: usize = 8;

/// One finding's fix, as its record carries it.
struct Planned {
    code: String,
    place: String,
    edits: Vec<Edit>,
}

fn place(record: &Record) -> String {
    let text = |key: &str| record.get(key).map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string));
    format!(
        "{}:{}:{}",
        text("file").unwrap_or_default(),
        text("line").unwrap_or_default(),
        text("col").unwrap_or_default()
    )
}

/// The fixes a run may apply: safe ones, and unsafe ones when asked for.
pub fn applicable(record: &Record, unsafe_fixes: bool) -> bool {
    match record.get("applicability").and_then(Json::as_str).and_then(Applicability::parse) {
        Some(Applicability::Safe) => true,
        Some(Applicability::Unsafe) => unsafe_fixes,
        None => false,
    }
}

fn planned(records: &[Record], unsafe_fixes: bool) -> Vec<Planned> {
    let mut out: Vec<Planned> = records
        .iter()
        .filter(|record| applicable(record, unsafe_fixes))
        .filter_map(|record| {
            let edits: Option<Vec<Edit>> = record.get("edits")?.as_array()?.iter().map(Edit::from_json).collect();
            Some(Planned {
                code: record.get("code").and_then(Json::as_str).unwrap_or_default().to_string(),
                place: place(record),
                edits: edits.filter(|edits| !edits.is_empty())?,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        let key = |p: &Planned| (p.edits[0].path.clone(), p.edits[0].start, p.edits[0].end, p.code.clone());
        key(a).cmp(&key(b))
    });
    out
}

/// A fix left out because its edits overlap one applied before it.
pub struct Skipped {
    pub code: String,
    pub place: String,
    pub by: String,
}

fn overlaps(a: &Edit, b: &Edit) -> bool {
    // Two different insertions at one place both go in, in fix order; the
    // same one twice is one too many.
    let same_insertion = a.start == a.end && a == b;
    a.path == b.path && ((a.start < b.end && b.start < a.end) || same_insertion)
}

/// Apply every fix whose edits overlap no fix taken before it, in the order
/// of where they start: the result does not depend on the order the rules
/// ran in.  `texts` holds the current text of every file touched so far.
fn apply_round(texts: &mut BTreeMap<String, String>, fixes: Vec<Planned>, skipped: &mut Vec<Skipped>) -> usize {
    let mut taken: Vec<(Edit, String)> = Vec::new();
    let mut applied = 0;
    for fix in fixes {
        for edit in &fix.edits {
            if !texts.contains_key(&edit.path)
                && let Ok(text) = crate::overlay::read_to_string(Path::new(&edit.path))
            {
                texts.insert(edit.path.clone(), text);
            }
        }
        let valid = fix.edits.iter().all(|edit| {
            texts.get(&edit.path).is_some_and(|body| {
                let text = Text::new(body.clone());
                matches!((text.offset(edit.start), text.offset(edit.end)), (Some(a), Some(b)) if a <= b)
            })
        });
        if !valid {
            continue;
        }
        let clash = fix.edits.iter().enumerate().find_map(|(i, edit)| {
            let inner = fix.edits[..i].iter().any(|other| overlaps(edit, other));
            let outer = taken.iter().find(|(other, _)| overlaps(edit, other)).map(|(_, by)| by.clone());
            outer.or_else(|| inner.then(|| format!("{} at {}", fix.code, fix.place)))
        });
        if let Some(by) = clash {
            skipped.push(Skipped { code: fix.code, place: fix.place, by });
            continue;
        }
        let owner = format!("{} at {}", fix.code, fix.place);
        taken.extend(fix.edits.into_iter().map(|edit| (edit, owner.clone())));
        applied += 1;
    }
    let mut by_file: BTreeMap<String, Vec<(usize, Edit)>> = BTreeMap::new();
    for (index, (edit, _)) in taken.into_iter().enumerate() {
        by_file.entry(edit.path.clone()).or_default().push((index, edit));
    }
    for (path, mut edits) in by_file {
        let Some(body) = texts.get_mut(&path) else { continue };
        let text = Text::new(std::mem::take(body));
        // From the end backwards, so that every offset still holds; at one
        // place the replaced range goes first and the insertions land before
        // it, the first fix's first.
        edits.sort_by(|(i, a), (j, b)| (b.start, b.end, j).cmp(&(a.start, a.end, i)));
        let mut result = text.body().to_string();
        for (_, edit) in edits {
            let (Some(start), Some(end)) = (text.offset(edit.start), text.offset(edit.end)) else { continue };
            result.replace_range(start..end, &edit.replacement);
        }
        *body = result;
    }
    applied
}

/// What fixing the document came to.
pub struct Outcome {
    /// Every file changed, with its text before and after.
    pub changed: BTreeMap<String, (String, String)>,
    pub applied: usize,
    pub rounds: usize,
    pub skipped: Vec<Skipped>,
    /// The findings of the last analysis, of the fixed text.
    pub remaining: Vec<Record>,
}

/// Fix, analyze again, and repeat until no applicable fix is left, no fix
/// changes anything, or [`MAX_ROUNDS`] is reached.  `main` is the document
/// as the analysis names it; `select` keeps the findings the run is about.
pub fn fixpoint(
    main: &str,
    cfg: &Config,
    first: Vec<Record>,
    select: &dyn Fn(&Record) -> bool,
    unsafe_fixes: bool,
) -> Outcome {
    let mut texts: BTreeMap<String, String> = BTreeMap::new();
    let mut originals: BTreeMap<String, String> = BTreeMap::new();
    let mut records = first;
    let mut applied = 0;
    let mut rounds = 0;
    let mut skipped = Vec::new();
    while rounds < MAX_ROUNDS {
        let fixes = planned(&records, unsafe_fixes);
        if fixes.is_empty() {
            break;
        }
        let before = texts.clone();
        for edit in fixes.iter().flat_map(|fix| &fix.edits) {
            if !originals.contains_key(&edit.path)
                && let Ok(text) = crate::overlay::read_to_string(Path::new(&edit.path))
            {
                originals.insert(edit.path.clone(), text);
            }
        }
        let count = apply_round(&mut texts, fixes, &mut skipped);
        rounds += 1;
        if count == 0 || texts == before {
            break;
        }
        applied += count;
        for (path, text) in &texts {
            crate::overlay::set(Path::new(path), text.clone());
        }
        let Ok(source) = crate::overlay::read_to_string(Path::new(main)) else { break };
        let analysis = Machine::analyze(&source, Some(Path::new(main)), cfg);
        records = crate::lint::lint(&analysis).into_iter().filter(|r| select(r)).collect();
    }
    crate::overlay::clear();
    let changed = texts
        .into_iter()
        .filter_map(|(path, text)| {
            let original = originals.remove(&path)?;
            (original != text).then_some((path, (original, text)))
        })
        .collect();
    Outcome { changed, applied, rounds, skipped, remaining: records }
}

/// Write the fixed files.  Only files a fix could name are here: the
/// project's, never the installation's (see [`crate::lint::fix::editable`]).
pub fn write(outcome: &Outcome) -> Result<(), String> {
    for (path, (_, text)) in &outcome.changed {
        std::fs::write(path, text).map_err(|e| format!("{path}: {e}"))?;
    }
    Ok(())
}

/// Lines of context around a change, as `diff -u` prints them.
const CONTEXT: usize = 3;
/// Beyond this many cells the line diff gives up on a minimal edit script
/// and replaces the changed region whole.
const MAX_TABLE: usize = 1 << 24;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    Same,
    Delete,
    Insert,
}

/// The shortest edit script between two lists of lines, by the longest
/// common subsequence of what lies between their common prefix and suffix.
fn script(a: &[&str], b: &[&str]) -> Vec<Op> {
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..].iter().rev().zip(b[prefix..].iter().rev()).take_while(|(x, y)| x == y).count();
    let (a_mid, b_mid) = (&a[prefix..a.len() - suffix], &b[prefix..b.len() - suffix]);
    let (m, n) = (a_mid.len(), b_mid.len());
    let mut ops = vec![Op::Same; prefix];
    if (m + 1) * (n + 1) > MAX_TABLE {
        ops.extend(std::iter::repeat_n(Op::Delete, m));
        ops.extend(std::iter::repeat_n(Op::Insert, n));
    } else {
        let width = n + 1;
        let mut table = vec![0u32; (m + 1) * width];
        for i in (0..m).rev() {
            for j in (0..n).rev() {
                table[i * width + j] = if a_mid[i] == b_mid[j] {
                    table[(i + 1) * width + j + 1] + 1
                } else {
                    table[(i + 1) * width + j].max(table[i * width + j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < m || j < n {
            if i < m && j < n && a_mid[i] == b_mid[j] {
                ops.push(Op::Same);
                i += 1;
                j += 1;
            } else if j == n || (i < m && table[(i + 1) * width + j] >= table[i * width + j + 1]) {
                ops.push(Op::Delete);
                i += 1;
            } else {
                ops.push(Op::Insert);
                j += 1;
            }
        }
    }
    ops.extend(std::iter::repeat_n(Op::Same, suffix));
    ops
}

fn push_line(out: &mut String, sign: char, line: &str) {
    out.push(sign);
    out.push_str(line);
    if !line.ends_with('\n') {
        out.push_str("\n\\ No newline at end of file\n");
    }
}

/// `old` against `new` in unified format, as `diff -u` writes it.
pub fn unified(path: &str, old: &str, new: &str) -> String {
    let a: Vec<&str> = old.split_inclusive('\n').collect();
    let b: Vec<&str> = new.split_inclusive('\n').collect();
    let ops = script(&a, &b);
    let mut out = format!("--- a/{path}\n+++ b/{path}\n");
    // Positions in `ops`, and the line of each file they start at.
    let mut at_a = Vec::with_capacity(ops.len() + 1);
    let mut at_b = Vec::with_capacity(ops.len() + 1);
    let (mut i, mut j) = (0, 0);
    for op in &ops {
        at_a.push(i);
        at_b.push(j);
        match op {
            Op::Same => {
                i += 1;
                j += 1;
            }
            Op::Delete => i += 1,
            Op::Insert => j += 1,
        }
    }
    at_a.push(i);
    at_b.push(j);
    let changes: Vec<usize> = (0..ops.len()).filter(|k| ops[*k] != Op::Same).collect();
    let mut k = 0;
    while k < changes.len() {
        let start = changes[k].saturating_sub(CONTEXT);
        let mut last = changes[k];
        while k + 1 < changes.len() && changes[k + 1] <= last + 2 * CONTEXT + 1 {
            k += 1;
            last = changes[k];
        }
        let end = (last + 1 + CONTEXT).min(ops.len());
        let (a_len, b_len) = (at_a[end] - at_a[start], at_b[end] - at_b[start]);
        let first = |at: usize, len: usize| if len == 0 { at } else { at + 1 };
        out.push_str(&format!("@@ -{},{a_len} +{},{b_len} @@\n", first(at_a[start], a_len), first(at_b[start], b_len)));
        for index in start..end {
            match ops[index] {
                Op::Same => push_line(&mut out, ' ', a[at_a[index]]),
                Op::Delete => push_line(&mut out, '-', a[at_a[index]]),
                Op::Insert => push_line(&mut out, '+', b[at_b[index]]),
            }
        }
        k += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::fix::Pos;

    #[test]
    fn unified_diff_marks_the_changed_line() {
        let diff = unified("x.tex", "a\nb\nc\n", "a\nB\nc\n");
        assert_eq!(diff, "--- a/x.tex\n+++ b/x.tex\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n");
    }

    #[test]
    fn unified_diff_of_a_deleted_line() {
        let diff = unified("x.tex", "a\nb\n", "a\n");
        assert_eq!(diff, "--- a/x.tex\n+++ b/x.tex\n@@ -1,2 +1,1 @@\n a\n-b\n");
    }

    #[test]
    fn overlapping_fixes_keep_the_first() {
        let edit = |start: u32, end: u32, text: &str| Edit {
            path: "virtual.tex".into(),
            start: Pos::new(1, start),
            end: Pos::new(1, end),
            replacement: text.into(),
        };
        let mut texts = BTreeMap::from([("virtual.tex".to_string(), "abcdef\n".to_string())]);
        let fixes = vec![
            Planned { code: "one".into(), place: "1".into(), edits: vec![edit(2, 4, "X")] },
            Planned { code: "two".into(), place: "2".into(), edits: vec![edit(3, 5, "Y")] },
            Planned { code: "three".into(), place: "3".into(), edits: vec![edit(5, 5, "+")] },
        ];
        let mut skipped = Vec::new();
        assert_eq!(apply_round(&mut texts, fixes, &mut skipped), 2);
        assert_eq!(texts["virtual.tex"], "aXd+ef\n");
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].code, "two");
    }
}
