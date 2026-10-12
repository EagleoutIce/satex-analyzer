//! TeX, e-TeX and pdfTeX compliance of the interpreter, pinned against
//! `pdftex -ini -etex`.  Each test runs a small primitive-only probe and reads
//! back the replacement text of `\R` (set with `\edef`), which is what the
//! real engine prints with `\meaning\R`.  Tests that record a known
//! divergence are `#[ignore]`d with a one-line reason, so the suite stays
//! green while the divergence is documented.

use satex::machine::{Analysis, Machine};

const PRELUDE: &str =
    "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\catcode`\\^=7 \\catcode`\\ =10 \\catcode`\\%=14\n";

fn analyze(source: &str) -> Analysis {
    Machine::analyze(&format!("{PRELUDE}{source}"), None, &crate::support::bare())
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
    bodies(&analyze(source), "R").into_iter().map(|(t, _)| t.split_whitespace().collect()).collect()
}

/// The last certain text of `\R`, whitespace removed.
fn last(source: &str) -> String {
    r(source).pop().unwrap_or_else(|| "<undefined>".into())
}

/// Development aid: `PROBE=file cargo test --test it tex_compliance::probe -- --nocapture`
/// prints the texts of `\R` for each `%%% name` section of the file.
#[test]
// A development probe, driven by $PROBE: without it, nothing runs.
fn probe() {
    let Ok(path) = std::env::var("PROBE") else { return };
    let text = std::fs::read_to_string(path).unwrap();
    for section in text.split("%%% ").skip(1) {
        let (name, body) = section.split_once('\n').unwrap();
        let a = analyze(body);
        let b = bodies(&a, "R");
        let b: Vec<String> =
            b.iter().map(|(t, c)| format!("{}{}", if *c { "" } else { "?" }, t.replace(' ', "_"))).collect();
        println!("SATEX {name}: {} steps={} exh={}", b.join(" | "), a.steps, a.exhausted);
        // Each one-of token of the last `\R`, with its texts.
        if let Some(d) = a.facts.defs.iter().rev().find(|d| a.interner.name(d.name) == "R")
            && let Some(m) = &d.mac
        {
            for t in m.replacement_text.iter() {
                let Some(sym) = t.cs() else { continue };
                let Some(may) = a.env.slot(sym).and_then(|b| b.may.clone()) else { continue };
                let texts: Vec<String> = may
                    .iter()
                    .map(|m| {
                        m.as_macro().map_or_else(
                            || format!("{m:?}"),
                            |m| satex::tex::detokenize(&m.replacement_text, &a.interner),
                        )
                    })
                    .collect();
                println!("  one of {texts:?}");
            }
        }
    }
}

/// Whether the last definition of `\R` is marked certain.
fn last_certain(source: &str) -> bool {
    bodies(&analyze(source), "R").last().is_some_and(|(_, c)| *c)
}

/// What is wrong with the text `\R` ends with, against what pdfTeX gives it
/// (whitespace ignored), or `None`.
fn mismatch(source: &str, expected: &str) -> Option<String> {
    let got = last(source);
    let expected: String = expected.split_whitespace().collect();
    (got != expected).then(|| format!("\\R is {got:?}, expected {expected:?} for probe: {source}"))
}

/// Asserts that `\R` ends with the text pdfTeX gives it (whitespace ignored).
fn check(source: &str, expected: &str) {
    if let Some(why) = mismatch(source, expected) {
        panic!("{why}");
    }
}

/// Asserts that every outcome pdfTeX can reach is among the texts `\R` may have.
fn includes_all(source: &str, outcomes: &[&str]) {
    let got = r(source);
    for o in outcomes {
        assert!(got.iter().any(|g| g == o), "outcome {o:?} missing from {got:?} for probe: {source}");
    }
}

// Agreement with pdfTeX (these pass).

/// Probes and the text pdfTeX leaves in `\R`, whitespace ignored.
const AGREES: &[crate::support::PairRow] = &[
    (
        "numexpr_rounds_half_away_from_zero",
        &[(
            r"\edef\R{\number\numexpr 7/2\relax,\number\numexpr -7/2\relax,\number\numexpr 3*(4+5)/2\relax}",
            "4,-4,14",
        )],
    ),
    (
        "dimexpr_and_glueexpr_print_like_tex",
        &[(
            r"\edef\R{\the\dimexpr 1pt*3/4\relax,\the\glueexpr 1pt plus 2fil minus 1fill\relax}",
            "0.75pt,1.0pt plus 2.0fil minus 1.0fill",
        )],
    ),
    (
        "romannumeral_of_zero_and_negative_is_empty",
        &[(r"\edef\R{\romannumeral 1984 ,\romannumeral 0 ,\romannumeral -5 x}", "mcmlxxxiv,,x")],
    ),
    (
        "number_reads_signs_and_radixes",
        &[(r#"\edef\R{\number -+-00012 ,\number `\a,\number"FF ,\number'17 }"#, "12,97,255,15")],
    ),
    (
        "string_honours_escapechar_out_of_range",
        &[
            (r"\escapechar=-1 \edef\R{\string\foo}", "foo"),
            (r"\escapechar=300 \edef\R{\string\foo}", "foo"),
            (r"\escapechar=`\  \edef\R{\string\x}", "x"),
        ],
    ),
    (
        "csname_of_an_undefined_name_is_relax",
        &[
            (r"\edef\R{\expandafter\meaning\csname qqq\endcsname}", r"\relax"),
            (r"\edef\R{\expandafter\ifx\csname qq\endcsname\relax T\else F\fi\ifdefined\qq T\else F\fi}", "TT"),
        ],
    ),
    ("ifcase_nests_or_and_else", &[(r"\edef\R{\ifcase 2 a\or b\or c\ifcase 1 x\or y\else z\fi\or d\else e\fi}", "cy")]),
    (
        "ifx_compares_chardefs_parameters_and_long",
        &[
            (r"\chardef\c=65 \chardef\d=65 \edef\R{\ifx\c\d T\else F\fi}", "T"),
            (r"\def\a#1{x}\def\b#1{x}\def\c#1.{x}\edef\R{\ifx\a\b T\else F\fi\ifx\a\c T\else F\fi}", "TF"),
            (r"\long\def\a{x}\def\b{x}\edef\R{\ifx\a\b T\else F\fi}", "F"),
            (r"\edef\R{\ifx\undefa\relax T\else F\fi\ifx\undefa\undefb T\else F\fi}", "FT"),
        ],
    ),
    (
        "let_takes_one_optional_space_after_equals",
        &[
            (r"\let\x= a\edef\R{\meaning\x}", "the letter a"),
            (r"\def\s{ }\expandafter\let\expandafter\x\expandafter=\s\s b\edef\R{\meaning\x}", "macro:-> "),
        ],
    ),
    (
        "lowercase_and_uppercase_map_characters_only",
        &[
            (r"\lccode`\A=`\b \lowercase{\def\R{A\A}}", r"b\A"),
            (r"\uccode`a=`Z \uppercase{\def\R{ab}}", "ZB"),
            (r"\lccode`\ =`x \lowercase{\def\R{a b}}", "axb"),
        ],
    ),
    (
        "large_register_numbers_are_valid",
        &[(r"\count300=5 \count32767=7 \edef\R{\the\count300,\the\count32767}", "5,7")],
    ),
    (
        "pdftex_string_functions",
        &[
            (r"\edef\R{\pdfstrcmp{a}{b},\pdfstrcmp{b}{a},\pdfstrcmp{ab}{ab}}", "-1,1,0"),
            (r"\edef\R{\pdfmdfivesum{abc}}", "900150983CD24FB0D6963F7D28E17F72"),
            (r"\edef\R{\pdfescapestring{a(b)\string\\c}}", r"a\(b\)\\\\c"),
            // microtype's `\MT@ifint`: a number keeps its slot known.
            (
                r"\edef\R{\pdfmatch{^-*[0-9]+ *$}{013},\pdflastmatch0,\pdflastmatch1,\pdfmatch{(a)|(b)}{xb},\pdflastmatch1,\pdflastmatch2,\pdfmatch{(}{x},\pdflastmatch2,\pdfmatch icase{A}{xa},\pdflastmatch0,\pdfmatch{z}{xa},\pdflastmatch0}",
                "1,0->013,-1->,1,-1->,1->b,-1,1->b,1,1->a,0,-1->",
            ),
        ],
    ),
    (
        "expanded_protected_and_unexpanded",
        &[
            (r"\def\a{A}\edef\R{\expanded{\a\noexpand\a}}", "AA"),
            (r"\protected\def\p{P}\edef\R{\p}", r"\p"),
            (r"\def\a{X}\edef\R{\unexpanded{\a}\a}", r"\aX"),
            (r"\toks0{#}\edef\R{\the\toks0\unexpanded{#}}", "####"),
        ],
    ),
    (
        "dimension_units_convert_like_tex",
        &[
            (
                r"\dimen0=1dd\dimen1=1cc\dimen2=1sp\dimen3=1pc\dimen4=1mm\edef\R{\the\dimen0,\the\dimen1,\the\dimen2,\the\dimen3,\the\dimen4}",
                "1.07pt,12.8401pt,0.00002pt,12.0pt,2.84526pt",
            ),
            (r"\dimen0=16384pt \edef\R{\the\dimen0}", "16383.99998pt"),
            (r"\count0=-7 \divide\count0 2 \edef\R{\the\count0}", "-3"),
        ],
    ),
    // No arm matches 9, so the `\else` text runs.
    // A negative selector runs the `\else` text, the inner `\fi` first.
    // An `\ifcase` inside the number of another one.
    (
        "ifcase_with_one_open_conditional_in_its_number",
        &[
            (r"\edef\R{\ifcase \ifnum 1>2 1 \else 9 \fi A\or B\else C\fi}", "C"),
            (r"\edef\R{\ifcase \ifnum 1<2 0 \fi A\or B\or C\fi}", "A"),
            (r"\edef\R{\ifcase \ifnum 1<2 -1 \fi A\or B\else C\fi}", "C"),
            (r"\edef\R{\ifcase \ifcase \ifnum 1<2 1 \fi 7\or 2\or 8\fi A\or B\or C\fi}", "C"),
        ],
    ),
    (
        "initex_code_tables",
        &[(r"\edef\R{\the\lccode`A,\the\uccode`a,\the\sfcode`A,\the\mathcode`a,\the\catcode`a}", "97,65,999,29025,11")],
    ),
    (
        "afterassignment_and_aftergroup_order",
        &[
            (r"\def\a{\xdef\R{\R A}}\def\b{\xdef\R{\R B}}\def\R{}{\aftergroup\a\aftergroup\b}", "AB"),
            (r"\def\a{\xdef\R{\R A}}\def\R{}\afterassignment\a\afterassignment\a\count1=5", "A"),
            (r"\def\R{}\def\a{\xdef\R{\R A[\meaning\q]}}\afterassignment\a\def\q{Q}", "A[macro:->Q]"),
        ],
    ),
    (
        "currentiftype_and_branch",
        &[
            (r"\ifnum1=1 \xdef\R{\the\currentiftype,\the\currentifbranch}\fi", "3,1"),
            (r"\unless\ifnum1=2 \xdef\R{\the\currentiftype,\the\currentifbranch}\fi", "-3,1"),
            (r"\ifcase1 \or\xdef\R{\the\currentiftype,\the\currentifbranch}\fi", "17,1"),
        ],
    ),
    ("lastnodetype_of_empty_list", &[(r"\edef\R{\the\lastnodetype}", "-1")]),
    (
        "if_and_ifcat_expand_active_characters",
        &[
            (r"\catcode`\~=13 \def~{a}\edef\R{\ifcat~a T\else F\fi}", "T"),
            (r"\catcode`\~=13 \def~{a}\edef\R{\if~a T\else F\fi}", "T"),
            (r"\catcode`\~=13 \let~\relax \edef\R{\ifcat~\relax T\else F\fi}", "T"),
        ],
    ),
    ("scantokens_everyeof_noexpand_idiom", &[(r"\everyeof{\noexpand}\edef\R{\scantokens{ab}}", "ab")]),
    (
        "control_space_is_a_primitive",
        &[(r"\edef\R{\meaning\ }", r"\ "), (r"\def\a{\edef\R{\meaning\x}}\futurelet\x\a\ b", r"\ ")],
    ),
    (
        "meaning_of_par_like_primitives",
        &[(r"\edef\R{\meaning\par,\meaning\noindent,\meaning\ignorespaces}", r"\par,\noindent,\ignorespaces")],
    ),
    (
        "meaning_of_marks",
        &[(r"\edef\R{\meaning\topmark,\meaning\firstmark,\meaning\botmark}", r"\topmark:,\firstmark:,\botmark:")],
    ),
    ("numexpr_overflow_is_zero", &[(r"\edef\R{\number\numexpr 2147483647+1\relax}", "0")]),
    (
        "missing_number_is_zero",
        &[(r"\edef\R{[\number\numexpr -(3)\relax]}", r"[0(3)\relax]"), (r#"\edef\R{\number"af }"#, "0af")],
    ),
    ("initex_delcode_of_period", &[(r"\edef\R{\the\delcode`.,\the\delcode`a}", "0,-1")]),
    // tex.web §§ 336-339: a (pseudo) file that ends in the middle of a scan
    // ends the scan: `}` for a definition or text, `\fi` for skipped text, and
    // an abandoned macro call.
    (
        "file_end_inside_a_scan",
        &[
            (r"\edef\R{\scantokens{ab}}", "ab"),
            (r"\scantokens{\def\R{ab}X", "ab"),
            (r"\def\a#1{\def\R{#1}}\def\R{none}\scantokens{\a}{x}", "none"),
            (r"\long\def\a#1.{\def\R{#1}}\def\R{none}\scantokens{\a x}.", "none"),
            (r"\def\R{0}\scantokens{\iffalse}\def\R{1}\fi", "1"),
        ],
    ),
    // The pseudo file is tokenized as it is read (etex.ch `pseudo_input`):
    // a catcode change inside it governs the rest of it, even on the same line.
    (
        "scantokens_tokenizes_lazily",
        &[
            (r"\begingroup\catcode`\#=12 \scantokens{\endgroup\def\x#1{[#1]}}\edef\R{\x{a}}", "[a]"),
            (r"\catcode`\~=12 \scantokens{\catcode`\~=13 \def~{q}\edef\R{~}}", "q"),
        ],
    ),
    // tex.web § 424 and § 1034: a paragraph's horizontal list has a tail and a
    // space factor of its own, which `\@bsphack` and `\@esphack` read on every
    // `\label`.
    // The `\relax` stops the glue scan, which would otherwise expand the
    // conditional while looking for `plus` (tex.web § 461).
    // tex.web § 418: outside horizontal mode it is an error read as zero.
    (
        "a_paragraph_keeps_its_tail_and_space_factor",
        &[
            (r"\noindent a\kern3pt\relax\ifdim\lastskip=0pt \def\R{Z}\else\def\R{N}\fi\par", "Z"),
            (r"\noindent a\hskip3pt\relax\ifdim\lastskip>0pt \def\R{P}\else\def\R{Z}\fi\par", "P"),
            (r"\noindent a\hskip3pt\unskip\ifdim\lastskip=0pt \def\R{Z}\else\def\R{N}\fi\par", "Z"),
            (r"\noindent a\kern3pt\relax\edef\R{\the\lastkern}\par", "3.0pt"),
            (r"\noindent A\edef\R{\the\spacefactor}\par", "999"),
            (r"\noindent\edef\R{\the\spacefactor}\par", "1000"),
            (r"\edef\R{\the\spacefactor}", "0"),
        ],
    ),
];

#[test]
fn r_agrees_with_pdftex() {
    crate::support::pair_rows(AGREES, mismatch);
}

/// The number of an `\ifcase` built by conditionals that are still open when
/// the case is chosen: tex.web § 500 pops each of them at its `\fi` and goes
/// on skipping, and passes over their `\or`/`\else`.  expl3's
/// `\__fp_sanitize:Nw` is written this way, so getting it wrong loses the
/// `\__fp_sep:` that ends every floating point value.
const CHAIN: &str =
    r"\def\chain#1#2#3{\ifcase \ifnum #1 1 \else\ifnum #2 2 \else\ifnum #3 3 \fi\fi\fi 0 A\or B\or C\or D\else E\fi}";

#[test]
fn ifcase_skips_past_the_fi_of_a_conditional_its_number_left_open() {
    check(&format!(r"{CHAIN}\edef\R{{\chain{{1>2}}{{3>4}}{{5<6}}}}"), "D");
    check(&format!(r"{CHAIN}\edef\R{{\chain{{1<2}}{{3>4}}{{5<6}}}}"), "B");
    check(&format!(r"{CHAIN}\edef\R{{\chain{{1>2}}{{3<4}}{{5<6}}}}"), "C");
    // Every test false: the innermost has no `\else`, so its own `\fi` closes
    // it and the number is the trailing 0.
    check(&format!(r"{CHAIN}\edef\R{{\chain{{1>2}}{{3>4}}{{5>6}}}}"), "A");
}

#[test]
fn ifcase_passes_over_the_else_of_a_conditional_its_number_left_open() {
    let pick = r"\def\pick#1{\ifcase \ifnum #1 2 \else 1 \fi 0 A\or B\or C\else D\fi}";
    check(&format!(r"{pick}\edef\R{{\pick{{1<2}}}}"), "C");
    check(&format!(r"{pick}\edef\R{{\pick{{1>2}}}}"), "B");
}

/// Two macros so that every `\aftergroup` probe below can say what ran, in
/// what order.
const AB: &str = r"\def\R{}\def\a{\xdef\R{\R A}}\def\b{\xdef\R{\R B}}";

/// tex.web § 265 makes `\aftergroup` and `\afterassignment` two commands, so
/// a `\let` copy is the command it copied and not the name it is reached
/// through.  expl3 reaches `\aftergroup` only through copies
/// (`\tex_aftergroup:D`, `\group_insert_after:N`), so a copy that waits for
/// an assignment instead holds its token back for good.
#[test]
fn a_let_copy_of_aftergroup_still_ends_the_group() {
    check(&format!(r"{AB}\let\ag\aftergroup{{\ag\a\ag\b}}"), "AB");
    check(&format!(r"{AB}\let\ag\aftergroup\begingroup\ag\a\ag\b\endgroup"), "AB");
    check(&format!(r"{AB}\def\c{{\xdef\R{{\R C}}}}\let\ag\aftergroup{{\ag\a\ag\b\ag\c}}"), "ABC");
    // An implicit `{` closes like an explicit one (tex.web § 1063).
    check(&format!(r"{AB}\let\ag\aftergroup\let\bg={{ \let\eg=}} \bg\ag\a\eg"), "A");
}

/// tex.web § 269: the tokens go back into the input at the end of the group,
/// in front of what followed it, so the macro among them reads its arguments
/// from there.
#[test]
fn a_token_from_aftergroup_reads_its_arguments_after_the_group() {
    let ag = r"\def\R{}\let\ag\aftergroup";
    // An undelimited argument, taken from after the group.
    check(&format!(r"{ag}\def\u#1{{\xdef\R{{\R[\string#1]}}}}{{\ag\u}}\relax"), r"[\relax]");
    // A braced one, which loses its braces.
    check(&format!(r"{ag}\def\u#1{{\xdef\R{{\R[#1]}}}}{{\ag\u}}{{ab}}"), "[ab]");
    // Two of them: the `\use_i:nn` shape every expl3 conditional returns with.
    check(&format!(r"{ag}\def\u#1#2{{\xdef\R{{\R(#1)}}}}{{\ag\u}}{{p}}{{q}}"), "(p)");
    // A delimited one, which scans past the group for its delimiter.
    check(&format!(r"{ag}\def\u#1\stop{{\xdef\R{{\R<\string#1>}}}}{{\ag\u}}\relax\stop"), r"<\relax>");
    // The same, with the group inside a macro body and the arguments at the
    // call site.
    check(&format!(r"{ag}\def\u#1#2{{\xdef\R{{\R(#1)}}}}\def\c{{{{\ag\u}}}}\c{{yes}}{{no}}"), "(yes)");
}

/// The innermost group ends first, so its tokens run first.
#[test]
fn aftergroup_in_nested_groups_runs_the_inner_group_first() {
    check(&format!(r"{AB}{{\aftergroup\a{{\aftergroup\b}}}}"), "BA");
    check(&format!(r"{AB}\let\ag\aftergroup{{\ag\a{{\ag\b}}}}"), "BA");
}

/// `\afterassignment` fires at the next assignment, `\aftergroup` at the end
/// of the group, whichever names they are reached through.
#[test]
fn aftergroup_and_afterassignment_are_told_apart() {
    check(&format!(r"{AB}\let\ag\aftergroup\let\aa\afterassignment{{\ag\a\aa\b\count255=1 }}"), "BA");
    check(&format!(r"{AB}\let\aa\afterassignment{{\aa\a}}\count255=3 "), "A");
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
    includes_all(
        r"\count1=0 \ifnum\pdfuniformdeviate2=0 \count1=5 \fi\ifnum\count1=5 \def\R{T}\else\def\R{F}\fi",
        &["T", "F"],
    );
    includes_all(r"\count1=\pdfuniformdeviate 10\relax \ifnum\count1>4 \def\R{T}\else\def\R{F}\fi", &["T", "F"]);
}

// Former divergences (see the critique report); each asserts the pdfTeX
// behavior, or, where one path's outcome cannot be told apart from the
// other's, that satex does not claim a certain one.

/// tex.web § 445: the scan of the digits expands the `\ifnum` after them,
/// which reads the register before the assignment (pdfTeX, any seed).
#[test]
fn unknown_number_does_not_swallow_the_following_conditional() {
    check(r"\count1=\pdfuniformdeviate 10 \ifnum\count1>4 \def\R{T}\else\def\R{F}\fi", "F");
    check(r"\count1=7 \count1=\pdfuniformdeviate 10 \ifnum\count1>4 \def\R{T}\else\def\R{F}\fi", "T");
    includes_all(r"\count1=\pdfuniformdeviate 10 \relax\ifnum\count1>4 \def\R{T}\else\def\R{F}\fi", &["T", "F"]);
}

/// Probes whose last definition of `\R` must not be marked certain.
const UNCERTAIN: &[(&str, &str)] = &[
    ("mdfivesum_of_unknown_text_is_not_definite", r"\edef\R{\pdfmdfivesum{\pdfuniformdeviate 10}}"),
    // The paths stay apart while their category codes differ; each defines
    // `\R` at the same site, so the definition is not certain.
    (
        "catcode_change_on_one_path_is_joined",
        r"\catcode`\~=13 \def~{T}\ifnum\pdfuniformdeviate2=0 \catcode`\Q=13 \let Q~\fi\edef\R{Q}",
    ),
    // `\escapechar` joins to unknown, and `\string` then gives unknown text.
    ("escapechar_change_on_one_path_is_joined", r"\ifnum\pdfuniformdeviate2=0 \escapechar=-1 \fi\edef\R{\string\x}"),
    ("uccode_change_on_one_path_is_joined", r"\ifnum\pdfuniformdeviate2=0 \uccode`a=`B \fi\uppercase{\def\R{a}}"),
    (
        "aftergroup_on_one_path_is_not_certain",
        r"\def\R{0}\def\a{\def\R{1}}{\ifnum\pdfuniformdeviate2=0 \aftergroup\a\fi}\edef\R{\R}",
    ),
    (
        "afterassignment_on_one_path_is_not_certain",
        r"\def\a{\def\R{A}}\def\R{0}\ifnum\pdfuniformdeviate2=0 \afterassignment\a\fi\count1=1",
    ),
    ("endinput_on_one_path_is_not_certain", "\\def\\R{0}\\ifnum\\pdfuniformdeviate2=0 \\endinput\\fi\n\\def\\R{1}\n"),
    // tex.web § 1105: `\unskip` on a list whose tail satex does not know must
    // widen the size, not leave it as it was.
    (
        "unskip_on_an_unknown_tail_is_not_free",
        r"\setbox0=\hbox{\kern3pt\pdfliteral{x}\unskip}\ifdim\wd0=3pt \def\R{K}\else\def\R{N}\fi",
    ),
];

#[test]
fn r_is_left_uncertain() {
    crate::support::table(
        UNCERTAIN,
        |r| r.0,
        |&(_, source)| last_certain(source).then(|| "\\R is certain".to_string()),
    );
}

/// tex.web § 1224: `\chardef` makes `\c` `\relax` before scanning, so the
/// `\ifnum` the digit scan expands is missing a number, taken as 0 (pdfTeX,
/// any seed). After `\relax` the value is unknown.
#[test]
fn chardef_from_unknown_number_is_unknown() {
    check(r"\chardef\c=\pdfuniformdeviate3 \ifnum\c=0 \def\R{T}\else\def\R{F}\fi", "T");
    includes_all(r"\chardef\c=\pdfuniformdeviate3 \relax\ifnum\c=0 \def\R{T}\else\def\R{F}\fi", &["T", "F"]);
}

#[test]
fn edef_with_unknown_value_is_not_certain() {
    assert!(!last_certain(r"\edef\R{\number\pdfuniformdeviate 10}"));
    assert!(!last_certain(r"\edef\R{\pdfnormaldeviate}"));
}

/// The name is unknown, so neither `\x` nor `\y` is known to be undefined.
#[test]
fn csname_from_joined_macro_keeps_both_names() {
    assert!(!last_certain(
        r"\ifnum\pdfuniformdeviate2=0 \def\n{x}\else\def\n{y}\fi\expandafter\def\csname\n\endcsname{Q}\edef\R{\ifdefined\x X\fi\ifdefined\y Y\fi}"
    ));
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
fn self_nesting_number_scan_stops_instead_of_crashing() {
    // Widened, not exhausted: the run ends and reports the gap.
    let a = analyze(r"\def\a{\number\a}\a");
    assert!(!a.exhausted);
    assert!(a.facts.diagnostics.iter().any(|d| d.code == "nesting-widened"));
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

/// tex.web § 358, § 325: the `\noexpand` marker goes with the read, so a
/// token a scan gives back is expanded when read again (pdfTeX, XeTeX and
/// LuaTeX agree).
#[test]
fn a_noexpanded_token_given_back_loses_the_marker() {
    let x = r"\def\R{}\def\x{\def\R{X}}";
    check(&format!(r"{x}\count1=1\noexpand\x "), "X");
    check(&format!(r"{x}\skip0=1pt plus 1pt\noexpand\x "), "X");
    check(&format!(r"{x}\expandafter\afterassignment\noexpand\x \count1=3 "), "X");
    check(&format!(r"{x}\def\z{{}}\expandafter\futurelet\expandafter\y\expandafter\z\noexpand\x "), "X");
}

/// tex.web § 420: what `\wd` reads for a box whose list satex could not
/// follow is unknown, never zero — a document that asks `\ifdim\wd0=0pt`
/// would otherwise take an arm with confidence.
#[test]
fn a_box_satex_cannot_follow_has_no_width() {
    // `\pdfuniformdeviate` is unknown, so the kern's width is too.
    assert!(!last_certain(r"\setbox0=\hbox{\kern\pdfuniformdeviate 10sp}\ifdim\wd0=0pt \def\R{Z}\else\def\R{N}\fi"));
    // tex.web § 649: `hpack` of an empty list really is zero wide.
    check(r"\setbox0=\hbox{}\ifdim\wd0=0pt \def\R{Z}\else\def\R{N}\fi", "Z");
    // tex.web § 644: `\hbox to 0pt` is zero wide whatever the list holds.
    check(r"\setbox0=\hbox to 0pt{\kern\pdfuniformdeviate 10sp}\ifdim\wd0=0pt \def\R{Z}\else\def\R{N}\fi", "Z");
    // tex.web § 420: a void register reads as zero.
    check(r"\ifdim\wd7=0pt \def\R{Z}\else\def\R{N}\fi", "Z");
    // tex.web § 668: a vertical list of glue and kerns has no width.
    check(r"\setbox0=\vbox{\kern\pdfuniformdeviate 10sp}\ifdim\wd0=0pt \def\R{Z}\else\def\R{N}\fi", "Z");
    // tex.web § 644: a `to` size satex does not know is not zero either.
    assert!(!last_certain(
        r"\dimen0=\pdfuniformdeviate 10sp \setbox0=\hbox to \dimen0{}\ifdim\wd0=0pt \def\R{Z}\else\def\R{N}\fi"
    ));
}

/// tex.web §§ 649, 668: `hpack` and `vpack` start the height and the depth
/// at zero and only take maxima, so a packed box has neither below zero —
/// until `\ht` is assigned one.
#[test]
fn a_packed_height_is_not_negative_unless_it_was_assigned() {
    check(r"\setbox0=\hbox{\kern\pdfuniformdeviate 10sp}\ifdim\ht0<0pt \def\R{N}\else\def\R{P}\fi", "P");
    assert!(!last_certain(
        r"\setbox0=\hbox{\kern\pdfuniformdeviate 10sp}\ifnum\pdfuniformdeviate2=0 \ht0=-3pt\fi
          \ifdim\ht0<0pt \def\R{N}\else\def\R{P}\fi"
    ));
}

/// A name defined on one path only is undefined on the other: `\ifx`
/// against an undefined name and `\ifdefined` see both (tex.web § 507).
#[test]
fn name_defined_on_one_path_may_be_undefined() {
    let set = r"\ifnum\pdfuniformdeviate2=0 \def\x{a}\fi";
    includes_all(&format!(r"{set}\ifx\x\undefined \def\R{{U}}\else\def\R{{D}}\fi"), &["U", "D"]);
    includes_all(&format!(r"{set}\ifdefined\x \def\R{{D}}\else\def\R{{U}}\fi"), &["U", "D"]);
    // Where it is defined, it is read with its meaning.
    includes_all(&format!(r"{set}\ifdefined\x \edef\R{{\x}}\else\def\R{{U}}\fi"), &["a", "U"]);
}

/// tex.web § 373: "Missing \endcsname inserted" is TeX's error, raised and
/// counted. Met in a command whose text satex does not know, it may be
/// satex's own: a gap then, not raised.
#[test]
fn engine_error_after_unknown_text_is_a_gap() {
    let severity = |source: &str| {
        let a = analyze(source);
        let d = a.facts.diagnostics.iter().find(|d| d.code == "missing-endcsname").expect("reported");
        (d.severity, a.facts.raised.iter().any(|r| r.text == "Missing \\endcsname inserted"))
    };
    use satex::facts::Severity;
    assert_eq!(severity(r"\expandafter\let\csname x\relax\endcsname\relax"), (Severity::Warning, true));
    assert_eq!(
        severity(r"\expandafter\let\csname x\pdfnormaldeviate\relax\endcsname\relax"),
        (Severity::Unsupported, false)
    );
}

/// tex.web § 1110: `\unhcopy` of an hbox of rules adds their natural width,
/// whatever `\wd` was set to since (`\strut` in a box, LaTeX's `\strutbox`).
#[test]
fn unhcopy_of_rules_adds_their_natural_width() {
    check(
        r"\setbox1\hbox{\vrule width 3pt height 2pt depth 1pt}\setbox0\hbox{\unhcopy1}\edef\R{\the\wd0/\the\ht0/\the\dp0}",
        "3.0pt/2.0pt/1.0pt",
    );
    check(r"\setbox1\hbox{\vrule width 3pt}\wd1=7pt \setbox0\hbox{\unhcopy1}\edef\R{\the\wd0}", "3.0pt");
    // The list's last node is the new tail: `\unskip` finds a rule there and
    // takes nothing; an empty list leaves the tail as it was.
    check(
        r"\setbox1\hbox{\vrule width 3pt height 2pt depth 1pt}\setbox0\hbox{\hskip2pt\unhcopy1\unskip}\edef\R{\the\wd0/\the\ht0/\the\dp0}",
        "5.0pt/2.0pt/1.0pt",
    );
    check(r"\setbox1\hbox{}\setbox0\hbox{\vrule width 1pt\kern2pt\unhcopy1\unkern}\edef\R{\the\wd0}", "1.0pt");
}

/// tex.web § 1375: `\immediate\write` and `\immediate\openout` leave the
/// list as it was; a deferred `\write` puts a whatsit at its end (§ 1352).
/// LaTeX's `\wlog` in a font's `.fd` file read inside a box is the case.
#[test]
fn immediate_write_leaves_the_tail() {
    check(r"\setbox0\hbox{\kern3pt\immediate\write-1{}\unkern}\edef\R{\the\wd0}", "0.0pt");
    check(r"\setbox0\hbox{\kern3pt\write-1{}\unkern}\edef\R{\the\wd0}", "3.0pt");
}

/// etex.ch: after a rule `\lastnodetype` is 3, never glue (11). LaTeX's
/// `\par` asks this, and a line of one rule stays one line.
#[test]
fn lastnodetype_after_a_rule_is_not_glue() {
    check(r"\setbox0\hbox{\vrule\edef\R{\ifnum\lastnodetype=11 G\else N\fi}}", "N");
    check(
        r"\setbox0\vbox{\hsize=100pt\parfillskip=0pt\noindent\vrule height 1pt\ifnum\lastnodetype=11 \hskip0pt\fi\par}\edef\R{\the\ht0}",
        "1.0pt",
    );
}

/// tex.web §§ 868, 1096: glue after a penalty is no breakpoint, and glue
/// alone makes a paragraph not null: `~~` in a narrow box is one line.
#[test]
fn glue_after_a_penalty_is_no_breakpoint() {
    check(
        r"\setbox0\vbox{\hsize=10pt\parfillskip=0pt\noindent\vrule height 1pt\penalty10000\hskip 5pt\penalty10000\hskip 5pt\vrule height 2pt\par}\edef\R{\the\wd0/\the\ht0}",
        "10.0pt/2.0pt",
    );
}
