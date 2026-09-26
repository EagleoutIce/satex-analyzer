//! Engine truth for the classes behind unicode-math under XeTeX: the engine
//! a magic comment names brings only its own primitives, `\everyeof` ends an
//! `\input` file, and a name built from partly unknown text is known by its
//! prefix and suffix.  Each expectation is what `pdftex -ini -etex` and
//! `xetex -ini -etex` print for the same input.

use satex::config::{Config, Engine};
use satex::machine::{Analysis, Machine};
use satex::plugin::Kernel;

fn run(source: &str, path: Option<&std::path::Path>, engine: Option<Engine>) -> Analysis {
    let cfg = Config { engine, kernel: Some(Kernel::None), ..Config::default() };
    Machine::analyze(source, path, &cfg)
}

// Without a format the catcodes are INITEX's (tex.web § 232); each probe
// below carries this prelude in its own source, so a probe stays
// byte-identical to what `pdftex -ini -etex`/`xetex -ini -etex` would see.
const PRELUDE: &str = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\catcode`\\^=7 ";

fn defined(analysis: &Analysis, name: &str) -> bool {
    analysis.interner.lookup(name).is_some_and(|sym| analysis.env.is_defined(sym))
}

fn body(analysis: &Analysis, name: &str) -> String {
    let def = analysis.facts.defs.iter().rev().find(|d| analysis.interner.name(d.name) == name).unwrap();
    satex::tex::detokenize(&def.mac.as_ref().unwrap().replacement_text, &analysis.interner)
}

#[test]
fn a_magic_comment_engine_has_only_its_own_primitives() {
    // xetex: `\pdftexversion` is undefined.  The magic comment must stay the
    // first thing on its own line, so the prelude gets a line of its own.
    let source = format!(
        "% !TEX program = xetex\n{PRELUDE}\\ifdefined\\pdftexversion\\def\\bad{{}}\\fi\\ifdefined\\XeTeXversion\\def\\good{{}}\\fi\n"
    );
    let analysis = run(&source, None, None);
    assert_eq!(analysis.plugins.engine, Engine::XeTeX);
    assert!(!defined(&analysis, "bad"));
    assert!(defined(&analysis, "good"));
}

#[test]
fn everyeof_is_read_before_an_input_file_ends() {
    // pdftex and xetex: `[macro:->ab ]`.
    let dir = std::env::temp_dir().join(format!("satex-everyeof-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("ef.tex"), "ab\n").unwrap();
    let main = dir.join("main.tex");
    let source = format!(
        "{PRELUDE}\\def\\get#1\\stop{{\\def\\got{{#1}}}}\\everyeof{{\\stop}}\\expandafter\\get\\input ef \\relax\n"
    );
    std::fs::write(&main, &source).unwrap();
    let analysis = run(&source, Some(&main), Some(Engine::PdfTeX));
    assert_eq!(body(&analysis, "got"), "ab ");
    assert!(!analysis.facts.diagnostics.iter().any(|d| d.code == "file-ended"));
}

#[test]
fn a_name_from_unknown_text_keeps_its_prefix_and_suffix() {
    // `\pdfuniformdeviate` is unknown to satex; the name it helps build
    // begins `MT@inh@` and ends `@x`, so `\zzz` stays undefined while
    // `\MT@inh@3@x` may be defined.
    let source = format!(
        "{PRELUDE}\\catcode`@=11 \\expandafter\\def\\csname MT@inh@\\the\\pdfelapsedtime @x\\endcsname{{}}\
\\ifdefined\\zzz\\def\\bad{{}}\\fi\\ifdefined\\@nil\\def\\bad{{}}\\fi\
\\ifcsname MT@inh@3@x\\endcsname\\def\\maybe{{}}\\fi\n"
    );
    let analysis = run(&source, None, Some(Engine::PdfTeX));
    assert!(!defined(&analysis, "bad"), "an unrelated name is still undefined");
    assert!(analysis.facts.diagnostics.iter().any(|d| d.code == "unknown-name"));
    // Undecided: `\maybe` is defined on one path only.
    assert!(defined(&analysis, "maybe"));
}

#[test]
fn the_last_line_of_a_file_without_a_newline_gets_the_endlinechar() {
    // pdftex and xetex: `[ab ]` whether `ef.tex` ends in a newline or not.
    let dir = std::env::temp_dir().join(format!("satex-lastline-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for text in ["ab", "ab  ", "ab\n"] {
        std::fs::write(dir.join("ef.tex"), text).unwrap();
        let main = dir.join("main.tex");
        let source = format!(
            "{PRELUDE}\\def\\get#1\\stop{{\\def\\got{{[#1]}}}}\\everyeof{{\\stop}}\\expandafter\\get\\input ef \\relax"
        );
        std::fs::write(&main, &source).unwrap();
        let analysis = run(&source, Some(&main), Some(Engine::PdfTeX));
        assert_eq!(body(&analysis, "got"), "[ab ]", "{text:?}");
    }
    // The main file too: its last line ends in a `^^M` of category 12.
    let source = format!("{PRELUDE}\\catcode13=12 \\def\\y#1{{\\def\\z{{[#1]}}}}\\y");
    let analysis = run(&source, None, Some(Engine::PdfTeX));
    assert_eq!(body(&analysis, "z"), "[\r]");
}

#[test]
fn a_command_lua_defines_runs_its_function() {
    // luatex -ini: `\a=macro:->10-1:4::`.  expl3.lua's shape: a forwarding
    // `luacmd`, module-level locals bound to the token library, and a local
    // function asking kpathsea and LuaFileSystem for a file's size.
    let dir = std::env::temp_dir().join(format!("satex-luacmd-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("cmds.lua"),
        "local token = token\nlocal scan_string = token.scanstring or token.scan_string\n\
local put_next = token.put_next\nlocal write = tex.write\n\
local minus_tok = token.new(string.byte'-', 12)\nlocal zero_tok = token.new(string.byte'0', 12)\n\
local one_tok = token.new(string.byte'1', 12)\n\
local kpse_find = (resolvers and resolvers.findfile) or kpse.find_file\n\
local function filesize(name)\n  local file = kpse_find(name, \"tex\", true)\n  if file then\n    return lfs.attributes(file, \"size\")\n  end\nend\n\
local set_lua = token.set_lua\nlocal functions = lua.get_functions_table()\nlocal n = 0\n\
local function luacmd(name, func)\n  n = n + 1\n  set_lua(name, n)\n  functions[n] = func\nend\n\
luacmd('cmp', function()\n  local first = scan_string()\n  local second = scan_string()\n  if first < second then\n    put_next(minus_tok, one_tok)\n  else\n    put_next(first == second and zero_tok or one_tok)\n  end\nend)\n\
luacmd('size', function()\n  local size = filesize(scan_string())\n  if size then write(size) end\nend)\n",
    )
    .unwrap();
    std::fs::write(dir.join("f.tex"), "abc\n").unwrap();
    let main = dir.join("main.tex");
    let source = "\\catcode`\\{=1 \\catcode`\\}=2 \\directlua{require(\"cmds\")}\
\\edef\\a{\\cmp{b}{a}\\cmp{a}{a}\\cmp{a}{b}:\\size{f.tex}:\\size{nofile.tex}:}\n";
    std::fs::write(&main, source).unwrap();
    let analysis = run(source, Some(&main), Some(Engine::LuaTeX));
    assert_eq!(body(&analysis, "a"), "10-1:4::");
}

#[test]
fn an_installed_font_answers_its_opentype_scripts() {
    // xetex: `3:1145457748:0` (`DFLT` first; past the list, 0).
    if which::which("kpsewhich").is_err() {
        return;
    }
    let source = format!(
        "{PRELUDE}\\font\\f=\"[lmroman10-regular]\"\
\\edef\\a{{\\the\\XeTeXOTcountscripts\\f:\\the\\XeTeXOTscripttag\\f0:\\the\\XeTeXOTscripttag\\f 9}}\n"
    );
    let analysis = run(&source, None, Some(Engine::XeTeX));
    assert_eq!(body(&analysis, "a"), "3:1145457748:0");
}

#[test]
fn a_line_ends_as_its_endlinechar_and_catcode_say() {
    // pdftex and xetex -ini -etex: `\meaning\r` after `\input probe`, with
    // `data.tex` = `a⏎␣␣b␣⏎` open for reading.
    let dir = std::env::temp_dir().join(format!("satex-lineend-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("data.tex"), "a\n  b \n").unwrap();
    let cases: &[(&str, &str)] = &[
        // `\def^^M` with `^^M` active
        (
            r"\catcode13=13 \def^^M{X}\edef\r{a
b
}\catcode13=5 %
",
            "aXbX",
        ),
        // category 12
        (
            r"\catcode13=12 \def\r{a
  b}\catcode13=5 %
",
            "a\rb",
        ),
        // category 10
        (
            r"\catcode13=10 \def\r{a
  b}\catcode13=5 %
",
            "a b",
        ),
        // category 9
        (
            r"\catcode13=9 \def\r{a
  b}\catcode13=5 %
",
            "ab",
        ),
        // category 14
        (
            r"\catcode13=14 \def\r{a
  b}\catcode13=5 %
",
            "ab",
        ),
        // blank lines are `\par`
        (
            r"\def\r{a

  b


c}",
            "a \\par b \\par \\par c",
        ),
        // `\endlinechar` from the next line
        (
            r"\endlinechar=`\X \def\r{a
b
c}\endlinechar=13 %
",
            "a bXc",
        ),
        // `\endlinechar=-1`
        (
            r"\endlinechar=-1 \def\r{a
b
c}\endlinechar=13 %
",
            "a bc",
        ),
        // out of range
        (
            r"\endlinechar=256 \def\r{a
b
c}\endlinechar=13 %
",
            "a bc",
        ),
        // a comment character
        (
            r"\endlinechar=37 \def\r{a
 b 
c}\endlinechar=13 %
",
            "a bc",
        ),
        // active spaces; trailing spaces dropped
        (
            r"\catcode32=13 \def {S}\edef\r{a b  c
  d   
e}\catcode32=10 %
",
            "aSbSSc SSd e",
        ),
        // `\scantokens`
        (
            r"\everyeof{\noexpand}\edef\r{\scantokens{a}}\everyeof{}%
",
            "a ",
        ),
        // `\scantokens`, none appended
        (
            r"\everyeof{\noexpand}\endlinechar=-1 \edef\r{\scantokens{a}}\endlinechar=13 \everyeof{}%
",
            "a",
        ),
        // `\scantokens`, category 12
        (
            r"\everyeof{\noexpand}\catcode13=12 \edef\r{\scantokens{a}}\catcode13=5 \everyeof{}%
",
            "a\r",
        ),
        // `\scantokens`, two lines
        (
            r"\everyeof{\noexpand}\edef\r{\scantokens{a
b}}\everyeof{}%
",
            "a b ",
        ),
        // a verbatim reader
        (
            r"{\catcode`\^^M=13 \gdef\vb{\catcode`\^^M=13 \def^^M{|}\vbx}\gdef\vbx#1\stopv{\gdef\r{#1}\catcode13=5 }}%
\vb a
b
\stopv
",
            "a\rb\r",
        ),
        // `\obeylines`
        (
            r"{\catcode`\^^M=13 \gdef\obeylines{\catcode`\^^M=13 \let^^M\par}}%
\def\par{P}{\obeylines\xdef\r{a
b
}}
",
            "aPbP",
        ),
        // active, empty line
        (
            r"\catcode13=13 \def^^M{X}\def\r{a

b}\catcode13=5 %
",
            "a\r\rb",
        ),
        // category 10, empty line
        (
            r"\catcode13=10 \def\r{a

b}\catcode13=5 %
",
            "a b",
        ),
        // category 12 after a trailing space
        (
            r"\catcode13=12 \def\r{a 
b}\catcode13=5 %
",
            "a\rb",
        ),
        // optional space reads the next line
        (
            r"\endlinechar=`\z %
\def\r{\foo
b}\endlinechar=13 %
",
            "\\foo b",
        ),
        // `\read`
        (
            r"\openin1=data \read1 to\r \closein1 %
",
            "a ",
        ),
        // `\readline`
        (
            r"\openin1=data \readline1 to\r \closein1 %
",
            "a\r",
        ),
        // `\readline`, none appended
        (
            r"\openin1=data \endlinechar=-1 \readline1 to\x\readline1 to\y\edef\r{\x|\y}\endlinechar=13 \closein1 %
",
            "a|  b",
        ),
        // `\read`, category 12
        (
            r"\openin1=data \catcode13=12 \read1 to\x\read1 to\y\edef\r{\x|\y}\catcode13=5 \closein1 %
",
            "a\r|b\r",
        ),
        // `\scantokens`, active
        (
            r"\everyeof{\noexpand}\catcode13=13 \def^^M{X}\edef\r{\scantokens{a
 b}}\catcode13=5 \everyeof{}%
",
            "aXbX",
        ),
        // a letter ends a control word
        (
            r"\endlinechar=122\relax
\def\r{\foo
b}\endlinechar=13 %
",
            "\\fooz b",
        ),
        // `\scantokens` drops trailing spaces
        (
            r"\everyeof{\noexpand}\catcode13=12 \edef\r{\scantokens{a  }}\catcode13=5 \everyeof{}%
",
            "a\r",
        ),
        // `\ ` at a line end names the `\endlinechar`
        (
            r"\catcode13=12 \def\r{a\ 
}\catcode13=5 %
",
            "a\\\r",
        ),
        // a line of active spaces is empty
        (
            r"\catcode32=13 \def {S}\def\r{a
   
b}\catcode32=10 %
",
            "a \\par b",
        ),
        // escape at a line end
        (
            r"\endlinechar=122\relax
\def\r{\
b}\endlinechar=13 %
",
            "\\z b",
        ),
    ];
    let main = dir.join("main.tex");
    // The prelude runs before `\input`, so its catcodes reach probe.tex too
    // (a catcode change is not scoped to the file it is made in).
    let source = format!("{PRELUDE}\\input probe \\relax\n");
    std::fs::write(&main, &source).unwrap();
    for (probe, expected) in cases {
        std::fs::write(dir.join("probe.tex"), probe).unwrap();
        let analysis = run(&source, Some(&main), Some(Engine::PdfTeX));
        assert_eq!(body(&analysis, "r"), *expected, "{probe:?}");
    }
}

#[test]
fn an_unknown_dimension_prints_as_digits_point_digits_pt() {
    // pdftex and xetex: `\the` of a dimension is `⟨digits⟩.⟨digits⟩pt`
    // (tex.web § 103), so `#1.#2\relax` splits it at the point, `#2` ends
    // in `pt`, and it reads back as a dimension; with `\dimen0=5pt` in place
    // of `\wd0`: `\one=5`, `\two=0pt`, `\r=5.0pt`.  SaTeX does not typeset, so the
    // width is unknown, but the shape is the same and nothing runs away.
    let source = format!(
        "{PRELUDE}\\setbox0\\hbox{{x}}\\dimen0=\\wd0 \\dimen2=\\the\\dimen0 \\edef\\r{{\\the\\dimen2}}\
\\def\\look#1.#2\\relax{{\\def\\one{{#1}}\\def\\two{{#2}}\\count2=#1 \\dimen4=#1.#2\\relax}}\
\\expandafter\\look\\the\\dimen0\\relax\\def\\after{{}}\n"
    );
    let analysis = run(&source, None, Some(Engine::PdfTeX));
    let r = body(&analysis, "r");
    assert!(r.contains('.') && r.ends_with("pt"), "{r}");
    assert!(body(&analysis, "two").ends_with("pt"));
    assert!(!body(&analysis, "one").contains('.'));
    assert!(defined(&analysis, "after"));
    let codes: Vec<&str> = analysis.facts.diagnostics.iter().map(|d| d.code).collect();
    assert!(!codes.iter().any(|c| ["missing-number", "runaway-argument", "stalled-loop"].contains(&&**c)), "{codes:?}");
}

/// What `pdftex -ini -etex` prints as `[…]` messages for `source`, when it
/// is installed.
fn pdftex_messages(source: &str) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("satex-truth-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::write(dir.join("t.tex"), format!("{source}\\end\n")).ok()?;
    let out = std::process::Command::new("pdftex")
        .args(["-ini", "-etex", "-interaction=batchmode", "t.tex"])
        .current_dir(&dir)
        .output()
        .ok()?;
    out.status.success().then_some(())?;
    let log = std::fs::read_to_string(dir.join("t.log")).ok()?;
    let line = log.lines().find(|l| l.starts_with('['))?;
    Some(line.split_whitespace().next()?.to_string())
}

#[test]
fn initex_catcodes_and_unknown_digits_as_characters() {
    // pdftex: `[12,1,12,12][a][B][c]` — without a format a tab, `$` and `~`
    // are other; the digits of a number are characters of category 12 to
    // `\futurelet`, `\ifx`, `\ifcat` and `\if`, whatever the number.
    let source = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \
\\edef\\A{[\\the\\catcode9,\\the\\catcode`\\{,\\the\\catcode`\\$,\\the\\catcode`\\~]}\
\\count1=\\pdfuniformdeviate 999 \\edef\\x{\\the\\count1}\
\\expandafter\\futurelet\\expandafter\\t\\expandafter\\relax\\x\\relax\
\\ifx\\t\\relax\\def\\B{[A]}\\else\\def\\B{[a]}\\fi\
\\ifcat\\t 1\\def\\C{[B]}\\else\\def\\C{[b]}\\fi\
\\if\\t\\relax\\def\\D{[C]}\\else\\def\\D{[c]}\\fi\
\\message{\\A\\B\\C\\D}";
    let expected = "[12,1,12,12][a][B][c]";
    if let Some(real) = pdftex_messages(source) {
        assert_eq!(real, expected);
    }
    let analysis = run(source, None, Some(Engine::PdfTeX));
    let got: String = ["A", "B", "C", "D"].iter().map(|n| body(&analysis, n)).collect();
    assert_eq!(got, expected);
}

/// What `pdftex -ini -etex` writes to its log as `R:…` for `source`, when
/// it is installed; errors (as "Arithmetic overflow") do not stop it.
fn pdftex_result(source: &str) -> Option<String> {
    let dir = std::env::temp_dir().join(format!("satex-arith-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::write(dir.join("a.tex"), format!("{source}\\end\n")).ok()?;
    std::process::Command::new("pdftex")
        .args(["-ini", "-etex", "-interaction=batchmode", "a.tex"])
        .current_dir(&dir)
        .output()
        .ok()?;
    let log = std::fs::read_to_string(dir.join("a.log")).ok()?;
    Some(log.lines().find_map(|l| l.strip_prefix("R:"))?.to_string())
}

/// TeX's arithmetic at its edges: `\advance` wraps at 32 bits (counts and
/// dimensions alike), `\multiply` is `mult_integers`/`nx_plus_y` and
/// `\divide` `x_over_n` (tex.web §§ 105-106, 1238-1240; an overflow keeps the
/// register), `trap_zero_glue` (§ 1229), `scan_dimen`'s rounding, units,
/// `true`, internal units and "Dimension too large" (§§ 102, 448-461), and
/// e-TeX's `\numexpr`/`\dimexpr`/`\glueexpr` (`add_or_sub`, `quotient`,
/// `fract`, `nx_plus_y`, overflow making the whole expression 0).  Each
/// probe sets `\r`; the expectation is what `pdftex -ini -etex` writes for
/// the same input.
const ARITHMETIC: &[(&str, &str)] = &[
    ("\\count1=2147483647 \\advance\\count1 by 1 \\edef\\r{\\the\\count1}", "-2147483648"),
    ("\\count1=-2147483647 \\advance\\count1 by -2 \\edef\\r{\\the\\count1}", "2147483647"),
    ("\\dimen1=16383.99999pt \\advance\\dimen1 by\\dimen1 \\edef\\r{\\the\\dimen1}", "32767.99997pt"),
    (
        "\\dimen1=16383.99999pt \\advance\\dimen1\\dimen1 \\advance\\dimen1\\dimen1 \\edef\\r{\\the\\dimen1}",
        "-16384.00005pt",
    ),
    ("\\count1=65536 \\multiply\\count1 by 32768 \\edef\\r{\\the\\count1}", "65536"),
    ("\\count1=-65536 \\multiply\\count1 by 32768 \\edef\\r{\\the\\count1}", "-65536"),
    ("\\count1=46341 \\multiply\\count1 46340 \\edef\\r{\\the\\count1}", "2147441940"),
    ("\\count1=-2147483647 \\advance\\count1 -1 \\multiply\\count1 1 \\edef\\r{\\the\\count1}", "-2147483648"),
    (
        "\\count1=-2147483647 \\advance\\count1 -1 \\count2=\\count1 \\count3=1 \\multiply\\count3\\count2 \\edef\\r{\\the\\count3}",
        "-2147483648",
    ),
    ("\\dimen1=8192pt \\multiply\\dimen1 2 \\edef\\r{\\the\\dimen1}", "8192.0pt"),
    ("\\dimen1=8191pt \\multiply\\dimen1 -2 \\edef\\r{\\the\\dimen1}", "-16382.0pt"),
    (
        "\\skip1=10000pt plus 1fil minus 2fill \\multiply\\skip1 2 \\edef\\r{\\the\\skip1}",
        "10000.0pt plus 1.0fil minus 2.0fill",
    ),
    (
        "\\skip1=100pt plus 1fil minus -3pt \\multiply\\skip1 -3 \\edef\\r{\\the\\skip1}",
        "-300.0pt plus -3.0fil minus 9.0pt",
    ),
    ("\\skip1=1pt plus 1fil \\multiply\\skip1 0 \\edef\\r{\\the\\skip1,\\the\\gluestretchorder\\skip1}", "0.0pt,0"),
    ("\\count1=-7 \\divide\\count1 2 \\edef\\r{\\the\\count1}", "-3"),
    ("\\count1=7 \\divide\\count1 -2 \\edef\\r{\\the\\count1}", "-3"),
    ("\\count1=5 \\divide\\count1 0 \\edef\\r{\\the\\count1}", "5"),
    ("\\count1=-2147483647 \\advance\\count1 -1 \\divide\\count1 -1 \\edef\\r{\\the\\count1}", "-2147483648"),
    ("\\count1=-2147483647 \\advance\\count1-1 \\divide\\count1 2 \\edef\\r{\\the\\count1}", "-1073741824"),
    ("\\count1=-2147483647 \\advance\\count1-1 \\divide\\count1 -2 \\edef\\r{\\the\\count1}", "-1073741824"),
    (
        "\\count1=-2147483647 \\advance\\count1-1 \\count2=\\count1 \\count3=7 \\multiply\\count3\\count2 \\edef\\r{\\the\\count3}",
        "-2147483648",
    ),
    ("\\count1=-2147483647 \\advance\\count1-1 \\count2=-\\count1 \\edef\\r{\\the\\count2}", "-2147483648"),
    ("\\dimen1=-1sp \\divide\\dimen1 2 \\edef\\r{\\the\\dimen1}", "0.0pt"),
    ("\\dimen1=-3sp \\divide\\dimen1 2 \\edef\\r{\\the\\dimen1}", "-0.00002pt"),
    ("\\dimen1=3pt \\divide\\dimen1 0 \\edef\\r{\\the\\dimen1}", "3.0pt"),
    (
        "\\skip1=10pt plus -7sp minus 3fil \\divide\\skip1 2 \\edef\\r{\\the\\skip1}",
        "5.0pt plus -0.00005pt minus 1.5fil",
    ),
    (
        "\\skip1=1pt plus 2fil \\advance\\skip1 by 3pt plus 4pt minus 1fill \\edef\\r{\\the\\skip1}",
        "4.0pt plus 2.0fil minus 1.0fill",
    ),
    (
        "\\skip1=1pt plus 2fil \\advance\\skip1 by 0pt plus -2fil \\edef\\r{\\the\\skip1,\\the\\gluestretchorder\\skip1}",
        "1.0pt,1",
    ),
    ("\\skip1=1pt plus 0fil \\advance\\skip1 by 0pt plus 2pt \\edef\\r{\\the\\skip1}", "1.0pt plus 2.0pt"),
    ("\\skip1=16383pt \\advance\\skip1 by 16383pt \\advance\\skip1\\skip1 \\edef\\r{\\the\\skip1}", "-4.0pt"),
    ("\\dimen1=16384pt \\edef\\r{\\the\\dimen1}", "16383.99998pt"),
    ("\\dimen1=-16384pt \\edef\\r{\\the\\dimen1}", "-16383.99998pt"),
    ("\\dimen1=16383.99999pt \\edef\\r{\\the\\dimen1}", "16383.99998pt"),
    ("\\dimen1=16383.999999pt \\edef\\r{\\the\\dimen1}", "16383.99998pt"),
    ("\\dimen1=0.00000762939453125pt \\edef\\r{\\number\\dimen1}", "1"),
    ("\\dimen1=0.0000076293945312pt \\edef\\r{\\number\\dimen1}", "0"),
    ("\\dimen1=-0.00002288818359375pt \\edef\\r{\\number\\dimen1}", "-2"),
    ("\\dimen1=0.000000000000000009999pt \\edef\\r{\\number\\dimen1}", "0"),
    (
        "\\dimen1=1in \\dimen2=1cm \\dimen3=1mm \\dimen4=1bp \\edef\\r{\\number\\dimen1,\\number\\dimen2,\\number\\dimen3,\\number\\dimen4}",
        "4736286,1864679,186467,65781",
    ),
    (
        "\\dimen1=1dd \\dimen2=1cc \\dimen3=1pc \\dimen4=1nd \\dimen5=1nc \\edef\\r{\\number\\dimen1,\\number\\dimen2,\\number\\dimen3,\\number\\dimen4,\\number\\dimen5}",
        "70124,841489,786432,69925,839105",
    ),
    (
        "\\dimen1=-1.5cm \\dimen2=0.3mm \\dimen3=226.7in \\dimen4=226.8in \\edef\\r{\\number\\dimen1,\\number\\dimen2,\\number\\dimen3,\\number\\dimen4}",
        "-2797019,55940,1073716184,1073741823",
    ),
    (
        "\\dimen1=600in \\dimen2=16383.5dd \\dimen3=1400cc \\edef\\r{\\number\\dimen1,\\number\\dimen2,\\number\\dimen3}",
        "1073741823,1073741823,1073741823",
    ),
    (
        "\\dimen1=1073741823sp \\dimen2=1073741824sp \\dimen3=-1.9sp \\edef\\r{\\number\\dimen1,\\number\\dimen2,\\number\\dimen3}",
        "1073741823,1073741823,-1",
    ),
    (
        "\\mag=3000 \\dimen1=10truept \\dimen2=1truein \\dimen3=7truesp \\dimen4=-1.3truecm \\edef\\r{\\number\\dimen1,\\number\\dimen2,\\number\\dimen3,\\number\\dimen4}",
        "218453,1578738,2,-808029",
    ),
    ("\\dimen1=3em \\dimen2=-2.5ex \\edef\\r{\\the\\dimen1,\\the\\dimen2}", "0.0pt,0.0pt"),
    ("\\dimen2=-1pt \\dimen1=20000\\dimen2 \\edef\\r{\\the\\dimen1}", "16383.99998pt"),
    ("\\dimen2=1pt \\dimen1=-20000\\dimen2 \\edef\\r{\\the\\dimen1}", "-16383.99998pt"),
    ("\\dimen2=3pt \\dimen1=-.5\\dimen2 \\edef\\r{\\the\\dimen1}", "-1.5pt"),
    ("\\dimen2=-3sp \\dimen1=.5\\dimen2 \\edef\\r{\\number\\dimen1}", "-1"),
    ("\\dimen2=1pt \\dimen1=16384\\dimen2 \\edef\\r{\\the\\dimen1}", "16383.99998pt"),
    ("\\dimen2=1pt \\dimen1=16383.99999\\dimen2 \\edef\\r{\\number\\dimen1}", "1073741823"),
    ("\\count2=-7 \\dimen1=\\count2 sp \\dimen2=-\\count2 pt \\edef\\r{\\number\\dimen1,\\the\\dimen2}", "-7,7.0pt"),
    ("\\count2=-7 \\skip1=-\\count2 pt plus -\\count2 fil \\edef\\r{\\the\\skip1}", "7.0pt plus 7.0fil"),
    (
        "\\skip2=1pt plus 2fil \\dimen1=-\\skip2 \\count1=-\\skip2 \\edef\\r{\\the\\dimen1,\\the\\count1}",
        "-1.0pt,-65536",
    ),
    ("\\skip1=1pt plus 16384fil \\edef\\r{\\the\\skip1}", "1.0pt plus 16383.99998fil"),
    ("\\skip1=1pt plus 1fillll minus 2fIlL \\edef\\r{\\the\\skip1}", "1.0pt plus 1.0filll minus 2.0fill"),
    (
        "\\edef\\r{\\the\\numexpr 7/2\\relax,\\the\\numexpr -7/2\\relax,\\the\\numexpr 7/-2\\relax,\\the\\numexpr 5/2\\relax,\\the\\numexpr -5/2\\relax}",
        "4,-4,-4,3,-3",
    ),
    (
        "\\edef\\r{\\the\\numexpr 2147483647+1\\relax,\\the\\numexpr -2147483647-1\\relax,\\the\\numexpr 2147483647-1+1\\relax}",
        "0,0,2147483647",
    ),
    (
        "\\edef\\r{\\the\\numexpr 7*11/3\\relax,\\the\\numexpr 2147483647*2/2\\relax,\\the\\numexpr (7)/0\\relax,\\the\\numexpr 3*(4-5)/-2\\relax}",
        "26,2147483647,0,2",
    ),
    (
        "\\edef\\r{\\the\\numexpr 65536*32768\\relax,\\the\\numexpr 65536*32768/2\\relax,\\the\\numexpr -65536*32767\\relax,\\the\\numexpr 5*-3/2\\relax}",
        "0,1073741824,-2147418112,-8",
    ),
    (
        "\\edef\\r{\\the\\numexpr 1+(2*3\\relax,\\the\\numexpr (1+2)*(3+4)/5\\relax,\\the\\numexpr 10/4*4\\relax,\\the\\numexpr -9/4/2\\relax}",
        "7,4,12,-1",
    ),
    (
        "\\edef\\r{\\the\\dimexpr 1pt*3/4\\relax,\\the\\dimexpr 1sp*3/2\\relax,\\the\\dimexpr -1sp/2\\relax,\\the\\dimexpr 16383pt+2pt\\relax}",
        "0.75pt,0.00003pt,-0.00002pt,0.0pt",
    ),
    (
        "\\edef\\r{\\the\\dimexpr 1pt*16384\\relax,\\the\\dimexpr 8192pt*2/2\\relax,\\the\\dimexpr 3sp*-1/2\\relax,\\the\\dimexpr 1pt/3*3\\relax}",
        "0.0pt,8192.0pt,-0.00003pt,0.99998pt",
    ),
    (
        "\\edef\\r{\\the\\glueexpr 1pt plus 2fil - 3pt plus 1fill\\relax,\\the\\glueexpr 1pt plus 1fil*3/2\\relax}",
        "-2.0pt plus 1.0fill,1.5pt plus 1.5fil",
    ),
    (
        "\\edef\\r{\\the\\glueexpr 0pt plus 1fil - 0pt plus 1fil\\relax,\\the\\gluestretchorder\\glueexpr 0pt plus 1fil - 0pt plus 1fil\\relax}",
        "0.0pt,0",
    ),
    (
        "\\edef\\r{\\the\\glueexpr 1pt minus 2pt*-1\\relax,\\the\\glueexpr 10pt plus 3sp/2\\relax,\\the\\glueexpr 16383pt plus 1pt*2\\relax}",
        "-1.0pt minus -2.0pt,5.0pt plus 0.00003pt,0.0pt",
    ),
    (
        "\\edef\\r{\\the\\gluestretchorder\\glueexpr 0pt plus 0fil\\relax,\\the\\gluestretchorder\\glueexpr 0pt plus 0fil+0pt\\relax}",
        "1,0",
    ),
    ("\\edef\\r{\\ifodd-3 T\\else F\\fi\\ifodd 0 T\\else F\\fi\\ifodd-2147483647 T\\else F\\fi}", "TFT"),
    (
        "\\edef\\r{\\ifcase -1 a\\or b\\else c\\fi\\ifcase 2 a\\or b\\else c\\fi\\ifcase 1 a\\or b\\else c\\fi\\ifcase 5 a\\or b\\fi.}",
        "ccb.",
    ),
];

#[test]
fn arithmetic_is_texs_at_every_edge() {
    let mut wrong = Vec::new();
    for (probe, expected) in ARITHMETIC {
        let source = format!("{PRELUDE}{probe}\\immediate\\write-1{{R:\\r}}\n");
        if let Some(real) = pdftex_result(&source) {
            assert_eq!(real, *expected, "pdftex: {probe}");
        }
        let analysis = run(&source, None, Some(Engine::PdfTeX));
        let got = body(&analysis, "r");
        if got != *expected {
            wrong.push(format!("{probe}\n  satex {got}, TeX {expected}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
