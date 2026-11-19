//! `satex compile`: the build's errors and warnings, each where TeX says it
//! arose and explained by satex's finding there.

#[test]
fn the_build_log_is_explained_where_it_arose() {
    if !crate::support::installed() || satex::project::paths::which("pdflatex").is_err() {
        return;
    }
    let dir = crate::support::tmp_root().join(format!("satex-compile-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("doc.tex"),
        "\\documentclass{article}\n\\begin{document}\nSee \\ref{nope}.\n\\input{part}\n\\end{document}\n",
    )
    .unwrap();
    std::fs::write(dir.join("part.tex"), "Some \\foo text.\n").unwrap();
    let out = crate::support::satex()
        .args(["compile", "-f", "doc.tex", "--format", "json", "--set", "limits.seconds=0"])
        .current_dir(&dir)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .unwrap();
    let rows: Vec<serde_json::Value> =
        serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stderr)));
    let find = |code: &str| rows.iter().find(|r| r["code"] == code).unwrap_or_else(|| panic!("no {code}: {rows:?}"));
    let undefined = find("undefined-control-sequence");
    assert_eq!((undefined["file"].as_str(), undefined["line"].as_u64()), (Some("part.tex"), Some(1)));
    assert!(undefined["fix"].as_str().is_some_and(|f| f.contains("\\foo")), "{undefined:?}");
    let reference = find("undefined-reference");
    assert_eq!((reference["file"].as_str(), reference["line"].as_u64()), (Some("doc.tex"), Some(3)));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A plain TeX file (`% !TeX program = tex`) is built with the plain format by pdfTeX, not
/// with latexmk's LaTeX: the playground's leapyear example and its slices.
#[test]
fn a_plain_tex_file_builds_with_pdftex() {
    if !crate::support::installed() || satex::project::paths::which("pdftex").is_err() {
        return;
    }
    let dir = crate::support::tmp_root().join(format!("satex-compile-plain-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("doc.tex"), "% !TeX program = tex\nHello.\n\\bye\n").unwrap();
    let out = crate::support::satex()
        .args(["compile", "-f", "doc.tex", "--format", "json", "--set", "limits.seconds=0"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&out.stderr);
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}: {said}"));
    assert!(rows.iter().all(|r| r["severity"] != "error"), "{rows:?}");
    // pdfTeX runs the plain format and writes the PDF a viewer shows
    assert!(said.contains("$ pdftex "), "not run by pdftex: {said}");
    assert!(dir.join("doc.pdf").is_file(), "no PDF: {said}");
    // satex's own build without an analysis reads the kernel from the source too
    let (_, said) = satex_build(&dir, "doc.tex");
    assert!(said.contains("$ pdftex ") && !said.contains("pdflatex"), "{said}");
    // a file that never ends is TeX's fatal error, said as one
    std::fs::write(dir.join("open.tex"), "% !TeX program = tex\nHello.\n").unwrap();
    let out = crate::support::satex()
        .args(["compile", "-f", "open.tex", "--format", "json", "--set", "limits.seconds=0"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap_or_default();
    assert!(rows.iter().any(|r| r["message"].as_str().is_some_and(|m| m.contains("no legal \\end found"))), "{rows:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Directives in the main file pick arara, as `arara` itself would read them:
/// `satex compile` runs it, not latexmk. A project that names a build system
/// satex does not know gets an error naming the ones it knows.
#[test]
fn arara_directives_pick_arara_and_unknown_systems_are_named() {
    if !crate::support::installed()
        || satex::project::paths::which("arara").is_err()
        || satex::project::paths::which("pdflatex").is_err()
    {
        return;
    }
    let dir = crate::support::scratch_dir(crate::support::tmp_root().join("satex-compile-arara"));
    std::fs::write(
        dir.join("doc.tex"),
        "% arara: pdflatex\n\\documentclass{article}\n\\begin{document}\nHi.\n\\end{document}\n",
    )
    .unwrap();
    let run = |extra: &[&str]| {
        crate::support::satex()
            .args(["compile", "-f", "doc.tex", "--set", "limits.seconds=0"])
            .args(extra)
            .current_dir(&dir)
            .output()
            .unwrap()
    };
    let out = run(&[]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("$ arara"), "{stderr}");
    assert!(dir.join("doc.pdf").is_file(), "{stderr}");
    let out = run(&["--system", "nonsense"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success() && stderr.contains("latexmk, arara, tectonic"), "{stderr}");
}

/// satex's own build (`--system satex`) in `dir`, explained by rules only: its findings and stderr.
fn satex_build(dir: &std::path::Path, main: &str) -> (Vec<serde_json::Value>, String) {
    let out = crate::support::satex()
        .args(["compile", "-f", main, "--format", "json", "--system", "satex"])
        .args(["--set", "limits.seconds=0", "--set", "compile.explain=false"])
        .current_dir(dir)
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&out.stderr).into_owned();
    (serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}: {said}")), said)
}

/// The text of a PDF, page by page (form feeds between pages).
fn pdf_text(pdf: &std::path::Path) -> String {
    let out = std::process::Command::new("pdftotext").arg(pdf).arg("-").output().unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// satex's own build reads the preamble from a format it dumped once, reruns only while the
/// build files move, and runs BibTeX and makeindex where their inputs changed. Its PDF must
/// equal latexmk's, also after an edit of the body and of a file the preamble reads (the
/// format is dumped again then). Pins: an incremental build that is faster but different.
#[test]
fn the_satex_build_equals_latexmk_across_edits() {
    let tools = ["pdflatex", "latexmk", "bibtex", "makeindex", "pdftotext"];
    if !crate::support::installed()
        || tools.iter().any(|t| satex::project::paths::which(t).is_err())
        || std::process::Command::new("kpsewhich")
            .arg("mylatexformat.ltx")
            .output()
            .map_or(true, |o| o.stdout.is_empty())
    {
        return;
    }
    let root = crate::support::scratch_dir(crate::support::tmp_root().join("satex-compile-own"));
    let (ours, theirs) = (root.join("ours"), root.join("theirs"));
    let doc = "\\documentclass{article}\n\\usepackage{amsmath}\n\\usepackage{makeidx}\n\\input{macros}\n\\makeindex\n\
               \\begin{document}\n\\section{One}\\label{one}\n\\greet{world}, see \\ref{two} and \\cite{knuth}\\index{tex}.\n\
               \\section{Two}\\label{two}\nBODY\n\\bibliographystyle{plain}\n\\bibliography{refs}\n\\printindex\n\\end{document}\n";
    let bib = "@book{knuth, author={Donald Knuth}, title={The TeXbook}, year={1984}, publisher={AW}}\n";
    let write = |dir: &std::path::Path, body: &str, greet: &str| {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("doc.tex"), doc.replace("BODY", body)).unwrap();
        std::fs::write(dir.join("macros.tex"), format!("\\newcommand{{\\greet}}[1]{{{greet}, #1!}}\n")).unwrap();
        std::fs::write(dir.join("refs.bib"), bib).unwrap();
    };
    let latexmk = |dir: &std::path::Path| {
        let _ = std::fs::remove_dir_all(dir.join("x"));
        let status = std::process::Command::new("latexmk")
            .args(["-pdf", "-interaction=nonstopmode", "-f", "-pv-", "-pvc-", "-gg", "doc.tex"])
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(dir.join("doc.pdf").is_file(), "{}", String::from_utf8_lossy(&status.stdout));
        pdf_text(&dir.join("doc.pdf"))
    };
    for (step, (body, greet)) in
        [("First text.", "Hello"), ("Edited text.", "Hello"), ("Edited text.", "Howdy")].into_iter().enumerate()
    {
        write(&ours, body, greet);
        write(&theirs, body, greet);
        let (rows, said) = satex_build(&ours, "doc.tex");
        assert!(rows.iter().all(|r| r["severity"] != "error"), "{rows:?}\n{said}");
        assert!(said.contains("-fmt="), "the preamble is not read from a format: {said}");
        assert_eq!(pdf_text(&ours.join("doc.pdf")), latexmk(&theirs), "step {step}: {said}");
        let dumps = said.matches("$ pdftex -ini").count();
        // dumped at the start and after the preamble's file changed, reused for a body edit
        assert_eq!(dumps, usize::from(step != 1), "step {step}: {said}");
        if step == 1 {
            assert_eq!(said.matches("$ pdflatex").count(), 1, "a body edit is one pass: {said}");
        }
    }
}

/// An error and a warning in the preamble are reported though the preamble comes from a
/// format: the error keeps the format from being used, the warning is read from the dump's
/// log, also in a later build that reuses the format. Pins: messages lost to the format.
#[test]
fn the_satex_build_reports_what_the_preamble_says() {
    if !crate::support::installed()
        || satex::project::paths::which("pdflatex").is_err()
        || std::process::Command::new("kpsewhich")
            .arg("mylatexformat.ltx")
            .output()
            .map_or(true, |o| o.stdout.is_empty())
    {
        return;
    }
    let dir = crate::support::scratch_dir(crate::support::tmp_root().join("satex-compile-own-preamble"));
    std::fs::write(
        dir.join("bad.tex"),
        "\\documentclass{article}\n\\newcommand{\\section}{Oops}\n\\begin{document}\nHi.\n\\end{document}\n",
    )
    .unwrap();
    let (rows, said) = satex_build(&dir, "bad.tex");
    assert!(rows.iter().any(|r| r["severity"] == "error" && r["line"] == 2), "{rows:?}\n{said}");
    assert!(!said.contains("-fmt="), "a preamble with an error is read from a format: {said}");
    let warned = "\\documentclass{article}\n\\makeatletter\\PackageWarningNoLine{demo}{said in the preamble}\n\
                  \\begin{document}\nHi.\n\\end{document}\n";
    std::fs::write(dir.join("warn.tex"), warned).unwrap();
    for build in 0..2 {
        let (rows, said) = satex_build(&dir, "warn.tex");
        assert!(said.contains("-fmt="), "{said}");
        let found = rows.iter().any(|r| r["message"].as_str().is_some_and(|m| m.contains("said in the preamble")));
        assert!(found, "build {build}: {rows:?}\n{said}");
    }
}

/// The playground keeps a page's image across builds when its key (`pagekeys`) is the one the
/// image was drawn from. Pins that equal keys draw equal pixels (pdftocairo) and that a changed
/// page gets another key: two pages of rules (INITEX has no font map), the second widened.
#[test]
fn a_page_keeps_its_key_only_while_it_draws_alike() {
    let tools = ["pdftex", "qpdf", "pdftocairo"];
    if tools.iter().any(|t| satex::project::paths::which(t).is_err()) {
        return;
    }
    let dir = crate::support::tmp_root().join(format!("satex-pagekeys-{}", std::process::id()));
    let build = |name: &str, second: &str| {
        let at = dir.join(name);
        std::fs::create_dir_all(&at).unwrap();
        let page = |text: &str| format!("\\shipout\\hbox{{{text}}}\n");
        let text = format!(
            "\\catcode`\\{{=1 \\catcode`\\}}=2 \\pdfoutput=1 \\pdfpagewidth=200pt \\pdfpageheight=200pt\n{}{}\\end\n",
            page("\\vrule width 30pt height 20pt"),
            page(second)
        );
        std::fs::write(at.join("p.tex"), text).unwrap();
        let status = std::process::Command::new("pdftex")
            .args(["-ini", "-interaction=nonstopmode", "p.tex"])
            .current_dir(&at)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        let pdf = at.join("p.pdf");
        let png = |n: u32| {
            let out = std::process::Command::new("pdftocairo")
                .args(["-png", "-singlefile", "-f", &n.to_string(), "-l", &n.to_string()])
                .arg(&pdf)
                .arg(at.join(format!("page{n}")))
                .status()
                .unwrap();
            assert!(out.success());
            std::fs::read(at.join(format!("page{n}.png"))).unwrap()
        };
        (satex::cli::gui::compile::pagekeys(&pdf), png(1), png(2))
    };
    let (a, a1, a2) = build("a", "\\vrule width 10pt height 20pt");
    let (b, b1, b2) = build("b", "\\vrule width 40pt height 20pt");
    assert_eq!((a.len(), b.len()), (2, 2), "{a:?} {b:?}");
    // the first page draws alike: one key
    assert_eq!(a[0], b[0]);
    assert_eq!(a1, b1);
    // the second does not: another key
    assert_ne!(a2, b2);
    assert_ne!(a[1], b[1]);
    let _ = std::fs::remove_dir_all(&dir);
}
