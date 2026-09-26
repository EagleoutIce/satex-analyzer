//! `satex::rename`: a rename has to move every name one declaration made
//! from one key, and every site that spells it, or refuse.

use satex::config::Config;
use satex::lint::fix::{Pos, Sources};
use satex::machine::{Analysis, Machine};
use satex::rename;

// Without a format the catcodes are INITEX's (tex.web § 232).
const PRELUDE: &str = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\catcode`\\^=7\n";

fn installed() -> bool {
    which::which("kpsewhich").is_ok()
}

struct Fixture {
    directory: std::path::PathBuf,
    path: std::path::PathBuf,
    analysis: Analysis,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn write(name: &str, source: &str, cfg: Config) -> Fixture {
    let directory = std::env::temp_dir().join(format!("satex-rename-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("doc.tex");
    std::fs::write(&path, source).unwrap();
    let analysis = Machine::analyze(source, Some(&path), &cfg);
    Fixture { directory, path, analysis }
}

fn plain(name: &str, source: &str) -> Fixture {
    let cfg = Config {
        load_packages: false,
        load_classes: false,
        load_inputs: false,
        load_format: false,
        use_kpsewhich: false,
        ..Config::default()
    };
    write(name, &format!("{PRELUDE}{source}"), cfg)
}

/// The rename of the name at `line:col`, as `line:col` of every site.
fn rename(
    fixture: &Fixture,
    line: u32,
    col: u32,
    name: &str,
    is_command: bool,
    new_name: &str,
) -> Result<Vec<String>, String> {
    let path = fixture.path.to_string_lossy().to_string();
    let sources = Sources::default();
    let plan = rename::plan(&fixture.analysis, &sources, &path, Pos::new(line, col), name, is_command)?;
    let edits = rename::edits(&plan, &fixture.analysis, new_name)?;
    Ok(edits.iter().map(|e| format!("{}:{}={}", e.start.line, e.start.col, e.replacement)).collect())
}

#[test]
fn a_macro_moves_with_every_site_that_spells_it() {
    let fixture = plain(
        "macro",
        "\\def\\greet#1{Hello, #1!}\n\\greet{world}\n\\def\\unused{\\greet{x}}\n\\let\\alias\\greet\n\\csname greet\\endcsname{z}\n",
    );
    let sites = rename(&fixture, 3, 2, "greet", true, "shout").expect("a rename");
    assert_eq!(
        sites,
        ["2:6=shout", "3:2=shout", "4:14=shout", "5:12=shout", "6:9=shout"],
        "the definition, the call, the unexpanded body, the `\\let` and the `\\csname` all spell it"
    );
}

#[test]
fn an_alias_keeps_the_name_it_was_made_from() {
    let fixture = plain("alias", "\\def\\greet#1{Hello, #1!}\n\\let\\alias\\greet\n\\alias{there}\n");
    let sites = rename(&fixture, 4, 2, "alias", true, "other").expect("a rename");
    assert_eq!(sites, ["3:6=other", "4:2=other"], "`\\greet` is another binding and stays");
}

#[test]
fn a_name_another_definition_holds_is_refused() {
    let fixture = plain("clash", "\\def\\greet#1{x}\n\\def\\hello{y}\n\\greet{a}\\hello\n");
    let error = rename(&fixture, 4, 2, "greet", true, "hello").expect_err("a refusal");
    assert!(error.contains("already defined"), "{error}");
}

#[test]
fn an_environment_moves_with_its_two_halves() {
    if !installed() {
        return;
    }
    let cfg = Config { load_classes: true, ..Config::default() };
    let fixture = write(
        "environment",
        "\\documentclass{article}\n\\newenvironment{note}{\\par}{\\par}\n\\begin{document}\n\\begin{note}x\\end{note}\n\\end{document}\n",
        cfg,
    );
    // The key of `\newenvironment`, and `\begin`/`\end` reading it back.
    let sites = rename(&fixture, 4, 8, "note", false, "remark").expect("a rename");
    assert_eq!(sites, ["2:17=remark", "4:8=remark", "4:19=remark"]);
}

#[test]
fn a_counter_moves_with_the_names_it_allocates() {
    if !installed() {
        return;
    }
    let cfg = Config { load_classes: true, ..Config::default() };
    let fixture = write(
        "counter",
        "\\documentclass{article}\n\\newcounter{step}\n\\begin{document}\n\\stepcounter{step}\\thestep\n\\end{document}\n",
        cfg,
    );
    // `\thestep` is `\the` + the key: the key moves inside it too.
    for (line, col, name, is_command) in [(2, 13, "step", false), (4, 15, "step", false), (4, 23, "thestep", true)] {
        let sites = rename(&fixture, line, col, name, is_command, "phase").expect("a rename");
        assert_eq!(sites, ["2:13=phase", "4:14=phase", "4:23=phase"], "from {line}:{col}");
    }
}
