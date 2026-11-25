//! The rules every `satex` command follows the same way: where the input
//! comes from, which `--format` applies, what goes to stderr. See
//! doc/wiki/cli.md.

use std::io::Read;
use std::process::Stdio;

/// A project folder with one root document that loads a second file.
fn project(tag: &str) -> std::path::PathBuf {
    let dir = crate::support::scratch_dir(crate::support::tmp_root().join(format!("cli-rules-{tag}")));
    let root =
        "\\documentclass{article}\n\\newcommand\\rootmacro{x}\n\\begin{document}\n\\input{chap}\n\\end{document}\n";
    std::fs::write(dir.join("main.tex"), root).unwrap();
    std::fs::write(dir.join("chap.tex"), "A chapter.\n").unwrap();
    dir
}

/// The files `query files` lists, as JSON text.
fn files_with(dir: &std::path::Path, stdin: Stdio, args: &[&str]) -> (String, String) {
    let out = crate::support::satex()
        .args(["query", "files", "--format", "json"])
        .args(args)
        .current_dir(dir)
        .stdin(stdin)
        .output()
        .unwrap();
    let (stdout, stderr) =
        (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into());
    assert!(out.status.success(), "{stderr}");
    (stdout, stderr)
}

/// An empty stdin (`</dev/null`, as cron, CI and editors start satex) is
/// nothing piped in: the folder's root document is read, the same as from a
/// terminal. Bug pinned: every command analyzed an empty `<input>` instead,
/// and `lint` reported "0 errors" for a project it never read.
#[test]
fn empty_stdin_reads_the_root() {
    if !crate::support::installed() {
        return;
    }
    let dir = project("empty-stdin");
    let (stdout, _) = files_with(&dir, Stdio::null(), &[]);
    assert!(stdout.contains("main.tex") && stdout.contains("chap.tex"), "{stdout}");
    assert!(!stdout.contains("<input>"), "{stdout}");
}

/// `explain` with an empty stdin explains in the root document too, never in
/// the smallest LaTeX document. Bug pinned: it said "nothing was piped in"
/// and did not know the project's own macros.
#[test]
fn explain_with_empty_stdin_uses_the_root() {
    if !crate::support::installed() {
        return;
    }
    let dir = project("explain");
    let out =
        crate::support::satex().args(["explain", "rootmacro"]).current_dir(&dir).stdin(Stdio::null()).output().unwrap();
    let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "{stderr}");
    assert!(stdout.contains("main.tex"), "{stdout}");
    assert!(!stderr.contains("smallest LaTeX document"), "{stderr}");
}

/// `-f -` names stdin, as `--request -` does: the piped text is the document.
#[test]
fn dash_names_stdin() {
    if !crate::support::installed() {
        return;
    }
    let dir = project("dash");
    let mut child = crate::support::satex()
        .args(["query", "files", "--format", "json", "-f", "-"])
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child.stdin.take().unwrap().write_all(b"\\documentclass{article}\\begin{document}x\\end{document}").unwrap();
    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(stdout.contains("<input>") && !stdout.contains("chap.tex"), "{stdout}");
}

/// A stdin left open and silent gets one note on stderr instead of an
/// unexplained wait, and closing it then reads the root. Bug pinned: a run
/// started from a pipe that never closes hung without a word.
#[test]
fn silent_stdin_gets_a_note() {
    if !crate::support::installed() {
        return;
    }
    let dir = project("silent");
    let mut child = crate::support::satex()
        .args(["query", "files", "--format", "json"])
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stderr = child.stderr.take().unwrap();
    // stdout drains on its own thread: a child blocked writing a full pipe
    // would never close stderr, and reading stderr to its end would hang
    let mut stdout = child.stdout.take().unwrap();
    let drained = std::thread::spawn(move || {
        let mut out = String::new();
        stdout.read_to_string(&mut out).map(|_| out)
    });
    // the note's first byte, then close stdin
    let mut first = [0u8; 1];
    stderr.read_exact(&mut first).unwrap();
    drop(child.stdin.take());
    let mut rest = String::new();
    stderr.read_to_string(&mut rest).unwrap();
    child.wait().unwrap();
    let out = drained.join().unwrap().unwrap();
    assert!(rest.contains("reading the document from stdin"), "{rest}");
    assert!(out.contains("main.tex"));
}

/// `satex cache` and its subcommands never read stdin, so an open pipe does
/// not hold them, and they print no note about a missing document. Bug
/// pinned: `satex cache` hung on an open stdin, and `cache show`, `prune`,
/// `stop` said "no document named (nothing was piped in)".
#[test]
fn cache_never_reads_stdin() {
    let dir = project("cache-stdin");
    let mut commands = vec![&["cache", "show"][..], &["cache", "prune"]];
    if crate::support::installed() {
        commands.push(&["cache"]);
    }
    for args in commands {
        let mut child = crate::support::satex()
            .args(args)
            .current_dir(&dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        // stdin stays open while the command runs
        let held = child.stdin.take();
        let mut stderr = String::new();
        child.stderr.take().unwrap().read_to_string(&mut stderr).unwrap();
        drop(held);
        assert!(child.wait().unwrap().success(), "{args:?}: {stderr}");
        assert!(!stderr.contains("no document named") && !stderr.contains("from stdin"), "{args:?}: {stderr}");
    }
}

/// The one-line package cache summary is on stderr for every command that
/// analyzes a document, not only `lint`. Bug pinned: `query`, `explain` and
/// the others built caches without saying so.
#[test]
fn every_analysis_names_its_package_caches() {
    if !crate::support::installed() {
        return;
    }
    let dir = project("caches-line");
    for args in [&["query", "files", "-f", "main.tex"][..], &["explain", "rootmacro", "-f", "main.tex"]] {
        let out = crate::support::satex().args(args).current_dir(&dir).stdin(Stdio::null()).output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{args:?}: {stderr}");
        assert!(stderr.contains("package caches:"), "{args:?}: {stderr}");
    }
}

/// A broken satex.yaml is named once, by one path, however the run reaches
/// it. Bug pinned: `lint -f main.tex` warned three times, as `satex.yaml`,
/// `./satex.yaml` and the absolute path.
#[test]
fn a_broken_config_warns_once() {
    if !crate::support::installed() {
        return;
    }
    let dir = project("broken-config");
    std::fs::write(dir.join("satex.yaml"), "limits:\n  steps: notanumber\n").unwrap();
    let out = crate::support::satex()
        .args(["lint", "-f", "main.tex"])
        .current_dir(&dir)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(stderr.matches("ignored: invalid type").count(), 1, "{stderr}");
}

/// A language client that closes stdin before `initialize` gets a sentence,
/// not the channel library's "disconnected channel".
#[test]
fn lsp_names_a_closed_client() {
    let out = crate::support::satex().arg("lsp").stdin(Stdio::null()).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("closed the connection before `initialize`"), "{stderr}");
}

/// Intuition: a lint filter on a field no finding has selects nothing, so it says which fields
/// there are instead of "no findings". Pins `lint --filter rule=unused-label` (the rule's name is
/// the field `code`), which printed "no findings" while unused-label findings stood.
#[test]
fn a_lint_filter_on_a_missing_field_names_the_fields() {
    let dir = crate::support::scratch_dir(crate::support::tmp_root().join("cli-rules-filter-field"));
    let doc = "\\documentclass{article}\n\\begin{document}\n\\section{A}\\label{sec:a}\n\\end{document}\n";
    std::fs::write(dir.join("main.tex"), doc).unwrap();
    let run = |filter: &str| {
        let out = crate::support::satex()
            .args(["lint", "-f", "main.tex", "--format", "json", "--filter", filter])
            .current_dir(&dir)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    let (ok, stdout, _) = run("code=unused-label");
    assert!(ok && stdout.contains("sec:a"), "{stdout}");
    let (ok, _, stderr) = run("rule=unused-label");
    assert!(!ok && stderr.contains("`rule`") && stderr.contains("code"), "{stderr}");
}

/// Intuition: `satex -f chapter.tex lint` on a chapter a document `\input`s reads it through that
/// document, as the editor does: alone, `\section` reads as undefined and every line is an error.
/// Pins the 47 false errors on doc/site/examples/chapter.tex; the root sits below the head of its
/// file, and a macro's bare `{#1}` in a sibling document must not count as naming the chapter.
#[test]
fn an_input_file_is_linted_through_its_root() {
    let dir = project("input-root");
    let pad = "% filler line\n".repeat(40);
    std::fs::write(
        dir.join("aside.tex"),
        "\\documentclass{article}\n\\def\\twice#1{#1#1}\n\\begin{document}\nx\n\\end{document}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("main.tex"),
        format!("\\documentclass{{article}}\n{pad}\\begin{{document}}\n\\input{{chapter}}\n\\end{{document}}\n"),
    )
    .unwrap();
    std::fs::write(dir.join("chapter.tex"), "\\section{One}\nText.\n").unwrap();
    let out = crate::support::satex()
        .current_dir(&dir)
        .args(["-f", "chapter.tex", "--no-config", "--set", "limits.seconds=0", "lint"])
        .output()
        .unwrap();
    let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(stderr.contains("through its root") && stderr.contains("main.tex"), "{stderr}");
    assert!(!stdout.contains("never defined") && !stdout.contains("Missing \\begin{document}"), "{stdout}");
}

/// Intuition: a slice's `filecontents*` copies are named as the document names its files, relative to
/// its folder. Run from that folder (`-f main.tex`, the folder `""`), the copy of a local package was
/// also written under its absolute path: compiling a stale slice would overwrite the user's file.
#[test]
fn a_slice_names_its_file_copies_relative_to_the_document() {
    let dir = project("slice-relative");
    std::fs::write(dir.join("mine.sty"), "\\ProvidesPackage{mine}\n\\newcommand\\mine{M}\n").unwrap();
    std::fs::write(
        dir.join("main.tex"),
        "\\documentclass{article}\n\\usepackage{mine}\n\\begin{document}\n\\mine\n\\end{document}\n",
    )
    .unwrap();
    let out = crate::support::satex()
        .current_dir(&dir)
        .args(["-f", "main.tex", "--no-config", "--set", "limits.seconds=0", "slice", "--at", "4"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let copies: Vec<&str> = stdout.lines().filter(|l| l.starts_with("\\begin{filecontents*}")).collect();
    assert!(copies.iter().all(|l| !l.contains('/')) && copies.len() == 1, "{stdout}");
}
