//! `.dtx` and `.ins` files: what satex reads of a documented source is what
//! docstrip extracts from it, and a batch file says which files it makes.

use std::path::{Path, PathBuf};
use std::process::Command;

use satex::config::Config;
use satex::literate::{Guards, code_view};
use satex::machine::{Analysis, Machine};

fn installed() -> bool {
    which::which("kpsewhich").is_ok()
}

fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join("satex-literate-test").join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    directory
}

fn analyze(directory: &Path, name: &str, source: &str) -> Analysis {
    let file = directory.join(name);
    std::fs::write(&file, source).expect("writing the source");
    let cfg = Config { load_classes: true, ..Config::default() };
    Machine::analyze(source, Some(&file), &cfg)
}

fn definition<'a>(analysis: &'a Analysis, name: &str) -> Option<&'a satex::facts::Definition> {
    analysis.facts.defs.iter().find(|def| analysis.interner.name(def.name) == name)
}

fn occurrences(analysis: &Analysis) -> Vec<satex::query::Record> {
    satex::query::run(analysis, satex::query::Query::Occurrences, &satex::query::Filter::Always)
}

const TWO_GUARDS: &str = r"% \iffalse meta-comment
%<*package>
%    \begin{macrocode}
\def\fromthepackage{1}
%    \end{macrocode}
%</package>
%<*driver>
\documentclass{ltxdoc}
\begin{document}
\DocInput{two.dtx}
\end{document}
%</driver>
% \fi
";

#[test]
fn guarded_code_is_read_and_the_driver_is_not() {
    if !installed() {
        return;
    }
    let directory = scratch("two-guards");
    let analysis = analyze(&directory, "two.dtx", TWO_GUARDS);
    let def = definition(&analysis, "fromthepackage").expect("the guarded definition");
    assert_eq!(def.span.line, 4, "the span points at the line of the .dtx it came from");
    assert!(
        !analysis.facts.loads.iter().any(|load| load.name == "ltxdoc"),
        "the driver is guarded `driver`, so its \\documentclass is not code"
    );
    assert_eq!(satex::query::identity(&analysis).kind, "documented source (.dtx), read as docstrip extracts it");
}

#[test]
fn the_documentation_is_not_code() {
    if !installed() {
        return;
    }
    let directory = scratch("documentation");
    let analysis = analyze(
        &directory,
        "prose.dtx",
        r"% \section{What this does}
% \begin{macro}{\real}
% Writing \def\notcode{oops} in the documentation explains the macro
% below; \begin{verbatim}\newcommand\alsonot{}\end{verbatim} does too.
%    \begin{macrocode}
\def\real{1}
%    \end{macrocode}
% \end{macro}
",
    );
    assert!(definition(&analysis, "real").is_some(), "the macrocode line is code");
    for name in ["notcode", "alsonot"] {
        assert!(definition(&analysis, name).is_none(), "\\{name} is prose, not code");
    }
}

#[test]
fn a_percent_inside_the_code_stays_in_the_code() {
    if !installed() {
        return;
    }
    let directory = scratch("percent");
    let analysis = analyze(
        &directory,
        "percent.dtx",
        r"%    \begin{macrocode}
\def\share{50\% of it}% the rest of this line is a comment
\def\after{1}
%    \end{macrocode}
",
    );
    let def = definition(&analysis, "share").expect("the definition");
    let body = def
        .mac
        .as_ref()
        .map(|mac| satex::tex::detokenize(&mac.replacement_text, &analysis.interner))
        .unwrap_or_default();
    assert!(body.contains("\\%"), "the escaped percent is part of the body: {body}");
    assert!(definition(&analysis, "after").is_some(), "the comment ends with its line");
}

#[test]
fn an_ignored_percent_stops_being_a_comment() {
    if !installed() {
        return;
    }
    // doc.sty typesets a `.dtx` by making `%` catcode 9 (`\MakePercentIgnore`,
    // doc.sty), so the leading `%` of a documentation line disappears rather
    // than hiding the line.  satex reads the catcode table for every
    // character it tokenizes, so the change takes effect where it is made.
    let cfg = Config::default();
    let analysis =
        Machine::analyze("\\catcode`\\%=9 %\\def\\shown{1}\n\\catcode`\\%=14 %\\def\\hidden{2}\n", None, &cfg);
    assert!(definition(&analysis, "shown").is_some(), "an ignored percent is not a comment");
    assert!(definition(&analysis, "hidden").is_none(), "a comment percent still hides its line");
}

#[test]
fn a_batch_file_records_what_it_generates() {
    if !installed() {
        return;
    }
    let directory = scratch("batch");
    let analysis = analyze(
        &directory,
        "demo.ins",
        r"\input docstrip
\keepsilent
\askforoverwritefalse
\usedir{tex/latex/demo}
\preamble
This file was generated from demo.dtx; \endinput is not read here.
\endpreamble
\generate{\file{demo.sty}{\from{demo.dtx}{package}}
          \file{demo-extra.sty}{\from{demo.dtx}{extra,!package}}}
\endbatchfile
",
    );
    let generated: Vec<_> = occurrences(&analysis).into_iter().filter(|record| record["kind"] == "generate").collect();
    assert_eq!(generated.len(), 2, "one record per file generated: {generated:?}");
    assert_eq!(generated[0]["key"], "demo.sty");
    assert_eq!(generated[0]["detail"], "from demo.dtx (package) into tex/latex/demo");
    assert_eq!(generated[1]["key"], "demo-extra.sty");
    assert!(generated[1]["detail"].as_str().is_some_and(|d| d.contains("(extra,!package)")));
    assert_eq!(satex::query::identity(&analysis).kind, "docstrip installation script (.ins)");
}

/// What docstrip itself writes, for the comparison below.
fn unpacked(directory: &Path, dtx: &Path, options: &str) -> Option<String> {
    let name = dtx.file_stem()?.to_str()?.to_string();
    std::fs::copy(dtx, directory.join(format!("{name}.dtx"))).ok()?;
    let batch = format!(
        "\\input docstrip\n\\keepsilent\n\\askforoverwritefalse\n\\nopreamble\\nopostamble\n\
         \\generate{{\\file{{{name}.sty}}{{\\from{{{name}.dtx}}{{{options}}}}}}}\n\\endbatchfile\n"
    );
    std::fs::write(directory.join("unpack.ins"), batch).ok()?;
    let status = Command::new("tex").current_dir(directory).arg("unpack.ins").output().ok()?;
    status
        .status
        .success()
        .then_some(())
        .and_then(|()| std::fs::read_to_string(directory.join(format!("{name}.sty"))).ok())
}

#[test]
fn the_code_view_is_what_docstrip_writes() {
    if !installed() || which::which("tex").is_err() {
        return;
    }
    let Ok(found) = Command::new("kpsewhich").arg("array.dtx").output() else { return };
    let dtx = PathBuf::from(String::from_utf8_lossy(&found.stdout).trim().to_string());
    if !dtx.is_file() {
        return;
    }
    let directory = scratch("docstrip");
    let Some(extracted) = unpacked(&directory, &dtx, "package,ncols") else { return };
    let source = std::fs::read_to_string(&dtx).expect("the documented source");
    let view = code_view(&source, &Guards::Options(vec!["package".into(), "ncols".into()]));
    // docstrip keeps at most one of a run of empty lines, so only the lines
    // that carry something are comparable, and it reads its input with
    // `\read`, which drops the spaces at the end of a line (tex.web § 31).
    let lines = |text: &str| -> Vec<String> {
        text.lines().map(str::trim_end).filter(|line| !line.is_empty()).map(str::to_string).collect()
    };
    let code = lines(&view);
    let written = lines(&extracted);
    assert_eq!(code, written, "satex extracts what docstrip writes");
}

#[test]
fn a_documented_source_defines_what_the_package_does() {
    if !installed() || which::which("tex").is_err() {
        return;
    }
    let Ok(found) = Command::new("kpsewhich").arg("array.dtx").output() else { return };
    let dtx = PathBuf::from(String::from_utf8_lossy(&found.stdout).trim().to_string());
    if !dtx.is_file() {
        return;
    }
    let directory = scratch("comparison");
    if unpacked(&directory, &dtx, "package,ncols").is_none() {
        return;
    }
    let cfg = Config::default();
    let source = std::fs::read_to_string(&dtx).expect("the documented source");
    let from_dtx = Machine::analyze(&source, Some(&directory.join("array.dtx")), &cfg);
    let extracted = directory.join("array.sty");
    let sty = std::fs::read_to_string(&extracted).expect("what docstrip wrote");
    let from_sty = Machine::analyze(&sty, Some(&extracted), &cfg);

    let names = |analysis: &Analysis, file: &str| -> Vec<String> {
        analysis
            .facts
            .defs
            .iter()
            .filter(|def| analysis.file_name(def.span.file).ends_with(file))
            .map(|def| analysis.interner.name(def.name).to_string())
            .collect()
    };
    let in_dtx = names(&from_dtx, "array.dtx");
    let in_sty = names(&from_sty, "array.sty");
    assert!(!in_sty.is_empty(), "the extracted package defines something");
    for name in &in_sty {
        assert!(in_dtx.contains(name), "\\{name} is defined in the .dtx as well");
    }
    // A definition made by the documented source itself points into the
    // `.dtx`, at the line the code stands on.  Definitions the rollback
    // releases bring in belong to those files, not to this one.
    let own: Vec<_> =
        from_dtx.facts.defs.iter().filter(|def| from_dtx.file_name(def.span.file).ends_with("array.dtx")).collect();
    assert!(!own.is_empty(), "the documented source defines something of its own");
    for def in own {
        let name = from_dtx.interner.name(def.name);
        let line = source.lines().nth(def.span.line as usize - 1).unwrap_or_default();
        // A name built from characters (`\csname`, or any macro that ends in
        // one) stands where the first of its characters that this line wrote
        // stands: that character is part of the name.
        let written = line.chars().nth(def.span.col as usize - 1).is_some_and(|c| name.contains(c));
        assert!(line.contains(name) || written, "line {} of the .dtx defines \\{name}: {line}", def.span.line);
    }
}
