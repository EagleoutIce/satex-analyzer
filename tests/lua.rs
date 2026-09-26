//! Lua under LuaTeX: `\directlua` is read as the Lua it hands the engine,
//! what it prints of literals is read back, the modules it loads are
//! dependencies, and catcode tables switch the regime.

use satex::builtins::{LoadKind, OccKind};
use satex::config::{Config, Engine};
use satex::machine::{Analysis, Machine};
use satex::plugin::Kernel;

/// The LuaTeX primitives and nothing else.
fn primitives(source: &str) -> Analysis {
    let cfg = Config { engine: Some(Engine::LuaTeX), kernel: Some(Kernel::None), ..Config::default() };
    // initex has no grouping characters.
    let source = format!("\\catcode`\\{{=1 \\catcode`\\}}=2 {source}");
    Machine::analyze(&source, None, &cfg)
}

fn defined(analysis: &Analysis, name: &str) -> bool {
    analysis.interner.lookup(name).is_some_and(|sym| analysis.env.is_defined(sym))
}

#[test]
fn a_printed_literal_is_read_back() {
    let analysis = primitives(
        // An undefined `\\` would expand to nothing, as in LuaTeX; `\string`
        // is how a document writes a backslash into Lua.
        r"\directlua{tex.print('\string\\def\string\\hello{world}') tex.sprint('\string\\def\string\\two{2}')}
\hello",
    );
    assert!(defined(&analysis, "hello"));
    assert!(defined(&analysis, "two"));
    let chunk = analysis.facts.occurrences.iter().find(|o| o.kind == OccKind::Lua).expect("recorded");
    assert!(chunk.key.starts_with("tex.print"), "{}", chunk.key);
}

#[test]
fn a_conditional_print_is_a_gap_not_a_guess() {
    let analysis = primitives("\\directlua{if os.clock() > 1 then tex.print(\"\\\\def\\\\a{}\") end}");
    assert!(!defined(&analysis, "a"));
    assert!(analysis.facts.diagnostics.iter().any(|d| d.code == "lua-output"));
}

#[test]
fn the_argument_is_expanded_first() {
    let analysis = primitives("\\def\\name{greet}\\directlua{local s = \"\\name\"}");
    let chunk = analysis.facts.occurrences.iter().find(|o| o.kind == OccKind::Lua).unwrap();
    assert_eq!(chunk.key, "local s = \"greet\"");
}

#[test]
fn required_modules_are_dependencies() {
    let dir = std::env::temp_dir().join(format!("satex-lua-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mylib.lua"), "local helper = require('myhelper')\nreturn {}\n").unwrap();
    std::fs::write(dir.join("myhelper.lua"), "return 1\n").unwrap();
    let path = dir.join("doc.tex");
    let source = "\\catcode`\\{=1 \\catcode`\\}=2 \\directlua{require(\"mylib\") local lpeg = require('lpeg')}";
    std::fs::write(&path, source).unwrap();
    let cfg = Config { engine: Some(Engine::LuaTeX), kernel: Some(Kernel::None), ..Config::default() };
    let analysis = Machine::analyze(source, Some(&path), &cfg);
    let lua: Vec<(&str, bool)> = analysis
        .facts
        .loads
        .iter()
        .filter(|l| l.kind == LoadKind::Lua)
        .map(|l| (l.name.as_str(), l.path.is_some()))
        .collect();
    assert!(lua.contains(&("mylib", true)));
    assert!(lua.contains(&("myhelper", true)), "followed into mylib.lua: {lua:?}");
    assert!(!lua.iter().any(|(name, _)| *name == "lpeg"), "lpeg is built in");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn catcode_tables_switch_the_regime_locally() {
    let analysis = primitives(
        "\\catcode`\\@=12 \\catcode`\\@=11 \\savecatcodetable 7 \\catcode`\\@=12 \
         \\begingroup\\catcodetable 7 \\gdef\\my@name{}\\endgroup",
    );
    assert!(defined(&analysis, "my@name"), "table 7 has @ as a letter");
}

#[test]
fn luaescapestring_escapes_for_a_lua_string() {
    let analysis = primitives("\\directlua{local s = \"\\luaescapestring{a\"b}\"}");
    let chunk = analysis.facts.occurrences.iter().find(|o| o.kind == OccKind::Lua).unwrap();
    assert_eq!(chunk.key, "local s = \"a\\\"b\"");
}

#[test]
fn luacode_environments_keep_the_stream_in_step() {
    if which::which("kpsewhich").is_err() {
        return;
    }
    let source = "% !TeX program = lualatex\n\\documentclass{article}\n\\usepackage{luacode}\n\
\\begin{luacode}\nlocal x = \"{}\"\ntex.print(\"\\\\gdef\\\\fromlua{1}\")\n\\end{luacode}\n\
\\begin{luacode*}\nlocal y = \"}{\" -- % braces and percent are other here\ntex.print(\"\\\\gdef\\\\starred{2}\")\n\\end{luacode*}\n\
\\begin{document}\n\\fromlua\n\\end{document}\n";
    let cfg = Config::default();
    let analysis = Machine::analyze(source, None, &cfg);
    assert_eq!(analysis.plugins.engine, Engine::LuaTeX);
    // `tex.print` runs inside the environment's group, hence `\gdef`.
    assert!(defined(&analysis, "fromlua"));
    assert!(defined(&analysis, "starred"), "the comment ends at the line end the body keeps");
    let mismatched: Vec<_> = analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| d.span.file == analysis.main_file)
        .filter(|d| matches!(d.code, "environment-mismatch" | "unbalanced-argument" | "unbalanced-file"))
        .map(|d| d.message.clone())
        .collect();
    assert!(mismatched.is_empty(), "{mismatched:?}");
}

#[test]
fn scantextokens_has_no_end_of_line_or_file() {
    // luatex: `abXcd|abXcdXE`
    let analysis = primitives(
        "\\everyeof{E}\\endlinechar`X \\newlinechar`| \\edef\\a{\\scantextokens{ab|cd}}\\edef\\b{\\scantokens{ab|cd}}\\endlinechar13 \\edef\\out{\\a|\\b}",
    );
    let out = analysis.facts.defs.iter().rev().find(|d| analysis.interner.name(d.name) == "out").unwrap();
    let text = satex::tex::detokenize(&out.mac.as_ref().unwrap().replacement_text, &analysis.interner);
    assert_eq!(text, "abXcd|abXcdXE");
}

#[test]
fn the_token_library_defines_what_lua_runs() {
    // expl3.lua's shape: a function defining a name only while it is
    // undefined, and functions registered in `lua.get_functions_table()`.
    let analysis = primitives(
        "\\directlua{local set_lua = token.setlua or token.set_lua \
         local functions = lua.get_functions_table() \
         local function luacmd(name, id, func) if not token.is_defined(name) then set_lua(name, id) end functions[id] = func end \
         luacmd('texfilesize', 1, function() local n = token.scan_string() end) \
         luacmd('relax', 2, function() end) \
         token.set_macro(\"direct\", \"x\")}\
         \\edef\\out{\\texfilesize{a}b}",
    );
    let meaning = |name: &str| analysis.env.meaning(analysis.interner.lookup(name).expect("interned"));
    assert!(matches!(meaning("direct"), satex::tex::Meaning::Macro(_)));
    assert!(matches!(
        meaning("texfilesize"),
        satex::tex::Meaning::Primitive(satex::builtins::Primitive::LuaCall { id: 1, .. })
    ));
    assert!(!matches!(meaning("relax"), satex::tex::Meaning::Primitive(satex::builtins::Primitive::LuaCall { .. })));
    let out = analysis.facts.defs.iter().rev().find(|d| analysis.interner.name(d.name) == "out").unwrap();
    let text = satex::tex::detokenize(&out.mac.as_ref().unwrap().replacement_text, &analysis.interner);
    assert_eq!(text, "b");
}

#[test]
fn a_chunk_runs_loops_tables_and_string_format() {
    let analysis = primitives(
        "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\%=12 \\directlua{local t = {} for i = 1, 3 do t[#t + 1] = string.format('%d', i * i) end \
         tex.sprint('\\string\\\\def\\string\\\\squares{' .. table.concat(t, ',') .. '}')}",
    );
    let out = analysis.facts.defs.iter().rev().find(|d| analysis.interner.name(d.name) == "squares").expect("defined");
    let text = satex::tex::detokenize(&out.mac.as_ref().unwrap().replacement_text, &analysis.interner);
    assert_eq!(text, "1,4,9");
}

#[test]
fn a_test_of_an_unknown_value_is_unknown() {
    // `status.shell_escape` is not known: neither branch is taken for sure.
    let analysis = primitives("\\directlua{if status.shell_escape then tex.print('a') else tex.print('b') end}");
    assert!(analysis.facts.diagnostics.iter().any(|d| d.code == "lua-output"));
    let analysis = primitives("\\directlua{local t = type(os.name) if t then tex.sprint(t) end}\\edef\\out{x}");
    assert!(analysis.facts.diagnostics.iter().any(|d| d.code == "lua-output"));
    // The clock is unknown: neither `a` nor `b` is printed for sure, and
    // neither `==` nor `type` tells.
    let source = |test: &str| {
        format!(
            "\\directlua{{if {test} then tex.print('\\string\\\\gdef\\string\\\\a{{}}') else tex.print('\\string\\\\gdef\\string\\\\b{{}}') end}}"
        )
    };
    assert!(defined(&primitives(&source("1 > 0")), "a"));
    for test in ["os.clock() + 1", "os.clock() == 0", "type(os.clock()) == 'number'", "not os.clock()"] {
        let analysis = primitives(&source(test));
        assert!(!defined(&analysis, "a") && !defined(&analysis, "b"), "{test}");
        assert!(analysis.facts.diagnostics.iter().any(|d| d.code == "lua-output"), "{test}");
    }
}

#[test]
fn a_library_function_satex_lacks_is_not_an_error_pcall_catches() {
    // `node.new` exists in LuaTeX; `pcall` must not take the error branch.
    for call in ["node.new, 'glyph'", "pdf.getpos", "font.getfont, 1"] {
        let source = format!(
            "\\directlua{{if pcall({call}) then tex.print('\\string\\\\gdef\\string\\\\a{{}}') else tex.print('\\string\\\\gdef\\string\\\\b{{}}') end}}"
        );
        let analysis = primitives(&source);
        assert!(!defined(&analysis, "a") && !defined(&analysis, "b"), "{call}");
        assert!(analysis.facts.diagnostics.iter().any(|d| d.code == "lua-output"), "{call}");
    }
    // A field LuaTeX does not have is nil, as there.
    let analysis =
        primitives("\\directlua{if node.no_such_field == nil then tex.print('\\string\\\\gdef\\string\\\\c{}') end}");
    assert!(defined(&analysis, "c"));
}

#[test]
fn an_upper_case_magic_comment_selects_the_engine() {
    // TeXworks and TeXShop read `% !TEX program` and `% !TEX TS-program` alike.
    for magic in ["% !TEX program = xelatex", "% !TeX TS-program = XeLaTeX"] {
        let source = format!("{magic}\n\\relax\n");
        let analysis = Machine::analyze(&source, None, &Config::default());
        assert_eq!(analysis.plugins.engine, Engine::XeTeX, "{magic}");
    }
}
