//! The soundness oracle over every plain-TeX program the suite has:
//! `pdftex -ini -etex` runs each with several seeds for its unknowns, and
//! SaTeX's end state has to allow what every run ended with
//! ([`satex::oracle`]). A violation names the program, the seed and the
//! fact SaTeX ruled out.

use std::path::PathBuf;

use satex::oracle;

/// Seeds per program: each fixes every `\pdfuniformdeviate` of a run.
const SEEDS: [u32; 6] = [1, 2, 3, 5, 8, 13];

/// Generated programs checked by the suite; `satex-oracle` runs more.
const GENERATED: u64 = 40;

/// `\pdfelapsedtime` is the suite's other unknown; a seed cannot fix it,
/// so the oracle reads it as a small uniform deviate, the same for SaTeX.
fn seedable(source: &str) -> String {
    source.replace("\\pdfelapsedtime", "\\pdfuniformdeviate 9 ")
}

/// Every program, by name: the progressive and write-set tables, and the
/// files in tests/oracle.
fn programs() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (name, source, _) in crate::progressive::EXACT.iter().chain(crate::progressive::COVERED) {
        out.push((format!("progressive/{name}"), format!("{}{source}", crate::progressive::PRE)));
    }
    let tables = [
        crate::write_set::REGISTERS,
        crate::write_set::MEANINGS,
        crate::write_set::CODES,
        crate::write_set::PARAMETERS,
        crate::write_set::NESTING,
    ];
    for &(name, setup, body, check) in tables.iter().flat_map(|t| t.iter()) {
        out.push((format!("write_set/{name}"), crate::write_set::source(setup, body, check)));
    }
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/oracle");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
    files.sort();
    for path in files.into_iter().filter(|p| p.extension().is_some_and(|e| e == "tex")) {
        let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        out.push((format!("oracle/{name}"), std::fs::read_to_string(&path).unwrap_or_default()));
    }
    // Generated programs, a fixed set: their seeds are their names.
    for seed in 1..=GENERATED {
        out.push((format!("generated/{seed}"), oracle::generate(seed, 6)));
    }
    out.into_iter().map(|(n, s)| (n, seedable(&s))).collect()
}

#[test]
fn every_program_covers_every_tex_run() {
    if satex::paths::which("pdftex").is_err() {
        return;
    }
    let cfg = crate::support::bare();
    let root = crate::support::tmp_root().join(format!("satex-oracle-{}", std::process::id()));
    let mut failed = Vec::new();
    let programs = programs();
    for (i, (name, source)) in programs.iter().enumerate() {
        let Some(oracle::Report { violations, .. }) = oracle::verify(source, &SEEDS, &cfg, &root.join(i.to_string()))
        else {
            continue;
        };
        if !violations.is_empty() {
            let shown: Vec<String> =
                violations.iter().take(4).map(|v| format!("seed {}: {}", v.seed, v.what)).collect();
            failed.push(format!("{name}: {}", shown.join("; ")));
        }
    }
    let _ = std::fs::remove_dir_all(&root);
    assert!(failed.is_empty(), "{} of {} programs not covered:\n{}", failed.len(), programs.len(), failed.join("\n"));
}
