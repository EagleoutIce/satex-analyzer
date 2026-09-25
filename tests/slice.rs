//! `satex slice`: the part of a document a name depends on, or that depends
//! on it.
//!
//! The property every test below is written against: a backward slice holds
//! every definition and every call that can decide what the sliced name means
//! at the slice point, and the source it reconstructs is balanced TeX that
//! still gives that name the same meaning.  Keeping something that cannot
//! matter is imprecision; dropping something that can is a bug.

use std::path::{Path, PathBuf};
use std::process::Command;

use satex::config::Config;
use satex::machine::{Analysis, Machine};
use satex::query::{self, At, Direction, Record};

fn installed() -> bool {
    which::which("kpsewhich").is_ok()
}

/// No format, no packages: the primitives are enough for documents whose
/// answer is decidable by hand.
fn analyze(source: &str) -> Analysis {
    Machine::analyze(source, None, &bare())
}

/// The LaTeX kernel read for real (and cached): `\newcommand`, `\newcounter`
/// and the rest of the interfaces this file slices are `latex.ltx` macros,
/// not primitives, so they need the format loaded.
fn analyze_kernel(source: &str) -> Analysis {
    let cfg = Config { load_packages: false, load_classes: false, ..Config::default() };
    Machine::analyze(source, None, &cfg)
}

fn bare() -> Config {
    Config {
        load_packages: false,
        load_classes: false,
        load_inputs: false,
        load_format: false,
        use_kpsewhich: false,
        ..Config::default()
    }
}

// Without a format the catcodes are INITEX's (tex.web § 232).  Kept out of
// `analyze()`/`bare()` themselves: `reconstruction()` below indexes back
// into the exact `source` a test passed it, so a hidden prelude there would
// offset every span it reads out of `analysis` against an unprefixed
// `source`.  Each failing test instead carries the prelude in its own
// literal, on line 1 with no newline so its line numbers stay put (moved to
// a line of its own only where a test's own position names line 1).
const PRELUDE: &str = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\catcode`\\^=7 ";

fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join("satex-slice-test").join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    directory
}

fn slice_at(
    analysis: &Analysis,
    names: &[&str],
    at: Option<At>,
    direction: Direction,
) -> Vec<Record> {
    let names: Vec<String> = names.iter().map(|n| (*n).to_string()).collect();
    query::slice(analysis, &names, at.as_ref(), direction)
}

/// `\name@line` for every vertex the slice keeps: enough to say which
/// definition site is in and which is out.
fn marks(records: &[Record]) -> Vec<String> {
    let mut out: Vec<String> = records
        .iter()
        .map(|record| {
            format!(
                "{}@{}",
                record["name"].as_str().unwrap_or_default(),
                record["line"].as_u64().unwrap_or_default()
            )
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

fn backward(analysis: &Analysis, names: &[&str]) -> Vec<String> {
    marks(&slice_at(analysis, names, None, Direction::Backward))
}

fn at(analysis: &Analysis, line: u32, col: u32) -> Vec<String> {
    marks(&slice_at(analysis, &[], Some(At::here(line, col)), Direction::Backward))
}

fn holds(found: &[String], wanted: &str) -> bool {
    found.iter().any(|m| m == wanted)
}

fn assert_holds(found: &[String], wanted: &[&str], missing: &[&str]) {
    for mark in wanted {
        assert!(holds(found, mark), "{mark} belongs in the slice: {found:?}");
    }
    for mark in missing {
        assert!(!holds(found, mark), "{mark} cannot affect the slice: {found:?}");
    }
}

// --- what the slice point decides --------------------------------------

#[test]
fn a_slice_point_takes_the_definition_in_force_and_not_the_last_one() {
    // `\def` neither tests nor keeps the meaning it overwrites, so the first
    // definition cannot reach the use on line 3 at all.
    let analysis = analyze(&format!("{PRELUDE}\\def\\foo{{first}}\n\\def\\foo{{second}}\n\\foo\n"));
    assert_holds(&at(&analysis, 3, 1), &["\\foo@2"], &["\\foo@1"]);
    // Sliced by name, every site of that name is a criterion, so both are in.
    assert_holds(&backward(&analysis, &["\\foo"]), &["\\foo@1", "\\foo@2"], &[]);
}

#[test]
fn a_redefinition_reads_the_definition_it_replaces() {
    // latex.ltx defines `\renewcommand` through `\@ifundefined`, which errors
    // out unless the name is already there: the earlier definition is a real
    // dependency of the later one, and of everything downstream.
    let analysis =
        analyze_kernel("\\newcommand{\\foo}{first}\n\\renewcommand{\\foo}{second}\n\\foo\n");
    assert_holds(&at(&analysis, 3, 1), &["\\foo@1", "\\foo@2"], &[]);
}

#[test]
fn a_provision_that_does_nothing_is_not_in_the_slice() {
    // latex.ltx's `\provide@command` defines a scratch name when the name is
    // already taken, so line 2 is what the use on line 3 depends on and the
    // body on line 3 never takes effect.
    let analysis =
        analyze_kernel("\\newcommand{\\p}{first}\n\\providecommand{\\p}{second}\n\\p\n");
    assert_holds(&at(&analysis, 3, 1), &["\\p@1"], &["\\p@2"]);
}

#[test]
fn a_provision_of_a_free_name_defines_it() {
    let analysis = analyze_kernel("\\providecommand{\\q}{only}\n\\q\n");
    assert_holds(&at(&analysis, 2, 1), &["\\q@1"], &[]);
}

#[test]
fn a_let_chain_reaches_the_definition_at_the_end_of_it() {
    let analysis = analyze(&format!(
        "{PRELUDE}\\def\\base{{B}}\n\\let\\alias\\base\n\\let\\second\\alias\n\\second\n"
    ));
    assert_holds(
        &at(&analysis, 4, 1),
        &["\\base@1", "\\alias@2", "\\second@3"],
        &[],
    );
}

#[test]
fn every_definition_dialect_reaches_its_use() {
    let analysis = analyze_kernel(concat!(
        // Bodies that typeset nothing: text in the preamble would run
        // `\\everypar`, whose `Missing \\begin{document}` error reads
        // every definition.
        "\\def\\plain{\\relax}\n",
        "\\newcommand{\\fresh}{\\relax}\n",
        "\\DeclareRobustCommand{\\robust}{\\relax}\n",
        "\\plain\\fresh\\robust\n",
    ));
    assert_holds(&at(&analysis, 4, 1), &["\\plain@1"], &["\\fresh@2", "\\robust@3"]);
    assert_holds(&at(&analysis, 4, 7), &["\\fresh@2"], &["\\plain@1", "\\robust@3"]);
    assert_holds(&at(&analysis, 4, 13), &["\\robust@3"], &["\\plain@1", "\\fresh@2"]);
}

#[test]
fn a_definition_rolled_back_with_its_group_is_not_in_a_later_slice() {
    // The TeXbook § 275: a local assignment is undone when the group ends,
    // so line 3 sees the outer meaning and line 2 sees the inner one.
    let analysis = analyze(&format!("{PRELUDE}\\def\\g{{outer}}\n{{\\def\\g{{inner}}\\g}}\n\\g\n"));
    assert_holds(&at(&analysis, 3, 1), &["\\g@1"], &["\\g@2"]);
    assert_holds(&at(&analysis, 2, 15), &["\\g@2"], &["\\g@1"]);
}

#[test]
fn a_definition_rolled_back_with_begingroup_is_not_in_a_later_slice() {
    // `\begingroup` opens a group like a brace does (tex.web § 269), so the
    // inner definition cannot reach line 5.
    let analysis = analyze(&format!(
        "{PRELUDE}\\def\\g{{outer}}\n\\begingroup\n\\def\\g{{inner}}\n\\endgroup\n\\g\n"
    ));
    assert_holds(&at(&analysis, 5, 1), &["\\g@1"], &["\\g@3"]);
    assert_holds(&at(&analysis, 3, 1), &["\\g@3"], &[]);
}

#[test]
fn a_global_definition_inside_a_group_survives_it() {
    let analysis = analyze(&format!("{PRELUDE}\\def\\g{{outer}}\n{{\\global\\def\\g{{inner}}}}\n\\g\n"));
    assert_holds(&at(&analysis, 3, 1), &["\\g@2"], &["\\g@1"]);
}

#[test]
fn both_arms_of_an_undecided_condition_are_in_the_slice() {
    // a random number cannot be settled, so either definition
    // may be the one in force at line 6 and the slice has to keep both,
    // together with the test that chose between them.
    let analysis = analyze(&format!(
        "{PRELUDE}\\ifnum\\pdfuniformdeviate2=0\n\\def\\m{{a}}\n\\else\n\\def\\m{{b}}\n\\fi\n\\m\n"
    ));
    assert_holds(&at(&analysis, 6, 1), &["\\m@2", "\\m@4", "\\ifnum@1"], &[]);
}

#[test]
fn an_undecided_condition_keeps_the_definition_it_may_not_replace() {
    // No `\else`: the arm may not run at all, so the definition from before
    // the conditional is still possible.
    let analysis = analyze(&format!(
        "{PRELUDE}\\def\\m{{a}}\n\\ifnum\\pdfuniformdeviate2=0 \\def\\m{{b}}\\fi\n\\m\n"
    ));
    assert_holds(&at(&analysis, 3, 1), &["\\m@1", "\\m@2", "\\ifnum@2"], &[]);
}

// --- registers ---------------------------------------------------------

#[test]
fn a_counter_slice_holds_every_assignment_to_it() {
    // `\setcounter` needs the counter declared and `\addtocounter` needs its
    // value, so line 3 depends on both lines above it.
    let analysis = analyze_kernel(
        "\\newcounter{step}\n\\setcounter{step}{3}\n\\addtocounter{step}{2}\n",
    );
    assert_holds(&at(&analysis, 3, 1), &["\\c@step@1", "\\c@step@2", "\\c@step@3"], &[]);
}

#[test]
fn a_length_slice_holds_setlength_and_addtolength() {
    let analysis = analyze_kernel(
        "\\newlength{\\gap}\n\\setlength{\\gap}{1pt}\n\\addtolength{\\gap}{2pt}\n",
    );
    assert_holds(&at(&analysis, 3, 1), &["\\gap@1", "\\gap@2", "\\gap@3"], &[]);
}

#[test]
fn reading_a_register_depends_on_what_was_assigned_to_it() {
    // `\the\gap` is a use of the register (tex.web § 465), so the slice at
    // that position has to hold the assignments that decided its value.
    let analysis = analyze_kernel(
        "\\newlength{\\gap}\n\\setlength{\\gap}{1pt}\n\\addtolength{\\gap}{2pt}\n\\the\\gap\n",
    );
    assert_holds(
        &at(&analysis, 4, 1),
        &["\\gap@1", "\\gap@2", "\\gap@3", "\\gap@4"],
        &[],
    );
}

#[test]
fn a_forward_slice_from_a_declaration_reaches_the_assignments() {
    let analysis = analyze_kernel("\\newlength{\\gap}\n\\setlength{\\gap}{1pt}\n\\the\\gap\n");
    let forward =
        marks(&slice_at(&analysis, &[], Some(At::here(1, 1)), Direction::Forward));
    assert_holds(&forward, &["\\gap@1", "\\gap@2", "\\gap@3"], &[]);
}

// --- names that are not plain control words ----------------------------

#[test]
fn an_environment_slice_holds_both_halves_of_it() {
    // `\newenvironment{note}` defines `\note` and `\endnote`; a slice on the
    // environment name has to reach the closing half too.
    let analysis =
        analyze_kernel("\\newenvironment{note}{A}{B}\n\\begin{note}\nx\n\\end{note}\n");
    assert_holds(&backward(&analysis, &["note"]), &["\\note@1", "\\endnote@1"], &[]);
}

#[test]
fn an_expl3_name_with_colons_can_be_sliced() {
    // expl3 is `expl3-code.tex`, which only the (cached) kernel brings.
    let cfg = Config { load_format: true, use_kpsewhich: true, ..bare() };
    let analysis = Machine::analyze(concat!(
        "\\ExplSyntaxOn\n",
        "\\cs_new:Npn \\my_helper:n #1 { [#1] }\n",
        "\\cs_new:Npn \\my_wrap:n #1 { \\my_helper:n {#1} }\n",
        "\\ExplSyntaxOff\n",
    ), None, &cfg);
    assert_holds(
        &backward(&analysis, &["\\my_wrap:n"]),
        &["\\my_helper:n@2", "\\my_wrap:n@3"],
        &[],
    );
}

#[test]
fn an_active_character_can_be_sliced() {
    // `~` names the active character, which is kept apart from the control
    // symbol `\~` (tex.web § 222).
    let analysis = analyze(&format!("{PRELUDE}\\def\\sub{{S}}\n\\catcode`\\~=13\n\\def~{{\\sub}}\na~b\n"));
    assert_holds(&backward(&analysis, &["~"]), &["\\sub@1", "~@3"], &[]);
}

#[test]
fn a_name_built_with_csname_can_be_sliced() {
    let analysis = analyze(&format!(
        "{PRELUDE}{}",
        concat!(
            "\\expandafter\\def\\csname built\\endcsname{D}\n",
            "\\csname built\\endcsname\n",
        )
    ));
    assert_holds(&backward(&analysis, &["\\built"]), &["\\built@1", "\\built@2"], &[]);
}

// --- criteria that name nothing ----------------------------------------

#[test]
fn a_name_that_is_nowhere_slices_to_nothing() {
    let analysis = analyze("\\def\\foo{A}\n\\foo\n");
    assert!(slice_at(&analysis, &["\\nowhere"], None, Direction::Backward).is_empty());
    assert!(slice_at(&analysis, &["\\nowhere"], None, Direction::Forward).is_empty());
}

#[test]
fn a_position_past_the_end_of_the_file_slices_to_nothing() {
    let analysis = analyze(&format!("{PRELUDE}\\def\\foo{{A}}\n\\foo\n"));
    assert!(slice_at(&analysis, &[], Some(At::here(999, 1)), Direction::Backward).is_empty());
    // A column past the end of a line that does hold something still names it.
    assert_holds(&at(&analysis, 2, 999), &["\\foo@1", "\\foo@2"], &[]);
}

// --- what a column means -----------------------------------------------

#[test]
fn a_column_picks_the_construct_that_starts_at_or_before_it() {
    let analysis = analyze(&format!("{PRELUDE}\\def\\a{{A}}\n\\def\\b{{B}}\n\\a\\b\n"));
    assert_holds(&at(&analysis, 3, 1), &["\\a@1"], &["\\b@2"]);
    assert_holds(&at(&analysis, 3, 3), &["\\b@2"], &["\\a@1"]);
}

#[test]
fn a_column_inside_an_argument_names_the_call_that_owns_it() {
    // `\a` stands at column 8, but a token in a body that is never expanded
    // is no program point of its own; the construct the position belongs to
    // is the definition that begins at column 1, and that definition already
    // reads what its body names.
    let analysis = analyze(&format!("{PRELUDE}\\def\\a{{A}}\n\\def\\b{{\\a}}\n"));
    assert_holds(&at(&analysis, 2, 8), &["\\b@2", "\\a@1"], &[]);
}

#[test]
fn a_column_before_everything_on_the_line_names_the_first_construct() {
    // The opening brace is not a vertex of its own; the position still names
    // the definition that follows it rather than nothing.
    let analysis = analyze(&format!("{PRELUDE}\\def\\g{{outer}}\n{{\\def\\g{{inner}}}}\n"));
    assert_holds(&at(&analysis, 2, 1), &["\\g@2"], &[]);
}

#[test]
fn a_position_can_name_a_file_the_run_read() {
    let directory = scratch("input-position");
    std::fs::write(directory.join("part.tex"), "\\def\\shared{S}\n\\def\\other{O}\n").unwrap();
    let main = directory.join("main.tex");
    // The prelude runs before `\input`, so its catcodes reach part.tex too.
    let source = format!("{PRELUDE}\\input{{part}}\n\\shared\n");
    std::fs::write(&main, &source).unwrap();
    let cfg = Config { load_inputs: true, ..bare() };
    let analysis = Machine::analyze(&source, Some(&main), &cfg);
    let place = At { file: Some("part.tex".into()), line: 2, col: 1, end: None };
    let found = marks(&slice_at(&analysis, &[], Some(place), Direction::Backward));
    assert_holds(&found, &["\\other@2"], &["\\shared@1"]);
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn a_position_in_a_file_the_run_never_read_names_nothing() {
    let analysis = analyze("\\def\\foo{A}\n\\foo\n");
    let place = At { file: Some("nowhere.tex".into()), line: 1, col: 1, end: None };
    assert!(slice_at(&analysis, &[], Some(place), Direction::Backward).is_empty());
}

#[test]
fn a_position_parses_with_and_without_a_file() {
    assert_eq!(satex::cmd::parse_place("3:7"), Ok(At::here(3, 7)));
    assert_eq!(
        satex::cmd::parse_place("part.tex:3:7"),
        Ok(At { file: Some("part.tex".into()), line: 3, col: 7, end: None })
    );
    assert_eq!(
        satex::cmd::parse_place("/a/b/part.tex:3:7"),
        Ok(At { file: Some("/a/b/part.tex".into()), line: 3, col: 7, end: None })
    );
    assert_eq!(
        satex::cmd::parse_place("part-a.tex:45:1-48"),
        Ok(At { file: Some("part-a.tex".into()), line: 45, col: 1, end: Some((48, u32::MAX)) })
    );
    assert_eq!(satex::cmd::parse_place("4:2-5:3").map(|at| at.end), Ok(Some((5, 3))));
    assert!(satex::cmd::parse_place("3").is_err());
    assert!(satex::cmd::parse_place("part.tex:x:7").is_err());
}

#[test]
fn a_forward_slice_from_a_position_reaches_the_uses() {
    let analysis = analyze(&format!("{PRELUDE}\\def\\helper{{H}}\n\\def\\caller{{\\helper}}\n\\caller\n"));
    // The prelude shares line 1 with `\def\helper`, so the position names the
    // column right after it (tex.web § 232's catcodes stay off line 1).
    let forward = marks(&slice_at(
        &analysis,
        &[],
        Some(At::here(1, PRELUDE.len() as u32 + 1)),
        Direction::Forward,
    ));
    assert_holds(&forward, &["\\helper@1", "\\caller@2", "\\caller@3"], &[]);
}

#[test]
fn a_name_and_a_position_are_sliced_together() {
    let analysis = analyze(&format!("{PRELUDE}\\def\\a{{A}}\n\\def\\b{{B}}\n\\a\n\\b\n"));
    let found =
        marks(&slice_at(&analysis, &["\\a"], Some(At::here(4, 1)), Direction::Backward));
    assert_holds(&found, &["\\a@1", "\\a@3", "\\b@2", "\\b@4"], &[]);
}

// --- reconstruction ----------------------------------------------------

/// Brace depth over a whole text, with `\x` escaping the next character and
/// `%` commenting out the rest of the line.  A reconstruction that ends at
/// depth zero and never goes below it is a group-balanced fragment.
fn brace_balance(text: &str) -> i32 {
    let mut depth = 0;
    for line in text.lines() {
        let mut chars = line.chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => {
                    chars.next();
                }
                '%' => break,
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
            assert!(depth >= 0, "a closing brace with nothing open in {text:?}");
        }
    }
    depth
}

fn reconstruction(analysis: &Analysis, source: &str, names: &[&str], at: Option<At>) -> String {
    let records = slice_at(analysis, names, at, Direction::Backward);
    query::reconstruct(analysis, &records, source)
}

#[test]
fn a_reconstruction_keeps_a_multi_line_definition_whole() {
    let source = "\\newcommand{\\multi}{%\n  one\n  two\n}\n\\multi\n";
    let analysis = analyze_kernel(source);
    let text = reconstruction(&analysis, source, &["\\multi"], None);
    assert_eq!(brace_balance(&text), 0, "unbalanced reconstruction:\n{text}");
    for line in ["\\newcommand{\\multi}{%", "  one", "  two", "}"] {
        assert!(text.contains(line), "{line:?} is missing from\n{text}");
    }
}

#[test]
fn a_reconstruction_is_balanced_for_every_shape_of_definition() {
    let cases: &[(&str, &str, &str)] = &[
        ("one line", "\\def\\a{A}\n\\a\n", "\\a"),
        ("nested braces", "\\def\\a{{A}{B}}\n\\a\n", "\\a"),
        (
            "body over lines",
            "\\newcommand{\\a}{%\n  {A}\n  {B}\n}\n\\a\n",
            "\\a",
        ),
        (
            "argument over lines",
            "\\newenvironment{env}\n  {before}\n  {after}\n\\begin{env}x\\end{env}\n",
            "env",
        ),
        (
            "group over lines",
            "\\def\\a{A}\n{%\n  \\def\\a{B}\n}\n\\a\n",
            "\\a",
        ),
        ("escaped braces", "\\def\\a{\\{A\\}}\n\\a\n", "\\a"),
    ];
    for (what, source, name) in cases {
        let analysis = analyze(source);
        let text = reconstruction(&analysis, source, &[name], None);
        assert_eq!(brace_balance(&text), 0, "{what}: unbalanced\n{text}");
    }
}

#[test]
fn a_reconstruction_gives_the_name_the_same_meaning() {
    let cases: &[(&str, &str)] = &[
        ("\\def\\foo{first}\n\\def\\foo{second}\n\\foo\n", "foo"),
        ("\\newcommand{\\foo}{first}\n\\renewcommand{\\foo}{second}\n\\foo\n", "foo"),
        ("\\def\\base{B}\n\\let\\alias\\base\n\\alias\n", "alias"),
        ("\\def\\g{outer}\n{\\def\\g{inner}}\n\\g\n", "g"),
        ("\\newcommand{\\multi}{%\n  one\n  two\n}\n\\multi\n", "multi"),
        ("\\def\\inner{I}\n\\def\\outer{\\inner}\n\\outer\n", "outer"),
    ];
    for (source, name) in cases {
        let analysis = analyze_kernel(source);
        let text = reconstruction(&analysis, source, &[&format!("\\{name}")], None);
        let again = analyze_kernel(&text);
        assert_eq!(
            meaning(&analysis, name),
            meaning(&again, name),
            "the reconstruction changes what \\{name} means:\n{text}"
        );
    }
}

/// What `satex explain` says the name finally means: its body and the call
/// shape it accepts.
fn meaning(analysis: &Analysis, name: &str) -> (String, String) {
    let record = query::explain(analysis, &[name.to_string()], false)
        .unwrap_or_else(|e| panic!("\\{name} did not explain: {e}"))
        .0
        .into_iter()
        .next_back()
        .unwrap_or_else(|| panic!("\\{name} did not explain"));
    let field = |key: &str| {
        record.get(key).and_then(serde_json::Value::as_str).unwrap_or_default().to_string()
    };
    (field("body"), field("effective"))
}

#[test]
fn a_reconstruction_puts_an_input_file_before_the_lines_that_use_it() {
    let directory = scratch("input-order");
    std::fs::write(directory.join("part.tex"), "\\def\\shared{S}\n").unwrap();
    let main = directory.join("main.tex");
    // The prelude runs before `\input`, so its catcodes reach part.tex too.
    let source = format!("{PRELUDE}\\input{{part}}\n\\shared\n");
    std::fs::write(&main, &source).unwrap();
    let cfg = Config { load_inputs: true, ..bare() };
    let analysis = Machine::analyze(&source, Some(&main), &cfg);
    let records = slice_at(&analysis, &["\\shared"], None, Direction::Backward);
    let text = query::reconstruct(&analysis, &records, &source);
    let definition = text.find("\\def\\shared").expect("the definition is kept");
    let use_site = text.find("\n\\shared").expect("the use is kept");
    assert!(definition < use_site, "the definition has to come first:\n{text}");
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn a_slice_reaches_into_an_input_file() {
    let directory = scratch("input-reach");
    std::fs::write(directory.join("part.tex"), "\\def\\shared{S}\n").unwrap();
    let main = directory.join("main.tex");
    // The prelude runs before `\input`, so its catcodes reach part.tex too.
    let source = format!("{PRELUDE}\\input{{part}}\n\\shared\n");
    std::fs::write(&main, &source).unwrap();
    let cfg = Config { load_inputs: true, ..bare() };
    let analysis = Machine::analyze(&source, Some(&main), &cfg);
    let found = at(&analysis, 2, 1);
    assert_holds(&found, &["\\shared@1", "\\shared@2"], &[]);
    let _ = std::fs::remove_dir_all(&directory);
}

// --- the real engine ---------------------------------------------------

fn pdflatex() -> Option<PathBuf> {
    let which = Command::new("kpsewhich").arg("--var-value=SELFAUTOLOC").output().ok()?;
    let directory = PathBuf::from(String::from_utf8(which.stdout).ok()?.trim());
    let binary = directory.join("pdflatex");
    binary.is_file().then_some(binary)
}

/// What pdflatex reports for `\meaning\foo`, taken from the log.
fn engine_meaning(binary: &Path, directory: &Path, name: &str, source: &str) -> String {
    let file = directory.join(format!("{name}.tex"));
    std::fs::write(&file, source).expect("writing the document");
    let output = Command::new(binary)
        .current_dir(directory)
        .args(["-interaction=nonstopmode", "-halt-on-error", "-no-shell-escape"])
        .arg(&file)
        .output()
        .expect("running the engine");
    let log = String::from_utf8_lossy(&output.stdout).replace('\n', "");
    let start = log.find("[SATEX:").unwrap_or_else(|| panic!("no probe in the log of {name}:\n{log}"));
    let rest = &log[start + "[SATEX:".len()..];
    rest[..rest.find(']').expect("a closed probe")].to_string()
}

#[test]
fn pdflatex_agrees_that_the_reconstruction_preserves_the_meaning() {
    if !installed() {
        return;
    }
    let Some(binary) = pdflatex() else { return };
    let directory = scratch("engine");
    // The reconstruction is a document of its own, so the probe goes in
    // before its `\end{document}` just as it does in the original.
    let body = concat!(
        "\\newcommand{\\foo}{first}\n",
        "\\renewcommand{\\foo}{%\n",
        "  second\n",
        "}\n",
        "\\foo\n",
    );
    let source = format!("\\documentclass{{article}}\n\\begin{{document}}\n{body}\\end{{document}}\n");
    let cfg = Config { load_classes: true, ..Config::default() };
    let analysis = Machine::analyze(&source, None, &cfg);
    let records = slice_at(&analysis, &["\\foo"], None, Direction::Backward);
    let text = query::reconstruct(&analysis, &records, &source);
    assert_eq!(brace_balance(&text), 0, "unbalanced reconstruction:\n{text}");
    let probe = "\\message{[SATEX:\\meaning\\foo]}\n\\end{document}";
    let wrap = |document: &str| document.replacen("\\end{document}", probe, 1);
    let original = engine_meaning(&binary, &directory, "original", &wrap(&source));
    let sliced = engine_meaning(&binary, &directory, "sliced", &wrap(&text));
    assert_eq!(original, sliced, "the reconstruction changes \\foo:\n{text}");
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn a_macro_from_a_package_is_sliced_back_to_the_package() {
    if !installed() {
        return;
    }
    let directory = scratch("package");
    std::fs::write(
        directory.join("slicepkg.sty"),
        "\\ProvidesPackage{slicepkg}\n\\newcommand{\\frompackage}{P}\n",
    )
    .unwrap();
    let main = directory.join("main.tex");
    let source = concat!(
        "\\documentclass{article}\n",
        "\\usepackage{slicepkg}\n",
        "\\begin{document}\n",
        "\\frompackage\n",
        "\\end{document}\n",
    );
    std::fs::write(&main, source).unwrap();
    let cfg = Config::discover(&directory);
    let analysis = Machine::analyze(source, Some(&main), &cfg);
    let records = slice_at(&analysis, &["\\frompackage"], None, Direction::Backward);
    assert!(
        records.iter().any(|record| record["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("slicepkg.sty"))
            && record["line"].as_u64() == Some(2)),
        "the package's definition belongs in the slice: {:?}",
        marks(&records)
    );
    let text = query::reconstruct(&analysis, &records, source);
    assert!(
        text.contains("\\usepackage{slicepkg}") && !text.contains("\\newcommand{\\frompackage}"),
        "the reconstruction loads the package rather than copying lines out of it:\n{text}"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn place_range_start_column_optional() {
    let at = satex::cmd::parse_place("45-48").unwrap();
    assert_eq!((at.line, at.col, at.end), (45, 1, Some((48, u32::MAX))));
    let at = satex::cmd::parse_place("p.tex:45-48").unwrap();
    assert_eq!((at.file.as_deref(), at.line, at.col), (Some("p.tex"), 45, 1));
}
