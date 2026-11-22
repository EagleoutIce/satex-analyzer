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
        .args(["--set", "compile.wait=true"])
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

/// Waits until the format a build of `job` in `dir` had dumped aside is there: its key is written
/// when the dump and its proof are done. No build waits for it, so a test that wants the next
/// build to read the format does.
fn dumped(dir: &std::path::Path, job: &str) {
    let key = dir.join(".satex-build").join(format!("{job}.key"));
    let until = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while !key.is_file() && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(key.is_file(), "no format key after a minute: {}", key.display());
}

/// The text of a PDF, page by page (form feeds between pages).
fn pdf_text(pdf: &std::path::Path) -> String {
    let out = std::process::Command::new("pdftotext").arg(pdf).arg("-").output().unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// satex's own build reads the preamble from a format dumped aside once per preamble, reruns only
/// while the build files move, and runs BibTeX and makeindex where their inputs changed. Its PDF
/// must equal latexmk's, also after an edit of the body and of a file the preamble reads (the
/// format is dumped again then, and the build that starts the dump reads the preamble itself).
/// Pins: an incremental build that is faster but different, and a build that waits for a dump.
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
        assert_eq!(pdf_text(&ours.join("doc.pdf")), latexmk(&theirs), "step {step}: {said}");
        let dumps = said.matches("dumping the preamble's format aside").count();
        // dumped at the start and after the preamble's file changed, reused for a body edit
        assert_eq!(dumps, usize::from(step != 1), "step {step}: {said}");
        if step == 1 {
            assert!(said.contains("-fmt="), "the preamble is not read from a format: {said}");
            assert_eq!(said.matches("$ pdflatex").count(), 1, "a body edit is one pass: {said}");
        }
        dumped(&ours, "doc");
    }
}

/// A build without an `.aux` starts from the `.aux` of a one-page document without references, as
/// latexmk does: a pass that writes that very file again is at rest, so such a document is one pass,
/// and one with a table of contents is more. The pass writes its SyncTeX data uncompressed and satex
/// compresses it beside the pass. Pins: a second pass that can change nothing, a first build that
/// stops before its references rest, and SyncTeX data cut short or left in two files.
#[test]
fn a_fresh_build_passes_once_where_nothing_is_referred_to() {
    let tools = ["pdflatex", "pdftotext", "gzip"];
    if !crate::support::installed() || tools.iter().any(|t| satex::project::paths::which(t).is_err()) {
        return;
    }
    let dir = crate::support::scratch_dir(crate::support::tmp_root().join("satex-compile-seed"));
    let doc = |body: &str| format!("\\documentclass{{article}}\n\\begin{{document}}\n{body}\n\\end{{document}}\n");
    std::fs::write(dir.join("one.tex"), doc("Just text.")).unwrap();
    std::fs::write(dir.join("toc.tex"), doc("\\tableofcontents\n\\section{Listed}\nText.")).unwrap();
    let passes = |said: &str| said.matches("$ pdflatex").count();
    let (rows, said) = satex_build(&dir, "one.tex");
    assert!(rows.is_empty(), "{rows:?}\n{said}");
    assert_eq!(passes(&said), 1, "{said}");
    assert!(pdf_text(&dir.join("one.pdf")).contains("Just text."), "{said}");
    // the whole data, compressed, in one file
    let unpacked = std::process::Command::new("gzip").arg("-dc").arg(dir.join("one.synctex.gz")).output().unwrap();
    let data = String::from_utf8_lossy(&unpacked.stdout).into_owned();
    assert!(
        unpacked.status.success() && data.starts_with("SyncTeX Version:") && data.contains("Post scriptum:"),
        "{data}"
    );
    assert!(!dir.join("one.synctex").exists() && !dir.join("one.synctex.gz.next").exists());
    let (_, said) = satex_build(&dir, "toc.tex");
    assert!(passes(&said) >= 2, "the table of contents needs its second pass: {said}");
    assert!(pdf_text(&dir.join("toc.pdf")).matches("Listed").count() == 2, "{said}");
}

/// XeTeX hands its pages to xdvipdfmx, which takes a third to a half of a pass: satex's build
/// keeps the pages of every pass (`-no-pdf`) and runs the driver once after the last, as latexmk
/// does. The PDF's text is latexmk's, an edit builds it again, and a PDF that is gone is built
/// again though no source changed. Pins: a driver run per pass, and a build that calls itself up
/// to date without its PDF.
#[test]
fn the_satex_build_drives_xetex_pages_once() {
    let tools = ["xelatex", "xdvipdfmx", "latexmk", "pdftotext"];
    if !crate::support::installed() || tools.iter().any(|t| satex::project::paths::which(t).is_err()) {
        return;
    }
    let root = crate::support::scratch_dir(crate::support::tmp_root().join("satex-compile-xetex"));
    let (ours, theirs) = (root.join("ours"), root.join("theirs"));
    let doc = |body: &str| {
        format!(
            "\\documentclass{{article}}\n\\begin{{document}}\n\\tableofcontents\n\\section{{Listed}}\n{body}\n\\end{{document}}\n"
        )
    };
    let build = |dir: &std::path::Path| {
        let out = crate::support::satex()
            .args(["compile", "-f", "doc.tex", "--format", "json", "--system", "satex", "--set", "engine=xetex"])
            .args(["--set", "limits.seconds=0", "--set", "compile.explain=false"])
            .env("SOURCE_DATE_EPOCH", "1700000000")
            .env("FORCE_SOURCE_DATE", "1")
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stderr).into_owned()
    };
    for body in ["First text.", "Edited text."] {
        for dir in [&ours, &theirs] {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join("doc.tex"), doc(body)).unwrap();
        }
        let said = build(&ours);
        assert_eq!(said.matches("$ xdvipdfmx").count(), 1, "{said}");
        assert!(said.contains("$ xelatex") && said.contains("-no-pdf"), "{said}");
        let made = std::process::Command::new("latexmk")
            .args(["-xelatex", "-interaction=nonstopmode", "doc.tex"])
            .current_dir(&theirs)
            .output()
            .unwrap();
        assert!(theirs.join("doc.pdf").is_file(), "{}", String::from_utf8_lossy(&made.stdout));
        assert_eq!(pdf_text(&ours.join("doc.pdf")), pdf_text(&theirs.join("doc.pdf")), "{said}");
    }
    let said = build(&ours);
    assert!(said.contains("up to date"), "{said}");
    std::fs::remove_file(ours.join("doc.pdf")).unwrap();
    let said = build(&ours);
    assert!(ours.join("doc.pdf").is_file() && !said.contains("up to date"), "{said}");
}

/// A build whose read files, the directories it looked in and the clock are as the last rested
/// build left them runs no pass, as latexmk skips one. A file the document looked for and did not
/// find is in no `.fls`: its appearing must rebuild, as must an edit of it. Pins: a skip that
/// keeps a PDF a plain build would not give.
#[test]
fn the_satex_build_skips_only_what_cannot_change() {
    if !crate::support::installed()
        || satex::project::paths::which("pdflatex").is_err()
        || satex::project::paths::which("pdftotext").is_err()
    {
        return;
    }
    let dir = crate::support::scratch_dir(crate::support::tmp_root().join("satex-compile-skip"));
    let doc = "\\documentclass{article}\n\\begin{document}\nMain.\n\\IfFileExists{sub/extra.tex}{\\input{sub/extra}}{}\n\\end{document}\n";
    std::fs::write(dir.join("doc.tex"), doc).unwrap();
    // a fixed clock: the builds of this test fall in one minute or not, the skip may not tell
    let build = || {
        let out = crate::support::satex()
            .args(["compile", "-f", "doc.tex", "--format", "json", "--system", "satex"])
            .args(["--set", "limits.seconds=0", "--set", "compile.explain=false"])
            .env("SOURCE_DATE_EPOCH", "1700000000")
            .env("FORCE_SOURCE_DATE", "1")
            .current_dir(&dir)
            .output()
            .unwrap();
        let said = String::from_utf8_lossy(&out.stderr).into_owned();
        (said.contains("$ pdflatex"), pdf_text(&dir.join("doc.pdf")), said)
    };
    let (ran, text, said) = build();
    assert!(ran && text.contains("Main."), "{said}");
    let (ran, _, said) = build();
    assert!(!ran && said.contains("up to date"), "nothing changed, yet a pass ran: {said}");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub/extra.tex"), "Extra.\n").unwrap();
    let (ran, text, said) = build();
    assert!(ran && text.contains("Extra."), "a file that was missing appeared: {said}");
    std::fs::write(dir.join("sub/extra.tex"), "Other.\n").unwrap();
    let (ran, text, said) = build();
    assert!(ran && text.contains("Other."), "a read file changed: {said}");
    let (ran, _, said) = build();
    assert!(!ran, "{said}");
}

/// An explained build whose logs, source, settings and every file the analysis read are as the
/// last explained build left them gives that build's records without an analysis; an edit of an
/// `\input` file explains anew. Pins: a stored explanation served past a change it depends on.
#[test]
fn an_unchanged_build_reuses_its_explanation_only() {
    if !crate::support::installed() || satex::project::paths::which("pdflatex").is_err() {
        return;
    }
    let dir = crate::support::tmp_root().join(format!("satex-explained-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("doc.tex"),
        "\\documentclass{article}\n\\begin{document}\n\\input{part}\n\\end{document}\n",
    )
    .unwrap();
    std::fs::write(dir.join("part.tex"), "Some \\foo text.\n").unwrap();
    let build = || {
        let out = crate::support::satex()
            .args(["compile", "-f", "doc.tex", "--format", "json", "--system", "satex", "--set", "limits.seconds=0"])
            .args(["--set", "compile.wait=true"])
            .current_dir(&dir)
            .output()
            .unwrap();
        let said = String::from_utf8_lossy(&out.stderr).into_owned();
        let rows: Vec<serde_json::Value> =
            serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}: {said}"));
        (rows, said.contains("explained as the last build was"))
    };
    let fix = |rows: &[serde_json::Value]| {
        let row = rows.iter().find(|r| r["code"] == "undefined-control-sequence");
        let row = row.unwrap_or_else(|| panic!("no undefined name: {rows:?}"));
        row["fix"].as_str().unwrap_or_default().to_string()
    };
    let (first, reused) = build();
    assert!(!reused && fix(&first).contains("\\foo"), "{first:?}");
    let (second, reused) = build();
    assert!(reused && second == first, "{second:?}");
    std::fs::write(dir.join("part.tex"), "Some \\baz text.\n").unwrap();
    let (third, reused) = build();
    assert!(!reused && fix(&third).contains("\\baz"), "{third:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// `satex compile` returns with the PDF: its messages are explained by rule at once, and a detached
/// satex analyzes the document and stores what its findings say. The next call of the same text
/// gives those records; a call after an edit that leaves a message as it was repeats what the last
/// analysis said of it, marked as of an earlier text, while a new analysis runs. Pins: a build that
/// waits for the analysis, an explanation that never arrives, and one of an earlier text passed
/// off as of this one.
#[test]
fn a_build_returns_before_its_explanation_and_shows_it_later() {
    if !crate::support::installed() || satex::project::paths::which("pdflatex").is_err() {
        return;
    }
    let dir = crate::support::scratch_dir(crate::support::tmp_root().join("satex-compile-aside"));
    let doc = |end: &str| {
        format!("\\documentclass{{article}}\n\\begin{{document}}\nSome \\foo text.\n{end}\\end{{document}}\n")
    };
    std::fs::write(dir.join("doc.tex"), doc("")).unwrap();
    let build = || {
        let out = crate::support::satex()
            .args(["compile", "-f", "doc.tex", "--format", "json", "--system", "satex", "--set", "limits.seconds=0"])
            .current_dir(&dir)
            .output()
            .unwrap();
        let said = String::from_utf8_lossy(&out.stderr).into_owned();
        let rows: Vec<serde_json::Value> =
            serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}: {said}"));
        let row = rows.iter().find(|r| r["code"] == "undefined-control-sequence");
        let fix = row.unwrap_or_else(|| panic!("no undefined name: {rows:?}"))["fix"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        (fix, said)
    };
    // the explanation of the text as it is now: its key is written last
    let explained = |since: std::time::SystemTime| {
        let key = dir.join(".satex-build/doc.explained");
        let until = std::time::Instant::now() + std::time::Duration::from_secs(300);
        let fresh = || std::fs::metadata(&key).and_then(|m| m.modified()).is_ok_and(|t| t >= since);
        while !fresh() && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(fresh(), "no explanation after five minutes");
    };
    let start = std::time::SystemTime::now();
    let (fix, said) = build();
    assert!(!fix.contains("\\foo") && said.contains("explains the messages aside"), "{fix}\n{said}");
    explained(start);
    let (found, said) = build();
    assert!(found.contains("\\foo") && said.contains("explained as the last build was"), "{found}\n{said}");
    // an edit elsewhere: the message is the same, what was said of it is repeated and marked
    let edited = std::time::SystemTime::now();
    std::fs::write(dir.join("doc.tex"), doc("More.\n")).unwrap();
    let (fix, said) = build();
    assert!(fix.contains("\\foo") && fix.ends_with("(said by the analysis of an earlier text)"), "{fix}\n{said}");
    explained(edited);
    let (fix, _) = build();
    assert_eq!(fix, found);
}

/// mylatexformat replays the preamble's `\\openout`s at `\\begin{document}`, and glossaries'
/// `\\makeglossaries` before another package breaks that replay ("Extra \\fi" in `\\MYLATEX@opens`),
/// though a plain run has no error. The dump proves its format on an empty body before any pass
/// reads it. Pins: the format build reporting errors pdflatex has not, and a pass lost to finding out.
#[test]
fn a_preamble_the_format_breaks_is_read_by_the_run() {
    let tools = ["pdflatex", "kpsewhich"];
    if !crate::support::installed()
        || tools.iter().any(|t| satex::project::paths::which(t).is_err())
        || ["mylatexformat.ltx", "glossaries.sty", "cleveref.sty"]
            .iter()
            .any(|f| std::process::Command::new("kpsewhich").arg(f).output().map_or(true, |o| o.stdout.is_empty()))
    {
        return;
    }
    let dir = crate::support::scratch_dir(crate::support::tmp_root().join("satex-compile-mylatex"));
    let doc = "\\documentclass{book}\n\\usepackage{glossaries}\\makeglossaries\\usepackage{cleveref}\n\
               \\begin{document}\n\\frontmatter\nHi.\n\\end{document}\n";
    std::fs::write(dir.join("m.tex"), doc).unwrap();
    for build in 0..2 {
        let (rows, said) = satex_build(&dir, "m.tex");
        assert!(rows.iter().all(|r| r["severity"] != "error"), "build {build}: {rows:?}\n{said}");
        // the dump's proof found the break: no pass ever reads this format
        assert!(!said.contains("-fmt="), "build {build}: {said}");
        dumped(&dir, "m");
        std::fs::write(dir.join("m.tex"), doc.replace("Hi.", "Ho.")).unwrap();
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
    dumped(&dir, "bad");
    std::fs::write(dir.join("bad.tex"), std::fs::read_to_string(dir.join("bad.tex")).unwrap().replace("Hi.", "Ho."))
        .unwrap();
    let (rows, said) = satex_build(&dir, "bad.tex");
    assert!(rows.iter().any(|r| r["severity"] == "error" && r["line"] == 2), "{rows:?}\n{said}");
    assert!(!said.contains("-fmt="), "a preamble with an error is read from a format: {said}");
    let warned = "\\documentclass{article}\n\\makeatletter\\PackageWarningNoLine{demo}{said in the preamble}\n\
                  \\begin{document}\nHi.\n\\end{document}\n";
    for build in 0..2 {
        // another body each build: an unchanged one is not built again
        std::fs::write(dir.join("warn.tex"), warned.replace("Hi.", ["Hi.", "Ho."][build])).unwrap();
        let (rows, said) = satex_build(&dir, "warn.tex");
        // the first build reads the preamble itself, the second the format dumped meanwhile
        assert_eq!(said.contains("-fmt="), build == 1, "{said}");
        dumped(&dir, "warn");
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

/// What a warm engine (pre-started on its format, blocked opening the main file) needs: pdfTeX
/// started on a named pipe in the main file's place, fed the text once a build asks, writes the PDF,
/// SyncTeX and log a fresh run writes, byte for byte. Pins that the pipe leaves no trace: TeX opens
/// the name it was given (tex.web § 537), and the real file is put in the pipe's place before TeX
/// reads a line, for any later open by name.
#[cfg(unix)]
#[test]
fn an_engine_fed_through_a_pipe_writes_what_a_fresh_one_writes() {
    let tools = ["pdftex", "mkfifo"];
    if tools.iter().any(|t| satex::project::paths::which(t).is_err()) {
        return;
    }
    let dir = crate::support::tmp_root().join(format!("satex-warm-{}", std::process::id()));
    let text = "\\catcode`\\{=1 \\catcode`\\}=2 \\pdfoutput=1 \\pdfpagewidth=200pt \\pdfpageheight=200pt\n\
                \\shipout\\hbox{\\vrule width 30pt height 20pt}\n\\end\n";
    let run = |at: &std::path::Path, warm: bool| {
        std::fs::create_dir_all(at).unwrap();
        let main = at.join("w.tex");
        let mut engine = std::process::Command::new("pdftex");
        engine
            .args(["-ini", "-interaction=nonstopmode", "-synctex=-1", "w.tex"])
            .current_dir(at)
            .env("SOURCE_DATE_EPOCH", "1")
            .env("FORCE_SOURCE_DATE", "1")
            .stdout(std::process::Stdio::null());
        if !warm {
            std::fs::write(&main, text).unwrap();
            assert!(engine.status().unwrap().success());
        } else {
            assert!(std::process::Command::new("mkfifo").arg(&main).status().unwrap().success());
            let mut child = engine.spawn().unwrap();
            // the engine waits in the open; the writer's open ends that wait
            let mut pipe = std::fs::OpenOptions::new().write(true).open(&main).unwrap();
            std::fs::write(at.join("w.next"), text).unwrap();
            std::fs::rename(at.join("w.next"), &main).unwrap();
            std::io::Write::write_all(&mut pipe, text.as_bytes()).unwrap();
            drop(pipe);
            assert!(child.wait().unwrap().success());
        }
        ["w.pdf", "w.synctex"].map(|f| std::fs::read(at.join(f)).unwrap())
    };
    let fresh = run(&dir.join("fresh"), false);
    // the same directory name: SyncTeX names the input by its full path
    let _ = std::fs::remove_dir_all(dir.join("fresh"));
    let warm = run(&dir.join("fresh"), true);
    assert!(fresh[0] == warm[0], "the PDF differs");
    assert!(fresh[1] == warm[1], "the SyncTeX data differs");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A run by hand in a `satex gui` build directory finds the warm engine's pipe where the main file
/// was (src/cli/warm.rs): it must not wait on it forever. Pins that the kept text goes back in
/// place and the run reads it.
#[cfg(target_os = "linux")]
#[test]
fn a_run_by_hand_never_waits_on_the_warm_engines_pipe() {
    if satex::project::paths::which("mkfifo").is_err() {
        return;
    }
    let dir = crate::support::tmp_root().join(format!("satex-warm-pipe-{}", std::process::id()));
    let warm = dir.join(".satex-build").join("warm");
    std::fs::create_dir_all(&warm).unwrap();
    let text = "\\relax\n";
    std::fs::write(warm.join("source"), text).unwrap();
    assert!(std::process::Command::new("mkfifo").arg(dir.join("main.tex")).status().unwrap().success());
    let mut run = crate::support::satex()
        .args(["--format", "json", "tokens", "-f", "main.tex"])
        .current_dir(&dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    // a run that waits on the pipe is stopped by the test, not left behind
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while run.try_wait().unwrap().is_none() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let done = run.try_wait().unwrap().is_some();
    if !done {
        let _ = run.kill();
    }
    assert!(done, "the run waited on the pipe");
    assert_eq!(std::fs::read_to_string(dir.join("main.tex")).unwrap(), text);
    let _ = std::fs::remove_dir_all(&dir);
}

/// memoize in satex's own build: memoize extracts nothing (`extract=no` before the document),
/// the build extracts each new extern with memoize's own TeX method between passes, so no shell
/// escape and no Perl or Python module is needed. The PDF then shows each picture once and no
/// extern page, and the next build includes every extern, compiling no picture again. Pins the
/// compile half of memoize support: a TeX Live without Perl's PDF::API2 errors on every pdflatex
/// run with memoize's default `extract=perl`, and pictures were compiled on every build.
#[test]
fn the_satex_build_extracts_memoize_externs_itself() {
    let tools = ["pdflatex", "pdftex", "pdftotext", "kpsewhich"];
    if !crate::support::installed()
        || tools.iter().any(|t| satex::project::paths::which(t).is_err())
        || std::process::Command::new("kpsewhich").arg("memoize.sty").output().map_or(true, |o| o.stdout.is_empty())
    {
        return;
    }
    let dir = crate::support::scratch_dir(crate::support::tmp_root().join("satex-compile-memoize"));
    let doc = "\\documentclass{article}\n\\usepackage{memoize}\n\\usepackage{tikz}\n\\begin{document}\nPICTURE\n\
               \\begin{tikzpicture}\\node{first};\\end{tikzpicture}\n\
               \\begin{tikzpicture}\\node{second};\\end{tikzpicture}\n\\end{document}\n";
    let pages = |text: &str| text.matches('\u{c}').count();
    // the first build compiles both pictures, extracts both, and its last pass includes them
    std::fs::write(dir.join("doc.tex"), doc.replace("PICTURE", "Pictures:")).unwrap();
    let (rows, said) = satex_build(&dir, "doc.tex");
    assert!(rows.iter().all(|r| r["severity"] != "error"), "{rows:?}\n{said}");
    assert_eq!(said.matches("$ pdftex").count(), 2, "one extraction per picture: {said}");
    let text = pdf_text(&dir.join("doc.pdf"));
    assert_eq!((pages(&text), text.matches("first").count()), (1, 1), "an extern page is left: {text}");
    let externs = |dir: &std::path::Path| {
        let memo = std::fs::read_dir(dir.join("doc.memo.dir")).unwrap().flatten();
        memo.filter(|e| e.path().extension().is_some_and(|x| x == "pdf")).count()
    };
    assert_eq!(externs(&dir), 2);
    // an edit beside the pictures: both are read from their memos, none extracted again
    std::fs::write(dir.join("doc.tex"), doc.replace("PICTURE", "Edited:")).unwrap();
    let (_, said) = satex_build(&dir, "doc.tex");
    assert_eq!(said.matches("$ pdftex").count(), 0, "a picture was compiled again: {said}");
    let mmz = std::fs::read_to_string(dir.join("doc.mmz")).unwrap();
    assert!(!mmz.contains("\\mmzNewExtern") && mmz.matches("\\mmzUsedExtern").count() == 2, "{mmz}");
}

/// `compile.warm` from the command line: the next build's pass goes to the engine the last one left
/// waiting on a pipe under `.satex-build`, given to it in the main file's stead. The engine runs in
/// the project's directory, so a name means what it means to a fresh engine: `\input{../shared}`,
/// a file behind a symbolic link, the main file opened again by its own name. The project's main
/// file is never a pipe. With the clock fixed, the PDF, the log (but its memory statistics) and the
/// SyncTeX data of a warm pass are those of a fresh pass on the same text, byte for byte. Pins: a warm build that ran in a copy
/// of the project and found no `../shared.tex`, the CLI stopping its own warm engine, the pipe's
/// name left in the log or the SyncTeX data, and an engine started under another `TEXINPUTS` taken
/// for the one a pass would start.
#[test]
#[cfg(target_os = "linux")]
fn a_warm_build_writes_what_a_fresh_build_writes() {
    let tools = ["pdflatex", "gzip", "mkfifo"];
    if !crate::support::installed() || tools.iter().any(|t| satex::project::paths::which(t).is_err()) {
        return;
    }
    let root = crate::support::scratch_dir(crate::support::tmp_root().join("satex-warm-cli"));
    let dir = root.join("proj");
    for d in ["proj", "real"] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    std::fs::write(root.join("shared.tex"), "Shared above.\n").unwrap();
    std::fs::write(root.join("real/a.tex"), "Behind a link.\n").unwrap();
    std::os::unix::fs::symlink("../real", dir.join("sub")).unwrap();
    let doc = |body: &str| {
        format!(
            "\\documentclass{{article}}\n\\begin{{document}}\n{body}\n\\input{{../shared}}\\input{{sub/a}}\n\
             \\IfFileExists{{doc.tex}}{{\\newread\\r\\openin\\r=doc.tex \\read\\r to\\first\\closein\\r Named.}}{{}}\n\
             \\end{{document}}\n"
        )
    };
    let build_under = |warm: &str, texinputs: &str| {
        let out = crate::support::satex()
            .args(["compile", "-f", "doc.tex", "--format", "json", "--system", "satex"])
            .args(["--set", "limits.seconds=0", "--set", "compile.explain=false", "--set"])
            .arg(format!("compile.warm={warm}"))
            .env("TEXINPUTS", texinputs)
            .env("SOURCE_DATE_EPOCH", "1700000000")
            .env("FORCE_SOURCE_DATE", "1")
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(std::fs::symlink_metadata(dir.join("doc.tex")).unwrap().is_file(), "the project's file is no pipe");
        String::from_utf8_lossy(&out.stderr).into_owned()
    };
    let build = |warm: &str| build_under(warm, ".:");
    let wrote = || {
        let synctex = std::process::Command::new("gzip").arg("-dc").arg(dir.join("doc.synctex.gz")).output().unwrap();
        (std::fs::read(dir.join("doc.pdf")).unwrap(), std::fs::read(dir.join("doc.log")).unwrap(), synctex.stdout)
    };
    std::fs::write(dir.join("doc.tex"), doc("First text.")).unwrap();
    assert!(!build("true").contains("(warm)"), "a first build has no engine waiting");
    dumped(&dir, "doc");
    // an edit each time, so no build is skipped; the engine reads the clock's minute as it starts
    // Under load the engine may still be loading its format: wait until it is seen in its open.
    let record = dir.join(".satex-build/warm/engine");
    let waits = || {
        for _ in 0..300 {
            let pid = std::fs::read_to_string(&record).ok().and_then(|r| r.lines().next().map(str::to_string));
            // No engine, one gone, or one waiting: nothing to wait for.
            let Some(wchan) = pid.and_then(|p| std::fs::read_to_string(format!("/proc/{p}/wchan")).ok()) else {
                return;
            };
            if wchan.trim() == "wait_for_partner" {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    };
    let mut warm = None;
    for i in 0..4 {
        waits();
        std::fs::write(dir.join("doc.tex"), doc(&format!("Edit {i}."))).unwrap();
        if build("true").contains("(warm)") {
            warm = Some((i, wrote()));
            break;
        }
    }
    // the engine waiting now started under this search path: a build under another starts its own
    if warm.is_some() {
        std::fs::write(dir.join("doc.tex"), doc("Elsewhere.")).unwrap();
        let said = build_under("true", ".:..:");
        assert!(said.contains("$ pdflatex") && !said.contains("(warm)"), "{said}");
    }
    if let Some(pid) = std::fs::read_to_string(&record).ok().and_then(|r| r.lines().next().map(str::to_string)) {
        let _ = std::process::Command::new("kill").arg(pid).status();
    }
    let (i, warm) = warm.expect("no pass went to the waiting engine");
    // the same text by a fresh engine
    std::fs::write(dir.join("doc.tex"), doc(&format!("Edit {i}."))).unwrap();
    let said = build("false");
    assert!(said.contains("$ pdflatex") && !said.contains("(warm)"), "{said}");
    let fresh = wrote();
    let text = pdf_text(&dir.join("doc.pdf"));
    for said in [format!("Edit {i}."), "Shared above.".into(), "Behind a link.".into(), "Named.".into()] {
        assert!(text.contains(&said), "no `{said}`: {text}");
    }
    assert!(warm.0 == fresh.0, "the PDF differs");
    // up to TeX's memory statistics: the pipe's longer name takes more of the string pool
    let said = |log: &[u8]| {
        let log = String::from_utf8_lossy(log).into_owned();
        log.split("Here is how much of TeX's memory you used").next().unwrap_or_default().to_string()
    };
    assert_eq!(said(&warm.1), said(&fresh.1), "the log differs");
    assert!(warm.2 == fresh.2 && !fresh.2.is_empty(), "the SyncTeX data differs");
}
