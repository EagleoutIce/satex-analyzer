//! Font primitives against the engines: each expected value is what
//! `pdftex -ini`, `xetex -ini` answered for the same input (TeX Live 2026).

use crate::support::installed;
use satex::config::{Config, Engine, Load, Texmf};
use satex::machine::Machine;

fn out_in(engine: Engine, source: &str) -> String {
    let cfg = Config {
        engine: Some(engine),
        load: Load { packages: false, classes: false, inputs: false, format: false, ..Default::default() },
        texmf: Texmf { kpsewhich: false, ..Default::default() },
        ..crate::support::config()
    };
    // Without a format the catcodes are INITEX's (tex.web § 232).
    let source = format!("\\catcode`\\{{=1 \\catcode`\\}}=2 \\catcode`\\#=6 \\catcode`\\^=7 {source}");
    let analysis = Machine::analyze(&source, None, &cfg);
    analysis
        .facts
        .defs
        .iter()
        .rev()
        .find(|d| analysis.interner.name(d.name) == "out")
        .and_then(|d| d.mac.as_ref())
        .map(|m| satex::tex::detokenize(&m.replacement_text, &analysis.interner))
        .unwrap_or_default()
}

#[test]
fn fonts_are_shared_by_name_and_size() {
    if !installed() {
        return;
    }
    // pdftex: `YY|cmr10 at 10.00002pt|cmr10`
    assert_eq!(
        out_in(
            Engine::PdfTeX,
            r"\font\w=cmr10 \font\v=cmr10 scaled 1000 \font\u=cmr10 at 10pt \font\z=cmr10 at 10.00001pt
              \edef\out{\ifx\w\v Y\fi\ifx\w\u Y\fi\ifx\w\z Y\fi|\fontname\z|\fontname\u}"
        ),
        "YY|cmr10 at 10.00002pt|cmr10"
    );
}

#[test]
fn a_missing_font_is_the_null_font() {
    if !installed() {
        return;
    }
    // pdftex: `select font nullfont`
    assert_eq!(out_in(Engine::PdfTeX, r"\font\w=nonexistentfont \edef\out{\meaning\w}"), "select font nullfont");
}

#[test]
fn the_space_after_a_font_name_is_consumed() {
    if !installed() {
        return;
    }
    // pdftex: `select font cmr10`, the space not typeset.
    assert_eq!(out_in(Engine::PdfTeX, r"\font\w=cmr10 \edef\out{\meaning\w}"), "select font cmr10");
}

#[test]
fn xetex_queries_of_a_tfm_font_read_no_further() {
    if !installed() {
        return;
    }
    // xetex: `0,0,127|0,01,-11,-11 1|065,0.0pt1 65|-1"wght"|1|0,1,0`
    assert_eq!(
        out_in(
            Engine::XeTeX,
            r#"\font\w=cmr10 \w \edef\out{\the\XeTeXfonttype\w,\the\XeTeXfirstfontchar\w,\the\XeTeXlastfontchar\w|\the\XeTeXcountglyphs\w,\the\XeTeXvariation\w 1,\the\XeTeXfeaturecode\w 1,\the\XeTeXselectorcode\w 1 1|\the\XeTeXcharglyph65,\the\XeTeXglyphbounds1 65|\the\XeTeXfindvariationbyname\w "wght"|\XeTeXvariationname\w 1|\the\XeTeXfonttype\nullfont,\the\XeTeXfirstfontchar\nullfont,\the\XeTeXlastfontchar\nullfont}"#
        ),
        r#"0,0,127|0,01,-11,-11 1|065,0.0pt1 65|-1"wght"|1|0,1,0"#
    );
}

#[test]
fn xetex_string_utilities_and_characters() {
    // xetex: `-1ABY0.999997` (TeX Live 2025): the version is the installed
    // engine's, 0.999998 where there is none.
    let release = satex::distribution::engine_release("xetex").unwrap_or_else(|| "0.999998".into());
    assert_eq!(
        out_in(
            Engine::XeTeX,
            r"\edef\out{\strcmp{a}{b}\Uchar65\Ucharcat 66 11 \ifprimitive\relax Y\fi\the\XeTeXversion\XeTeXrevision}"
        ),
        format!("-1ABY{release}")
    );
}

#[test]
fn xetex_character_classes_and_their_tokens() {
    // xetex: `ab,3,0`
    assert_eq!(
        out_in(
            Engine::XeTeX,
            r"\XeTeXinterchartoks 1 2 {ab}\XeTeXcharclass`a=3 \edef\out{\the\XeTeXinterchartoks 1 2,\the\XeTeXcharclass`a,\the\XeTeXcharclass`b}"
        ),
        "ab,3,0"
    );
}

#[test]
fn an_installed_font_has_unknown_metrics() {
    if !installed() {
        return;
    }
    // xetex: `"[lmroman10-regular]",3,…`; without an `at` clause its size
    // is the design size, which satex does not read.  Its `GSUB` scripts
    // (`DFLT`, `cyrl`, `latn`) it does.
    let out = out_in(
        Engine::XeTeX,
        r"\font\x=[lmroman10-regular] \edef\out{\fontname\x,\the\XeTeXOTcountscripts\x,\the\fontdimen6\x}",
    );
    assert!(out.starts_with(r#""[lmroman10-regular]",3,"#), "{out}");
    // The unknown `\fontdimen` prints as `⟨digits⟩.⟨digits⟩pt`.
    assert_eq!(out.matches("unknown").count(), 2, "{out}");
    assert!(out.ends_with("pt"), "{out}");
}

/// XeTeX reads a native font's parameters, advances and character map
/// from its OpenType tables (xetex.web `load_native_font`), and so does
/// satex; `\meaning` of it prints the size clause `\fontname` does, which
/// satex cannot know without the design size.
#[test]
fn a_native_font_is_measured_from_its_tables() {
    if !installed() {
        return;
    }
    let out = out_in(
        Engine::XeTeX,
        r#"\font\x="[lmroman10-italic]" at 12pt \edef\out{\the\fontdimen1\x,\the\fontdimen2\x,\the\fontdimen3\x,\the\fontdimen4\x,\the\fontdimen5\x,\the\fontdimen6\x,\the\fontdimen7\x,\the\fontdimen8\x|\the\fontcharwd\x`W,\the\fontcharwd\x"4E00|\iffontchar\x`W Y\else N\fi\iffontchar\x"4E00 Y\else N\fi|\the\XeTeXcountglyphs\x,\the\XeTeXfonttype\x,\the\XeTeXfirstfontchar\x,\the\XeTeXlastfontchar\x}"#,
    );
    assert_eq!(
        out,
        "0.25pt,4.296pt,2.148pt,1.43199pt,5.172pt,12.0pt,1.43199pt,8.196pt|11.988pt,3.36pt|YN|821,2,32,64260"
    );
    let meaning = out_in(Engine::XeTeX, r#"\font\x="[lmroman10-italic]" at 12pt \edef\out{\meaning\x}"#);
    assert!(meaning.starts_with("select font ") && meaning.contains("unknown"), "{meaning}");
}

#[test]
fn a_font_dimension_coerces_to_a_number() {
    if !installed() {
        return;
    }
    // pdftex: `491521,655361,491521`
    assert_eq!(
        out_in(
            Engine::PdfTeX,
            r"\font\w=cmr10 \count0=\fontcharwd\w`A\relax \count1=\fontdimen6\w \def\c{\count2}\c=\fontcharwd\w 65\relax \edef\out{\the\count0,\the\count1,\the\count2}"
        ),
        "491521,655361,491521"
    );
}

#[test]
fn a_character_width_through_macros_is_assigned() {
    if !installed() {
        return;
    }
    // pdftex: `491521,491521,491521`
    assert_eq!(
        out_in(
            Engine::PdfTeX,
            r"\countdef\mc=284 \font\w=cmr10 \w \def\f{\w}\def\ch{65}\mc=\fontcharwd\f\ch\relax
              \count1=\fontcharwd\f 65\relax \count2=\fontcharwd\f\ch\relax
              \edef\out{\the\mc,\the\count1,\the\count2}"
        ),
        "491521,491521,491521"
    );
}

#[test]
fn character_codes_start_at_the_engine_default() {
    if !installed() {
        return;
    }
    // pdftex: `1000,0,1000,1,0`
    assert_eq!(
        out_in(
            Engine::PdfTeX,
            r"\font\w=cmr10 \edef\out{\the\efcode\w`A,\the\lpcode\w`A,\the\efcode\nullfont`A,\the\tagcode\w`A,\the\knbscode\w`A}"
        ),
        "1000,0,1000,1,0"
    );
}

#[test]
fn tag_codes_come_from_the_font_and_assignments_only_clear_them() {
    if !installed() {
        return;
    }
    let source = r"\font\r=cmr10 \font\x=cmex10
        \edef\a{\the\tagcode\r`A,\the\tagcode\r`B,\the\tagcode\r 200,\the\tagcode\nullfont`A,\the\tagcode\x 0,\the\tagcode\x 64}
        \tagcode\r`B=5 \tagcode\x 0=-1 \tagcode\x 1=-5 \tagcode\x 2=-6 \tagcode\x 64=-3 \tagcode\x 65=-4 \tagcode\r`A=-6 \tagcode\r`a=-7
        \edef\out{\a;\the\tagcode\r`B,\the\tagcode\x 0,\the\tagcode\x 1,\the\tagcode\x 2,\the\tagcode\x 64,\the\tagcode\x 65,\the\tagcode\r`A,\the\tagcode\r`a}";
    // pdftex: `1,0,-1,-1,2,4;0,2,2,0,4,0,1,0`
    assert_eq!(out_in(Engine::PdfTeX, source), "1,0,-1,-1,2,4;0,2,2,0,4,0,1,0");
    // luatex (with `\tagcode` enabled): the same, but `-7` leaves the
    // ligature/kerning tag of `a`
    assert_eq!(out_in(Engine::LuaTeX, source), "1,0,-1,-1,2,4;0,2,2,0,4,0,1,1");
}

#[test]
fn a_copy_of_a_character_code_writes_the_primitive_s_table() {
    if !installed() {
        return;
    }
    let source = r"\font\r=cmr10 \let\t\tagcode \let\l\lpcode
        \t\r`A=-1 \lpcode\r`B=7 \l\r`C=9
        \edef\out{\the\tagcode\r`A,\the\t\r`A,\the\l\r`B,\the\lpcode\r`C}";
    // pdftex: `0,0,7,9`
    assert_eq!(out_in(Engine::PdfTeX, source), "0,0,7,9");
}

#[test]
fn unicode_math_codes_pack_class_family_and_slot() {
    // xetex: `0,35771470,0,0,1080037940,0,\Umathchar"4"5"6789,92301193`
    assert_eq!(
        out_in(
            Engine::XeTeX,
            r#"\Umathcode`a="1 "2 "1D44E \Udelcode`b="3 "1234 \Umathchardef\m="4 "5 "6789
               \edef\out{\the\Umathcode`a,\the\Umathcodenum`a,\the\mathcode`a,\the\Udelcode`b,\the\Udelcodenum`b,\the\delcode`b,\meaning\m,\the\m}"#
        ),
        r#"0,35771470,0,0,1080037940,0,\Umathchar"4"5"6789,92301193"#
    );
}

#[test]
fn luatex_names_fonts_by_number() {
    if !installed() {
        return;
    }
    // luatex: `1,2,3,0,select font cmr10|cmr10 at 5.0pt`
    assert_eq!(
        out_in(
            Engine::LuaTeX,
            r"\font\w=cmr10 \font\v=cmr10 at 5pt \copyfont\c\w \setfontid 2
              \edef\out{\fontid\w,\fontid\v,\fontid\c,\fontid\nullfont,\meaning\c|\fontname\font}"
        ),
        "1,2,3,0,select font cmr10|cmr10 at 5.0pt"
    );
}

#[test]
fn a_line_ends_with_the_endlinechar_in_force_when_it_is_read() {
    // etex: `[macro:->abc de f][macro:->g{}h]` (tex.web §§ 362, 483)
    assert_eq!(
        out_in(
            Engine::PdfTeX,
            "\\catcode`\\^^J=12 \\def\\grab#1^^J{\\def\\got{#1}\\grabb}\\def\\grabb#1^^J{\\def\\gotb{#1}\\endlinechar13 }\\endlinechar10 \\grab abc\nde f\ng{}h\n\\edef\\out{[\\meaning\\got][\\meaning\\gotb]}\n"
        ),
        "[macro:->abc de f][macro:->g{}h]"
    );
}

/// XeTeX's OpenType queries (`XeTeX_ext.c`): a script's languages are
/// `GSUB`'s then `GPOS`'s at the same script index — for LibertinusSans
/// `hebr` is `GSUB`'s fourth script and `GPOS`'s fourth is `latn` — and
/// its features `GSUB`'s then `GPOS`'s, language 0 being the default.
/// Glyph names come from `post` (Andika) or the CFF charset (lmroman).
#[test]
fn opentype_languages_features_and_glyph_names() {
    if !installed() {
        return;
    }
    // xetex: `3,1096434976,26,1835102827|7,0,0,12,1801810542,0|acute.dup,127,space` and `0,201,.`
    let source = r#"\font\x="[LibertinusSans-Italic.otf]" \font\c="[clarar.otf]" \font\l="[lmroman10-regular]" \font\a="[Andika-Regular.ttf]"
\l\edef\out{\the\XeTeXOTcountlanguages\x"68656272,\the\XeTeXOTlanguagetag\x"68656272 0,\the\XeTeXOTcountfeatures\x"44464C54 0,\the\XeTeXOTfeaturetag\x"44464C54 0 25|\the\XeTeXOTcountlanguages\c"6C61746E,\the\XeTeXOTlanguagetag\c"6C61746E 5,\the\XeTeXOTlanguagetag\c"6C61746E 6,\the\XeTeXOTcountfeatures\c"6C61746E "454E4720,\the\XeTeXOTfeaturetag\c"6C61746E "454E4720 9,\the\XeTeXOTcountfeatures\c"6C61746E "64666C74|\XeTeXglyphname\l 1,\the\XeTeXglyphindex "Aacute" ,\XeTeXglyphname\a 3}"#;
    assert_eq!(out_in(Engine::XeTeX, source), "3,1096434976,26,1835102827|7,0,0,12,1801810542,0|acute.dup,127,space");
    let source = r#"\font\a="[Andika-Regular.ttf]" \a\edef\out{\the\XeTeXglyphindex "nonexist" ,\the\XeTeXglyphindex "Aacute" ,\XeTeXglyphname\a 99999.}"#;
    assert_eq!(out_in(Engine::XeTeX, source), "0,201,.");
}

/// xetex.web `load_native_font`: a font with a `MATH` table has 65
/// parameters, the ninth their count and the tenth on its constants
/// (`get_ot_math_constant`), percentages unscaled.
#[test]
fn a_math_font_has_its_constants_as_parameters() {
    if !installed() {
        return;
    }
    // xetex: `0.00099pt|0.00107pt|0.00076pt|15.6pt|3.336pt|-6.67198pt|0.00092pt|0.0pt`
    let source = r#"\font\m="[latinmodern-math.otf]" at 12pt \font\r="[lmroman10-regular]"
\edef\out{\the\fontdimen9\m|\the\fontdimen10\m|\the\fontdimen11\m|\the\fontdimen12\m|\the\fontdimen63\m|\the\fontdimen64\m|\the\fontdimen65\m|\the\fontdimen9\r}"#;
    assert_eq!(
        out_in(Engine::XeTeX, source),
        "0.00099pt|0.00107pt|0.00076pt|15.6pt|3.336pt|-6.67198pt|0.00092pt|0.0pt"
    );
}

/// `\lastnodetype` reads the list's tail (etex.ch): a ligature is 7, a
/// character 0, a kern 12.  The EC fonts carry boundary characters, which
/// kern and ligature with a word's first and last character (tex.web
/// §§ 545, 1034-1040); `\noboundary` suppresses them.
#[test]
fn node_types_and_boundaries_follow_the_font() {
    if !installed() {
        return;
    }
    let source = r"\font\r=ecrm1000 \r
        \setbox0\hbox{fi\xdef\A{\the\lastnodetype}}\setbox0\hbox{a\xdef\B{\the\lastnodetype}}
        \setbox0\hbox{a\kern1pt\xdef\C{\the\lastnodetype}}\setbox0\hbox{AV}\xdef\D{\the\wd0}
        \setbox0\hbox{f\noboundary}\xdef\E{\the\wd0}\setbox0\hbox{.a}\xdef\F{\the\wd0}
        \edef\out{\A,\B,\C,\D,\E,\F}";
    // pdftex -ini -etex: `7,0,12,13.8855pt,3.05481pt,7.77588pt`
    assert_eq!(out_in(Engine::PdfTeX, source), "7,0,12,13.8855pt,3.05481pt,7.77588pt");
}

/// Without the font's metrics nothing about its ligatures is known, so the
/// node type is unknown rather than a guess.
#[test]
fn a_font_without_metrics_leaves_the_node_type_unknown() {
    let cfg = Config {
        engine: Some(Engine::PdfTeX),
        load: Load { packages: false, classes: false, inputs: false, format: false, ..Default::default() },
        texmf: Texmf { kpsewhich: false, fallback: false, ..Default::default() },
        ..crate::support::config()
    };
    let source = "\\catcode`\\{=1 \\catcode`\\}=2 \\font\\r=ecrm1000 \\r\\setbox0\\hbox{fi\\ifnum\\lastnodetype=7 \\gdef\\L{lig}\\else\\gdef\\L{char}\\fi}";
    let analysis = Machine::analyze(source, None, &cfg);
    let certain: Vec<bool> =
        analysis.facts.defs.iter().filter(|d| analysis.interner.name(d.name) == "L").map(|d| d.certain).collect();
    assert_eq!(certain, [false, false], "both arms, neither certain");
}
