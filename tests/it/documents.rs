//! Real documents, cut down: each must reach its `\end{document}`, read every
//! file it inputs, and stay inside a step budget. Failures are the to-do list.

use std::path::PathBuf;

use crate::support::{defined, installed};
use satex::builtins::OccKind;
use satex::facts::Severity;
use satex::machine::{Analysis, Machine};

/// Steps a fixture may take.
const STEP_BUDGET: u64 = 20_000_000;

/// Diagnostics that mean the run was cut short.
const CUT_CODES: [&str; 3] = ["limit-reached", "budget-exhausted", "analysis-ended-early"];

fn dir(id: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/documents").join(id)
}

/// Analyzes `<id>/<main>`, then checks the shared expectations. `also` names
/// files besides the `\input`s that must have been read.
fn check(id: &str, main: &str, also: &[&str]) -> Option<Analysis> {
    check_allowing(id, main, also, &[])
}

/// `check`, where the diagnostics coded `allowed` are what the fixture
/// should raise, not failures.
fn check_allowing(id: &str, main: &str, also: &[&str], allowed: &[&str]) -> Option<Analysis> {
    if !installed() {
        return None;
    }
    let root = dir(id);
    let path = root.join(main);
    let source = std::fs::read_to_string(&path).expect("fixture source");
    let mut cfg = crate::support::discover(&root);
    cfg.cache.dir = Some(crate::support::scratch_dir(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/scratch").join(format!("documents-{id}")),
    ));
    crate::support::seed_kernel(&cfg);
    let analysis = Machine::analyze(&source, Some(&path), &cfg);
    let mut failures = Vec::new();
    if !defined(&analysis, "SatexReachedEnd") {
        failures.push("\\SatexReachedEnd is undefined: the end was not reached".to_string());
    }
    for name in afters(&source) {
        if !defined(&analysis, &name) {
            failures.push(format!("\\{name} is undefined: the text after a construct was lost"));
        }
    }
    for key in labels(&source) {
        if !analysis.facts.occurrences.iter().any(|o| o.kind == OccKind::Label && o.key == key) {
            failures.push(format!("label {key} was not observed"));
        }
    }
    for name in inputs(&source).iter().map(String::as_str).chain(also.iter().copied()) {
        if !read(&analysis, name) {
            failures.push(format!("{name} is not among the analysis's files"));
        }
    }
    for d in &analysis.facts.diagnostics {
        if CUT_CODES.contains(&d.code) {
            failures.push(format!("diagnostic {}: {}", d.code, d.message));
        }
    }
    if analysis.untyped > 0 {
        failures.push(format!("{} untyped unknown texts made; every unknown is typed", analysis.untyped));
    }
    let steps = analysis.steps - analysis.load_steps;
    eprintln!("{id}/{main}: {steps} analysis steps, {} loading", analysis.load_steps);
    if steps >= STEP_BUDGET {
        failures.push(format!("{steps} analysis steps ({} loading), budget {STEP_BUDGET}", analysis.load_steps));
    }
    let own = |s: &satex::tex::Span| {
        analysis.files.get(s.file as usize).is_some_and(|f| std::path::Path::new(&f.path).starts_with(&root))
    };
    let bad: Vec<_> = analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| !matches!(d.severity, Severity::Info | Severity::Imprecision | Severity::Assumption))
        .filter(|d| !allowed.contains(&d.code))
        .filter(|d| own(&d.span) || d.origin.as_ref().is_some_and(own))
        .collect();
    for d in bad.iter().take(10) {
        let f = analysis.files.get(d.span.file as usize).map_or("?", |f| f.short.as_str());
        failures.push(format!(
            "{} {} at {f}:{}:{}: {}",
            d.severity.as_str(),
            d.code,
            d.span.line,
            d.span.col,
            d.message
        ));
    }
    assert!(failures.is_empty(), "{id}:\n{}", failures.join("\n"));
    Some(analysis)
}

/// How often `code` was diagnosed.
fn count(analysis: &Analysis, code: &str) -> usize {
    analysis.facts.diagnostics.iter().filter(|d| d.code == code).count()
}

/// The files named by `\input{..}`, `\include{..}` and `\subfile{..}` in `source`.
fn inputs(source: &str) -> Vec<String> {
    let lines = || source.lines().filter(|l| !l.trim_start().starts_with('%'));
    ["\\input{", "\\include{", "\\subfile{"]
        .iter()
        .flat_map(|cmd| lines().flat_map(move |l| l.split(cmd).skip(1).filter_map(|r| r.split('}').next())))
        .map(str::to_string)
        .collect()
}

/// The `\After..` macros `source` defines with `\gdef`: each marks the
/// text right after a construct.
fn afters(source: &str) -> Vec<String> {
    source
        .split("\\gdef\\")
        .skip(1)
        .filter_map(|r| r.split('{').next())
        .filter(|n| n.starts_with("After"))
        .map(str::to_string)
        .collect()
}

/// The keys of the `\label`s in `source`.
fn labels(source: &str) -> Vec<String> {
    source.split("\\label{").skip(1).filter_map(|r| r.split('}').next()).map(str::to_string).collect()
}

fn read(analysis: &Analysis, name: &str) -> bool {
    let name = if name.contains('.') { name.to_string() } else { format!("{name}.tex") };
    analysis.files.iter().any(|f| f.path.ends_with(&name))
}

/// One `#[test]` per fixture: `name(id, main, files also read…)`.
macro_rules! fixtures {
    ($($(#[$doc:meta])* $name:ident($id:literal, $main:literal $(, $also:literal)*);)*) => {$(
        $(#[$doc])*
        #[test]
        fn $name() {
            check($id, $main, &[$($also),*]);
        }
    )*};
}

/// Like [`fixtures!`], and the run keeps to three paths at most.
macro_rules! peak_fixtures {
    ($($(#[$doc:meta])* $name:ident($id:literal, $main:literal);)*) => {$(
        $(#[$doc])*
        #[test]
        fn $name() {
            let Some(a) = check($id, $main, &[]) else { return };
            assert!(a.peak_paths <= 3, "{} paths", a.peak_paths);
        }
    )*};
}

fixtures! {
    tikz_figure_with_math_nodes_t3("tikz_t3", "main.tex");
    tikz_figure_with_fit_node_in_resizebox_b("tikz_fit_b", "main.tex");
    tikz_picture_in_resizebox_v2("tikz_resizebox_v2", "main.tex");
    includegraphics_of_pdf_banner("includegraphics", "main.tex", "title-banner-image.pdf");
    tikz_notebook_environment_from_input_file("notebook", "main.tex");
    expl3_hex_string_to_utf16_conversion_h4("expl3_hex_h4", "main.tex");
    expl3_hex_string_with_empty_target_encoding_h6("expl3_hex_h6", "main.tex");
    notecolumn_loop_over_slnc_file("thesis_slnc", "mwe.tex", "mwe.slnc");
    /// A math label `$A_1$` sizes a node exactly; the frame around the
    /// bounding box then parses known numbers.
    tikz_frame_around_math_label("tikz_math_label", "main.tex");
    thesis_class_with_notebook_figure_a4("thesis_notebook_a4", "main.tex");
    tikz_custom_shape_and_saveboxes_ma("tikz_shapes_ma", "mwe.tex");
    tikz_styles_forest_and_dataflow_savebox("forest_savebox", "mwe.tex");
    tcolorbox_with_tikz_overlay_fse("tcolorbox_fse", "mwe.tex");
    /// A breakable box at the top of the page: tcolorbox reads the page room
    /// (tcbbreakable.code.tex, `\tcb@comp@h@page`), and pgf draws the frame.
    tcolorbox_breakable_without_overlay("tcolorbox_breakable_plain", "mwe.tex");
    /// A `\ref` to a label not yet written, inside tikz nodes: `??` is exact text.
    tikz_node_with_a_ref("tikz_node_with_ref", "mwe.tex");
    /// An arrow tip toward a point with an unknown coordinate.
    tikz_arrow_to_an_unknown_point("tikz_arrow_to_unknown_point", "mwe.tex");
    /// An undefined name in a node's formula: the node's size must stay known.
    tikz_node_with_an_undefined_name_in_math("tikz_node_undefined_in_math", "mwe.tex");
    /// PNG images scaled to keep their aspect ratio inside beamer columns.
    beamer_raster_images_scaled_to_their_aspect("beamer_raster_images", "mwe.tex");
}

/// Capabilities: unknown inputs, typesetting feedback, package interpreters.
/// A footnote gives the box an unknown size, so pgfmath draws the frame on
/// approximated values and may misread a number. That call is summarized:
/// the path never reads on into the list's `\end{enumerate}`. The
/// misread itself is still said, as a gap (pgfmath decorrelation, open).
#[test]
fn tcolorbox_with_a_footnote_in_a_list_item_ends() {
    let found = check_allowing(
        "tcolorbox_footnote_in_list",
        "mwe.tex",
        &[],
        &["missing-number", "runaway-argument", "extra-right-brace"],
    );
    let Some(analysis) = found else { return };
    assert!(count(&analysis, "fault-contained") > 0);
}

/// Capabilities: unknown inputs, global state, typesetting feedback.
/// A breakable box of unknown height closes every group it opens, so a
/// node's saved anchors stay local (fse: later figures summarized on them).
#[test]
fn tcolorbox_of_unknown_height_keeps_its_anchors_local() {
    let Some(analysis) = check("tcolorbox_unknown_height_anchors", "mwe.tex", &[]) else { return };
    assert!(!defined(&analysis, "southwest"), "\\southwest leaked out of the box's groups");
}

fixtures! {
    tikz_bent_path_on_background_layer("tikz_bend_pingus", "mwe.tex");
    notecolumn_slnc_on_master_thesis_title_page("notecolumn_slnc_ma", "mwe.tex", "mwe.slnc");
    notecolumn_slnc_at_cleardoublepage_koma("notecolumn_slnc_koma", "mwe.tex", "mwe.slnc");
    input_path_extended_with_etoolbox_appto("input_path_appto", "main.tex");
    resizebox_tikz_scopes_keep_font_known("fontenc_after_preamble", "main.tex");
    notecolumn_read_loop_at_frontmatter_clearpage("notecolumn_read_loop", "mwe.tex", "mwe.slnc");
    notecolumn_clear_with_unknown_page_height("notecolumn_unknown_height", "mwe.tex");
}

// Gobblers must stop at their real end: text after them is never lost.

fixtures! {
    gobble_listings_stop_at_their_delimiter("gobble_listings", "main.tex", "code.c");
    gobble_comment_environments_stop_at_their_end("gobble_comment", "main.tex");
    gobble_environ_body_stops_at_its_end("gobble_environ", "main.tex");
    gobble_verbatim_stops_at_its_delimiter("gobble_verbatim", "main.tex");
    gobble_with_unknown_catcodes_and_comments_never_reads_past_the_end("gobble_unknown", "main.tex");
    gobble_verbatim_of_unknown_environment_name_stops_at_its_end("gobble_unknown_name", "main.tex");
    gobble_lstinline_with_unknown_delimiter_stops_at_its_end("gobble_unknown_delim", "main.tex");
}

/// A missing `\lstinputlisting` file asks the terminal for a name, which
/// ends a nonstop run (tex.web § 484): the text before it is read, and
/// the end is said.
#[test]
fn gobble_lstinputlisting_of_missing_file_halts_the_run() {
    if !installed() {
        return;
    }
    let root = dir("gobble_listings_missing");
    let path = root.join("main.tex");
    let source = std::fs::read_to_string(&path).expect("fixture source");
    let cfg = crate::support::discover(&root);
    let analysis = Machine::analyze(&source, Some(&path), &cfg);
    assert!(defined(&analysis, "BeforeMissing"));
    assert!(analysis.facts.diagnostics.iter().any(|d| d.code == "terminal-read-fatal"));
}

fixtures! {
    pdfcomment_markup_in_math_with_unknown_positions("pdfcomment_math", "main.tex");
    pdfcomment_markup_in_math_with_aux_positions("pdfcomment_math_aux", "main.tex");
    pgfmath_seed_from_unknown_random_seed("pgfmath_random_seed", "mwe.tex");
    pgfmath_operator_on_unknown_integer_and_rounded_strut_node("pgfmath_unknown_operand", "mwe.tex");
}

/// xinttrig's `\xintdeffloatfunc @Arg` nests expansion about 80 levels
/// deep, far inside TeX's limits: no nesting may be given up.
#[test]
fn xintexpr_loads_without_giving_up_nesting() {
    check("xintexpr_deep_nesting", "main.tex", &[]);
    if !installed() {
        return;
    }
    let root = dir("xintexpr_deep_nesting");
    let path = root.join("main.tex");
    let source = std::fs::read_to_string(&path).expect("fixture source");
    let cfg = crate::support::discover(&root);
    let analysis = Machine::analyze(&source, Some(&path), &cfg);
    let widened: Vec<_> = analysis.facts.diagnostics.iter().filter(|d| d.code == "nesting-widened").collect();
    assert!(widened.is_empty(), "{}", widened.iter().map(|d| d.message.as_str()).collect::<Vec<_>>().join("\n"));
}

fixtures! {
    /// A node with `text width`, `align=center` and rounded corners: its text
    /// box has a known size, so the corners are computed, not forked over.
    tikz_centered_text_width_node_with_rounded_corners("tikz_centered_text_width_node", "main.tex");
    /// A bent path to a signal node holding centered `~~~~`: a known size.
    tikz_bend_to_signal_node_with_tildes("tikz_bend_to_signal_node", "main.tex");
}

// Termination: every loop point is summarized at most once per entry cap,
// live paths stay within `limits.paths`, and nothing ends early.

#[test]
fn loop_with_unknown_bound_is_summarized() {
    let Some(a) = check("loop_count_random", "main.tex", &[]) else { return };
    assert_eq!(count(&a, "unbalanced-path"), 0);
}

peak_fixtures! {
    mutual_recursion_with_unknown_bound_is_summarized_once("loop_mutual_random", "main.tex");
}

#[test]
fn csname_of_unknown_text_keeps_its_endcsname() {
    let Some(a) = check("csname_unknown_mt", "main.tex", &[]) else { return };
    assert_eq!(count(&a, "filename-missing-endcsname"), 0);
}

#[test]
fn map_over_unknown_token_list_is_summarized_once() {
    let Some(a) = check("expl_map_unknown_text", "main.tex", &[]) else { return };
    let strays: Vec<_> = a
        .facts
        .diagnostics
        .iter()
        .filter(|d| d.message.contains("\\fi") || d.message.contains("\\endcsname"))
        .filter(|d| !matches!(d.severity, Severity::Info | Severity::Imprecision | Severity::Assumption))
        .map(|d| d.message.clone())
        .collect();
    assert!(strays.is_empty(), "{strays:?}");
}

peak_fixtures! {
    pgfmath_of_unknown_integer_stays_a_number("pgfmath_unknown_seed", "main.tex");
}

#[test]
fn repeated_undecided_ifx_stays_within_the_path_limit() {
    let Some(a) = check("crowd_ifx_unknown", "main.tex", &[]) else { return };
    assert!(a.peak_paths <= usize::from(a.settings.limits.paths), "{} paths", a.peak_paths);
    assert_eq!(count(&a, "undecided-loop"), 0);
}

#[test]
fn rule_of_unknown_height_before_clearpage() {
    let Some(a) = check("vrule_unknown_height", "main.tex", &[]) else { return };
    assert!(count(&a, "recursion-widened") <= 1, "{}", count(&a, "recursion-widened"));
}

/// `\catcode<unknown>=11` from a summarized loop may reach `\ { }`: the
/// reader splits there, and on the path where they kept their categories
/// the `\include` still reads its file. Where `}` became a letter, its
/// argument never closes. Both `\u` and `\z` loop forever in TeX
/// (pdflatex never gets past them), so each is reported as an
/// `endless-loop` before it is summarized.
#[test]
fn include_after_unknown_char_catcode_loop_is_read() {
    check_allowing("catcode_loop_include", "main.tex", &["ch1"], &["endless-loop", "runaway-argument"]);
}

/// A recursion cut inside a group that set `\escapechar` before it began:
/// only what the recursion wrote is widened, so `\escapechar` stays -1.
#[test]
fn cut_widens_only_what_the_recursion_wrote() {
    if !installed() {
        return;
    }
    let path = dir("cut_keeps_earlier_writes").join("mwe.tex");
    let source = std::fs::read_to_string(&path).expect("fixture source");
    let a = Machine::analyze(&source, Some(&path), &crate::support::discover(&dir("cut_keeps_earlier_writes")));
    assert!(defined(&a, "SatexReachedEnd") && !defined(&a, "EscapeWidened"));
}

/// `\def\z{\z}\z` loops forever in TeX (tex.web § 390), so it is
/// reported as an `endless-loop`, summarized, and the `\include` still
/// reads its file.
#[test]
fn include_after_endless_macro_is_read() {
    check_allowing("include_after_loop", "main.tex", &["ch1"], &["endless-loop"]);
}

/// A loop summarized inside `\begingroup\let\protect\string ... \endgroup`:
/// the group closes, `\protect` is back, and the `\include` reads its file.
#[test]
fn group_around_summarized_loop_closes_before_include() {
    let Some(a) = check("group_cut_include", "main.tex", &["ch1"]) else { return };
    assert!(!defined(&a, "ProtectLeaked"));
}

/// A recursion cut while its levels stand in groups opened and
/// closed through macros: the cut closes exactly the groups its dropped
/// levels opened, by identity, so the code after it runs at its own depth.
#[test]
fn cut_recursion_closes_macro_groups() {
    if !installed() {
        return;
    }
    let path = dir("loop_macro_groups").join("mwe.tex");
    let source = std::fs::read_to_string(&path).expect("fixture source");
    let a = Machine::analyze(&source, Some(&path), &crate::support::discover(&dir("loop_macro_groups")));
    assert!(defined(&a, "SatexReachedEnd") && !defined(&a, "Leaked"));
}

/// A branch met once shallow and then deeper in one call is no loop pass:
/// no group its caller opened is unwound, so no group closer goes astray.
#[test]
fn loop_entry_met_deeper_keeps_caller_groups() {
    let Some(a) = check("loop_entry_deeper_groups", "mwe.tex", &[]) else { return };
    assert_eq!((count(&a, "endless-loop"), count(&a, "mismatched-group")), (0, 0));
}

fixtures! {
    local_package_after_rounded_node_in_box_is_read("input_after_rounded_node", "mwe.tex", "zzlocal.sty");
    /// Capabilities: typesetting feedback, unknown inputs, termination.
    /// A rounded node around a broken paragraph: no limit, no fork explosion.
    rounded_node_around_a_broken_paragraph_ends("rounded_node_broken_paragraph", "mwe.tex");
    /// pgfmath on a node of unknown width: summarized, never exploding.
    rounded_node_of_unknown_width_ends("rounded_node_unknown_width", "mwe.tex");
    /// Capabilities: termination, engine.
    /// A loop over a data file read with `\read`: each data line has its own
    /// step budget, so cheap lines that add up past one budget are never cut.
    read_loop_has_a_budget_per_data_line("read_loop_line_budget", "mwe.tex");
}

/// A project file that exists is either read or said not to be.
#[test]
fn include_is_read_or_said_unread() {
    if !installed() {
        return;
    }
    let root = dir("include_never_opened");
    let path = root.join("main.tex");
    let source = std::fs::read_to_string(&path).expect("fixture source");
    let cfg = crate::support::discover(&root);
    let analysis = Machine::analyze(&source, Some(&path), &cfg);
    let unread = count(&analysis, "file-not-read");
    assert!(read(&analysis, "ch2") != (unread > 0), "read {} unread {unread}", read(&analysis, "ch2"));
}

fixtures! {
    /// A two-line `align=center` node with rounded corners: its \halign box
    /// must have a known size (tex.web §§ 785-812).
    tikz_halign_node_with_rounded_corners("tikz_halign_node_rounded", "main.tex");
}

/// Twenty nodes of unknown height in a `\foreach`: calls met again with
/// unknown operands are summarized, at most once each.
#[test]
fn tikz_loop_of_unknown_height_nodes_converges() {
    let start = std::time::Instant::now();
    let Some(a) = check("call_summary_tikz_loop", "main.tex", &[]) else { return };
    assert!(start.elapsed().as_secs() < 5, "{:?}", start.elapsed());
    assert!(count(&a, "call-summarized") <= 20, "{}", count(&a, "call-summarized"));
}

/// Nodes of a set text width whose text is inline math, in a picture saved
/// in a box: the paragraphs keep their height (tex.web §§ 866, 1196), so
/// no anchor of the picture is unknown and no path parts on one.
#[test]
fn inline_math_nodes_in_a_saved_box_stay_known() {
    let Some(a) = check("inline_math_nodes_in_a_saved_box", "main.tex", &[]) else { return };
    assert_eq!(a.peak_paths, 1, "paths parted");
}

/// The same with sub- and superscripts, nested and in pairs: the formulas
/// are rigid, so their paragraphs keep their height (tex.web §§ 756-759).
#[test]
fn inline_math_scripts_in_a_saved_box_stay_known() {
    let Some(a) = check("inline_math_scripts_in_a_saved_box", "main.tex", &[]) else { return };
    assert_eq!(a.peak_paths, 1, "paths parted");
}

/// A picture in a `center` environment in a parbox, saved in a box: the
/// paragraph hooks put the indent box back, `\item` takes it with
/// `\lastbox` (tex.web § 1080), and the box's size is exact, as `pdflatex`
/// prints it, so the scaled node showing it parses known numbers.
#[test]
fn centered_picture_in_a_saved_parbox_has_its_exact_size() {
    let Some(a) = check("centered_picture_in_a_saved_parbox", "main.tex", &[]) else { return };
    assert_eq!(crate::support::body(&a, "SketchSize"), "72.45232pt/43.58429pt/38.58427pt");
    assert_eq!(a.peak_paths, 1, "paths parted");
}

peak_fixtures! {
    /// `\pgfmathparse` of an unknown integer three times: a typed result.
    pgfmath_of_unknown_integer_three_times("call_summary_pgfmath_thrice", "main.tex");
}

fixtures! {
    /// Notebook-like cells of `obeylines` text with inline code in nodes.
    notebook_cells_with_inline_code_end("call_summary_cells", "main.tex");
}

peak_fixtures! {
    /// An inline listing with a two-character literate, through a macro
    /// argument, whose break mark and literate load math fonts at a new size:
    /// its lost space stays exact, so no conditional on it parts paths.
    inline_listing_with_two_char_literate_converges("inline_listing_two_char_literate", "main.tex");
}

fixtures! {
    /// A quotation in a small-font node with a frame fitted around it: its
    /// size stays known (tex.web §§ 1034-1036, 1375).
    tikz_fit_around_quotation_node("tikz_fit_enquote_small_node", "main.tex");
    /// A rounded node of finite-stretch centred lines that may break more than
    /// one way: pgfmath runs on its uncertain height and must not explode.
    tikz_rounded_node_of_uncertain_lines("tikz_rounded_node_uncertain_lines", "main.tex");
    /// `\date{\today}` in the body with hyperref and siunitx raises no error:
    /// satex's own rendering of the date is no part of the run.
    date_today_with_siunitx_units("date_today_siunitx", "main.tex");
}

/// Rounding never stalls: pgfmath's `round`, `floor` and `int`, and
/// `\pgfprocessround` on a rounded-corner path, over a dimension of any
/// sign and over one in an interval. Each costs at most a million steps
/// over the same preamble, keeps at most 3 paths, and a bounded operand
/// gives a bounded result (`\the` of a dimension in an interval keeps it
/// through text, tex.web §§ 103, 448).
#[test]
fn rounding_an_unknown_dimension_ends_bounded() {
    let Some(control) = check("round_unknown_dimension", "control.tex", &[]) else { return };
    for main in ["math_unknown.tex", "math_interval.tex", "round_unknown.tex", "round_interval.tex"] {
        let a = check("round_unknown_dimension", main, &[]).expect("analysis");
        assert!(a.peak_paths <= 3, "{main}: {} paths", a.peak_paths);
        assert!(a.steps <= control.steps + 1_000_000, "{main}: {} steps over {}", a.steps, control.steps);
        assert!(a.limits.is_empty(), "{main}: limits hit");
        for name in ["NegativeRound", "NegativeFloor", "NegativeInt", "HugeRound"] {
            assert!(!defined(&a, name), "{main}: \\{name} is defined: the result is not bounded");
        }
    }
}

/// Capabilities: typesetting feedback, global state, termination.
/// `\clearnotecolumns` loops over `\clearpage` while a note may be left:
/// the paths must not grow with the number of notes in the `.slnc` file.
#[test]
fn notecolumn_clear_loop_paths_do_not_grow_with_notes() {
    let few = check("notecolumn_clear_paths", "few.tex", &[]);
    let many = check("notecolumn_clear_paths", "many.tex", &[]);
    let (Some(few), Some(many)) = (few, many) else { return };
    assert!(
        many.peak_paths <= few.peak_paths + 2,
        "peak paths {} with 60 notes, {} with 6",
        many.peak_paths,
        few.peak_paths
    );
}

/// ba: a float with a minipage caption in one `\include`d part, hyperref
/// loaded: the output routine the part's `\clearpage` runs places the float,
/// and the next part is still read (pdflatex reads both).
#[test]
fn include_after_a_float_with_hyperref_reads_the_next_part() {
    let Some(a) = check("include_after_float_with_hyperref", "main.tex", &[]) else { return };
    assert!(crate::support::defined(&a, "ReadB"), "the second part was not read");
}

/// fse: a short breakable tcolorbox; pdflatex sets it on one page with no
/// error, and so must the analysis, with no page break forced in a loop.
#[test]
fn a_short_breakable_tcolorbox_ends_cleanly() {
    check("breakable_tcolorbox_short_text", "main.tex", &[]);
}

/// fse: a node whose minimum height is not known. A summary of the parse
/// of that height must not stand for later parses of known lengths.
#[test]
fn a_node_of_unknown_minimum_height_parses_its_other_lengths() {
    check("tikz_node_unknown_minimum_height", "main.tex", &[]);
}
