//! l3build bundles: the `build.lua` is run, and what it configures decides
//! where the package under development is found and which engine runs.

use std::path::{Path, PathBuf};

use satex::config::{Config, Engine};
use satex::facts::LoadStatus;
use satex::machine::Machine;

fn installed() -> bool {
    which::which("kpsewhich").is_ok()
}

/// A bundle in a fresh directory: `build.lua`, a `.dtx` with its `.ins`
/// under `src/`, a document and a test.
fn bundle(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("satex-l3build-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::create_dir_all(dir.join("testfiles")).unwrap();
    let write = |file: &str, text: &str| std::fs::write(dir.join(file), text).unwrap();
    write(
        "build.lua",
        "module = \"demo\"\nsourcefiledir = \"src\"\ntypesetfiles = {\"demo-doc.tex\"}\n\
         typesetexe = \"lualatex\"\ncheckengines = {\"xetex\", \"pdftex\"}\n\
         -- l3build's own functions are stubbed, so this does not stop the run\n\
         uploadconfig = { pkg = module, author = uploadconfig.author }\n",
    );
    write(
        "src/demo.dtx",
        "% \\iffalse\n%<*driver>\n\\documentclass{ltxdoc}\n%</driver>\n% \\fi\n\
         %<*package>\n\\ProvidesPackage{demo}\n\\newcommand\\demo{x}\n%</package>\n\
         %<*other>\n\\newcommand\\notdemo{y}\n%</other>\n",
    );
    write("src/demo.ins", "\\input docstrip\n\\generate{\\file{demo.sty}{\\from{demo.dtx}{package}}}\n\\endbatchfile\n");
    write("demo-doc.tex", "\\documentclass{article}\n\\usepackage{demo}\n\\begin{document}\n\\demo\n\\end{document}\n");
    write("testfiles/basic.lvt", "\\documentclass{article}\n\\usepackage{demo}\n\\begin{document}\\demo\\end{document}\n");
    dir
}

fn analyze(path: &Path) -> satex::machine::Analysis {
    let source = std::fs::read_to_string(path).unwrap();
    let cfg = Config::default();
    Machine::analyze(&source, Some(path), &cfg)
}

#[test]
fn the_package_under_development_is_read_from_its_dtx() {
    if !installed() {
        return;
    }
    let dir = bundle("dtx");
    let analysis = analyze(&dir.join("demo-doc.tex"));
    let config = analysis.project.l3build.as_ref().expect("build.lua is read");
    assert_eq!(config.string("module"), Some("demo"));
    let load = analysis.facts.loads.iter().find(|l| l.name == "demo").expect("demo is loaded");
    assert_eq!(load.status, LoadStatus::Read);
    assert!(load.path.as_deref().is_some_and(|p| p.ends_with("demo.dtx")), "{:?}", load.path);
    let defined = |name: &str| {
        analysis.interner.lookup(name).is_some_and(|sym| analysis.env.is_defined(sym))
    };
    assert!(defined("demo"), "the `package` guard is extracted");
    assert!(!defined("notdemo"), "the `other` guard is not what demo.ins generates");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn documents_are_typeset_with_typesetexe_and_tests_checked_with_the_first_engine() {
    let dir = bundle("engines");
    let doc = analyze(&dir.join("demo-doc.tex"));
    assert_eq!(doc.plugins.engine, Engine::LuaTeX);
    let test = analyze(&dir.join("testfiles/basic.lvt"));
    assert_eq!(test.plugins.engine, Engine::XeTeX);
    let config = test.project.l3build.as_ref().expect("found from a subdirectory");
    assert_eq!(config.check_engines(), vec![Engine::XeTeX, Engine::PdfTeX]);
    assert_eq!(config.tests().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn defaults_come_from_l3build_itself() {
    if which::which("texlua").is_err() || !installed() {
        return;
    }
    let dir = bundle("defaults");
    let config = satex::plugin::l3build::L3build::read(&dir).expect("build.lua");
    assert_eq!(config.how, "texlua");
    // l3build-variables.lua: `unpackfiles = unpackfiles or {"*.ins"}`.
    assert_eq!(config.list("unpackfiles"), vec!["*.ins".to_string()]);
    assert!(config.unpacked().contains_key("demo.sty"));
    let _ = std::fs::remove_dir_all(&dir);
}
