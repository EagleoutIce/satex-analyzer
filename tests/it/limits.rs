//! Parameters an engine range-checks, refuses to change, or starts away from
//! zero (`builtins::limit`), and quantities the current list or the page
//! builder owns rather than the parameter table (`builtins::owned`).  Every
//! value here was measured with `\showthe` on the TeX Live 2026 binaries,
//! engine by engine, not read off a manual.

use satex::builtins::{Primitive, initial_meanings};
use satex::config::{Config, Engine, Interaction, Load, Texmf};
use satex::machine::{Analysis, Machine};
use satex::plugin::Kernel;
use satex::tex::Meaning;

const ENGINES: [Engine; 4] = [Engine::Tex, Engine::PdfTeX, Engine::XeTeX, Engine::LuaTeX];

/// Without a format the catcodes are INITEX's (tex.web § 232).
const PRELUDE: &str = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\catcode`\\^=7 ";

/// The marker a widened text carries, which is what a value satex could not
/// pin down reads back as.
const WIDENED: char = '\u{2}';

fn analyze(engine: Engine, source: &str) -> Analysis {
    let cfg = Config {
        engine: Some(engine),
        kernel: Some(Kernel::None),
        load: Load { packages: false, classes: false, inputs: false, format: false, ..Default::default() },
        texmf: Texmf { kpsewhich: false, ..Default::default() },
        ..crate::support::config()
    };
    Machine::analyze(&format!("{PRELUDE}{source}"), None, &cfg)
}

fn body(analysis: &Analysis, name: &str) -> String {
    analysis
        .facts
        .defs
        .iter()
        .find(|d| analysis.interner.name(d.name) == name)
        .and_then(|d| d.mac.as_ref())
        .map(|m| satex::tex::detokenize(&m.replacement_text, &analysis.interner))
        .unwrap_or_default()
}

/// What `\the⟨what⟩` reads after `source` has run under `engine`.
fn the(engine: Engine, source: &str, what: &str) -> String {
    let analysis = analyze(engine, &format!("{source}\\edef\\zR{{\\the{what}}}"));
    body(&analysis, "zR")
}

/// What `text` expands to after `source` has run: `\meaning`, `\string`
/// and the like, which `the` cannot reach because it prepends `\the`.
fn expand(engine: Engine, source: &str, text: &str) -> String {
    let analysis = analyze(engine, &format!("{source}\\edef\\zR{{{text}}}"));
    body(&analysis, "zR")
}

/// How many diagnostics of `code` the run raised.
fn raised(engine: Engine, source: &str, code: &str) -> usize {
    analyze(engine, source).facts.diagnostics.iter().filter(|d| d.code == code).map(|d| d.count as usize).sum()
}

/// Whether satex gave a widened answer rather than one confident value,
/// which is what it must do where it cannot follow the engine.
fn widened(engine: Engine, source: &str, what: &str) -> bool {
    the(engine, source, what).contains(WIDENED)
}

/// `source` run in a directory of its own that holds `zprobe.tex`, which is
/// the file the `\openin` probes read.
fn beside_a_file(engine: Engine, name: &str, source: &str) -> Analysis {
    let directory = std::env::temp_dir().join("satex-limits").join(format!("{name}-{engine:?}"));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    std::fs::write(directory.join("zprobe.tex"), "hello\n").unwrap();
    let main = directory.join("main.tex");
    let text = format!("{PRELUDE}{source}");
    std::fs::write(&main, &text).unwrap();
    let cfg = Config {
        engine: Some(engine),
        kernel: Some(Kernel::None),
        load: Load { packages: false, classes: false, inputs: false, format: false, ..Default::default() },
        texmf: Texmf { kpsewhich: false, ..Default::default() },
        ..crate::support::config()
    };
    let analysis = Machine::analyze(&text, Some(&main), &cfg);
    let _ = std::fs::remove_dir_all(&directory);
    analysis
}

fn defined(engine: Engine, name: &str) -> bool {
    let mut interner = satex::tex::Interner::default();
    let table = initial_meanings(&mut interner, engine);
    interner.lookup(name).and_then(|sym| table.get(&sym)).is_some()
}

// `\outputbox`: LuaTeX's, 0..65535, and a refused value leaves the old one.

#[test]
fn outputbox_starts_at_255_and_takes_a_box_number() {
    assert_eq!(the(Engine::LuaTeX, "", "\\outputbox"), "255");
    assert_eq!(the(Engine::LuaTeX, "\\outputbox=300 ", "\\outputbox"), "300");
    assert_eq!(the(Engine::LuaTeX, "\\outputbox=0 ", "\\outputbox"), "0");
    assert_eq!(the(Engine::LuaTeX, "\\outputbox=65535 ", "\\outputbox"), "65535");
    assert_eq!(raised(Engine::LuaTeX, "\\outputbox=65535 ", "invalid-code"), 0);
}

#[test]
fn outputbox_out_of_range_keeps_the_old_value() {
    for bad in ["70000", "65536", "-1"] {
        let source = format!("\\outputbox=300 \\outputbox={bad} ");
        assert_eq!(the(Engine::LuaTeX, &source, "\\outputbox"), "300", "\\outputbox={bad}");
        assert_eq!(raised(Engine::LuaTeX, &source, "invalid-code"), 1, "\\outputbox={bad}");
    }
}

#[test]
fn outputbox_is_a_luatex_name_only() {
    assert!(defined(Engine::LuaTeX, "outputbox"));
    for engine in [Engine::Tex, Engine::PdfTeX, Engine::XeTeX] {
        assert!(!defined(engine, "outputbox"), "\\outputbox must be undefined under {engine:?}");
    }
}

// `\prevgraf`: every engine, 0 upwards (tex.web § 1244).

#[test]
fn prevgraf_takes_a_non_negative_number() {
    for engine in ENGINES {
        assert!(defined(engine, "prevgraf"), "\\prevgraf under {engine:?}");
        assert_eq!(the(engine, "", "\\prevgraf"), "0", "{engine:?}");
        assert_eq!(the(engine, "\\prevgraf=5 ", "\\prevgraf"), "5", "{engine:?}");
        assert_eq!(raised(engine, "\\prevgraf=5 ", "invalid-code"), 0, "{engine:?}");
    }
}

#[test]
fn prevgraf_out_of_range_keeps_the_old_value() {
    for engine in ENGINES {
        let source = "\\prevgraf=5 \\prevgraf=-3 ";
        assert_eq!(the(engine, source, "\\prevgraf"), "5", "{engine:?}");
        assert_eq!(raised(engine, source, "invalid-code"), 1, "{engine:?}");
    }
}

// `\interactionmode`: 0..3, and not on the save stack (tex.web § 1265).

#[test]
fn interactionmode_starts_at_the_engines_option() {
    // `Config::default()` is `-interaction=nonstopmode` (tex.web § 1337).
    assert_eq!(Config::default().interaction, Interaction::Nonstopmode);
    for engine in ENGINES {
        assert_eq!(the(engine, "", "\\interactionmode"), "1", "{engine:?}");
        for mode in ["0", "1", "2", "3"] {
            let source = format!("\\interactionmode={mode} ");
            assert_eq!(the(engine, &source, "\\interactionmode"), mode, "{engine:?}");
            assert_eq!(raised(engine, &source, "invalid-code"), 0, "{engine:?}");
        }
    }
}

#[test]
fn interactionmode_out_of_range_keeps_the_old_mode() {
    for engine in ENGINES {
        for bad in ["9", "-1", "4"] {
            let source = format!("\\interactionmode=2 \\interactionmode={bad} ");
            assert_eq!(the(engine, &source, "\\interactionmode"), "2", "{engine:?} {bad}");
            assert_eq!(raised(engine, &source, "invalid-code"), 1, "{engine:?} {bad}");
        }
    }
}

/// `new_interaction` does not put the old mode on the save stack, so the
/// group it was set in never takes it back.
#[test]
fn interactionmode_outlives_the_group_it_was_set_in() {
    assert_eq!(the(Engine::PdfTeX, "{\\interactionmode=2 }", "\\interactionmode"), "2");
}

// The read-only LuaTeX math mode parameters.

/// Measured under `luatex 1.24.0`: each reads one value, before and after a
/// write of 2.
const READ_ONLY: [(&str, &str); 6] = [
    ("mathnolimitsmode", "0"),
    ("mathitalicsmode", "0"),
    ("mathdelimitersmode", "0"),
    ("mathdefaultsmode", "1"),
    ("mathscriptboxmode", "1"),
    ("mathscriptcharmode", "1"),
];

#[test]
fn the_read_only_math_mode_parameters_ignore_a_write() {
    for (name, reads) in READ_ONLY {
        assert_eq!(the(Engine::LuaTeX, "", &format!("\\{name}")), reads, "\\{name} before a write");
        let source = format!("\\{name}=2 ");
        assert_eq!(the(Engine::LuaTeX, &source, &format!("\\{name}")), reads, "\\{name} after a write");
        assert_eq!(raised(Engine::LuaTeX, &source, "read-only-parameter"), 1, "\\{name}");
        assert_eq!(raised(Engine::LuaTeX, &source, "invalid-code"), 0, "\\{name}");
    }
}

/// A value satex could not pin down changes nothing either: the parameter is
/// read-only whatever was written.
#[test]
fn a_read_only_parameter_stays_exact_under_an_unpinnable_write() {
    for (name, reads) in READ_ONLY {
        let source = format!("\\{name}=\\uniformdeviate 2 \\relax ");
        assert_eq!(the(Engine::LuaTeX, &source, &format!("\\{name}")), reads, "\\{name}");
    }
}

#[test]
fn the_read_only_math_mode_parameters_are_luatex_names_only() {
    for (name, _) in READ_ONLY {
        assert!(defined(Engine::LuaTeX, name), "\\{name} under LuaTeX");
        for engine in [Engine::Tex, Engine::PdfTeX, Engine::XeTeX] {
            assert!(!defined(engine, name), "\\{name} must be undefined under {engine:?}");
        }
    }
}

/// Their two siblings are writable, which is what says the read-only list is
/// a measurement and not a guess about the whole family.
#[test]
fn the_writable_math_mode_parameters_still_take_a_value() {
    for name in ["mathrulesmode", "mathpenaltiesmode"] {
        assert_eq!(the(Engine::LuaTeX, "", &format!("\\{name}")), "0", "\\{name}");
        let source = format!("\\{name}=2 ");
        assert_eq!(the(Engine::LuaTeX, &source, &format!("\\{name}")), "2", "\\{name}");
        assert_eq!(raised(Engine::LuaTeX, &source, "read-only-parameter"), 0, "\\{name}");
    }
}

// `\Umathcodenum`: the packed math code, whose recovery differs by engine.

#[test]
fn umathcodenum_takes_a_packed_math_code() {
    for engine in [Engine::XeTeX, Engine::LuaTeX] {
        for good in ["0", "100", "1114111", "2097152", "2146500607"] {
            let source = format!("\\Umathcodenum`\\A={good} ");
            assert_eq!(the(engine, &source, "\\Umathcodenum`\\A"), good, "{engine:?} {good}");
            assert_eq!(raised(engine, &source, "invalid-code"), 0, "{engine:?} {good}");
        }
    }
}

/// LuaTeX stores 0 for a code it refuses.  XeTeX stores `0x1FFFFF`, its
/// active marker, where the low 21 bits ask for one, and 0 otherwise.
#[test]
fn umathcodenum_out_of_range_stores_each_engines_own_recovery() {
    for (bad, lua, xe) in [("-1", "0", "2097151"), ("1073741823", "0", "2097151"), ("-2", "0", "0")] {
        let source = format!("\\Umathcodenum`\\A=100 \\Umathcodenum`\\A={bad} ");
        assert_eq!(the(Engine::LuaTeX, &source, "\\Umathcodenum`\\A"), lua, "luatex {bad}");
        assert_eq!(the(Engine::XeTeX, &source, "\\Umathcodenum`\\A"), xe, "xetex {bad}");
        assert_eq!(raised(Engine::LuaTeX, &source, "invalid-code"), 1, "luatex {bad}");
        assert_eq!(raised(Engine::XeTeX, &source, "invalid-code"), 1, "xetex {bad}");
    }
}

/// `0x1FFFFF` is XeTeX's active marker and a code of its own there, while
/// LuaTeX has no marker and refuses it.
#[test]
fn the_active_marker_is_a_xetex_code_only() {
    assert_eq!(the(Engine::XeTeX, "\\Umathcodenum`\\A=2097151 ", "\\Umathcodenum`\\A"), "2097151");
    assert_eq!(raised(Engine::XeTeX, "\\Umathcodenum`\\A=2097151 ", "invalid-code"), 0);
    assert_eq!(the(Engine::LuaTeX, "\\Umathcodenum`\\A=100 \\Umathcodenum`\\A=2097151 ", "\\Umathcodenum`\\A"), "0");
    assert_eq!(raised(Engine::LuaTeX, "\\Umathcodenum`\\A=2097151 ", "invalid-code"), 1);
}

/// Both engines check the low 21 bits alone, so a negative value whose
/// character fits is taken and one whose character does not is refused.
#[test]
fn a_negative_math_code_whose_character_fits_is_taken() {
    for engine in [Engine::XeTeX, Engine::LuaTeX] {
        for good in ["-2097152", "-4194304"] {
            let source = format!("\\Umathcodenum`\\A={good} ");
            assert_eq!(the(engine, &source, "\\Umathcodenum`\\A"), good, "{engine:?} {good}");
            assert_eq!(raised(engine, &source, "invalid-code"), 0, "{engine:?} {good}");
        }
        let source = "\\Umathcodenum`\\A=100 \\Umathcodenum`\\A=-65536 ";
        assert_eq!(the(engine, source, "\\Umathcodenum`\\A"), "0", "{engine:?}");
        assert_eq!(raised(engine, source, "invalid-code"), 1, "{engine:?}");
    }
    // `-2147483647` carries character 1, so both engines take it.
    let source = "\\Umathcodenum`\\A=-2147483647 ";
    assert_eq!(the(Engine::XeTeX, source, "\\Umathcodenum`\\A"), "-2147483647");
    assert_eq!(raised(Engine::XeTeX, source, "invalid-code"), 0);
    assert_eq!(raised(Engine::LuaTeX, source, "invalid-code"), 0);
}

/// LuaTeX splits an assigned code into a family, a class and a character and
/// stores it recomposed, so a negative one comes back changed.  XeTeX stores
/// the code whole.  Neither is an error, and a non-negative code is
/// untouched.
#[test]
fn luatex_stores_a_negative_math_code_recomposed() {
    for (code, lua) in [
        ("-2147483647", "-2145386495"),
        ("-1048576", "1048576"),
        ("-2097152", "-2097152"),
        ("-16777216", "-16777216"),
        ("-33554432", "-33554432"),
        ("-6291456", "-6291456"),
        ("-268435456", "-268435456"),
        ("-2145386496", "-2145386496"),
        ("2146500607", "2146500607"),
    ] {
        let source = format!("\\Umathcodenum`\\A={code} ");
        assert_eq!(the(Engine::LuaTeX, &source, "\\Umathcodenum`\\A"), lua, "luatex {code}");
        assert_eq!(the(Engine::LuaTeX, &source, "\\mathcode`\\A"), lua, "luatex mathcode {code}");
        assert_eq!(the(Engine::XeTeX, &source, "\\Umathcodenum`\\A"), code, "xetex {code}");
        assert_eq!(raised(Engine::LuaTeX, &source, "invalid-code"), 0, "luatex {code}");
    }
    // `\\Umathcharnumdef` takes the same packed code, so it is recomposed too.
    let source = "\\Umathcharnumdef\\zq=-2147483647 ";
    assert_eq!(the(Engine::LuaTeX, source, "\\zq"), "-2145386495");
    assert_eq!(the(Engine::XeTeX, source, "\\zq"), "-2147483647");
}

#[test]
fn umathcodenum_is_a_unicode_math_name_only() {
    for engine in [Engine::XeTeX, Engine::LuaTeX] {
        assert!(defined(engine, "Umathcodenum"), "\\Umathcodenum under {engine:?}");
    }
    for engine in [Engine::Tex, Engine::PdfTeX] {
        assert!(!defined(engine, "Umathcodenum"), "\\Umathcodenum must be undefined under {engine:?}");
    }
}

// The packed math code table the two extended engines share.

/// XeTeX reads the packed code back as TeX82's, and gives 0 where it does
/// not fit; LuaTeX hands the packed code over as it stands.
#[test]
fn the_mathcode_read_follows_the_engines_packing() {
    for (code, xe, lua) in [
        ("255", "255", "255"),
        ("256", "0", "256"),
        ("32767", "0", "32767"),
        ("2097152", "4096", "2097152"),
        ("31457345", "28993", "31457345"),
    ] {
        let source = format!("\\Umathcodenum`\\A={code} ");
        assert_eq!(the(Engine::XeTeX, &source, "\\mathcode`\\A"), xe, "xetex {code}");
        assert_eq!(the(Engine::LuaTeX, &source, "\\mathcode`\\A"), lua, "luatex {code}");
    }
}

/// A TeX82 `\mathcode` goes into the same table, so it is packed on the way
/// in.  XeTeX reads it back unchanged, LuaTeX reads the packed code.
#[test]
fn a_classic_mathcode_is_packed_on_the_way_in() {
    for (code, lua) in
        [("0", "0"), ("1", "1"), ("255", "255"), ("256", "16777216"), ("4096", "2097152"), ("28993", "31457345")]
    {
        let source = format!("\\mathcode`\\A={code} ");
        for engine in [Engine::Tex, Engine::PdfTeX, Engine::XeTeX] {
            assert_eq!(the(engine, &source, "\\mathcode`\\A"), code, "{engine:?} {code}");
        }
        assert_eq!(the(Engine::LuaTeX, &source, "\\mathcode`\\A"), lua, "luatex {code}");
        assert_eq!(the(Engine::XeTeX, &source, "\\Umathcodenum`\\A"), lua, "xetex packed {code}");
        assert_eq!(the(Engine::LuaTeX, &source, "\\Umathcodenum`\\A"), lua, "luatex packed {code}");
    }
    // XeTeX keeps `"8000` as its active marker, LuaTeX packs it as class 8.
    assert_eq!(the(Engine::XeTeX, "\\mathcode`\\A=32768 ", "\\Umathcodenum`\\A"), "2097151");
    assert_eq!(the(Engine::XeTeX, "\\mathcode`\\A=32768 ", "\\mathcode`\\A"), "32768");
    assert_eq!(the(Engine::LuaTeX, "\\mathcode`\\A=32768 ", "\\Umathcodenum`\\A"), "16777216");
    assert_eq!(the(Engine::LuaTeX, "\\mathcode`\\A=32768 ", "\\mathcode`\\A"), "16777216");
    // A code of full width packs family 15, class 7 and character 255.
    assert_eq!(the(Engine::XeTeX, "\\mathcode`\\A=32767 ", "\\Umathcodenum`\\A"), "266338559");
    assert_eq!(the(Engine::LuaTeX, "\\mathcode`\\A=32767 ", "\\mathcode`\\A"), "266338559");
}

// Soundness: a value satex could not pin down.

/// The parameter is then either the old value or the new one, so both are
/// kept and the read widens instead of picking one.
#[test]
fn an_unpinnable_value_widens_a_range_checked_parameter() {
    for (engine, name, deviate) in
        [(Engine::LuaTeX, "outputbox", "\\uniformdeviate 2 "), (Engine::PdfTeX, "prevgraf", "\\pdfuniformdeviate 2 ")]
    {
        let source = format!("\\{name}=7 \\{name}={deviate}\\relax \\edef\\zR{{\\the\\{name}}}");
        let analysis = analyze(engine, &source);
        let text = body(&analysis, "zR");
        assert!(text.contains(WIDENED), "\\{name} under {engine:?} must widen, not pick one value: {text:?}");
        assert_eq!(raised(engine, &source, "invalid-code"), 0, "\\{name} under {engine:?}");
    }
}

// The table itself.

/// Every limited name is a parameter or a code table satex really has, so a
/// rename cannot leave an entry naming nothing.
#[test]
fn every_limited_name_is_a_primitive_of_that_engine() {
    let mut found = 0;
    for engine in ENGINES {
        let mut interner = satex::tex::Interner::default();
        for (sym, meaning) in initial_meanings(&mut interner, engine) {
            let name = interner.name(sym);
            let Some(limit) = satex::builtins::limit(name, engine) else { continue };
            found += 1;
            match meaning {
                Meaning::Primitive(Primitive::IntegerParameter) => {
                    assert!(limit.start().is_some(), "\\{name} is a parameter, so it needs a start");
                }
                Meaning::Primitive(Primitive::Special(_) | Primitive::Command(_)) => {
                    assert!(limit.start().is_none(), "\\{name} is a code table, which has no one start");
                }
                other => panic!("\\{name} under {engine:?} is limited but is {other:?}"),
            }
        }
    }
    // `\prevgraf`, `\interactionmode` and `\spacefactor` under four
    // engines, `\outputbox` and the six read-only modes under LuaTeX,
    // `\Umathcodenum` and `\Umathcharnumdef` under two each.
    assert_eq!(found, 4 + 4 + 4 + 1 + 6 + 2 + 2);
}

/// Every owned name is a parameter of every engine: these are TeX82's, so a
/// rename or an engine gate that dropped one would show here.
#[test]
fn every_owned_name_is_a_parameter_of_every_engine() {
    use satex::builtins::Owned;
    for engine in ENGINES {
        let mut interner = satex::tex::Interner::default();
        let mut found = 0;
        for (sym, meaning) in initial_meanings(&mut interner, engine) {
            let name = interner.name(sym);
            let Some(owned) = satex::builtins::owned(name) else { continue };
            found += 1;
            match (owned, meaning) {
                (Owned::List { .. } | Owned::PageSoFar { .. }, Meaning::Primitive(Primitive::DimenParameter)) => {}
                (Owned::List { .. }, Meaning::Primitive(Primitive::IntegerParameter)) => {}
                (owned, other) => panic!("\\{name} under {engine:?} is {owned:?} but is {other:?}"),
            }
        }
        // `\spacefactor`, `\prevdepth` and the eight page dimensions.
        assert_eq!(found, 10, "{engine:?}");
    }
}

// `\spacefactor` and `\prevdepth`: the current list's, not the parameter
// table's (tex.web §§ 418, 1243).

/// The probes run at the outer vertical mode, so a list of the right kind
/// has to be opened around one that needs it.
fn in_hbox(source: &str) -> String {
    format!("\\setbox0=\\hbox{{{source}}}")
}

fn in_vbox(source: &str) -> String {
    format!("\\setbox0=\\vbox{{{source}}}")
}

#[test]
fn spacefactor_is_the_horizontal_lists_own() {
    for engine in ENGINES {
        // A box's horizontal list starts at 1000 (tex.web § 1083).
        let read = "\\edef\\zR{\\the\\spacefactor}";
        let analysis = analyze(engine, &in_hbox(read));
        assert_eq!(body(&analysis, "zR"), "1000", "{engine:?}");
        let analysis = analyze(engine, &in_hbox(&format!("\\spacefactor=500 {read}")));
        assert_eq!(body(&analysis, "zR"), "500", "{engine:?}");
        assert_eq!(raised(engine, &in_hbox("\\spacefactor=500 "), "invalid-code"), 0, "{engine:?}");
        assert_eq!(raised(engine, &in_hbox("\\spacefactor=500 "), "wrong-mode"), 0, "{engine:?}");
    }
}

/// "Improper \spacefactor": the error reads as zero, and `\the` prints it as
/// an integer, not as the number that was assigned.
#[test]
fn spacefactor_outside_horizontal_mode_reads_zero() {
    for engine in ENGINES {
        assert_eq!(the(engine, "", "\\spacefactor"), "0", "{engine:?} vertical");
        let analysis = analyze(engine, &in_vbox("\\edef\\zR{\\the\\spacefactor}"));
        assert_eq!(body(&analysis, "zR"), "0", "{engine:?} internal vertical");
    }
}

/// "You can't use `\spacefactor' in vertical mode": `report_illegal_case`
/// scans nothing, so the `=500` is left in the input and starts a paragraph.
#[test]
fn spacefactor_set_outside_horizontal_mode_does_nothing() {
    for engine in ENGINES {
        // The paragraph the leftover `=500` started is horizontal, so the
        // read is a fresh list's 1000, not the 500 that was refused.
        assert_eq!(the(engine, "\\spacefactor=500 ", "\\spacefactor"), "1000", "{engine:?}");
        assert_eq!(raised(engine, "\\spacefactor=500 ", "wrong-mode"), 1, "{engine:?}");
    }
}

/// tex.web § 1243: "Bad space factor", after which 1000 is used.
#[test]
fn spacefactor_takes_one_to_32767() {
    for engine in ENGINES {
        for good in ["1", "500", "32767"] {
            let source = in_hbox(&format!("\\spacefactor={good} \\edef\\zR{{\\the\\spacefactor}}"));
            assert_eq!(body(&analyze(engine, &source), "zR"), good, "{engine:?} {good}");
            assert_eq!(raised(engine, &source, "invalid-code"), 0, "{engine:?} {good}");
        }
        for bad in ["0", "-1", "32768", "40000"] {
            let source = in_hbox(&format!("\\spacefactor={bad} \\edef\\zR{{\\the\\spacefactor}}"));
            assert_eq!(body(&analyze(engine, &source), "zR"), "1000", "{engine:?} {bad}");
            assert_eq!(raised(engine, &source, "invalid-code"), 1, "{engine:?} {bad}");
        }
    }
}

#[test]
fn prevdepth_is_the_vertical_lists_own() {
    for engine in ENGINES {
        // tex.web § 212: a vertical list with nothing on it has
        // `ignore_depth`, which is -1000pt.
        assert_eq!(the(engine, "", "\\prevdepth"), "-1000.0pt", "{engine:?} main vertical");
        assert_eq!(the(engine, "\\prevdepth=3pt ", "\\prevdepth"), "3.0pt", "{engine:?}");
        // A box's list is fresh, and the `}` gives the enclosing one back.
        let source = "\\prevdepth=3pt \\setbox0=\\vbox{\\edef\\zI{\\the\\prevdepth}\\prevdepth=5pt }";
        let analysis = analyze(engine, &format!("{source}\\edef\\zR{{\\the\\prevdepth}}"));
        assert_eq!(body(&analysis, "zI"), "-1000.0pt", "{engine:?} inside a \\vbox");
        assert_eq!(body(&analysis, "zR"), "3.0pt", "{engine:?} after the \\vbox");
    }
}

/// "Improper \prevdepth" reads as zero, and as an integer under `\the`, so
/// it prints `0` and not `0.0pt`.
#[test]
fn prevdepth_outside_vertical_mode_reads_integer_zero() {
    for engine in ENGINES {
        let analysis = analyze(engine, &in_hbox("\\edef\\zR{\\the\\prevdepth}"));
        assert_eq!(body(&analysis, "zR"), "0", "{engine:?}");
    }
}

#[test]
fn prevdepth_set_outside_vertical_mode_does_nothing() {
    for engine in ENGINES {
        let source = in_hbox("\\prevdepth=7pt \\edef\\zR{\\the\\prevdepth}");
        assert_eq!(body(&analyze(engine, &source), "zR"), "0", "{engine:?}");
        assert_eq!(raised(engine, &source, "wrong-mode"), 1, "{engine:?}");
        // In vertical mode the same write is no error at all.
        assert_eq!(raised(engine, "\\prevdepth=7pt ", "wrong-mode"), 0, "{engine:?}");
    }
}

// The page-so-far dimensions, which the page builder owns (tex.web § 986).

/// `\pagegoal` reads `\maxdimen` and the other seven read zero while the
/// page is empty, whatever was assigned to them.
const PAGE_SO_FAR: [(&str, &str); 8] = [
    ("pagegoal", "16383.99998pt"),
    ("pagetotal", "0.0pt"),
    ("pagedepth", "0.0pt"),
    ("pagestretch", "0.0pt"),
    ("pageshrink", "0.0pt"),
    ("pagefilstretch", "0.0pt"),
    ("pagefillstretch", "0.0pt"),
    ("pagefilllstretch", "0.0pt"),
];

#[test]
fn an_empty_page_reads_its_own_dimensions_whatever_was_assigned() {
    for engine in ENGINES {
        for (name, empty) in PAGE_SO_FAR {
            assert_eq!(the(engine, "", &format!("\\{name}")), empty, "{engine:?} \\{name}");
            let assigned = format!("\\{name}=100pt ");
            assert_eq!(the(engine, &assigned, &format!("\\{name}")), empty, "{engine:?} \\{name} assigned");
            // The write itself is an ordinary assignment, not an error.
            assert_eq!(raised(engine, &assigned, "wrong-mode"), 0, "{engine:?} \\{name}");
        }
    }
}

/// A box inside a box leaves the page empty: only what reaches the main
/// vertical list freezes the page dimensions (tex.web §§ 987, 1000).
#[test]
fn material_inside_a_box_leaves_the_page_empty() {
    for engine in ENGINES {
        for boxed in ["\\setbox0=\\hbox{\\kern3pt}", "\\setbox0=\\vbox{\\hrule}"] {
            assert_eq!(the(engine, boxed, "\\pagegoal"), "16383.99998pt", "{engine:?} {boxed}");
            assert_eq!(the(engine, boxed, "\\pagetotal"), "0.0pt", "{engine:?} {boxed}");
        }
        // Glue, a kern and a penalty on the main vertical list are neither a
        // box nor a rule, so the page builder has still frozen nothing.
        for quiet in ["\\kern3pt ", "\\penalty100 ", "\\vskip3pt "] {
            assert_eq!(the(engine, quiet, "\\pagegoal"), "16383.99998pt", "{engine:?} {quiet}");
            assert_eq!(the(engine, quiet, "\\pagetotal"), "0.0pt", "{engine:?} {quiet}");
        }
    }
}

/// Once a box or a rule reaches the main vertical list the page builder owns
/// these, and satex does not run it, so the answer widens rather than
/// staying at the empty page's.
#[test]
fn a_box_on_the_page_makes_the_page_dimensions_unknown() {
    for engine in ENGINES {
        for contributed in ["\\setbox0=\\hbox{}\\box0 ", "\\hrule "] {
            for (name, _) in PAGE_SO_FAR {
                assert!(
                    widened(engine, contributed, &format!("\\{name}")),
                    "{engine:?} \\{name} after {contributed} must widen, not answer the empty page's",
                );
            }
        }
    }
}

// Reading and printing.

/// tex.web § 49: the escape character is printed, so one the engine cannot
/// show comes out as `^^X`.  Measured with `\string\z` under `pdftex`.
#[test]
fn an_unprintable_escape_character_prints_as_a_hat_pair() {
    for (code, shown) in [
        ("0", "^^@z"),
        ("1", "^^Az"),
        ("3", "^^Cz"),
        ("15", "^^Oz"),
        ("16", "^^Pz"),
        ("31", "^^_z"),
        ("32", " z"),
        ("65", "Az"),
        ("126", "~z"),
        ("127", "^^?z"),
        ("255", "^^ffz"),
        // Outside 0..255 no escape character is printed at all.
        ("-1", "z"),
        ("256", "z"),
    ] {
        for engine in ENGINES {
            let source = format!("\\let\\z\\relax \\escapechar={code} ");
            let analysis = analyze(engine, &format!("{source}\\edef\\zR{{\\string\\z}}"));
            assert_eq!(body(&analysis, "zR"), shown, "{engine:?} \\escapechar={code}");
        }
    }
}

#[test]
fn meaning_prints_under_the_same_escape_character() {
    for engine in ENGINES {
        assert_eq!(expand(engine, "\\let\\z\\relax ", "\\meaning\\z").as_str(), "\\relax", "{engine:?}");
        let source = "\\let\\z\\relax \\escapechar=3 ";
        assert_eq!(expand(engine, source, "\\meaning\\z").as_str(), "^^Crelax", "{engine:?}");
    }
}

/// tex.web § 1226: the output routine is kept inside a brace pair of its
/// own, which no other token parameter has.
#[test]
fn the_output_routine_reads_back_braced() {
    for engine in ENGINES {
        assert_eq!(the(engine, "\\output={ab\\relax}", "\\output").as_str(), "{ab\\relax }", "{engine:?}");
        assert_eq!(the(engine, "\\toks0={ab\\relax}", "\\toks0").as_str(), "ab\\relax ", "{engine:?}");
        assert_eq!(the(engine, "\\everypar={ab}", "\\everypar").as_str(), "ab", "{engine:?}");
        assert_eq!(the(engine, "\\errhelp={ab}", "\\errhelp").as_str(), "ab", "{engine:?}");
    }
}

// Parameter texts (tex.web §§ 476, 479).

/// The parameters are numbered as they come, whatever digit stands there:
/// "Parameters must be numbered consecutively" puts the token that is not
/// the next number back into the parameter text.
#[test]
fn parameters_are_numbered_as_they_come() {
    for (pattern, meaning) in [
        ("#1", "macro:#1->x"),
        ("#1#2", "macro:#1#2->x"),
        ("#1,#2.", "macro:#1,#2.->x"),
        ("#2", "macro:#12->x"),
        ("#1#3", "macro:#1#23->x"),
        ("#a", "macro:#1a->x"),
        ("##1", "macro:#1#21->x"),
    ] {
        for engine in ENGINES {
            let source = format!("\\def\\zq{pattern}{{x}}");
            assert_eq!(expand(engine, &source, "\\meaning\\zq"), meaning, "{engine:?} \\def\\zq{pattern}");
        }
    }
}

/// "You already have nine parameters" drops the `#` and the token it read.
#[test]
fn a_tenth_parameter_and_its_digit_are_dropped() {
    let nine = "#1#2#3#4#5#6#7#8#9";
    for tail in ["#1", "#a", "#1#2"] {
        for engine in ENGINES {
            let source = format!("\\def\\zq{nine}{tail}{{x}}");
            assert_eq!(expand(engine, &source, "\\meaning\\zq"), format!("macro:{nine}->x"), "{engine:?} {tail}");
        }
    }
    // A delimiter after the ninth is kept, so only the `#` case drops one.
    for engine in ENGINES {
        let source = format!("\\def\\zq{nine},{{x}}");
        assert_eq!(expand(engine, &source, "\\meaning\\zq"), format!("macro:{nine},->x"), "{engine:?}");
    }
}

// `\Umathchardef` is a meaning of its own (XeTeX reference).

#[test]
fn a_umathchardef_is_not_a_mathchardef() {
    for (engine, mathchar, umathchar) in [
        (Engine::XeTeX, "\\mathchar\"3", "\\Umathchar\"0\"0\"3"),
        (Engine::LuaTeX, "\\mathchar\"0003", "\\Umathchar\"0\"00\"000003"),
    ] {
        assert_eq!(expand(engine, "\\mathchardef\\za=3 ", "\\meaning\\za"), mathchar, "{engine:?}");
        assert_eq!(expand(engine, "\\Umathcharnumdef\\zb=3 ", "\\meaning\\zb"), umathchar, "{engine:?}");
        assert_eq!(expand(engine, "\\Umathchardef\\zc=0 0 3 ", "\\meaning\\zc"), umathchar, "{engine:?}");
        // `\ifx` tells the two apart, and the two `\U` forms together.
        let both = "\\mathchardef\\za=3 \\Umathcharnumdef\\zb=3 \\Umathchardef\\zc=0 0 3 ";
        let differ = format!("{both}\\ifx\\za\\zb\\def\\zR{{SAME}}\\else\\def\\zR{{DIFF}}\\fi");
        assert_eq!(body(&analyze(engine, &differ), "zR"), "DIFF", "{engine:?}");
        let same = format!("{both}\\ifx\\zb\\zc\\def\\zR{{SAME}}\\else\\def\\zR{{DIFF}}\\fi");
        assert_eq!(body(&analyze(engine, &same), "zR"), "SAME", "{engine:?}");
    }
}

/// `\Umathcharnumdef` range-checks its packed code the way `\Umathcodenum`
/// does, and recovers the same way.
#[test]
fn umathcharnumdef_takes_the_same_codes_as_umathcodenum() {
    for (code, xe, lua) in [
        ("31457345", "\\Umathchar\"7\"1\"41", "\\Umathchar\"7\"01\"000041"),
        ("2097151", "\\Umathchar\"0\"0\"1FFFFF", "\\Umathchar\"0\"00\"000000"),
        ("-1", "\\Umathchar\"0\"0\"1FFFFF", "\\Umathchar\"0\"00\"000000"),
        ("-2", "\\Umathchar\"0\"0\"0", "\\Umathchar\"0\"00\"000000"),
        ("1114112", "\\Umathchar\"0\"0\"0", "\\Umathchar\"0\"00\"000000"),
    ] {
        let source = format!("\\Umathcharnumdef\\zq={code} ");
        assert_eq!(expand(Engine::XeTeX, &source, "\\meaning\\zq"), xe, "xetex {code}");
        assert_eq!(expand(Engine::LuaTeX, &source, "\\meaning\\zq"), lua, "luatex {code}");
    }
    // A code the engine takes raises nothing.
    assert_eq!(raised(Engine::XeTeX, "\\Umathcharnumdef\\zq=31457345 ", "invalid-code"), 0);
    assert_eq!(raised(Engine::XeTeX, "\\Umathcharnumdef\\zq=1114112 ", "invalid-code"), 1);
}

/// `\meaning` splits the stored code into a class, a family and a character.
/// XeTeX takes the three as bits.  LuaTeX divides by the character's width
/// truncating towards zero, so a negative code shows a class one above its
/// bits, and a carry out of the class shows in the family as well.
#[test]
fn a_negative_math_char_shows_the_fields_its_engine_splits_it_into() {
    for (code, xe, lua) in [
        ("-2147483647", "\\Umathchar\"0\"80\"1", "\\Umathchar\"2\"80\"000001"),
        ("-16777215", "\\Umathchar\"0\"FF\"1", "\\Umathchar\"2\"FF\"000001"),
        ("-2064865171", "\\Umathchar\"7\"84\"CA86D", "\\Umathchar\"1\"85\"0CA86D"),
        ("-1048576", "\\Umathchar\"7\"FF\"100000", "\\Umathchar\"0\"00\"100000"),
        ("-6291456", "\\Umathchar\"5\"FF\"0", "\\Umathchar\"5\"FF\"000000"),
        ("-2097152", "\\Umathchar\"7\"FF\"0", "\\Umathchar\"7\"FF\"000000"),
        ("31457345", "\\Umathchar\"7\"1\"41", "\\Umathchar\"7\"01\"000041"),
        ("1114111", "\\Umathchar\"0\"0\"10FFFF", "\\Umathchar\"0\"00\"10FFFF"),
    ] {
        let source = format!("\\Umathcharnumdef\\zq={code} ");
        assert_eq!(expand(Engine::XeTeX, &source, "\\meaning\\zq"), xe, "xetex {code}");
        assert_eq!(expand(Engine::LuaTeX, &source, "\\meaning\\zq"), lua, "luatex {code}");
    }
}

// `\readline` (eTeX manual § 3.2).

/// `\readline` keeps the `\endlinechar` as a character of category 12, and
/// an unprintable one shows as `^^M`.  `\read` turns it into a space.
#[test]
fn readline_keeps_the_end_line_character() {
    for engine in ENGINES {
        let read = "\\openin0=zprobe \\readline0 to\\zq \\edef\\zR{[\\zq]}";
        assert_eq!(body(&beside_a_file(engine, "readline", read), "zR"), "[hello^^M]", "{engine:?}");
        let read = "\\openin0=zprobe \\read0 to\\zq \\edef\\zR{[\\zq]}";
        assert_eq!(body(&beside_a_file(engine, "read", read), "zR"), "[hello ]", "{engine:?}");
    }
}

// `\outer` (tex.web §§ 336-339).

/// An `\outer` name met while a definition is being scanned is forbidden
/// there: the scan ends where it stands and the run says so.
#[test]
fn an_outer_name_inside_a_definition_is_forbidden() {
    for engine in ENGINES {
        let source = "\\outer\\def\\zo{O}\\def\\zw{\\zo}";
        assert_eq!(raised(engine, source, "forbidden-outer"), 1, "{engine:?}");
        // Outside a scan it is no error at all, and `\meaning` reads its
        // argument with the scanner normal (tex.web § 471).
        assert_eq!(raised(engine, "\\outer\\def\\zo{O}\\edef\\zR{\\meaning\\zo}", "forbidden-outer"), 0, "{engine:?}");
        assert_eq!(expand(engine, "\\outer\\def\\zo{O}", "\\meaning\\zo"), "\\outer macro:->O", "{engine:?}");
    }
}

/// The scan ends with a space in the `\outer` name's place, and the name is
/// then read again as a command (tex.web § 336).  The engine's `\zm` is
/// `\outer \def` and a space, and `\zq` stays what it was.
#[test]
fn an_outer_name_ending_a_scan_is_read_again() {
    for engine in ENGINES {
        let source =
            "\\outer\\gdef\\zq#1{[#1]}\\def\\zm{\\outer\\def\\zq A}\\edef\\zR{\\meaning\\zm}\\edef\\zS{\\meaning\\zq}";
        let run = analyze(engine, source);
        assert_eq!(body(&run, "zR"), "macro:->\\outer \\def  ", "{engine:?}");
        assert_eq!(body(&run, "zS"), "\\outer macro:#1->[#1]", "{engine:?}");
    }
    // A `\chardef` in a group the recovery ends early is taken back with it.
    let source = "\\setbox254=\\vbox{\\begingroup\\globaldefs=1 \\outer\\def\\zq#1{[#1]}\\endgroup}\\setbox254=\\vbox{\\def\\zm{\\outer\\def\\zq A}\\edef\\zR{\\meaning\\zm}\\edef\\zS{\\meaning\\zq}}";
    let run = analyze(Engine::PdfTeX, source);
    assert_eq!((body(&run, "zR"), body(&run, "zS")), ("undefined".into(), "\\outer macro:#1->[#1]".into()));
}

/// Past `limits.degrade_at_percent` of `limits.steps` the run lowers its
/// precision limits once and says so; at 100 it never does.
#[test]
fn a_run_degrades_its_precision_once_past_the_soft_limit() {
    let source = "\\catcode`\\{=1 \\catcode`\\}=2 \\count255=0 \
        \\def\\x{\\advance\\count255 1 \\ifnum\\count255<300000 \\expandafter\\x\\fi}\\x";
    let run = |percent: u8| {
        let mut cfg = Config { ..crate::support::bare() };
        cfg.limits.seconds = 20;
        cfg.limits.degrade_at_percent = percent;
        let analysis = Machine::analyze(source, None, &cfg);
        analysis.facts.diagnostics.iter().filter(|d| d.code == "precision-degraded").count()
    };
    assert_eq!(run(1), 1);
    assert_eq!(run(100), 0);
}
