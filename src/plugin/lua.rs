//! Lua in a LuaTeX run: `\directlua`, `\latelua`, `\luaescapestring` and
//! the catcode tables (LuaTeX manual, "Lua related primitives", "Catcode
//! tables").  Packages built on them — `luacode`, expl3's `\lua_now:n`,
//! `ltluatex` — are run from their own source like any other.
//!
//! A chunk is the expanded argument turned into a string, as LuaTeX
//! receives it; it is recorded and run in the run's Lua state
//! ([`super::lua_run`]), and what it prints is read back.  A chunk that
//! does not run to its end leaves unknown output, and the names its text
//! defines through the token library unknown.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::builtins::{LoadKind, LuaOp, OccKind};
use crate::facts::{Load, LoadStatus};
use crate::machine::Machine;
use crate::tex::{Catcode, CatcodeTable, Span, Sym, Tok, Token};
use crate::value::Value;

/// The Lua files a run has read and its Lua state.  The catcode tables
/// themselves live in the save stack's table of meanings (`table_slot`),
/// so the format and package caches carry them like any other assignment.
#[derive(Default)]
pub struct Lua {
    read: HashSet<PathBuf>,
    /// Made when a run first needs it (see [`super::lua_run`]).
    pub(super) state: Option<mlua::Lua>,
    /// The runs in progress.
    pub(super) depth: usize,
    /// A cached segment ran Lua this state has not seen.
    pub(super) stale: bool,
}

impl Lua {
    /// The Lua files read so far, in path order.
    pub fn files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = self.read.iter().cloned().collect();
        files.sort();
        files
    }

    /// Whether the run has built Lua state: read a Lua file or run a chunk.
    pub fn ran(&self) -> bool {
        self.state.is_some() || !self.read.is_empty()
    }

    pub fn set_files(&mut self, files: Vec<PathBuf>) {
        self.stale |= files.iter().any(|f| !self.read.contains(f));
        self.read = files.into_iter().collect();
    }
}

/// How many `require`s deep satex follows a module into the modules it
/// loads itself.
const MODULE_DEPTH: usize = 8;

pub fn primitive(m: &mut Machine, op: LuaOp, by: Sym, span: Span) {
    match op {
        LuaOp::Direct | LuaOp::Late => {
            let code = chunk_text(m);
            let late = op == LuaOp::Late;
            m.occurrence(
                OccKind::Lua,
                code.trim().to_string(),
                late.then(|| "latelua".to_string()),
                span,
            );
            // `\latelua` runs at shipout, and what it prints then goes into
            // the page, not into the input satex reads.
            if late {
                for module in &Chunk::read(&code).modules {
                    load(m, module, span);
                }
                return;
            }
            match super::lua_run::chunk(m, &code, span) {
                Some(tokens) => m.push_tokens(Rc::from(tokens), None),
                None => {
                    unfinished(m, &code, span, 0);
                    m.note_gap("lua-output", by, span);
                    let unknown = m.unknown_token(span);
                    m.push_tokens(Rc::from(vec![unknown]), None);
                }
            }
        }
        LuaOp::Escape => {
            let text = chunk_text(m);
            let escaped = escape(&text);
            let tokens = string_tokens(&escaped, span);
            m.push_tokens(Rc::from(tokens), None);
        }
        LuaOp::InitCatcodes => {
            if let Some(n) = m.scan_number() {
                store(m, n, &initex());
            }
        }
        LuaOp::SaveCatcodes => {
            if let Some(n) = m.scan_number() {
                let table = m.catcodes.clone();
                store(m, n, &table);
            }
        }
        LuaOp::SelectCatcodes => {
            m.read_equals();
            let Some(n) = m.scan_number() else { return };
            select(m, n);
        }
    }
}

/// `\catcodetable n`: the table in force keeps what was done to it, and
/// table `n` comes in force.  The number is a local assignment, and so is
/// every catcode the switch changes, so a group undoes both.
fn select(m: &mut Machine, n: i64) {
    let current = current_table(m);
    let slot = m.intern("catcodetable");
    let table = m.catcodes.clone();
    store(m, current, &table);
    let global = m.prefixes.global;
    m.env.set_value(slot, Value::Int(n), global);
    if let Some(table) = stored(m, n) {
        let before = std::mem::replace(&mut m.catcodes, table);
        for (character, _) in m.catcodes.diff(&before) {
            m.env.save_catcode(character, before.get(character));
        }
    }
}

/// Where table `n` is kept: a name no tokenizer produces, bound to what
/// `\csname` would make of it anyway, `\relax`, with the table as its value
/// — each character whose catcode differs from LaTeX's, as a character
/// token of that catcode.  Tables are global (LuaTeX manual, "Catcode
/// tables").
fn table_slot(m: &mut Machine, n: i64) -> Sym {
    m.intern(&format!("catcodetable {n}"))
}

fn store(m: &mut Machine, n: i64, table: &CatcodeTable) {
    let slot = table_slot(m, n);
    let deltas: Vec<Token> = table
        .diff(&CatcodeTable::latex())
        .into_iter()
        .filter_map(|(c, code)| Catcode::from_u8(code).map(|cat| Token::new(Tok::Chr(c, cat), Span::default())))
        .collect();
    let mut binding = crate::env::Binding::builtin(crate::tex::Meaning::Primitive(
        crate::builtins::Primitive::Relax,
    ));
    binding.value = Value::Toks(Rc::from(deltas));
    m.env.set(slot, binding, true);
}

fn stored(m: &mut Machine, n: i64) -> Option<CatcodeTable> {
    let slot = table_slot(m, n);
    m.env.get(slot)?;
    let Value::Toks(deltas) = m.env.value(slot) else { return None };
    let mut table = CatcodeTable::latex();
    for token in deltas.iter() {
        if let Tok::Chr(c, cat) = token.tok {
            table.set(c, cat);
        }
    }
    Some(table)
}

/// `\the\catcodetable`: the table in force, 0 until one is selected.
pub fn current_table(m: &mut Machine) -> i64 {
    let slot = m.intern("catcodetable");
    m.env.value(slot).as_int().unwrap_or(0)
}

/// `\luatexversion` of the installed engine, as `luatex --version` states
/// it: version 1.24.0 is 124.
pub fn engine_version(cfg: &crate::config::Config) -> Option<i64> {
    let cache = cfg
        .cache
        .then(|| cfg.cache_dir.clone().or_else(crate::format::default_cache_dir))
        .flatten();
    let answer = crate::distribution::probed("luatex", "version", cache.as_deref(), || {
        let Ok(output) = std::process::Command::new("luatex").arg("--version").output() else {
            return Vec::new();
        };
        String::from_utf8_lossy(&output.stdout).lines().next().map(str::to_string).into_iter().collect()
    });
    let line = answer.into_iter().next()?;
    let version = line.split("Version ").nth(1)?.split_whitespace().next()?;
    let mut parts = version.split('.');
    let major: i64 = parts.next()?.parse().ok()?;
    let minor: i64 = parts.next()?.parse().ok()?;
    Some(major * 100 + minor)
}

/// The catcodes IniTeX starts with (tex.web § 232): letters are letters,
/// `\` escapes, `%` comments, space is a space, `^^M` ends a line, `^^@` is
/// ignored, `^^?` is invalid, and everything else is other.
fn initex() -> CatcodeTable {
    CatcodeTable::initex()
}

/// A general text, expanded and turned into the string LuaTeX hands Lua.
fn chunk_text(m: &mut Machine) -> String {
    let body = m.read_general_text();
    let expanded = m.expand_tokens(Rc::from(body));
    crate::tex::detokenize(&expanded, &m.out.interner)
}

/// `\luaescapestring`: a backslash, a quote and a newline are escaped so the
/// text reads back as itself inside a Lua string.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

/// Characters as a string conversion yields them: other, and space for a
/// space (tex.web § 464).
fn string_tokens(text: &str, span: Span) -> Vec<Token> {
    text.chars()
        .map(|c| {
            let cat = if c == ' ' { Catcode::Space } else { Catcode::Other };
            Token::new(Tok::Chr(c, cat), span)
        })
        .collect()
}

/// What one `tex.print` puts into the input: `print` makes every string a
/// line of its own, `sprint` continues the current one, and a leading number
/// picks the catcode table they are read with (-1 the one in force, -2
/// only other and space).
pub(super) fn print_tokens(m: &mut Machine, print: &Print, span: Span) -> Vec<Token> {
    let text = match print.lines {
        true => print.strings.iter().map(|s| format!("{s}\n")).collect::<String>(),
        false => print.strings.concat(),
    };
    match print.table {
        Some(-2) => string_tokens(&text, span),
        Some(n) if n >= 0 => match stored(m, n) {
            Some(table) => {
                let saved = std::mem::replace(&mut m.catcodes, table);
                let tokens = m.tokenize_at(&text, span);
                m.catcodes = saved;
                tokens
            }
            None => m.tokenize_at(&text, span),
        },
        _ => m.tokenize_at(&text, span),
    }
}

/// Records a module a chunk loads and, when satex finds its file, the
/// file: its path and number.
pub(super) fn load(m: &mut Machine, module: &Module, span: Span) -> Option<(PathBuf, crate::tex::FileId)> {
    let path = find(m, module, span);
    let status = if path.is_some() { LoadStatus::Read } else { LoadStatus::NotFollowed };
    let file = path.as_ref().map(|path| m.register_file(path.display().to_string(), LoadKind::Lua));
    let load_depth = m
        .out
        .facts
        .loads
        .iter()
        .rev()
        .find(|load| load.file == Some(span.file))
        .map_or(0, |load| load.depth + 1);
    let by = m.package();
    m.out.facts.loads.push(Load {
        name: module.name.clone(),
        kind: LoadKind::Lua,
        options: Vec::new(),
        span,
        by,
        file,
        path: path.as_ref().map(|p| p.display().to_string()),
        status,
        required: None,
        provided: None,
        depth: load_depth,
    });
    let path = path?;
    m.lua.read.insert(path.clone());
    Some((path, file?))
}

/// What a chunk that did not run to its end may have done, as far as its
/// text tells: the names it defines through the token library may mean
/// anything, and the modules it loads, read the same way, are loads.
fn unfinished(m: &mut Machine, code: &str, span: Span, depth: usize) {
    let chunk = Chunk::read(code);
    undefine(m, code, code);
    if depth >= MODULE_DEPTH {
        return;
    }
    for module in &chunk.modules {
        if m.out.facts.loads.iter().any(|l| l.kind == LoadKind::Lua && l.name == module.name) {
            continue;
        }
        let Some((path, file)) = load(m, module, span) else { continue };
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        unfinished(m, &text, Span::new(file, 1, 1), depth + 1);
    }
}

/// The names the part `rest` of Lua text `code` that did not run defines
/// through the token library may mean anything.
pub(super) fn undefine(m: &mut Machine, code: &str, rest: &str) {
    let named: HashSet<String> = lex(rest).into_iter().filter_map(|t| t.strip_prefix('"').map(str::to_string)).collect();
    for name in Chunk::read(code).defines.into_iter().filter(|n| named.contains(n)) {
        let sym = m.intern(&name);
        m.env.set(sym, crate::env::Binding::builtin(crate::tex::Meaning::Unknown), true);
    }
}

/// Where a module is: `require` looks for `name.lua`, with the dots of a
/// dotted name standing for directories, and `dofile` names the file
/// itself (Lua manual § 6.3; LuaTeX searches with kpathsea, "Lua modules").
fn find(m: &mut Machine, module: &Module, span: Span) -> Option<PathBuf> {
    let names: Vec<String> = match module.file {
        true => vec![module.name.clone()],
        false => {
            let name = module.name.strip_suffix(".lua").unwrap_or(&module.name);
            vec![format!("{name}.lua"), format!("{}.lua", name.replace('.', "/"))]
        }
    };
    let here = Path::new(m.out.file_name(span.file)).parent().map(Path::to_path_buf);
    let mut dirs: Vec<PathBuf> = here.into_iter().collect();
    dirs.push(m.base().to_path_buf());
    dirs.extend(m.cfg.search_paths.iter().cloned());
    dirs.extend(m.out.project.search_paths());
    for name in &names {
        for dir in &dirs {
            let path = dir.join(name);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    let base = m.base().to_path_buf();
    names.iter().find_map(|name| m.resolver_mut().resolve(name, LoadKind::Lua, &base))
}

/// A module a chunk loads.
#[derive(Clone, Debug, PartialEq)]
pub struct Module {
    pub name: String,
    /// `dofile`/`loadfile` name a file; `require` names a module.
    pub file: bool,
}

/// One call of `tex.print` or `tex.sprint`: its strings, whether each is a
/// line of its own, and the catcode table they are read with.
#[derive(Clone, Debug, PartialEq)]
pub struct Print {
    pub lines: bool,
    pub table: Option<i64>,
    pub strings: Vec<String>,
}

/// What satex reads from a chunk's text, for a chunk that did not finish.
#[derive(Clone, Debug, PartialEq)]
pub struct Chunk {
    pub modules: Vec<Module>,
    /// Control sequences the chunk defines through the token library, named
    /// by a string literal.
    pub defines: Vec<String>,
}

impl Chunk {
    pub fn read(code: &str) -> Chunk {
        let tokens = lex(code);
        let mut modules = Vec::new();
        for (at, token) in tokens.iter().enumerate() {
            if matches!(token.as_str(), "require" | "dofile" | "loadfile")
                && let Some(name) = argument(&tokens, at + 1)
            {
                modules.push(Module { name, file: token != "require" });
            }
        }
        let defines = defined_names(&tokens);
        Chunk { modules, defines }
    }
}

/// The functions of LuaTeX's token library that define the control
/// sequence their first argument names (LuaTeX manual, "The token library").
const DEFINING: [&str; 3] = ["set_lua", "set_macro", "set_char"];

fn is_name(token: &str) -> bool {
    token.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
}

/// Where the first argument of a call of a defining function starting at
/// `at` is: `token.set_lua(…)`, or a name standing for one, called with
/// parentheses or with a bare string.
fn defining_call(tokens: &[String], at: usize, definers: &HashSet<String>) -> Option<usize> {
    let after = if tokens[at] == "token"
        && tokens.get(at + 1).is_some_and(|t| t == ".")
        && tokens.get(at + 2).is_some_and(|t| DEFINING.contains(&t.as_str()))
    {
        at + 3
    } else if definers.contains(&tokens[at]) && (at == 0 || tokens[at - 1] != ".") {
        at + 1
    } else {
        return None;
    };
    let open = tokens.get(after)?;
    if open.starts_with('"') {
        return Some(after);
    }
    (open == "(").then_some(())?;
    // `token.set_macro(catcodetable, name, …)` may lead with a number.
    let first = after + 1;
    match tokens.get(first) {
        Some(n) if n.parse::<i64>().is_ok() && tokens.get(first + 1).is_some_and(|t| t == ",") => Some(first + 2),
        _ => Some(first),
    }
}

/// The names a chunk defines through the token library: literal first
/// arguments of `token.set_lua` and its relatives, of locals bound to them,
/// and of functions that pass their first parameter on to one (expl3.lua's
/// `luacmd`).
fn defined_names(tokens: &[String]) -> Vec<String> {
    let mut definers: HashSet<String> = HashSet::new();
    for at in 1..tokens.len() {
        if tokens[at] != "=" || !is_name(&tokens[at - 1]) {
            continue;
        }
        let rhs = &tokens[at + 1..tokens.len().min(at + 9)];
        let end = rhs.iter().position(|t| t == "=" || t == "local").unwrap_or(rhs.len());
        if rhs[..end].windows(3).any(|w| w[0] == "token" && w[1] == "." && DEFINING.contains(&w[2].as_str())) {
            definers.insert(tokens[at - 1].clone());
        }
    }
    loop {
        let mut found = false;
        for at in 0..tokens.len().saturating_sub(3) {
            if tokens[at] != "function" || tokens[at + 2] != "(" || !is_name(&tokens[at + 1]) || !is_name(&tokens[at + 3]) {
                continue;
            }
            let (function, parameter) = (&tokens[at + 1], &tokens[at + 3]);
            if definers.contains(function) {
                continue;
            }
            let Some(end) = block_end(tokens, at) else { continue };
            if (at + 1..end).any(|j| defining_call(tokens, j, &definers).is_some_and(|arg| tokens.get(arg) == Some(parameter))) {
                definers.insert(function.clone());
                found = true;
            }
        }
        if !found {
            break;
        }
    }
    (0..tokens.len())
        .filter_map(|at| defining_call(tokens, at, &definers))
        .filter_map(|arg| Some(tokens.get(arg)?.strip_prefix('"')?.to_string()))
        .collect()
}

/// A Lua-defined command: it calls function `id` (see
/// [`super::lua_run::function`]).  When that does not run to its end, what
/// it reads and what it puts back are unknown.
pub fn call(m: &mut Machine, id: u32, by: Sym, span: Span) {
    match super::lua_run::function(m, id, span) {
        Some(tokens) => m.push_tokens(Rc::from(tokens), None),
        None => {
            m.note_gap("lua-function", by, span);
            m.widen_mode();
            let unknown = m.unknown_token(span);
            m.push_tokens(Rc::from(vec![unknown]), None);
        }
    }
}

/// The `end` of the block `tokens[at]` opens: `function`, `if`, `do` (also
/// ending `while` and `for` headers) and `repeat` open one, `end` and
/// `until` close it.
fn block_end(tokens: &[String], at: usize) -> Option<usize> {
    let opens = |t: &str| matches!(t, "function" | "if" | "do" | "repeat");
    opens(tokens.get(at)?).then_some(())?;
    let mut depth = 0usize;
    for (j, t) in tokens.iter().enumerate().skip(at) {
        if opens(t) {
            depth += 1;
        } else if matches!(t.as_str(), "end" | "until") {
            depth -= 1;
            if depth == 0 {
                return Some(j);
            }
        }
    }
    None
}

/// The string a `require "x"` or `require("x")` names.
fn argument(tokens: &[String], at: usize) -> Option<String> {
    let token = tokens.get(at)?;
    if let Some(text) = token.strip_prefix('"') {
        return Some(text.to_string());
    }
    if token == "(" {
        let text = tokens.get(at + 1)?.strip_prefix('"')?;
        return (tokens.get(at + 2)? == ")").then(|| text.to_string());
    }
    None
}

/// Lua's tokens, as far as satex reads Lua: names and numbers, strings as
/// their value behind a `"`, and punctuation; comments are dropped (Lua
/// manual § 3.1).
pub fn lex(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c.is_whitespace() {
            at += 1;
        } else if c == '-' && chars.get(at + 1) == Some(&'-') {
            at += 2;
            match long_bracket(&chars, at) {
                Some(level) => at = skip_long(&chars, at, level),
                None => {
                    while at < chars.len() && chars[at] != '\n' {
                        at += 1;
                    }
                }
            }
        } else if c == '"' || c == '\'' {
            let mut s = String::from("\"");
            at += 1;
            while at < chars.len() && chars[at] != c {
                if chars[at] == '\\' && at + 1 < chars.len() {
                    at += 1;
                    s.push(match chars[at] {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        other => other,
                    });
                } else {
                    s.push(chars[at]);
                }
                at += 1;
            }
            at += 1;
            out.push(s);
        } else if let Some(level) = long_bracket(&chars, at) {
            let start = at + level + 2;
            let end = skip_long(&chars, at, level);
            let stop = end.saturating_sub(level + 2).max(start);
            // A newline right after the opening bracket is not part of the
            // string (Lua manual § 3.1).
            let start = if chars.get(start) == Some(&'\n') { start + 1 } else { start };
            let body: String = chars[start.min(stop)..stop].iter().collect();
            out.push(format!("\"{body}"));
            at = end;
        } else if c.is_alphanumeric() || c == '_' {
            let start = at;
            while at < chars.len() && (chars[at].is_alphanumeric() || chars[at] == '_') {
                at += 1;
            }
            out.push(chars[start..at].iter().collect());
        } else if c == '.' && chars.get(at + 1) == Some(&'.') {
            out.push("..".into());
            at += 2;
        } else if c == '=' && chars.get(at + 1) == Some(&'=') {
            out.push("==".into());
            at += 2;
        } else {
            out.push(c.to_string());
            at += 1;
        }
    }
    out
}

/// `[[`, `[==[`: the level of a long bracket opening at `at`.
fn long_bracket(chars: &[char], at: usize) -> Option<usize> {
    if chars.get(at) != Some(&'[') {
        return None;
    }
    let mut level = 0;
    while chars.get(at + 1 + level) == Some(&'=') {
        level += 1;
    }
    (chars.get(at + 1 + level) == Some(&'[')).then_some(level)
}

fn skip_long(chars: &[char], at: usize, level: usize) -> usize {
    let close: Vec<char> =
        std::iter::once(']').chain(std::iter::repeat_n('=', level)).chain(std::iter::once(']')).collect();
    let mut i = at + level + 2;
    while i + close.len() <= chars.len() {
        if chars[i..i + close.len()] == close[..] {
            return i + close.len();
        }
        i += 1;
    }
    chars.len()
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_a_chunk_loads_and_defines() {
        let chunk = Chunk::read(
            "local m = require('mylib') -- note\nlocal set = token.set_macro\nlocal function def(name) set(name, 'x') end\ndef('a') token.set_lua(\"b\", 1)",
        );
        assert_eq!(chunk.modules, vec![Module { name: "mylib".into(), file: false }]);
        assert_eq!(chunk.defines, vec!["a".to_string(), "b".to_string()]);
    }
}
