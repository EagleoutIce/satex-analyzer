//! Lua under LuaTeX: `\directlua` is read as the Lua it hands the engine,
//! what it prints of literals is read back, the modules it loads are
//! dependencies, and catcode tables switch the regime.

use crate::support::{analyze_latex, defined, undefined};
use satex::analysis::machine::{Analysis, Machine};
use satex::engine::builtins::{LoadKind, OccKind};
use satex::plugins::Kernel;
use satex::project::config::{Config, Engine};

/// The LuaTeX primitives and nothing else.
fn primitives(source: &str) -> Analysis {
    let cfg = Config { engine: Some(Engine::LuaTeX), kernel: Some(Kernel::None), ..crate::support::config() };
    // initex has no grouping characters.
    let source = format!("\\catcode`\\{{=1 \\catcode`\\}}=2 {source}");
    Machine::analyze(&source, None, &cfg)
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

/// `\pdfextension glyphtounicode` reads its two general texts (LuaTeX
/// manual, "PDF extensions"): they set nothing, so vertical mode stays.
#[test]
fn an_extension_reads_its_general_texts() {
    let analysis =
        primitives(r"\pdfextension glyphtounicode{A}{0041}\pdfextension info{/T (x)}\ifvmode\def\stays{}\fi");
    assert!(defined(&analysis, "stays"));
}

#[test]
fn a_conditional_print_is_a_gap_not_a_guess() {
    let analysis = primitives(r"\directlua{if os.clock() > 1 then tex.print('\string\\def\string\\a{}') end}");
    // one run per outcome of the test: defined on one, not on the other
    assert!(!undefined(&analysis, "a") && !defined(&analysis, "a"));
}

/// luaotfload.lua stops the run when `status.safer_option` is not 0; an unknown value
/// left every LuaTeX document without luaotfload, so fonts and tagging went unknown.
/// Pins: the value is that of a run without `--safer`.
#[test]
fn a_run_is_not_safer() {
    let analysis =
        primitives(r"\directlua{if status.safer_option == 0 then tex.print('\string\\def\string\\a{}') end}");
    assert!(defined(&analysis, "a"));
}

#[test]
fn the_argument_is_expanded_first() {
    let analysis = primitives("\\def\\name{greet}\\directlua{local s = \"\\name\"}");
    let chunk = analysis.facts.occurrences.iter().find(|o| o.kind == OccKind::Lua).unwrap();
    assert_eq!(chunk.key, "local s = \"greet\"");
}

#[test]
fn required_modules_are_dependencies() {
    let dir = crate::support::tmp_root().join(format!("satex-lua-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mylib.lua"), "local helper = require('myhelper')\nreturn {}\n").unwrap();
    std::fs::write(dir.join("myhelper.lua"), "return 1\n").unwrap();
    let path = dir.join("doc.tex");
    let source = "\\catcode`\\{=1 \\catcode`\\}=2 \\directlua{require(\"mylib\") local lpeg = require('lpeg')}";
    std::fs::write(&path, source).unwrap();
    let cfg = Config { engine: Some(Engine::LuaTeX), kernel: Some(Kernel::None), ..crate::support::config() };
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
    if satex::project::paths::which("kpsewhich").is_err() {
        return;
    }
    let source = "% !TeX program = lualatex\n\\documentclass{article}\n\\usepackage{luacode}\n\
\\begin{luacode}\nlocal x = \"{}\"\ntex.print(\"\\\\gdef\\\\fromlua{1}\")\n\\end{luacode}\n\
\\begin{luacode*}\nlocal y = \"}{\" -- % braces and percent are other here\ntex.print(\"\\\\gdef\\\\starred{2}\")\n\\end{luacode*}\n\
\\begin{document}\n\\fromlua\n\\end{document}\n";
    let analysis = analyze_latex(source);
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
    let text = satex::engine::tex::detokenize(&out.mac.as_ref().unwrap().replacement_text, &analysis.interner);
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
    assert!(matches!(meaning("direct"), satex::engine::tex::Meaning::Macro(_)));
    assert!(matches!(
        meaning("texfilesize"),
        satex::engine::tex::Meaning::Primitive(satex::engine::builtins::Primitive::LuaCall { id: 1, .. })
    ));
    assert!(!matches!(
        meaning("relax"),
        satex::engine::tex::Meaning::Primitive(satex::engine::builtins::Primitive::LuaCall { .. })
    ));
    let out = analysis.facts.defs.iter().rev().find(|d| analysis.interner.name(d.name) == "out").unwrap();
    let text = satex::engine::tex::detokenize(&out.mac.as_ref().unwrap().replacement_text, &analysis.interner);
    assert_eq!(text, "b");
}

#[test]
fn a_chunk_runs_loops_tables_and_string_format() {
    let analysis = primitives(
        "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\%=12 \\directlua{local t = {} for i = 1, 3 do t[#t + 1] = string.format('%d', i * i) end \
         tex.sprint('\\string\\\\def\\string\\\\squares{' .. table.concat(t, ',') .. '}')}",
    );
    let out = analysis.facts.defs.iter().rev().find(|d| analysis.interner.name(d.name) == "squares").expect("defined");
    let text = satex::engine::tex::detokenize(&out.mac.as_ref().unwrap().replacement_text, &analysis.interner);
    assert_eq!(text, "1,4,9");
}

#[test]
fn a_test_of_an_unknown_value_is_unknown() {
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
    // Its type cannot be asked: the run stops, a gap.
    let analysis = primitives(&source("type(os.clock()) == 'number'"));
    assert!(undefined(&analysis, "a") && undefined(&analysis, "b"));
    assert!(analysis.facts.diagnostics.iter().any(|d| d.code == "lua-output"));
    // A test of it has both outcomes, a run each (`status.cs_count` is not known either).
    for test in ["os.clock() + 1", "os.clock() == 0", "not os.clock()", "os.clock() < 2", "status.cs_count"] {
        let analysis = primitives(&source(test));
        for name in ["a", "b"] {
            assert!(!undefined(&analysis, name) && !defined(&analysis, name), "{test}: \\{name}");
        }
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
        assert!(undefined(&analysis, "a") && undefined(&analysis, "b"), "{call}");
        assert!(analysis.facts.diagnostics.iter().any(|d| d.code == "lua-output"), "{call}");
    }
    // A field LuaTeX does not have is nil, as there.
    let analysis =
        primitives("\\directlua{if node.no_such_field == nil then tex.print('\\string\\\\gdef\\string\\\\c{}') end}");
    assert!(defined(&analysis, "c"));
}

#[test]
fn an_upper_case_magic_comment_selects_the_engine() {
    // TeXworks and TeXShop read `% !TEX program` and `% !TEX TS-program` alike. Which engine a
    // comment names is a reading of the source, settled before any kernel runs, so the test
    // reads it there: a full run would build a XeTeX kernel only to ask this.
    for magic in ["% !TEX program = xelatex", "% !TeX TS-program = XeLaTeX"] {
        let source = format!("{magic}\n\\relax\n");
        let engine = satex::plugins::Plugins::sniff_engine(&crate::support::config(), &source);
        assert_eq!(engine, Engine::XeTeX, "{magic}");
    }
}

/// LuaTeX has 65536 registers of each kind (LuaTeX manual § 2.4): a number
/// e-TeX rejects names one register here, so `\newinsert` past 32767 in
/// the LaTeX kernel writes that register alone.
#[test]
fn a_register_number_past_etex_names_one_register() {
    let analysis = primitives(
        r"\count0=0 \count10=272 \global\count65501=1000
\ifnum\count65501=1000 \def\ok{}\fi
\ifnum\count0=0 \def\zero{}\fi
\ifnum\count10=272 \def\kept{}\fi",
    );
    assert!(defined(&analysis, "ok"));
    assert!(defined(&analysis, "zero"));
    assert!(defined(&analysis, "kept"));
    assert!(!analysis.facts.diagnostics.iter().any(|d| d.code == "bad-register-code"));
}

/// LuaTeX's case codes reach the whole of Unicode (LuaTeX manual § 2.2),
/// so the kernel's `load-unicode-data` sets `\lccode` past 255 without an
/// "Invalid code".
#[test]
fn a_case_code_past_255_is_valid_under_luatex() {
    let analysis = primitives(
        r"\lccode256=256 \ifnum\lccode256=256 \def\ok{}\fi \lccode`\A=`\a \ifnum\lccode`\A=`\a \def\still{}\fi",
    );
    assert!(defined(&analysis, "ok"));
    assert!(defined(&analysis, "still"));
    let codes: Vec<&str> = analysis.facts.diagnostics.iter().map(|d| d.code).collect();
    assert!(!codes.contains(&"invalid-code"), "{codes:?}");
}

/// `lua.run: false`: no chunk runs, so what it prints is unknown.
#[test]
fn lua_run_off_leaves_the_output_unknown() {
    let source = "\\directlua{tex.print('\\string\\\\gdef\\string\\\\a{}')}";
    assert!(defined(&primitives(source), "a"));
    let mut cfg = Config { engine: Some(Engine::LuaTeX), kernel: Some(Kernel::None), ..crate::support::config() };
    cfg.lua.run = false;
    let analysis = Machine::analyze(&format!("\\catcode`\\{{=1 \\catcode`\\}}=2 {source}"), None, &cfg);
    assert!(undefined(&analysis, "a"));
    assert!(analysis.facts.diagnostics.iter().any(|d| d.code == "lua-output"));
}

/// One Lua state serves every path. A global one arm of an undecided conditional sets is not
/// what the run without that arm reads: after the join `x` is 0 or 1, so neither test of it
/// is decided. Pins: the sibling path and the joined path read the arm's `x = 1` as certain.
#[test]
fn a_global_one_arm_sets_is_not_what_the_other_path_reads() {
    let analysis = primitives(
        r"\directlua{x = 0}
\ifodd\uniformdeviate 7 \directlua{x = 1}\fi
\ifnum\directlua{tex.print(x)}=1 \def\one{}\else\def\zero{}\fi",
    );
    assert!(!defined(&analysis, "one"), "x is 0 where the arm did not run");
    assert!(!undefined(&analysis, "zero"), "x is 0 where the arm did not run");
}

/// The arm that does not set the global reads it while the other arm's run is already in the
/// one state. Pins: the `\else` arm read `x = 1`, which only the first arm's run holds.
#[test]
fn an_arm_does_not_read_what_its_sibling_arm_set() {
    let analysis = primitives(
        r"\directlua{x = 0}
\ifodd\uniformdeviate 7 \directlua{x = 1}\else
\ifnum\directlua{tex.print(x)}=0 \def\zero{}\else\def\wrong{}\fi\fi",
    );
    assert!(!undefined(&analysis, "zero"), "the arm that sets nothing reads the 0 from before the choice");
}

/// A path that waits at its `\fi` while its sibling runs is no longer inside the conditional
/// when it goes on. Two choices in a row: the second one's arms each start from a state the
/// first one's arms both ran in. Pins: a counter stepped on every path read as their sum.
#[test]
fn a_counter_stepped_on_each_path_is_not_their_sum() {
    let analysis = primitives(
        r"\directlua{n = 0}
\ifodd\uniformdeviate 7 \directlua{n = n + 1}\else\directlua{n = n + 1}\fi
\ifnum\directlua{tex.print(n)}=2 \def\two{}\fi
\ifnum\directlua{tex.print(n)}=1 \def\one{}\fi",
    );
    assert!(!defined(&analysis, "two"), "no run steps twice");
    assert!(!undefined(&analysis, "one"), "every run steps once");
}

/// Without any undecided choice the one state is the run's own: nothing is given up.
#[test]
fn a_run_without_choices_keeps_its_lua_state_exact() {
    let analysis = primitives(
        r"\directlua{n = 0}\directlua{n = n + 1}\ifnum\directlua{tex.print(n)}=1 \def\one{}\fi
\ifodd\uniformdeviate 7 \def\a{}\fi
\directlua{n = n + 1}\ifnum\directlua{tex.print(n)}=2 \def\two{}\fi",
    );
    assert!(defined(&analysis, "one"));
    assert!(defined(&analysis, "two"), "a choice that runs no Lua leaves the state exact");
}

/// A run that writes no state it found leaves the state's identity alone, so the paths of an
/// undecided choice stay in step: both arms call a pure compare in an old table (as expl3's
/// `\str_if_eq` calls `l3kernel.strcmp`), and the run after the join still runs.
/// Pins: every run, even a pure one, advanced the state's last run, so after the first arm's
/// compare its sibling and the joined path were out of step and every later run was unknown.
#[test]
fn a_run_that_writes_nothing_keeps_forked_paths_in_step() {
    let analysis = primitives(
        r"\directlua{cmp = {} function cmp.eq(a, b) tex.print(a == b and 1 or 0) end n = 0}
\ifodd\uniformdeviate 7 \ifnum\directlua{cmp.eq('a', 'a')}=1 \fi\else\ifnum\directlua{cmp.eq('a', 'b')}=0 \fi\fi
\directlua{local t = {} t.v = 1 n = n + t.v}\ifnum\directlua{tex.print(n)}=1 \def\one{}\fi",
    );
    assert!(defined(&analysis, "one"), "the pure compares left both paths in step");
}

/// Node type, whatsit subtype and mode numbers are the engine's own tables: `node.id`,
/// `node.subtype` agree with `node.types()`, `node.whatsits()`, and `tex.getmodevalues()` names
/// `horizontal`. lualinksplit (loaded by every `\DocumentMetadata` under LuaTeX) asks for
/// `pdf_start_link` and the modes. Pins: a hand-written whatsit table knew 7 subtypes, with
/// `save_pos`, `late_lua`, `user_defined` one off (LuaHBTeX 1.24 has `late_special` at 4), and the
/// three functions were unknown, so loading lualinksplit failed.
#[test]
fn node_and_mode_numbers_are_the_engines() {
    let analysis = primitives(
        r"\directlua{local w, t = node.whatsits(), node.types()
local ok = w[node.subtype('late_lua')] == 'late_lua' and w[node.subtype('pdf_start_link')] == 'pdf_start_link'
  and t[node.id('glyph')] == 'glyph' and w[node.subtype('late_special')] == 'late_special'
local horizontal for k, v in pairs(tex.getmodevalues()) do if v == 'horizontal' then horizontal = k end end
if ok and horizontal then tex.print('\string\\gdef\string\\ok{}') end}",
    );
    assert!(defined(&analysis, "ok"));
}

/// A run that fails on something unknown before it writes leaves the state as it was, so the
/// paths of an undecided choice stay in step: one arm's run stops at `status.list()`, and the
/// runs after the join still run. The VM counts the write mlua makes when it raises the error
/// (the metatable of a pooled error userdata), which is not the run's.
/// Pins: every failed run counted as a change of the state; expl3 under LuaTeX then lost the
/// state on all but one path after the first failed `\directlua`, and every later string
/// compare (`l3kernel.strcmp`) was unknown there: 60k forks and no tagging facts in a tagged
/// LuaLaTeX document.
#[test]
fn a_run_that_fails_before_writing_keeps_forked_paths_in_step() {
    let analysis = primitives(
        r"\directlua{n = 0}
\ifodd\uniformdeviate 7 \directlua{tex.print(status.list().x)}\fi
\directlua{n = n + 1}\ifnum\directlua{tex.print(n)}=1 \def\one{}\fi",
    );
    assert!(defined(&analysis, "one"), "the failed run wrote nothing: both paths stay in step");
}

/// The sound side of the test above: a run that writes and then fails changed the state, so
/// the path without it does not read its write. Pins: the write count taken where the run
/// failed must not drop the writes made before that point.
#[test]
fn a_run_that_writes_then_fails_puts_the_other_path_out_of_step() {
    let analysis = primitives(
        r"\directlua{n = 0}
\ifodd\uniformdeviate 7 \directlua{n = 1 tex.print(status.list().x)}\fi
\ifnum\directlua{tex.print(n)}=1 \def\one{}\else\def\zero{}\fi",
    );
    assert!(!defined(&analysis, "one"), "n is 0 where the arm did not run");
    assert!(!undefined(&analysis, "zero"), "n is 0 where the arm did not run");
}

/// Every way Lua code writes state it found counts as a write: an upvalue, a raw set, a new
/// key, a metatable, a coroutine resumed, a library table. One arm of an
/// undecided choice writes; after the join the read is undecided, never the arm's value.
/// Pins: a write tracking that misses a channel keeps both paths in step on a state only one
/// of them changed, and the read is the arm's value as certain.
#[test]
fn every_kind_of_lua_write_puts_the_other_path_out_of_step() {
    let cases = [
        ("local c = 0 function bump() c = 1 end function get() return c end", "bump()", "get()"),
        ("t = {v = 0}", "rawset(t, 'v', 1)", "t.v"),
        ("t = {}", "t.v = 1", "t.v or 0"),
        ("t = {}", "setmetatable(t, {__index = function() return 1 end})", "t.v or 0"),
        (
            "co = coroutine.create(function() coroutine.yield() end)",
            "coroutine.resume(co) coroutine.resume(co)",
            "coroutine.status(co) == 'dead' and 1 or 0",
        ),
        ("", "string.extra = 1", "string.extra or 0"),
        ("", "pcall(function() x = 1 error('e') end)", "x or 0"),
        ("t = {0}", "table.sort(t, function() return false end) t[1] = 1", "t[1]"),
    ];
    let missed: Vec<&str> = cases
        .iter()
        .filter(|(setup, write, read)| {
            let analysis = primitives(&format!(
                "\\directlua{{{setup}}}\n\\ifodd\\uniformdeviate 7 \\directlua{{{write}}}\\fi\n\
                 \\ifnum\\directlua{{tex.print({read})}}=1 \\def\\one{{}}\\else\\def\\zero{{}}\\fi"
            ));
            // The path without the write reads 0: `\one` is not certain, `\zero` may be defined.
            defined(&analysis, "one") || undefined(&analysis, "zero")
        })
        .map(|(_, write, _)| *write)
        .collect();
    assert!(missed.is_empty(), "writes read as certain on both paths: {missed:?}");
}

/// A run that stops at a test of an unknown value and wrote nothing runs once per outcome
/// (the VM takes the outcomes the host gives, `satex_choose`), and what the runs print is one
/// of their texts. Up to the test the later runs get what the first run's library calls
/// returned, so the machine reads the scanned text once. Pins: expl3's `\tex_strcmp:D` under
/// LuaTeX (`scan_string` twice, `<`, `==`, `put_next`) on an unknown string was unknown text,
/// so every `\str_if_eq:nnTF` of unknown text ran garbage (tagpdf's namespace loop).
#[test]
fn a_test_of_an_unknown_value_in_a_run_that_writes_nothing_has_each_outcome() {
    let analysis = primitives(
        r"\edef\x{\uniformdeviate 9 }\directlua{local s = token.scan_string() if s == 'a' then tex.sprint('\string\\def\string\\same{}') else tex.sprint('\string\\def\string\\other{}') end}{\x}\def\after{}",
    );
    assert!(!undefined(&analysis, "same") && !defined(&analysis, "same"), "the test may hold");
    assert!(!undefined(&analysis, "other") && !defined(&analysis, "other"), "the test may fail");
    assert!(defined(&analysis, "after"), "the text was scanned once");
}

/// The three results of a string comparison written as expl3 writes it for LuaTeX: tokens put
/// back after two tests. `\ifnum` over them is undecided, each arm live, nothing else read.
#[test]
fn a_comparison_of_unknown_strings_puts_back_one_of_its_results() {
    let analysis = primitives(
        r"\edef\x{\uniformdeviate 9 }\directlua{local minus, zero, one = token.new(45, 12), token.new(48, 12), token.new(49, 12)
function cmp() local a, b = token.scan_string(), token.scan_string() if a < b then token.put_next(minus, one) else token.put_next(a == b and zero or one) end end}
\ifcase\numexpr 1+\directlua{cmp()}{\x}{5}\relax \def\less{}\or \def\same{}\or \def\more{}\else \def\none{}\fi",
    );
    for name in ["less", "same", "more"] {
        assert!(!undefined(&analysis, name) && !defined(&analysis, name), "\\{name} is one of the results");
    }
    assert!(undefined(&analysis, "none"), "a comparison gives -1, 0 or 1");
}

/// The sound side: a run per outcome is a reading without a guess only while it changes
/// nothing. Past a test that was given an outcome the VM stops a run before it stores into
/// state it found (`satex_guard`), and the host refuses calls that act on the machine
/// (`token.set_macro`): the result is unknown, as when the run stopped at the test.
/// Pins: an outcome's write must never be what a later run reads as certain.
#[test]
fn a_write_after_a_test_of_an_unknown_value_is_no_outcome() {
    let writes = [
        ("n = 0", "n = 1", "n"),
        ("t = {v = 0}", "t.v = 1", "t.v"),
        ("t = {}", "t.v = 1", "t.v or 0"),
        ("t = {v = 0}", "rawset(t, 'v', 1)", "t.v"),
        ("local c = 0 function bump() c = 1 end function get() return c end", "bump()", "get()"),
        ("t = {}", "setmetatable(t, {__index = function() return 1 end})", "t.v or 0"),
    ];
    for (setup, write, read) in writes {
        let analysis = primitives(&format!(
            "\\edef\\x{{\\uniformdeviate 9 }}\\directlua{{{setup}}}\\directlua{{local s = token.scan_string() if s == 'a' then {write} end}}{{\\x}}\n\
             \\ifnum\\directlua{{tex.print({read})}}=1 \\def\\one{{}}\\fi"
        ));
        assert!(!defined(&analysis, "one"), "{write}: only one outcome writes it");
    }
    let defines = primitives(
        r"\edef\x{\uniformdeviate 9 }\directlua{local s = token.scan_string() if s == 'a' then token.set_macro('made', 'x') end}{\x}",
    );
    assert!(!defined(&defines, "made"), "only one outcome defines it");
}

/// A run that stops on something unknown did not do what its rest does: a value that rest
/// writes is unknown afterwards. TeX runs `n = 1` here (`os.clock()` is a number).
/// Pins: the stopped run left the state as it found it, and a later run read the old value
/// as certain, since only globals that were missing became unknown (`taint`).
#[test]
fn a_run_that_stops_before_a_write_leaves_the_written_value_unknown() {
    let analysis = primitives(
        r"\directlua{n = 0}\directlua{if type(os.clock()) == 'number' then n = 1 end}
\ifnum\directlua{tex.print(n)}=0 \def\zero{}\fi",
    );
    assert!(!defined(&analysis, "zero"), "TeX prints 1");
}

/// More tests of unknown values than `lua.outcomes` runs can follow: the result is unknown,
/// as before, never the text of some of the outcomes.
#[test]
fn more_outcomes_than_runs_allowed_is_unknown() {
    let analysis = primitives(
        r"\edef\x{\uniformdeviate 9 }\directlua{local s, n = token.scan_string(), 0 for i = 1, 20 do if s == i then n = n + 1 end end tex.sprint('\string\\def\string\\done{}')}{\x}",
    );
    assert!(!defined(&analysis, "done"), "the run was given up");
}

/// LuaLaTeX looks a package up with `kpse.find_file` and reads the path it gets back. The run's
/// directory is the document's, so a file found there is named from there. Named from satex's
/// own directory it was looked for at `dir/dir/foo.sty`: `satex -f dir/main.tex lint` gave
/// "`foo.sty` is not in this installation" for a package beside the document, and only under
/// LuaTeX and only from another directory.
#[test]
#[cfg(unix)]
fn a_package_beside_a_lualatex_document_is_found_from_another_directory() {
    if !crate::support::installed() {
        return;
    }
    let dir = crate::support::scratch_dir(
        crate::support::tmp_root().join(format!("satex-lua-beside-{}", std::process::id())),
    );
    std::fs::write(dir.join("besidedoc.sty"), "\\ProvidesPackage{besidedoc}\n\\newcommand\\beside{x}\n").unwrap();
    let source = "\\documentclass{article}\n\\usepackage{besidedoc}\n\\begin{document}\n\\beside\n\\end{document}\n";
    std::fs::write(dir.join("main.tex"), source).unwrap();
    // The document by a relative path: up to the root, then down.
    let here = std::env::current_dir().unwrap();
    let up: std::path::PathBuf = here.components().skip(1).map(|_| "..").collect();
    let main = up.join(dir.strip_prefix("/").unwrap()).join("main.tex");
    let mut cfg = Config { engine: Some(Engine::LuaTeX), ..crate::support::config() };
    cfg.load.classes = true;
    let analysis = Machine::analyze(source, Some(&main), &cfg);
    let load = analysis.facts.loads.iter().find(|l| l.name == "besidedoc").expect("the load");
    assert_eq!(load.status, satex::analysis::facts::LoadStatus::Read);
    assert!(defined(&analysis, "beside"));
}
