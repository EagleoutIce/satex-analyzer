//! LuaTeX's Lua (LuaTeX manual, "Lua general"): one Lua 5.3 state a run, in
//! which `\directlua` chunks, the modules they `require` and the functions
//! `token.set_lua` names run for real.  The engine's libraries (`token`,
//! `tex`, `texio`, `kpse`, `lfs`, `status`, `lua`, `callback`, `unicode`)
//! are Rust functions over the machine; `string`, `table`, `math`, `utf8`
//! and `coroutine` are Lua's own.  There is no `io`, `os` or `debug`, and
//! no file access beyond kpathsea lookups, `lfs.attributes` and loading
//! modules.
//!
//! What satex cannot answer — a library field it does not implement, a
//! register of unknown value, a global the state may lack because an
//! earlier run did not finish — fails the run when it is read, `pcall`
//! included, so no unknown value ever reaches a test; the TeX side reads a
//! failed run as an unknown result.
//! A run is bounded by the step limit (an instruction-count hook) and a
//! memory limit, so it always ends.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use mlua::{
    AnyUserData, HookTriggers, IntoLuaMulti, Lua as State, LuaOptions, MetaMethod, MultiValue,
    StdLib, Table, UserData, UserDataMethods, Value as LV, VmState,
};

use mlua::chunk::ChunkMode;
use crate::builtins::{LoadKind, Primitive};
use crate::machine::Machine;
use crate::tex::{Catcode, Meaning, RegKind, Span, Tok, Token};
use crate::value::Value;

use super::lua::{Module, Print};

type R<T> = mlua::Result<T>;

unsafe extern "C" {
    /// The address that makes a light userdata the unknown value
    /// (vendor/lua-src/SATEX.patch).
    static satex_unknown_tag: std::ffi::c_char;
}

/// The unknown value: arithmetic, concatenation, length and indexing give
/// it again; the patched VM fails the run when it is tested, compared,
/// used as a key, converted or asked its type.
fn marker(_: &State) -> R<LV> {
    let tag = &raw const satex_unknown_tag;
    Ok(LV::LightUserData(mlua::LightUserData(tag as *mut std::ffi::c_void)))
}

fn is_unknown(v: &LV) -> bool {
    let tag = &raw const satex_unknown_tag;
    matches!(v, LV::LightUserData(p) if std::ptr::eq(p.0 as *const std::ffi::c_char, tag))
}

unsafe extern "C-unwind" {
    /// LPeg's opener (vendor/lpeg, built by build.rs).
    fn luaopen_lpeg(state: *mut mlua::ffi::lua_State) -> std::ffi::c_int;
}

/// The registry slot of the current run's dispatcher, and of the table
/// `lua.get_functions_table` returns.
const DISPATCH: &str = "satex.dispatch";
const FUNCTIONS: &str = "satex.functions";
/// Lua instructions one step of `limits.steps` pays for.
const INSTRUCTIONS_A_STEP: u32 = 1000;
/// Steps one run may take at most, whatever the limit leaves.
const RUN_STEPS: u64 = 200_000;
const MEMORY: u64 = 256 << 20;
/// How deep runs nest (Lua scanning TeX that runs Lua) and modules load.
const RUN_DEPTH: usize = 16;
const MODULE_DEPTH: usize = 8;
/// Names made for tokens `token.create(n, cmd)` makes without a name.
const ANONYMOUS: char = '\u{4}';

/// What the hook and the library functions share with the machine side.
#[derive(Default)]
struct Shared {
    used: Cell<u64>,
    limit: Cell<u64>,
    /// The current run has met something unknown.
    failed: Cell<bool>,
    /// Globals a run satex could not finish may have set: missing, they
    /// are unknown, not nil.  `all` stands for every name.
    tainted: RefCell<std::collections::HashSet<String>>,
    all: Cell<bool>,
}

/// Fails the run.
fn unknown<T>(lua: &State) -> R<T> {
    if let Some(shared) = lua.app_data_ref::<Shared>() {
        shared.failed.set(true);
    }
    Err(mlua::Error::runtime("satex: unknown"))
}

/// A token object (LuaTeX manual, "The token library"); its fields are
/// read from the machine when asked for.
#[derive(Clone, Copy)]
struct LTok(Token);

impl UserData for LTok {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_function(MetaMethod::Index, |lua, args: MultiValue| dispatch(lua, op("satex.field"), args));
        methods.add_meta_function(MetaMethod::Eq, |_, (a, b): (AnyUserData, AnyUserData)| {
            Ok(match (a.borrow::<LTok>(), b.borrow::<LTok>()) {
                (Ok(a), Ok(b)) => a.0.tok == b.0.tok,
                _ => false,
            })
        });
    }
}

/// The run a dispatched library function acts in.
pub(super) struct Run<'r, 'a> {
    m: &'r mut Machine<'a>,
    span: Span,
    out: Vec<Pending>,
    /// The modules being loaded, innermost last.
    files: Vec<crate::tex::FileId>,
    /// The modules whose loading stopped.
    stopped: Vec<crate::tex::FileId>,
    /// What `token.put_next` puts back inside `tex.runtoks`, innermost last.
    captured: Vec<Vec<Token>>,
}

enum Pending {
    Print(Print),
    Tokens(Vec<Token>),
}

type Op = for<'r, 'a> fn(&RefCell<Run<'r, 'a>>, &State, MultiValue) -> R<MultiValue>;

/// The library functions that act on the machine, by `library.field`;
/// `satex.*` are the prelude's.
const OPS: &[(&str, Op)] = &[
    ("satex.field", field),
    ("satex.tex_index", tex_index),
    ("satex.register_get", register_get),
    ("satex.register_set", register_set),
    ("satex.searcher", searcher),
    ("satex.loadfile", loadfile),
    ("satex.enter_file", enter_file),
    ("satex.leave_file", leave_file),
    ("satex.capture", capture),
    ("satex.read_file", read_file),
    ("token.scan_string", scan_string),
    ("token.scan_argument", scan_argument),
    ("token.scan_int", scan_int),
    ("token.scan_integer", scan_int),
    ("token.scan_dimen", scan_dimen),
    ("token.scan_keyword", scan_keyword),
    ("token.scan_csname", scan_csname),
    ("token.scan_token", scan_token),
    ("token.scan_toks", scan_toks),
    ("token.get_next", get_next),
    ("token.put_next", put_next),
    ("token.new", new_token),
    ("token.create", create),
    ("token.is_defined", is_defined),
    ("token.get_macro", get_macro),
    ("token.set_macro", set_macro),
    ("token.set_char", set_char),
    ("token.set_lua", set_lua),
    ("tex.write", tex_write),
    ("tex.print", tex_print),
    ("tex.sprint", tex_sprint),
    ("tex.tprint", tex_tprint),
    ("tex.cprint", tex_cprint),
    ("tex.getcatcode", get_catcode),
    ("tex.setcatcode", set_catcode),
    ("tex.enableprimitives", enable_primitives),
    ("kpse.find_file", find_file),
    ("os.date", os_date),
    ("status.luatex_engine", luatex_engine),
    ("status.ini_version", ini_version),
    ("lfs.attributes", attributes),
];

fn op(name: &str) -> usize {
    OPS.iter().position(|(n, _)| *n == name).expect("an op")
}

/// Calls library function `k` through the current run's dispatcher.
fn dispatch(lua: &State, k: usize, mut args: MultiValue) -> R<MultiValue> {
    let LV::Function(d) = lua.named_registry_value::<LV>(DISPATCH)? else { return unknown(lua) };
    args.push_front(LV::Integer(k as i64));
    d.call(args)
}

/// Sets Lua's library functions, the LuaTeX globals and the prelude up.
fn new_state(memory: u64) -> R<State> {
    let libs = StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::UTF8 | StdLib::COROUTINE;
    let lua = State::new_with(libs, LuaOptions::new())?;
    lua.set_app_data(Shared { limit: Cell::new(RUN_STEPS), ..Shared::default() });
    lua.set_memory_limit(usize::try_from(memory.min(MEMORY)).unwrap_or(usize::MAX))?;
    lua.set_hook(HookTriggers::new().every_nth_instruction(INSTRUCTIONS_A_STEP), |lua, _| {
        let shared = lua.app_data_ref::<Shared>().expect("shared");
        shared.used.set(shared.used.get() + 1);
        if shared.used.get() > shared.limit.get() {
            shared.failed.set(true);
            return Err(mlua::Error::runtime("satex: step limit"));
        }
        Ok(VmState::Continue)
    })?;
    lua.set_named_registry_value(FUNCTIONS, lua.create_table()?)?;
    let unknown_mt = lua.create_table()?;
    for mm in [MetaMethod::Call, MetaMethod::NewIndex, MetaMethod::Lt, MetaMethod::Le, MetaMethod::Eq, MetaMethod::ToString] {
        unknown_mt.set(mm.name(), lua.create_function(|lua, _: MultiValue| -> R<()> { unknown(lua) })?)?;
    }
    for mm in [
        MetaMethod::Index, MetaMethod::Add, MetaMethod::Sub, MetaMethod::Mul, MetaMethod::Div, MetaMethod::Mod,
        MetaMethod::Pow, MetaMethod::Unm, MetaMethod::IDiv, MetaMethod::BAnd, MetaMethod::BOr, MetaMethod::BXor,
        MetaMethod::BNot, MetaMethod::Shl, MetaMethod::Shr, MetaMethod::Concat, MetaMethod::Len,
    ] {
        unknown_mt.set(mm.name(), lua.create_function(|lua, _: MultiValue| marker(lua))?)?;
    }
    lua.set_type_metatable::<mlua::LightUserData>(Some(unknown_mt));
    let libraries = lua.create_table()?;
    for (k, (name, _)) in OPS.iter().enumerate() {
        let (library, field) = name.split_once('.').expect("library.field");
        let table: Table = match libraries.get::<Option<Table>>(library)? {
            Some(t) => t,
            None => {
                let t = lua.create_table()?;
                libraries.set(library, t.clone())?;
                t
            }
        };
        table.set(field, lua.create_function(move |lua, args: MultiValue| dispatch(lua, k, args))?)?;
    }
    let satex: Table = libraries.get("satex")?;
    satex.set("marker", marker(&lua)?)?;
    satex.set("fields", include_str!("luatex_fields.txt"))?;
    satex.set("unknown", lua.create_function(|lua, ()| -> R<()> { unknown(lua) })?)?;
    satex.set(
        "failed",
        lua.create_function(|lua, ()| Ok(lua.app_data_ref::<Shared>().is_some_and(|s| s.failed.get())))?,
    )?;
    satex.set(
        "tainted",
        lua.create_function(|lua, name: LV| {
            Ok(lua.app_data_ref::<Shared>().is_some_and(|s| {
                s.all.get() || matches!(&name, LV::String(n) if n.to_str().is_ok_and(|n| s.tainted.borrow().contains(&*n)))
            }))
        })?,
    )?;
    satex.set(
        "is_token",
        lua.create_function(|_, v: LV| Ok(matches!(v, LV::UserData(ud) if ud.is::<LTok>())))?,
    )?;
    satex.set("functions", lua.named_registry_value::<Table>(FUNCTIONS)?)?;
    let commands = lua.create_table()?;
    for (k, name) in COMMANDS.iter().enumerate() {
        commands.set(*name, k as i64)?;
    }
    satex.set("commands", commands)?;
    // SAFETY: `luaopen_lpeg` is LPeg's `lua_CFunction`, built against the
    // Lua 5.3 headers of the Lua this state is.
    let lpeg = unsafe { lua.create_c_function(luaopen_lpeg) }?.call::<Table>(())?;
    libraries.set("lpeg", lpeg)?;
    lua.load(PRELUDE).set_name("=satex").set_mode(ChunkMode::Text).call::<()>(libraries)?;
    Ok(lua)
}

/// The state of this machine, made on first use.
fn state(m: &mut Machine) -> Option<State> {
    if m.lua.state.is_none() {
        let memory = m.cfg.limits.memory.as_u64() / 4;
        m.lua.state = Some(new_state(memory).ok()?);
    }
    m.lua.state.clone()
}

/// Marks the globals `texts` may assign as possibly missing; `None`
/// stands for code satex has not the text of.
fn taint(m: &mut Machine, texts: &[Option<String>]) {
    let Some(lua) = &m.lua.state else { return };
    let Some(shared) = lua.app_data_ref::<Shared>() else { return };
    for text in texts {
        match text.as_deref().and_then(globals) {
            Some(names) => shared.tainted.borrow_mut().extend(names),
            None => shared.all.set(true),
        }
    }
}

/// The globals Lua text may assign: `name =` and `name.field =` outside a
/// `local`, and `function name`; `None` when it may assign any.
fn globals(text: &str) -> Option<Vec<String>> {
    let t = super::lua::lex(text);
    let mut names = Vec::new();
    for k in 0..t.len() {
        if matches!(t[k].as_str(), "_G" | "_ENV" | "rawset" | "load" | "loadstring") {
            return None;
        }
        let named = t[k].chars().next().is_some_and(|c| c.is_alphabetic() || c == '_');
        if !named {
            continue;
        }
        let before = |j: usize| (j > 0).then(|| t[j - 1].as_str());
        // The first name of a list `a, b =`.
        let mut first = k;
        while before(first) == Some(",") && first >= 2 && t[first - 2].chars().next().is_some_and(|c| c.is_alphabetic() || c == '_') {
            first -= 2;
        }
        let assigned = t.get(k + 1).is_some_and(|n| n == "=" || n == "," || n == ".")
            && !matches!(before(first), Some("local" | "for"))
            && !matches!(before(k), Some("." | ":"));
        let defined = before(k) == Some("function") && !(k >= 2 && t[k - 2] == "local");
        if assigned || defined {
            names.push(t[k].clone());
        }
    }
    Some(names)
}

/// The line of `source` the innermost frame of `trace` stopped at: 1 when
/// none tells.
fn stopped_at(trace: &str, source: &str) -> usize {
    trace
        .lines()
        .filter_map(|line| {
            let (src, rest) = line.trim().rsplit_once(": in ").map(|(a, _)| a)?.rsplit_once(':')?;
            let src = src.trim_start_matches("...");
            (source.ends_with(src) && !src.is_empty()).then(|| rest.parse().ok()).flatten()
        })
        .next()
        .unwrap_or(1)
}

/// `text` from line `from` on.
fn after(text: &str, from: usize) -> String {
    text.lines().skip(from.saturating_sub(1)).collect::<Vec<_>>().join("\n")
}

/// The text of lines `from..=to` of a Lua file.
fn lines(path: &str, from: usize, to: usize) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    Some(text.lines().skip(from.saturating_sub(1)).take((to + 1).saturating_sub(from)).collect::<Vec<_>>().join("\n"))
}

/// Runs `work` with the libraries acting on `m`: the tokens the run puts
/// into the input, or `None` when it did not finish or met something
/// unknown.
fn enter(
    m: &mut Machine,
    span: Span,
    text: Option<String>,
    origin: Option<(String, usize, usize)>,
    work: impl FnOnce(&State) -> R<()>,
) -> Option<Vec<Token>> {
    if m.lua.depth >= RUN_DEPTH {
        return None;
    }
    let lua = state(m)?;
    if std::mem::take(&mut m.lua.stale) {
        taint(m, &[None]);
    }
    let outer = m.lua.depth == 0;
    let failed_before = {
        let shared = lua.app_data_ref::<Shared>()?;
        if outer {
            let left = m.cfg.limits.steps.saturating_sub(m.out.steps);
            shared.used.set(0);
            shared.limit.set(left.min(RUN_STEPS));
        }
        shared.failed.replace(false)
    };
    m.lua.depth += 1;
    let run = RefCell::new(Run { m, span, out: Vec::new(), files: Vec::new(), stopped: Vec::new(), captured: Vec::new() });
    let result = lua.scope(|scope| {
        let d = scope.create_function(|lua, mut args: MultiValue| {
            let Some(LV::Integer(k)) = args.pop_front() else { return unknown(lua) };
            let Some((_, f)) = usize::try_from(k).ok().and_then(|k| OPS.get(k)) else { return unknown(lua) };
            f(&run, lua, args)
        })?;
        let previous: LV = lua.named_registry_value(DISPATCH)?;
        lua.set_named_registry_value(DISPATCH, d)?;
        let result = work(&lua);
        lua.set_named_registry_value(DISPATCH, previous)?;
        result
    });
    let Run { m, out, stopped, .. } = run.into_inner();
    m.lua.depth -= 1;
    let failed = {
        let shared = lua.app_data_ref::<Shared>()?;
        if outer {
            m.out.steps += shared.used.get();
        }
        result.is_err() || shared.failed.replace(failed_before)
    };
    // A run inside undecided conditionals changes the one state every
    // path shares: what it did cannot be undone for the other paths.
    if failed || m.env.forking() {
        // What did not run: the chunk or function from where it stopped,
        // and the modules whose loading stopped, from where they did.
        let trace = result.as_ref().err().map(ToString::to_string).unwrap_or_default();
        let mut texts = vec![(text, stopped_at(&trace, "[\\directlua]"))];
        for file in stopped {
            let path = m.out.files.get(file as usize).map(|f| f.path.clone()).unwrap_or_default();
            texts.push((std::fs::read_to_string(&path).ok(), stopped_at(&trace, &path)));
        }
        // A function's definers may sit anywhere in its file.
        if let Some((path, from, to)) = origin
            && let Ok(file) = std::fs::read_to_string(&path)
        {
            // From where the body stopped; on its first line, past the
            // `function` that starts it, not the call it is handed to.
            let line = stopped_at(&trace, &path).clamp(from, to.max(from));
            let mut own = lines(&path, line, to).unwrap_or_default();
            if line == from
                && let Some(at) = own.find("function")
            {
                own.drain(..at + "function".len());
            }
            super::lua::undefine(m, &file, &own);
        }
        for (text, line) in &texts {
            if let Some(text) = text {
                super::lua::undefine(m, text, &after(text, *line));
            }
        }
        let texts: Vec<Option<String>> = texts.into_iter().map(|(t, _)| t).collect();
        taint(m, &texts);
    }
    if failed {
        return None;
    }
    let mut tokens = Vec::new();
    for pending in out {
        match pending {
            Pending::Print(print) => tokens.extend(super::lua::print_tokens(m, &print, span)),
            Pending::Tokens(t) => tokens.extend(t),
        }
    }
    Some(tokens)
}

/// Runs a `\directlua` chunk.
pub(super) fn chunk(m: &mut Machine, code: &str, span: Span) -> Option<Vec<Token>> {
    enter(m, span, Some(code.to_string()), None, |lua| lua.load(code).set_name("=[\\directlua]").set_mode(ChunkMode::Text).exec())
}

/// Calls the function at `id` in `lua.get_functions_table()`, as a
/// command `token.set_lua` made does, with `id` as its argument.
pub(super) fn function(m: &mut Machine, id: u32, span: Span) -> Option<Vec<Token>> {
    // What the function may assign when it stops: its own lines.
    let origin = state(m).and_then(|lua| {
        let functions: Table = lua.named_registry_value(FUNCTIONS).ok()?;
        let LV::Function(f) = functions.raw_get::<LV>(id).ok()? else { return None };
        let info = f.info();
        Some((info.source?.strip_prefix('@')?.to_string(), info.line_defined?, info.last_line_defined?))
    });
    let text = origin.as_ref().and_then(|(path, from, to)| lines(path, *from, *to));
    enter(m, span, text, origin, |lua| {
        let functions: Table = lua.named_registry_value(FUNCTIONS)?;
        match functions.raw_get::<LV>(id)? {
            LV::Function(f) => f.call::<()>(id),
            _ => unknown(lua),
        }
    })
}

// ---- values ------------------------------------------------------------

fn arg(args: &MultiValue, k: usize) -> LV {
    args.get(k).cloned().unwrap_or(LV::Nil)
}

fn ret(lua: &State, v: impl IntoLuaMulti) -> R<MultiValue> {
    v.into_lua_multi(lua)
}

/// A string or number argument as text.
fn text(lua: &State, v: &LV) -> R<String> {
    if is_unknown(v) {
        return unknown(lua);
    }
    match lua.coerce_string(v.clone())? {
        Some(s) => match s.to_str() {
            Ok(s) => Ok(s.to_string()),
            Err(_) => unknown(lua),
        },
        None => Err(mlua::Error::runtime("string expected")),
    }
}

fn int(lua: &State, v: &LV) -> R<i64> {
    if is_unknown(v) {
        return unknown(lua);
    }
    match lua.coerce_integer(v.clone())? {
        Some(n) => Ok(n),
        None => Err(mlua::Error::runtime("number expected")),
    }
}


fn known<T>(lua: &State, v: Option<T>) -> R<T> {
    match v {
        Some(v) => Ok(v),
        None => unknown(lua),
    }
}

fn token_value(lua: &State, t: Token) -> R<LV> {
    Ok(LV::UserData(lua.create_userdata(LTok(t))?))
}

fn token_of(v: &LV) -> Option<Token> {
    match v {
        LV::UserData(ud) => ud.borrow::<LTok>().ok().map(|t| t.0),
        _ => None,
    }
}


fn char_of(lua: &State, code: i64) -> R<char> {
    known(lua, u32::try_from(code).ok().and_then(char::from_u32))
}

/// A category a character token can have on its own.
fn plain_catcode(lua: &State, cat: i64) -> R<Catcode> {
    match u8::try_from(cat).ok().and_then(Catcode::from_u8) {
        Some(Catcode::Escape | Catcode::Eol | Catcode::Ignored | Catcode::Active | Catcode::Comment | Catcode::Invalid)
        | None => unknown(lua),
        Some(cat) => Ok(cat),
    }
}

/// The flags after a definition's operands: `"global"`, `"protected"`, …
fn flags(lua: &State, args: &MultiValue, from: usize) -> R<Vec<String>> {
    args.iter().skip(from).map(|v| text(lua, v)).collect()
}

// ---- token objects -----------------------------------------------------

/// LuaTeX's command names, numbered as `token.commands()` numbers them
/// (LuaTeX 1.x); a character's command is its category code.
const COMMANDS: &[&str] = &[
    "relax", "left_brace", "right_brace", "math_shift", "tab_mark", "car_ret", "mac_param", "sup_mark",
    "sub_mark", "endv", "spacer", "letter", "other_char", "par_end", "stop", "delim_num", "char_num",
    "math_char_num", "mark", "node", "xray", "make_box", "hmove", "vmove", "un_hbox", "un_vbox",
    "remove_item", "hskip", "vskip", "mskip", "kern", "mkern", "leader_ship", "halign", "valign", "no_align",
    "vrule", "hrule", "novrule", "nohrule", "insert", "vadjust", "ignore_spaces", "after_assignment",
    "after_group", "partoken_name", "break_penalty", "start_par", "ital_corr", "accent", "math_accent",
    "discretionary", "eq_no", "left_right", "math_comp", "limit_switch", "above", "math_style", "math_choice",
    "non_script", "vcenter", "case_shift", "message", "normal", "extension", "option", "lua_function_call",
    "lua_bytecode_call", "lua_call", "in_stream", "begin_group", "end_group", "omit", "ex_space", "boundary",
    "radical", "super_sub_script", "no_super_sub_script", "math_shift_cs", "end_cs_name", "char_ghost",
    "assign_local_box", "char_given", "math_given", "xmath_given", "last_item", "toks_register",
    "assign_toks", "assign_int", "assign_attr", "assign_dimen", "assign_glue", "assign_mu_glue",
    "assign_font_dimen", "assign_font_int", "assign_hang_indent", "set_aux", "set_prev_graf",
    "set_page_dimen", "set_page_int", "set_box_dimen", "set_tex_shape", "set_etex_shape", "def_char_code",
    "def_del_code", "extdef_math_code", "extdef_del_code", "def_family", "set_math_param", "set_font",
    "def_font", "register", "assign_box_direction", "assign_box_dir", "assign_direction", "assign_dir",
    "combinetoks", "advance", "multiply", "divide", "prefix", "let", "shorthand_def", "def_lua_call",
    "read_to_cs", "def", "set_box", "hyph_data", "set_interaction", "letterspace_font", "expand_font",
    "copy_font", "set_font_id", "undefined_cs", "expand_after", "no_expand", "input", "lua_expandable_call",
    "lua_local_call", "if_test", "fi_or_else", "cs_name", "convert", "variable", "feedback", "the",
    "top_bot_mark", "call", "long_call", "outer_call", "long_outer_call", "end_template", "dont_expand",
    "glue_ref", "shape_ref", "box_ref", "data",
];

fn command(name: &str) -> i64 {
    COMMANDS.iter().position(|c| *c == name).expect("a command") as i64
}

/// What a token is to the engine: its command, its `mode` (the character
/// code, register number, constant or function number) and whether it is
/// expandable and protected.  `None` for a meaning satex has no command for.
fn classify(m: &mut Machine, t: Token) -> Option<(&'static str, Option<i64>, bool, bool)> {
    let meaning = match t.tok {
        Tok::Chr(c, Catcode::Active) => {
            let sym = m.active_sym(c);
            m.env.meaning(sym)
        }
        Tok::Chr(c, cat) => return Some((COMMANDS[cat as usize], Some(c as i64), false, false)),
        Tok::Cs(sym) => m.env.meaning(sym),
        Tok::Param(_) => return None,
    };
    Some(match meaning {
        Meaning::Undefined => ("undefined_cs", Some(0), false, false),
        Meaning::Macro(d) => {
            let name = match (d.long, d.outer) {
                (false, false) => "call",
                (true, false) => "long_call",
                (false, true) => "outer_call",
                (true, true) => "long_outer_call",
            };
            (name, None, true, d.protected)
        }
        Meaning::Char(c, cat) if !matches!(cat, Catcode::Active | Catcode::Escape) => {
            (COMMANDS[cat as usize], Some(c as i64), false, false)
        }
        Meaning::Register(kind, n) if n != crate::tex::UNNUMBERED => {
            let name = match kind {
                RegKind::Count => "assign_int",
                RegKind::Dimen => "assign_dimen",
                RegKind::Skip => "assign_skip",
                RegKind::MuSkip => "assign_mu_skip",
                RegKind::Toks => "assign_toks",
                RegKind::Char => "char_given",
                RegKind::MathChar => "math_given",
                _ => return None,
            };
            (name, Some(n.into()), false, false)
        }
        Meaning::Primitive(Primitive::LuaCall { id, protected }) => {
            (if protected { "lua_call" } else { "lua_expandable_call" }, Some(id.into()), !protected, false)
        }
        _ => return None,
    })
}

fn field<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let Some(t) = token_of(&arg(&args, 0)) else { return unknown(lua) };
    let key = text(lua, &arg(&args, 1))?;
    let mut run = run.borrow_mut();
    let m = &mut *run.m;
    let class = classify(m, t);
    let cs = match t.tok {
        Tok::Cs(sym) => Some(m.name(sym).to_string()),
        Tok::Chr(c, Catcode::Active) => Some(c.to_string()),
        _ => None,
    };
    let Some((cmd, mode, expandable, protected)) = class else {
        return match key.as_str() {
            "csname" => ret(lua, cs),
            "active" => ret(lua, matches!(t.tok, Tok::Chr(_, Catcode::Active))),
            _ => ret(lua, marker(lua)?),
        };
    };
    match key.as_str() {
        "command" => ret(lua, command(cmd)),
        "cmdname" => ret(lua, cmd),
        "csname" => ret(lua, cs),
        "active" => ret(lua, matches!(t.tok, Tok::Chr(_, Catcode::Active))),
        "expandable" => ret(lua, expandable),
        "protected" => ret(lua, protected),
        "mode" => match mode {
            Some(n) => ret(lua, n),
            None => ret(lua, marker(lua)?),
        },
        "index" => match (cmd, mode) {
            ("assign_int" | "assign_dimen" | "assign_skip" | "assign_mu_skip" | "assign_toks" | "char_given"
            | "math_given" | "lua_call" | "lua_expandable_call", Some(n)) => ret(lua, n),
            ("undefined_cs" | "letter" | "other_char" | "spacer", _) => ret(lua, LV::Nil),
            _ => ret(lua, marker(lua)?),
        },
        _ => ret(lua, marker(lua)?),
    }
}

// ---- the token library ---------------------------------------------------

fn scan_string<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, _: MultiValue) -> R<MultiValue> {
    let mut run = run.borrow_mut();
    let body = run.m.read_general_text();
    let body = run.m.expand_tokens(Rc::from(body));
    let text = run.m.text_known(&body);
    ret(lua, known(lua, text)?)
}

fn scan_argument<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let expand = !matches!(arg(&args, 0), LV::Boolean(false));
    let mut run = run.borrow_mut();
    let mut body = run.m.read_general_text();
    if expand {
        body = run.m.expand_tokens(Rc::from(body));
    }
    let text = run.m.text_known(&body);
    ret(lua, known(lua, text)?)
}

fn scan_int<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, _: MultiValue) -> R<MultiValue> {
    let n = run.borrow_mut().m.scan_int();
    ret(lua, known(lua, n)?)
}

fn scan_dimen<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, _: MultiValue) -> R<MultiValue> {
    let n = run.borrow_mut().m.scan_dimen();
    ret(lua, known(lua, n)?)
}

fn scan_keyword<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let keyword = text(lua, &arg(&args, 0))?;
    ret(lua, run.borrow_mut().m.scan_keyword(&keyword))
}

fn scan_csname<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, _: MultiValue) -> R<MultiValue> {
    let mut run = run.borrow_mut();
    let Some(t) = run.m.next_token() else { return unknown(lua) };
    match t.tok {
        Tok::Cs(sym) => ret(lua, run.m.name(sym).to_string()),
        _ => unknown(lua),
    }
}

fn scan_token<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, _: MultiValue) -> R<MultiValue> {
    let t = run.borrow_mut().m.next_x_token();
    ret(lua, token_value(lua, known(lua, t)?)?)
}

fn scan_toks<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    if !matches!(arg(&args, 0), LV::Nil | LV::Boolean(false)) {
        return unknown(lua);
    }
    let expand = matches!(arg(&args, 1), LV::Boolean(true));
    let mut run = run.borrow_mut();
    let mut body = run.m.read_general_text();
    if expand {
        body = run.m.expand_tokens(Rc::from(body));
    }
    if run.m.has_unknown(&body) {
        return unknown(lua);
    }
    let list = lua.create_table()?;
    for t in body {
        list.raw_push(token_value(lua, t)?)?;
    }
    ret(lua, list)
}

fn get_next<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, _: MultiValue) -> R<MultiValue> {
    let t = run.borrow_mut().m.next_token();
    ret(lua, token_value(lua, known(lua, t)?)?)
}

fn put_next<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let mut tokens = Vec::new();
    for v in args.iter() {
        match v {
            LV::Table(list) => {
                for v in list.sequence_values::<LV>() {
                    tokens.push(known(lua, token_of(&v?))?);
                }
            }
            v => tokens.push(known(lua, token_of(v))?),
        }
    }
    let mut run = run.borrow_mut();
    match run.captured.last_mut() {
        Some(captured) => {
            tokens.append(captured);
            *captured = tokens;
        }
        None => run.m.push_tokens(Rc::from(tokens), None),
    }
    ret(lua, ())
}

/// `tex.runtoks(f)`: `satex.capture(true)` before `f` runs, and
/// `satex.capture(false)` after, which executes what it put back as a
/// local main loop does.
fn capture<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let mut run = run.borrow_mut();
    if matches!(arg(&args, 0), LV::Boolean(true)) {
        run.captured.push(Vec::new());
        return ret(lua, ());
    }
    let Some(tokens) = run.captured.pop() else { return unknown(lua) };
    run.m.run_tokens(Rc::from(tokens));
    ret(lua, ())
}

/// `token.new(code, cmd)`: a character token of the category `cmd`.
fn new_token<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let c = char_of(lua, int(lua, &arg(&args, 0))?)?;
    let cat = plain_catcode(lua, int(lua, &arg(&args, 1))?)?;
    let span = run.borrow().span;
    ret(lua, token_value(lua, Token::new(Tok::Chr(c, cat), span))?)
}

/// `token.create("name")`, `token.create(code)` in the current category,
/// `token.create(code, cmd)`.
fn create<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let mut run = run.borrow_mut();
    let span = run.span;
    let first = arg(&args, 0);
    if let LV::String(_) = first {
        let name = text(lua, &first)?;
        let sym = run.m.intern(&name);
        return ret(lua, token_value(lua, Token::new(Tok::Cs(sym), span))?);
    }
    let n = int(lua, &first)?;
    let cmd = match arg(&args, 1) {
        LV::Nil => None,
        v => Some(int(lua, &v)?),
    };
    let tok = match cmd {
        None => {
            let c = char_of(lua, n)?;
            let cat = run.m.catcodes.get(c);
            plain_catcode(lua, cat as i64)?;
            Tok::Chr(c, cat)
        }
        Some(cmd) if cmd == command("char_given") || cmd == command("math_given") => {
            let kind = if cmd == command("char_given") { RegKind::Char } else { RegKind::MathChar };
            let Ok(index) = u16::try_from(n) else { return unknown(lua) };
            let sym = run.m.intern(&format!("{ANONYMOUS}luatoken.{}.{n}", COMMANDS[cmd as usize]));
            run.m.env.set(sym, crate::env::Binding::builtin(Meaning::Register(kind, index)), true);
            run.m.env.set_value(sym, Value::Int(n), true);
            Tok::Cs(sym)
        }
        Some(cmd) => Tok::Chr(char_of(lua, n)?, plain_catcode(lua, cmd)?),
    };
    ret(lua, token_value(lua, Token::new(tok, span))?)
}

fn is_defined<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let name = text(lua, &arg(&args, 0))?;
    let mut run = run.borrow_mut();
    let sym = run.m.intern(&name);
    match run.m.env.meaning(sym) {
        Meaning::Undefined => ret(lua, false),
        Meaning::Unknown => unknown(lua),
        _ => ret(lua, true),
    }
}

fn get_macro<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let name = text(lua, &arg(&args, 0))?;
    let mut run = run.borrow_mut();
    let sym = run.m.intern(&name);
    match run.m.env.meaning(sym) {
        Meaning::Macro(d) if d.parameter_text.items.is_empty() => {
            let text = run.m.text_known(&d.replacement_text);
            ret(lua, known(lua, text)?)
        }
        Meaning::Macro(_) | Meaning::Unknown => unknown(lua),
        _ => ret(lua, LV::Nil),
    }
}

/// `token.set_macro([table,] name, [body,] flags…)`: a macro without
/// parameters whose body is `body` read with catcode table `table`.
fn set_macro<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let mut k = 0;
    let table = match arg(&args, 0) {
        LV::Integer(_) | LV::Number(_) => {
            k = 1;
            Some(int(lua, &arg(&args, 0))?)
        }
        _ => None,
    };
    let name = text(lua, &arg(&args, k))?;
    let body = match arg(&args, k + 1) {
        LV::Nil => String::new(),
        v => text(lua, &v)?,
    };
    let flags = flags(lua, &args, k + 2)?;
    if flags.iter().any(|f| !matches!(f.as_str(), "global" | "protected" | "long" | "outer")) {
        return unknown(lua);
    }
    let mut run = run.borrow_mut();
    let span = run.span;
    let print = Print { lines: false, table, strings: vec![body] };
    let body = super::lua::print_tokens(run.m, &print, span);
    let def = crate::tex::MacroDef {
        parameter_text: Default::default(),
        arg_spec: None,
        replacement_text: Rc::from(crate::tex::parameterize(body)),
        long: flags.iter().any(|f| f == "long"),
        outer: flags.iter().any(|f| f == "outer"),
        protected: flags.iter().any(|f| f == "protected"),
    };
    let sym = run.m.intern(&name);
    let global = flags.iter().any(|f| f == "global");
    run.m.env.set(sym, crate::env::Binding::builtin(Meaning::Macro(Rc::new(def))), global);
    ret(lua, ())
}

/// `token.set_char(name, n, flags…)`: `\chardef`.
fn set_char<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let name = text(lua, &arg(&args, 0))?;
    let n = int(lua, &arg(&args, 1))?;
    let Ok(index) = u16::try_from(n) else { return unknown(lua) };
    let flags = flags(lua, &args, 2)?;
    let global = flags.iter().any(|f| f == "global");
    let mut run = run.borrow_mut();
    let sym = run.m.intern(&name);
    run.m.env.set(sym, crate::env::Binding::builtin(Meaning::Register(RegKind::Char, index)), global);
    run.m.env.set_value(sym, Value::Int(n), global);
    ret(lua, ())
}

/// `token.set_lua(name, id, flags…)`: a command that calls function `id`.
fn set_lua<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let name = text(lua, &arg(&args, 0))?;
    let Ok(id) = u32::try_from(int(lua, &arg(&args, 1))?) else { return unknown(lua) };
    let flags = flags(lua, &args, 2)?;
    if flags.iter().any(|f| !matches!(f.as_str(), "global" | "protected")) {
        return unknown(lua);
    }
    let protected = flags.iter().any(|f| f == "protected");
    let global = flags.iter().any(|f| f == "global");
    let mut run = run.borrow_mut();
    let sym = run.m.intern(&name);
    let meaning = Meaning::Primitive(Primitive::LuaCall { id, protected });
    run.m.env.set(sym, crate::env::Binding::builtin(meaning), global);
    ret(lua, ())
}

// ---- the tex library -----------------------------------------------------

/// A string, number or token list printed: strings to be read later,
/// tokens as they are.
fn printed(lua: &State, v: &LV, strings: &mut Vec<String>, tokens: &mut Vec<Token>) -> R<()> {
    match v {
        LV::Table(list) => {
            for v in list.sequence_values::<LV>() {
                printed(lua, &v?, strings, tokens)?;
            }
        }
        LV::UserData(_) => tokens.push(known(lua, token_of(v))?),
        v => strings.push(text(lua, v)?),
    }
    Ok(())
}

/// `tex.print` and `tex.sprint`: a leading number picks the catcode table.
fn print(run: &RefCell<Run>, lua: &State, args: MultiValue, lines: bool) -> R<MultiValue> {
    let mut rest = args.iter().peekable();
    let table = match rest.peek() {
        Some(LV::Integer(n)) if args.len() > 1 => Some(*n),
        Some(LV::Number(n)) if args.len() > 1 => Some(*n as i64),
        _ => None,
    };
    if table.is_some() {
        rest.next();
    }
    let mut run = run.borrow_mut();
    for v in rest {
        let (mut strings, mut tokens) = (Vec::new(), Vec::new());
        printed(lua, v, &mut strings, &mut tokens)?;
        if !tokens.is_empty() {
            if lines {
                return unknown(lua);
            }
            run.out.push(Pending::Tokens(tokens));
        }
        if !strings.is_empty() {
            run.out.push(Pending::Print(Print { lines, table, strings }));
        }
    }
    ret(lua, ())
}

fn tex_print<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    print(run, lua, args, true)
}

fn tex_sprint<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    print(run, lua, args, false)
}

/// `tex.tprint({n, s…}, …)`: `tex.sprint` for each table.
fn tex_tprint<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    for v in args.iter() {
        let LV::Table(t) = v else { return Err(mlua::Error::runtime("table expected")) };
        let parts: MultiValue = t.sequence_values::<LV>().collect::<R<_>>()?;
        print(run, lua, parts, false)?;
    }
    ret(lua, ())
}

/// `tex.write`: other characters and spaces, tokens as they are.
fn tex_write<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let mut run = run.borrow_mut();
    let span = run.span;
    for v in args.iter() {
        let (mut strings, mut tokens) = (Vec::new(), Vec::new());
        printed(lua, v, &mut strings, &mut tokens)?;
        tokens.extend(strings.concat().chars().map(|c| {
            let cat = if c == ' ' { Catcode::Space } else { Catcode::Other };
            Token::new(Tok::Chr(c, cat), span)
        }));
        run.out.push(Pending::Tokens(tokens));
    }
    ret(lua, ())
}

/// `tex.cprint(cat, …)`: every character in category `cat`.
fn tex_cprint<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let cat = plain_catcode(lua, int(lua, &arg(&args, 0))?)?;
    let mut run = run.borrow_mut();
    let span = run.span;
    for v in args.iter().skip(1) {
        let (mut strings, mut tokens) = (Vec::new(), Vec::new());
        printed(lua, v, &mut strings, &mut tokens)?;
        tokens.extend(strings.concat().chars().map(|c| Token::new(Tok::Chr(c, cat), span)));
        run.out.push(Pending::Tokens(tokens));
    }
    ret(lua, ())
}

/// `tex.inputlineno`, `tex.luatexversion` and the other fields that are
/// not functions.
fn tex_index<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let key = text(lua, &arg(&args, 0))?;
    let run = run.borrow_mut();
    match key.as_str() {
        "inputlineno" => ret(lua, known(lua, run.m.input_line())?),
        "luatexversion" => ret(lua, known(lua, super::lua::engine_version(run.m.cfg))?),
        _ => ret(lua, marker(lua)?),
    }
}

/// The register `tex.count[key]` and relatives name: a number, or the name
/// of a register control sequence of that kind.
fn register(run: &mut Run, lua: &State, kind: RegKind, key: &LV) -> R<crate::tex::Sym> {
    match key {
        LV::String(_) => {
            let name = text(lua, key)?;
            let sym = run.m.intern(&name);
            match run.m.env.meaning(sym) {
                Meaning::Register(k, _) if k == kind => Ok(run.m.storage(sym)),
                Meaning::Unknown => unknown(lua),
                _ => Err(mlua::Error::runtime("incorrect register")),
            }
        }
        _ => {
            let n = int(lua, key)?;
            Ok(run.m.register_sym(kind, n))
        }
    }
}

fn kind_of(lua: &State, name: &LV) -> R<RegKind> {
    match text(lua, name)?.as_str() {
        "count" => Ok(RegKind::Count),
        "dimen" => Ok(RegKind::Dimen),
        _ => unknown(lua),
    }
}

/// `tex.count[k]`, `tex.getcount(k)`, and the same for `dimen`.
fn register_get<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let kind = kind_of(lua, &arg(&args, 0))?;
    let mut run = run.borrow_mut();
    let sym = register(&mut run, lua, kind, &arg(&args, 1))?;
    let value = run.m.env.value(sym);
    let n = match kind {
        RegKind::Count => value.as_int(),
        _ => value.as_dimen(),
    };
    ret(lua, known(lua, n)?)
}

/// `tex.count[k] = v`, `tex.setcount(["global",] k, v)`, and `dimen`.
fn register_set<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let kind = kind_of(lua, &arg(&args, 0))?;
    let global = matches!(arg(&args, 1), LV::Boolean(true));
    let value = match arg(&args, 3) {
        v @ (LV::Integer(_) | LV::Number(_)) => int(lua, &v)?,
        _ => return unknown(lua),
    };
    let mut run = run.borrow_mut();
    let sym = register(&mut run, lua, kind, &arg(&args, 2))?;
    let value = match kind {
        RegKind::Count => Value::Int(value),
        _ => Value::Dimen(value),
    };
    let span = run.span;
    run.m.assign(sym, value, global, span);
    ret(lua, ())
}

fn get_catcode<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    if args.len() > 1 {
        return unknown(lua);
    }
    let c = char_of(lua, int(lua, &arg(&args, 0))?)?;
    ret(lua, run.borrow().m.catcodes.get(c) as i64)
}

/// `tex.setcatcode([global,] c, cat)`, in the current table.
fn set_catcode<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let numbers: Vec<LV> = args.iter().filter(|v| !matches!(v, LV::String(_))).cloned().collect();
    if numbers.len() != 2 || numbers.len() != args.len() {
        return unknown(lua);
    }
    let c = char_of(lua, int(lua, &numbers[0])?)?;
    let Some(cat) = u8::try_from(int(lua, &numbers[1])?).ok().and_then(Catcode::from_u8) else {
        return Err(mlua::Error::runtime("invalid catcode"));
    };
    run.borrow_mut().m.set_catcode(c, cat);
    ret(lua, ())
}

/// `tex.enableprimitives(prefix, names)` (LuaTeX manual,
/// "tex.enableprimitives"): each primitive also under the prefixed name.
fn enable_primitives<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let prefix = text(lua, &arg(&args, 0))?;
    let LV::Table(names) = arg(&args, 1) else { return unknown(lua) };
    let mut run = run.borrow_mut();
    for name in names.sequence_values::<LV>() {
        let name = text(lua, &name?)?;
        let source = run.m.intern(&name);
        let Meaning::Primitive(p) = run.m.env.meaning(source) else { continue };
        let target = run.m.intern(&format!("{prefix}{name}"));
        run.m.env.set(target, crate::env::Binding::builtin(Meaning::Primitive(p)), true);
    }
    ret(lua, ())
}

/// `status.ini_version`: true in an `-ini` run, which is what reading the
/// kernel (or no format) is.
fn ini_version<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, _: MultiValue) -> R<MultiValue> {
    let run = run.borrow();
    ret(lua, run.m.out.format.is_none() || run.m.in_format())
}

/// `status.luatex_engine`: the engine `fmtutil.cnf` builds the `lualatex`
/// format with, when the LaTeX kernel is what runs.
fn luatex_engine<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, _: MultiValue) -> R<MultiValue> {
    let mut run = run.borrow_mut();
    if run.m.out.format.is_none() && !run.m.in_format() {
        return unknown(lua);
    }
    let roots = run.m.resolver_mut().distribution().roots.clone();
    let text = roots.iter().find_map(|root| std::fs::read_to_string(root.join("web2c/fmtutil.cnf")).ok()).unwrap_or_default();
    let engine = text.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        (fields.next() == Some("lualatex")).then(|| fields.next()).flatten().map(str::to_string)
    });
    ret(lua, known(lua, engine)?)
}

/// `os.date("%z")`: the local offset from UTC; any other date is unknown.
fn os_date<'r, 'a>(_: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    if args.len() != 1 || text(lua, &arg(&args, 0))? != "%z" {
        return unknown(lua);
    }
    let Ok(offset) = time::UtcOffset::current_local_offset() else { return unknown(lua) };
    let (h, m, _) = offset.as_hms();
    let sign = if offset.is_negative() { '-' } else { '+' };
    ret(lua, format!("{sign}{:02}{:02}", h.unsigned_abs(), m.unsigned_abs()))
}

// ---- files -------------------------------------------------------------

/// `kpse.find_file(name, format)`: the path satex's resolver finds.
fn find_file<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let name = text(lua, &arg(&args, 0))?;
    let kind = match arg(&args, 1) {
        LV::String(s) if s.to_str().is_ok_and(|s| s == "lua") => LoadKind::Lua,
        _ => LoadKind::Input,
    };
    let mut run = run.borrow_mut();
    let base = run.m.base().to_path_buf();
    let path = run.m.resolver_mut().resolve(&name, kind, &base);
    ret(lua, path.map(|p| p.display().to_string()))
}

/// `lfs.attributes(path [, name])`: size, modification time and mode.
fn attributes<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let path = PathBuf::from(text(lua, &arg(&args, 0))?);
    let path = run.borrow().m.base().join(path);
    let Ok(meta) = std::fs::metadata(&path) else { return ret(lua, LV::Nil) };
    let size = i64::try_from(meta.len()).ok();
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_secs()).ok());
    let mode = if meta.is_dir() { "directory" } else { "file" };
    match arg(&args, 1) {
        LV::Nil => {
            let t = lua.create_table()?;
            t.set("size", known(lua, size)?)?;
            t.set("modification", known(lua, modified)?)?;
            t.set("mode", mode)?;
            ret(lua, t)
        }
        v => match text(lua, &v)?.as_str() {
            "size" => ret(lua, known(lua, size)?),
            "modification" => ret(lua, known(lua, modified)?),
            "mode" => ret(lua, mode),
            _ => unknown(lua),
        },
    }
}

/// The bytes of a file the resolver finds, for the read-only `io.open`:
/// a relative name as the resolver finds it, an absolute path when the
/// resolver finds its file name there.
fn read_file<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let name = text(lua, &arg(&args, 0))?;
    let mut run = run.borrow_mut();
    let base = run.m.base().to_path_buf();
    let given = std::path::Path::new(&name);
    let path = match given.file_name().and_then(|f| f.to_str()) {
        Some(file) if given.is_absolute() => run
            .m
            .resolver_mut()
            .resolve(file, LoadKind::Input, &base)
            .filter(|found| found.canonicalize().ok() == given.canonicalize().ok()),
        _ => run.m.resolver_mut().resolve(&name, LoadKind::Input, &base),
    };
    let Some(path) = path else { return ret(lua, (LV::Nil, format!("{name}: No such file or directory"))) };
    match std::fs::metadata(&path) {
        Ok(meta) if meta.len() <= MEMORY / 4 => {}
        _ => return unknown(lua),
    }
    match std::fs::read(&path) {
        Ok(bytes) => ret(lua, lua.create_string(bytes)?),
        Err(_) => unknown(lua),
    }
}

/// A module file satex finds, recorded as a load, compiled.
fn compile(run: &RefCell<Run>, lua: &State, module: Module) -> R<Option<(mlua::Function, String, crate::tex::FileId)>> {
    let mut run = run.borrow_mut();
    let span = match run.files.last() {
        Some(&file) => Span::new(file, 1, 1),
        None => run.span,
    };
    let Some((path, file)) = super::lua::load(run.m, &module, span) else { return Ok(None) };
    let Ok(source) = std::fs::read_to_string(&path) else { return unknown(lua) };
    let name = path.display().to_string();
    let f = lua.load(source).set_name(format!("@{name}")).set_mode(ChunkMode::Text).into_function()?;
    Ok(Some((f, name, file)))
}

/// The searcher `require` asks after `package.preload`: a module satex
/// cannot find may still exist, so it is unknown rather than missing.
fn searcher<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let name = text(lua, &arg(&args, 0))?;
    match compile(run, lua, Module { name, file: false })? {
        Some((f, path, file)) => ret(lua, (f, path, file)),
        None => unknown(lua),
    }
}

/// `loadfile(name)`, which `dofile` runs.
fn loadfile<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let name = text(lua, &arg(&args, 0))?;
    match compile(run, lua, Module { name, file: true })? {
        Some((f, _, file)) => ret(lua, (f, file)),
        None => unknown(lua),
    }
}

fn enter_file<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let file = int(lua, &arg(&args, 0))?;
    let mut run = run.borrow_mut();
    if run.files.len() >= MODULE_DEPTH {
        return unknown(lua);
    }
    run.files.push(file as crate::tex::FileId);
    ret(lua, ())
}

fn leave_file<'r, 'a>(run: &RefCell<Run<'r, 'a>>, lua: &State, args: MultiValue) -> R<MultiValue> {
    let mut run = run.borrow_mut();
    let file = run.files.pop();
    if let (Some(file), LV::Boolean(false)) = (file, arg(&args, 0)) {
        run.stopped.push(file);
    }
    ret(lua, ())
}

/// Lua's side of the set-up: the LuaTeX globals, `require` over satex's
/// searcher, `pcall` that does not catch an unknown, `load` of text only.
const PRELUDE: &str = include_str!("lua_prelude.lua");
