//! How a file's bytes become characters, per engine.  `tex` and `pdftex`
//! read bytes (tex.web § 232), so a two-byte UTF-8 character is two
//! character tokens of category 12; `xetex` and `luatex` read UTF-8
//! characters.  Every expectation below is what `pdftex -ini`, `xetex -ini`
//! and `luatex -ini` print for the same input.

use crate::support::{installed, last_body as body};
use satex::cmd::{Command, Context, Format, Output};
use satex::config::{Config, Encoding, Engine, Load, Texmf};
use satex::machine::{Analysis, Machine};
use satex::plugin::Kernel;
use satex::render::Links;
use satex::source;
use satex::tex::Catcode;

/// U+03A4 GREEK CAPITAL LETTER TAU, the bytes `CE A4`.
const TAU: &str = "\u{3a4}";
/// U+2014 EM DASH, the bytes `E2 80 94`.
const DASH: &str = "\u{2014}";

/// Without a format the catcodes are INITEX's (tex.web § 232); each probe
/// carries this prelude so it stays byte-identical to what the real engine
/// reads.
const PRELUDE: &str = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\catcode`\\^=7 \\catcode`\\%=14 ";

fn run(source: &str, engine: Engine) -> Analysis {
    let cfg = Config {
        engine: Some(engine),
        kernel: Some(Kernel::None),
        load: Load { packages: false, classes: false, inputs: false, format: false, ..Default::default() },
        texmf: Texmf { kpsewhich: false, ..Default::default() },
        ..crate::support::config()
    };
    Machine::analyze(&format!("{PRELUDE}{source}"), None, &cfg)
}

/// Byte `n` of `text` as the engine prints the character it reads: a byte
/// outside `' '`..`'~'` shows as `^^xx` (tex.web §§ 47-49).
fn byte(text: &str, n: usize) -> String {
    satex::tex::printable(char::from(text.as_bytes()[n]))
}

/// Every byte of `text` as printed by the engine
fn bytes_of(text: &str) -> String {
    text.bytes().map(|b| satex::tex::printable(char::from(b))).collect()
}

/// What `satex tokens` prints for `text` under `engine`: every character
/// token with its category and its column.
fn tokens(text: &str, engine: Engine) -> Vec<(char, u8, u32)> {
    let analysis = Analysis::empty();
    let context = Context { analysis: &analysis, source: text, format: Format::Json, links: Links(false) };
    let out = source::with(engine.encoding(), || Command::Tokens.run(&context, &mut Vec::new()).unwrap());
    let Output::Records(records) = out else { panic!("`tokens` answers with records") };
    records
        .iter()
        .filter(|r| r["kind"] == "char")
        .map(|r| {
            let name = r["name"].as_str().expect("a character token is named by its character");
            let detail = r["detail"].as_str().expect("a character token carries its category");
            let cat = detail.strip_prefix("cat ").and_then(|n| n.parse().ok()).expect("`cat N`");
            (name.chars().next().expect("one character"), cat, r["col"].as_u64().unwrap() as u32)
        })
        .collect()
}

#[test]
fn the_engines_split_into_eight_bit_and_unicode() {
    assert_eq!(Engine::Tex.encoding(), Encoding::Bytes);
    assert_eq!(Engine::PdfTeX.encoding(), Encoding::Bytes);
    assert_eq!(Engine::XeTeX.encoding(), Encoding::Utf8);
    assert_eq!(Engine::LuaTeX.encoding(), Encoding::Utf8);
}

// The engine probe every `loadhyph-*.tex` of hyph-utf8 uses.

/// `\def\testengine#1#2!{\def\secondarg{#2}}\testengine Τ!` leaves
/// `\secondarg` empty on a Unicode engine and holding the second byte on an
/// 8-bit one.
fn engine_probe() -> String {
    format!("\\def\\testengine#1#2!{{\\def\\secondarg{{#2}}}}\\testengine {TAU}!\\relax")
}

#[test]
fn pdftex_takes_the_eight_bit_branch_of_the_hyph_utf8_probe() {
    for engine in [Engine::PdfTeX, Engine::Tex] {
        // pdftex: `\meaning\secondarg` is `macro:->^^a4`, the second byte.
        let analysis = run(&engine_probe(), engine);
        assert_eq!(body(&analysis, "secondarg"), byte(TAU, 1), "{engine:?}");
    }
}

#[test]
fn luatex_and_xetex_take_the_unicode_branch_of_the_hyph_utf8_probe() {
    for engine in [Engine::LuaTeX, Engine::XeTeX] {
        // xetex and luatex: `\meaning\secondarg` is `macro:->`.
        let analysis = run(&engine_probe(), engine);
        assert_eq!(body(&analysis, "secondarg"), "", "{engine:?}");
    }
}

#[test]
fn a_unicode_engine_chardefs_the_largest_code_point() {
    // xetex -ini, luatex -ini: `\number\c` is 1114111, no error.
    for engine in [Engine::LuaTeX, Engine::XeTeX] {
        let analysis = run("\\chardef\\c=1114111 \\edef\\x{\\number\\c}", engine);
        assert_eq!(body(&analysis, "x"), "1114111", "{engine:?}");
    }
}

// Tokens, categories and columns.

#[test]
fn pdftex_reads_a_two_byte_character_as_two_tokens_of_category_twelve() {
    // pdftex -ini: `\the\catcode206` and `\the\catcode164` are both 12.
    let read = tokens(&format!("a{TAU}b"), Engine::PdfTeX);
    assert_eq!(read.len(), 4);
    assert_eq!(read[0], ('a', Catcode::Letter as u8, 1));
    assert_eq!(read[1], ('\u{ce}', Catcode::Other as u8, 2));
    assert_eq!(read[2], ('\u{a4}', Catcode::Other as u8, 3));
    assert_eq!(read[3], ('b', Catcode::Letter as u8, 4));
}

#[test]
fn pdftex_reads_a_three_byte_character_as_three_tokens_of_category_twelve() {
    let read = tokens(&format!("a{DASH}b"), Engine::PdfTeX);
    assert_eq!(read.len(), 5);
    assert_eq!(read[0], ('a', Catcode::Letter as u8, 1));
    assert_eq!(read[1], ('\u{e2}', Catcode::Other as u8, 2));
    assert_eq!(read[2], ('\u{80}', Catcode::Other as u8, 3));
    assert_eq!(read[3], ('\u{94}', Catcode::Other as u8, 4));
    assert_eq!(read[4], ('b', Catcode::Letter as u8, 5));
}

#[test]
fn a_unicode_engine_reads_a_multi_byte_character_as_one_token() {
    for engine in [Engine::LuaTeX, Engine::XeTeX] {
        let read = tokens(&format!("a{TAU}b"), engine);
        assert_eq!(read.len(), 3, "{engine:?}");
        assert_eq!(read[0], ('a', Catcode::Letter as u8, 1), "{engine:?}");
        // Past 255 the LaTeX table classifies a character as a letter.
        assert_eq!(read[1], ('\u{3a4}', Catcode::Letter as u8, 2), "{engine:?}");
        assert_eq!(read[2], ('b', Catcode::Letter as u8, 3), "{engine:?}");

        let read = tokens(&format!("a{DASH}b"), engine);
        assert_eq!(read.len(), 3, "{engine:?}");
        assert_eq!(read[1], ('\u{2014}', Catcode::Letter as u8, 2), "{engine:?}");
        assert_eq!(read[2], ('b', Catcode::Letter as u8, 3), "{engine:?}");
    }
}

// `\string`, `\detokenize` and `\meaning`.

#[test]
fn string_of_a_non_ascii_character_is_its_bytes_under_pdftex() {
    // pdftex: `\meaning\A` is `macro:->^^ce^^a4`; xetex and luatex print
    // `macro:->Τ`.
    let source = format!("\\edef\\A{{\\string {TAU}}}");
    assert_eq!(body(&run(&source, Engine::PdfTeX), "A"), bytes_of(TAU));
    assert_eq!(body(&run(&source, Engine::XeTeX), "A"), TAU);
    assert_eq!(body(&run(&source, Engine::LuaTeX), "A"), TAU);
}

#[test]
fn detokenize_of_a_non_ascii_character_is_its_bytes_under_pdftex() {
    // pdftex: `\meaning\B` is `macro:->{^^ce^^a4}`.
    let source = format!("\\edef\\B{{\\detokenize{{{TAU}}}}}");
    assert_eq!(body(&run(&source, Engine::PdfTeX), "B"), bytes_of(TAU));
    assert_eq!(body(&run(&source, Engine::XeTeX), "B"), TAU);
    assert_eq!(body(&run(&source, Engine::LuaTeX), "B"), TAU);
}

#[test]
fn meaning_of_a_non_ascii_character_is_its_first_byte_under_pdftex() {
    // pdftex: `\meaning\M` is `macro:->the character ^^ce`; xetex prints
    // `macro:->the character Τ`.
    let source = format!("\\let\\C={TAU}\\edef\\M{{\\meaning\\C}}");
    assert_eq!(body(&run(&source, Engine::PdfTeX), "M"), format!("the character {}", byte(TAU, 0)));
    assert_eq!(body(&run(&source, Engine::XeTeX), "M"), format!("the character {TAU}"));
}

// Argument grabbing.

#[test]
fn an_undelimited_argument_takes_one_byte_under_pdftex() {
    // pdftex: `\two ΤZ` gives `\gotA` = `^^ce` and `\gotB` = `^^a4`, so `Z`
    // is still in the stream; xetex and luatex give `Τ` and `Z`.
    let source = format!("\\def\\two#1#2{{\\def\\gotA{{#1}}\\def\\gotB{{#2}}}}\\two {TAU}Z");
    let analysis = run(&source, Engine::PdfTeX);
    assert_eq!(body(&analysis, "gotA"), byte(TAU, 0));
    assert_eq!(body(&analysis, "gotB"), byte(TAU, 1));
    for engine in [Engine::XeTeX, Engine::LuaTeX] {
        let analysis = run(&source, engine);
        assert_eq!(body(&analysis, "gotA"), TAU, "{engine:?}");
        assert_eq!(body(&analysis, "gotB"), "Z", "{engine:?}");
    }
}

// The per-character code tables.

#[test]
fn the_catcode_of_a_byte_past_127_is_twelve_under_pdftex() {
    // pdftex: `\meaning\R` is `macro:->12^^a4` — the backtick constant takes
    // the first byte, so the second one ends the number and stays.  xetex
    // prints `macro:->12`.
    let source = format!("\\edef\\R{{\\the\\catcode`{TAU}}}");
    assert_eq!(body(&run(&source, Engine::PdfTeX), "R"), format!("12{}", byte(TAU, 1)));
    assert_eq!(body(&run(&source, Engine::XeTeX), "R"), "12");
}

#[test]
fn an_lccode_set_on_a_byte_past_127_reads_back_under_pdftex() {
    // pdftex: `\lccode206=238 \the\lccode206` is 238.
    let source = "\\lccode206=238 \\edef\\R{\\the\\lccode206}";
    assert_eq!(body(&run(source, Engine::PdfTeX), "R"), "238");
}

#[test]
fn a_backtick_constant_of_a_non_ascii_character_is_its_first_byte_under_pdftex() {
    // pdftex: `\meaning\R` is `macro:->206^^a4`; xetex prints `macro:->932`.
    let source = format!("\\edef\\R{{\\number`{TAU}}}");
    assert_eq!(body(&run(&source, Engine::PdfTeX), "R"), format!("206{}", byte(TAU, 1)));
    assert_eq!(body(&run(&source, Engine::XeTeX), "R"), "932");
}

// Columns.

#[test]
fn a_column_counts_bytes_under_an_eight_bit_engine_and_characters_otherwise() {
    let line = format!("{TAU}x");
    source::with(Encoding::Bytes, || {
        assert_eq!(source::columns(&line), 3);
        assert_eq!(source::byte_of_col(&line, 3), 2);
        assert_eq!(source::col_of_byte(&line, 2), 3);
    });
    source::with(Encoding::Utf8, || {
        assert_eq!(source::columns(&line), 2);
        assert_eq!(source::byte_of_col(&line, 2), 2);
        assert_eq!(source::col_of_byte(&line, 2), 2);
    });
}

// Invalid UTF-8 under a Unicode engine.

#[test]
fn an_invalid_utf8_byte_reads_as_a_replacement_character_and_is_counted() {
    // xetex: `a^^ff^^feb` reads as 97, 65533, 65533, 98 — one replacement
    // character per sequence it cannot use.  luatex stops with "String
    // contains an invalid utf-8 sequence", so satex over-approximates with
    // XeTeX's reading and records a gap.
    source::with(Encoding::Utf8, || {
        let (text, invalid) = source::decode_counted(b"a\xff\xfeb");
        assert_eq!(text, "a\u{fffd}\u{fffd}b");
        assert_eq!(invalid, 2);
        // xetex: `^^e2^^80x` reads as 65533, 120 — the truncated sequence is
        // one replacement character, not two.
        let (text, invalid) = source::decode_counted(b"\xe2\x80x");
        assert_eq!(text, "\u{fffd}x");
        assert_eq!(invalid, 2);
        // xetex: `^^c3(y` reads as 65533, 40, 121.
        assert_eq!(source::decode_counted(b"\xc3(y"), ("\u{fffd}(y".to_string(), 1));
    });
    source::with(Encoding::Bytes, || {
        let (text, invalid) = source::decode_counted(b"a\xff\xfeb");
        assert_eq!(text, "a\u{ff}\u{fe}b");
        assert_eq!(invalid, 0);
    });
}

// The real-world symptom: hyph-utf8 picks a pattern file per engine.

/// Every file the run opened, by its short name.
fn opened(analysis: &Analysis) -> Vec<String> {
    (0..analysis.files.len()).map(|id| analysis.short_name(id as u16).to_string()).collect()
}

fn with_format(engine: Engine) -> Analysis {
    let cfg =
        Config { load: Load { classes: true, ..Default::default() }, engine: Some(engine), ..crate::support::config() };
    Machine::analyze("\\documentclass{article}\\begin{document}x\\end{document}\n", None, &cfg)
}

#[test]
fn hyph_utf8_gives_pdftex_the_eight_bit_greek_patterns() {
    if !installed() {
        return;
    }
    // `loadhyph-grc.tex` runs the engine probe above and inputs
    // `grahyph5.tex` on an 8-bit engine, `hyph-grc.tex` on a Unicode one.
    let files = opened(&with_format(Engine::PdfTeX));
    assert!(files.iter().any(|f| f == "loadhyph-grc.tex"), "{files:?}");
    assert!(files.iter().any(|f| f == "grahyph5.tex"), "{files:?}");
    assert!(!files.iter().any(|f| f == "hyph-grc.tex"), "{files:?}");
}

#[test]
fn hyph_utf8_gives_xetex_the_unicode_greek_patterns() {
    if !installed() {
        return;
    }
    let files = opened(&with_format(Engine::XeTeX));
    assert!(files.iter().any(|f| f == "loadhyph-grc.tex"), "{files:?}");
    assert!(files.iter().any(|f| f == "hyph-grc.tex"), "{files:?}");
    assert!(!files.iter().any(|f| f == "grahyph5.tex"), "{files:?}");
}
