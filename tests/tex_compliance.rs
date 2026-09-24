//! TeX, e-TeX and pdfTeX compliance of the interpreter, pinned against
//! `pdftex -ini -etex`.  Each test runs a small primitive-only probe and reads
//! back the replacement text of `\R` (set with `\edef`), which is what the
//! real engine prints with `\meaning\R`.  Tests that record a known
//! divergence are `#[ignore]`d with a one-line reason, so the suite stays
//! green while the divergence is documented.

use satex::config::Config;
use satex::machine::{Analysis, Machine};

const PRELUDE: &str = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\catcode`\\^=7 \\catcode`\\ =10 \\catcode`\\%=14\n";

fn analyze(source: &str) -> Analysis {
    let cfg = Config {
        load_packages: false,
        load_classes: false,
        load_inputs: false,
        load_format: false,
        use_kpsewhich: false,
        ..Config::default()
    };
    Machine::analyze(&format!("{PRELUDE}{source}"), None, &cfg)
}

/// Every replacement text `\name` was given, in order, with whether the
/// definition was certain.
fn bodies(analysis: &Analysis, name: &str) -> Vec<(String, bool)> {
    analysis
        .facts
        .defs
        .iter()
        .filter(|d| analysis.interner.name(d.name) == name)
        .map(|d| {
            let text = d
                .mac
                .as_ref()
                .map(|m| satex::tex::detokenize(&m.replacement_text, &analysis.interner))
                .unwrap_or_else(|| format!("<{}>", d.tag));
            (text, d.certain)
        })
        .collect()
}

/// The texts of `\R`, whitespace removed so that `\detokenize` spacing
/// conventions do not matter.
fn r(source: &str) -> Vec<String> {
    bodies(&analyze(source), "R")
        .into_iter()
        .map(|(t, _)| t.split_whitespace().collect())
        .collect()
}

/// The last certain text of `\R`, whitespace removed.
fn last(source: &str) -> String {
    r(source).pop().unwrap_or_else(|| "<undefined>".into())
}

/// Development aid: `PROBE=file cargo test --test tex_compliance probe -- --ignored --nocapture`
/// prints the texts of `\R` for each `%%% name` section of the file.
#[test]
#[ignore = "development probe, driven by $PROBE"]
fn probe() {
    let Ok(path) = std::env::var("PROBE") else { return };
    let text = std::fs::read_to_string(path).unwrap();
    for section in text.split("%%% ").skip(1) {
        let (name, body) = section.split_once('\n').unwrap();
        let a = analyze(body);
        let b = bodies(&a, "R");
        let b: Vec<String> = b
            .iter()
            .map(|(t, c)| format!("{}{}", if *c { "" } else { "?" }, t.replace(' ', "_")))
            .collect();
        println!("SATEX {name}: {} steps={} exh={}", b.join(" | "), a.steps, a.exhausted);
    }
}

/// Whether the last definition of `\R` is marked certain.
fn last_certain(source: &str) -> bool {
    bodies(&analyze(source), "R").last().is_some_and(|(_, c)| *c)
}

/// Asserts that `\R` ends with the text pdfTeX gives it (whitespace ignored).
fn check(source: &str, expected: &str) {
    let got = last(source);
    let expected: String = expected.split_whitespace().collect();
    assert_eq!(got, expected, "probe: {source}");
}

/// Asserts that every outcome pdfTeX can reach is among the texts `\R` may have.
fn includes_all(source: &str, outcomes: &[&str]) {
    let got = r(source);
    for o in outcomes {
        assert!(got.iter().any(|g| g == o), "outcome {o:?} missing from {got:?} for probe: {source}");
    }
}

// ---------------------------------------------------------------------------
// Agreement with pdfTeX (these pass).

#[test]
fn numexpr_rounds_half_away_from_zero() {
    check(r"\edef\R{\number\numexpr 7/2\relax,\number\numexpr -7/2\relax,\number\numexpr 3*(4+5)/2\relax}", "4,-4,14");
}

#[test]
fn dimexpr_and_glueexpr_print_like_tex() {
    check(r"\edef\R{\the\dimexpr 1pt*3/4\relax,\the\glueexpr 1pt plus 2fil minus 1fill\relax}", "0.75pt,1.0pt plus 2.0fil minus 1.0fill");
}

#[test]
fn romannumeral_of_zero_and_negative_is_empty() {
    check(r"\edef\R{\romannumeral 1984 ,\romannumeral 0 ,\romannumeral -5 x}", "mcmlxxxiv,,x");
}

#[test]
fn number_reads_signs_and_radixes() {
    check(r#"\edef\R{\number -+-00012 ,\number `\a,\number"FF ,\number'17 }"#, "12,97,255,15");
}

#[test]
fn string_honours_escapechar_out_of_range() {
    check(r"\escapechar=-1 \edef\R{\string\foo}", "foo");
    check(r"\escapechar=300 \edef\R{\string\foo}", "foo");
    check(r"\escapechar=`\  \edef\R{\string\x}", "x");
}

#[test]
fn csname_of_an_undefined_name_is_relax() {
    check(r"\edef\R{\expandafter\meaning\csname qqq\endcsname}", r"\relax");
    check(r"\edef\R{\expandafter\ifx\csname qq\endcsname\relax T\else F\fi\ifdefined\qq T\else F\fi}", "TT");
}

#[test]
fn ifcase_nests_or_and_else() {
    check(r"\edef\R{\ifcase 2 a\or b\or c\ifcase 1 x\or y\else z\fi\or d\else e\fi}", "cy");
}

#[test]
fn ifx_compares_chardefs_parameters_and_long() {
    check(r"\chardef\c=65 \chardef\d=65 \edef\R{\ifx\c\d T\else F\fi}", "T");
    check(r"\def\a#1{x}\def\b#1{x}\def\c#1.{x}\edef\R{\ifx\a\b T\else F\fi\ifx\a\c T\else F\fi}", "TF");
    check(r"\long\def\a{x}\def\b{x}\edef\R{\ifx\a\b T\else F\fi}", "F");
    check(r"\edef\R{\ifx\undefa\relax T\else F\fi\ifx\undefa\undefb T\else F\fi}", "FT");
}

#[test]
fn let_takes_one_optional_space_after_equals() {
    check(r"\let\x= a\edef\R{\meaning\x}", "the letter a");
    check(r"\def\s{ }\expandafter\let\expandafter\x\expandafter=\s\s b\edef\R{\meaning\x}", "macro:-> ");
}

#[test]
fn lowercase_and_uppercase_map_characters_only() {
    check(r"\lccode`\A=`\b \lowercase{\def\R{A\A}}", r"b\A");
    check(r"\uccode`a=`Z \uppercase{\def\R{ab}}", "ZB");
    check(r"\lccode`\ =`x \lowercase{\def\R{a b}}", "axb");
}

#[test]
fn large_register_numbers_are_valid() {
    check(r"\count300=5 \count32767=7 \edef\R{\the\count300,\the\count32767}", "5,7");
}

#[test]
fn pdftex_string_functions() {
    check(r"\edef\R{\pdfstrcmp{a}{b},\pdfstrcmp{b}{a},\pdfstrcmp{ab}{ab}}", "-1,1,0");
    check(r"\edef\R{\pdfmdfivesum{abc}}", "900150983CD24FB0D6963F7D28E17F72");
    check(r"\edef\R{\pdfescapestring{a(b)\string\\c}}", r"a\(b\)\\\\c");
}

#[test]
fn expanded_protected_and_unexpanded() {
    check(r"\def\a{A}\edef\R{\expanded{\a\noexpand\a}}", "AA");
    check(r"\protected\def\p{P}\edef\R{\p}", r"\p");
    check(r"\def\a{X}\edef\R{\unexpanded{\a}\a}", r"\aX");
    check(r"\toks0{#}\edef\R{\the\toks0\unexpanded{#}}", "####");
}

#[test]
fn dimension_units_convert_like_tex() {
    check(r"\dimen0=1dd\dimen1=1cc\dimen2=1sp\dimen3=1pc\dimen4=1mm\edef\R{\the\dimen0,\the\dimen1,\the\dimen2,\the\dimen3,\the\dimen4}", "1.07pt,12.8401pt,0.00002pt,12.0pt,2.84526pt");
    check(r"\dimen0=16384pt \edef\R{\the\dimen0}", "16383.99998pt");
    check(r"\count0=-7 \divide\count0 2 \edef\R{\the\count0}", "-3");
}

#[test]
fn initex_code_tables() {
    check(r"\edef\R{\the\lccode`A,\the\uccode`a,\the\sfcode`A,\the\mathcode`a,\the\catcode`a}", "97,65,999,29025,11");
}

#[test]
fn afterassignment_and_aftergroup_order() {
    check(r"\def\a{\xdef\R{\R A}}\def\b{\xdef\R{\R B}}\def\R{}{\aftergroup\a\aftergroup\b}", "AB");
    check(r"\def\a{\xdef\R{\R A}}\def\R{}\afterassignment\a\afterassignment\a\count1=5", "A");
    check(r"\def\R{}\def\a{\xdef\R{\R A[\meaning\q]}}\afterassignment\a\def\q{Q}", "A[macro:->Q]");
}

#[test]
fn read_and_readline_from_a_file() {
    let dir = std::env::temp_dir().join(format!("satex-compliance-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("data.txt");
    std::fs::write(&file, "abc\nsecond line\n").unwrap();
    let f = file.display();
    check(&format!(r"\openin3={f} \read3 to\x \edef\R{{\meaning\x}}"), "macro:->abc");
    check(&format!(r"\openin3={f} \read3 to\x\read3 to\x \ifeof3 \def\R{{T}}\else\def\R{{F}}\fi"), "F");
    check(&format!(r"\openin3={f} \read3 to\x\read3 to\x\read3 to\x \ifeof3 \def\R{{T}}\else\def\R{{F}}\fi"), "T");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn undecidable_conditionals_keep_both_arms() {
    includes_all(r"\ifnum\pdfelapsedtime>0 \def\R{T}\else\def\R{F}\fi", &["T", "F"]);
    includes_all(r"\ifnum\numexpr\pdfuniformdeviate3*2\relax=4 \def\R{T}\else\def\R{F}\fi", &["T", "F"]);
    includes_all(r"\count1=0 \ifnum\pdfuniformdeviate2=0 \count1=5 \fi\ifnum\count1=5 \def\R{T}\else\def\R{F}\fi", &["T", "F"]);
    includes_all(r"\count1=\pdfuniformdeviate 10\relax \ifnum\count1>4 \def\R{T}\else\def\R{F}\fi", &["T", "F"]);
}

// ---------------------------------------------------------------------------
// Former divergences (see the critique report); each asserts the pdfTeX
// behaviour, or, where one path's outcome cannot be told apart from the
// other's, that satex does not claim a certain one.

#[test]
fn unknown_number_does_not_swallow_the_following_conditional() {
    includes_all(r"\count1=\pdfuniformdeviate 10 \ifnum\count1>4 \def\R{T}\else\def\R{F}\fi", &["T", "F"]);
}

#[test]
fn mdfivesum_of_unknown_text_is_not_definite() {
    assert!(!last_certain(r"\edef\R{\pdfmdfivesum{\pdfuniformdeviate 10}}"));
}

#[test]
fn chardef_from_unknown_number_is_unknown() {
    includes_all(r"\chardef\c=\pdfuniformdeviate3 \ifnum\c=0 \def\R{T}\else\def\R{F}\fi", &["T", "F"]);
}

#[test]
fn edef_with_unknown_value_is_not_certain() {
    assert!(!last_certain(r"\edef\R{\number\pdfuniformdeviate 10}"));
    assert!(!last_certain(r"\edef\R{\pdfmatch{b+}{abbbc}}"));
}

#[test]
fn currentiftype_and_branch() {
    check(r"\ifnum1=1 \xdef\R{\the\currentiftype,\the\currentifbranch}\fi", "3,1");
    check(r"\unless\ifnum1=2 \xdef\R{\the\currentiftype,\the\currentifbranch}\fi", "-3,1");
    check(r"\ifcase1 \or\xdef\R{\the\currentiftype,\the\currentifbranch}\fi", "17,1");
}

#[test]
fn lastnodetype_of_empty_list() {
    check(r"\edef\R{\the\lastnodetype}", "-1");
}

#[test]
fn if_and_ifcat_expand_active_characters() {
    check(r"\catcode`\~=13 \def~{a}\edef\R{\ifcat~a T\else F\fi}", "T");
    check(r"\catcode`\~=13 \def~{a}\edef\R{\if~a T\else F\fi}", "T");
    check(r"\catcode`\~=13 \let~\relax \edef\R{\ifcat~\relax T\else F\fi}", "T");
}

#[test]
fn scantokens_everyeof_noexpand_idiom() {
    check(r"\everyeof{\noexpand}\edef\R{\scantokens{ab}}", "ab");
}

/// The paths stay apart while their category codes differ; each defines
/// `\R` at the same site, so the definition is not certain.
#[test]
fn catcode_change_on_one_path_is_joined() {
    assert!(!last_certain(r"\catcode`\~=13 \def~{T}\ifnum\pdfuniformdeviate2=0 \catcode`\Q=13 \let Q~\fi\edef\R{Q}"));
}

/// `\escapechar` joins to unknown, and `\string` then gives unknown text.
#[test]
fn escapechar_change_on_one_path_is_joined() {
    assert!(!last_certain(r"\ifnum\pdfuniformdeviate2=0 \escapechar=-1 \fi\edef\R{\string\x}"));
}

#[test]
fn uccode_change_on_one_path_is_joined() {
    assert!(!last_certain(r"\ifnum\pdfuniformdeviate2=0 \uccode`a=`B \fi\uppercase{\def\R{a}}"));
}

#[test]
fn aftergroup_on_one_path_is_not_certain() {
    assert!(!last_certain(r"\def\R{0}\def\a{\def\R{1}}{\ifnum\pdfuniformdeviate2=0 \aftergroup\a\fi}\edef\R{\R}"));
}

#[test]
fn afterassignment_on_one_path_is_not_certain() {
    assert!(!last_certain(r"\def\a{\def\R{A}}\def\R{0}\ifnum\pdfuniformdeviate2=0 \afterassignment\a\fi\count1=1"));
}

#[test]
fn endinput_on_one_path_is_not_certain() {
    assert!(!last_certain("\\def\\R{0}\\ifnum\\pdfuniformdeviate2=0 \\endinput\\fi\n\\def\\R{1}\n"));
}

/// The name is unknown, so neither `\x` nor `\y` is known to be undefined.
#[test]
fn csname_from_joined_macro_keeps_both_names() {
    assert!(!last_certain(r"\ifnum\pdfuniformdeviate2=0 \def\n{x}\else\def\n{y}\fi\expandafter\def\csname\n\endcsname{Q}\edef\R{\ifdefined\x X\fi\ifdefined\y Y\fi}"));
}

#[test]
fn gdef_on_one_path_in_a_group() {
    let src = r"\def\R{0}{\ifnum\pdfuniformdeviate2=0 \gdef\R{1}\fi}\edef\R{\R}";
    let got = last(src);
    assert!(!last_certain(src) || got == "0" || got == "1", "certain body {got:?} is neither outcome");
}

#[test]
fn read_from_terminal_is_unknown() {
    assert!(!last_certain(r"\read16 to\x \edef\R{\meaning\x}"));
    assert!(!last_certain(r"\read-1 to\x \edef\R{\meaning\x}"));
}

#[test]
fn control_space_is_a_primitive() {
    check(r"\edef\R{\meaning\ }", r"\ ");
    check(r"\def\a{\edef\R{\meaning\x}}\futurelet\x\a\ b", r"\ ");
}

#[test]
fn meaning_of_par_like_primitives() {
    check(r"\edef\R{\meaning\par,\meaning\noindent,\meaning\ignorespaces}", r"\par,\noindent,\ignorespaces");
}

#[test]
fn meaning_of_marks() {
    check(r"\edef\R{\meaning\topmark,\meaning\firstmark,\meaning\botmark}", r"\topmark:,\firstmark:,\botmark:");
}

#[test]
fn numexpr_overflow_is_zero() {
    check(r"\edef\R{\number\numexpr 2147483647+1\relax}", "0");
}

#[test]
fn missing_number_is_zero() {
    check(r"\edef\R{[\number\numexpr -(3)\relax]}", r"[0(3)\relax]");
    check(r#"\edef\R{\number"af }"#, "0af");
}

#[test]
fn initex_delcode_of_period() {
    check(r"\edef\R{\the\delcode`.,\the\delcode`a}", "0,-1");
}

#[test]
fn self_nesting_number_scan_stops_instead_of_crashing() {
    let a = analyze(r"\def\a{\number\a}\a");
    assert!(a.exhausted);
}

#[test]
fn scantokens_in_everyeof_runs_out_of_input_levels() {
    let a = analyze(r"\everyeof{\scantokens{}}\scantokens{}");
    assert!(a.exhausted);
}

#[test]
fn modes_follow_the_material() {
    let m = r"\catcode`\$=3 \def\m{\ifvmode V\fi\ifhmode H\fi\ifinner I\fi\ifmmode M\fi}";
    check(&format!(r"{m}\edef\R{{\m}}"), "V");
    check(&format!(r"{m}a\xdef\R{{\m}}"), "H");
    check(&format!(r"{m}a\par\xdef\R{{\m}}"), "V");
    check(&format!(r"{m}\hbox{{\xdef\R{{\m}}}}"), "HI");
    check(&format!(r"{m}\vbox{{\xdef\R{{\m}}}}"), "VI");
    check(&format!(r"{m}\vbox{{x\xdef\R{{\m}}}}"), "H");
    check(&format!(r"{m}\hbox{{x}}\xdef\R{{\m}}"), "V");
    check(&format!(r"{m}$\xdef\R{{\m}}$"), "IM");
    check(&format!(r"{m}x$$\xdef\R{{\m}}$$"), "M");
    check(&format!(r"{m}x$$ $$\xdef\R{{\m}}"), "H");
    check(&format!(r"{m}x\vskip 1pt\xdef\R{{\m}}"), "V");
    check(&format!(r"{m}\kern 1pt\penalty 100 \xdef\R{{\m}}"), "V");
    check(&format!(r"{m}\everypar{{\xdef\R{{\m}}}}x"), "H");
    check(&format!(r"{m}\noindent\xdef\R{{\m}}"), "H");
    check(r"\hbox{\xdef\R{\the\currentgrouptype}}", "3");
    check(r"\setbox0\hbox{\xdef\R{\the\currentgrouptype}}", "2");
}

#[test]
fn a_mode_left_open_by_one_path_is_undecided() {
    let m = r"\def\m{\ifvmode V\fi\ifhmode H\fi}";
    includes_all(&format!(r"{m}\ifnum\pdfuniformdeviate2=0 x\fi\ifvmode\xdef\R{{V}}\else\xdef\R{{H}}\fi"), &["V", "H"]);
    assert!(!last_certain(&format!(r"{m}\ifnum\pdfuniformdeviate2=0 x\fi\xdef\R{{\m}}")));
}

#[test]
fn box_registers_follow_setbox_box_copy_and_groups() {
    check(r"\edef\R{\ifvoid0 V\fi\ifhbox0 H\fi\ifvbox0 B\fi\the\wd0}", "V0.0pt");
    check(r"\setbox0\vbox{}\edef\R{\ifvoid0 V\fi\ifhbox0 H\fi\ifvbox0 B\fi}", "B");
    check(r"\setbox0\hbox{\xdef\R{\ifvoid0 V\fi}}", "V");
    check(r"{\setbox0\hbox{}}\edef\R{\ifvoid0 V\fi\ifhbox0 H\fi}", "V");
    check(r"{\global\setbox0\hbox{}}\edef\R{\ifvoid0 V\fi\ifhbox0 H\fi}", "H");
    check(r"\setbox0\hbox{}\setbox1\box0 \edef\R{\ifvoid0 V\fi\ifhbox1 H\fi}", "VH");
    check(r"\setbox0\hbox{}\hbox{\unhcopy0}\edef\R{\ifvoid0 V\fi\ifhbox0 H\fi}", "H");
    check(r"\setbox0\hbox{}\hbox{\unhbox0}\edef\R{\ifvoid0 V\fi}", "V");
    check(r"\setbox0\hbox{}{\box0}\edef\R{\ifvoid0 V\fi\ifhbox0 H\fi}", "V");
    check(r"\setbox0\hbox{}{\setbox0\hbox{}\box0}\edef\R{\ifvoid0 V\fi\ifhbox0 H\fi}", "H");
    check(r"\setbox0\hbox{}\wd0=5pt \edef\R{\the\wd0}", "5.0pt");
    check(r"\wd0=5pt \edef\R{\the\wd0}", "0.0pt");
    check(r"\setbox0\hbox{}\edef\R{\the\wd0,\the\ht0,\the\dp0}", "0.0pt,0.0pt,0.0pt");
    check(r"\setbox0\hbox to 7pt{x}\edef\R{\the\wd0}", "7.0pt");
    check(r"\setbox0\vbox to 7pt{}\edef\R{\the\ht0,\the\wd0}", "7.0pt,0.0pt");
    // tex.web § 1043: a space is interword glue, of `\nullfont`'s zero space.
    check(r"\setbox0\hbox{ }\edef\R{\the\wd0}", "0.0pt");
    // Material satex does not measure leaves the size to typesetting.
    assert!(!last_certain(r"\setbox0\hbox{\discretionary{}{}{}}\edef\R{\the\wd0}"));
}

/// tex.web §§ 336-339: a (pseudo) file that ends in the middle of a scan
/// ends the scan: `}` for a definition or text, `\fi` for skipped text, and
/// an abandoned macro call.
#[test]
fn file_end_inside_a_scan() {
    check(r"\edef\R{\scantokens{ab}}", "ab");
    check(r"\scantokens{\def\R{ab}X", "ab");
    check(r"\def\a#1{\def\R{#1}}\def\R{none}\scantokens{\a}{x}", "none");
    check(r"\long\def\a#1.{\def\R{#1}}\def\R{none}\scantokens{\a x}.", "none");
    check(r"\def\R{0}\scantokens{\iffalse}\def\R{1}\fi", "1");
}

/// The pseudo file is tokenized as it is read (etex.ch `pseudo_input`):
/// a catcode change inside it governs the rest of it, even on the same line.
#[test]
fn scantokens_tokenizes_lazily() {
    check(r"\begingroup\catcode`\#=12 \scantokens{\endgroup\def\x#1{[#1]}}\edef\R{\x{a}}", "[a]");
    check(r"\catcode`\~=12 \scantokens{\catcode`\~=13 \def~{q}\edef\R{~}}", "q");
}

/// tex.web § 358: behind `\noexpand` an expandable token is `relax` with
/// `no_expand_flag` to `\ifx`, which nothing else equals.  The same holds
/// when the other operand has several meanings and `\ifx` splits on it.
#[test]
fn ifx_sees_a_noexpanded_macro_as_no_macro() {
    check(r"\def\a{x}\expandafter\ifx\noexpand\a\a\def\R{T}\else\def\R{F}\fi", "F");
    let joined = r"\def\a{x}\ifnum\pdfuniformdeviate2=0 \def\m{x}\else\def\m{y}\fi
        \expandafter\ifx\noexpand\a\m\def\R{T}\else\def\R{F}\fi";
    assert_eq!(r(joined), ["F"], "probe: {joined}");
}
