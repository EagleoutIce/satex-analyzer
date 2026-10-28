//! satex-oracle: checks SaTeX's analysis of plain-TeX programs against
//! real `pdftex -ini -etex` runs ([`satex::oracle`]).
//!
//! `satex-oracle FILE.tex…` checks the files; `satex-oracle --generate
//! FROM..TO` checks generated programs by seed, `--print SEED` prints one,
//! `--save DIR` writes each program found uncovered there, and `--reduce`
//! first cuts it down to what still shows its first violation.

use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cfg = satex::oracle::config();
    let seeds = [1, 2, 3, 5, 8, 13, 21, 34];
    let work = std::env::temp_dir().join(format!("satex-oracle-{}", std::process::id()));
    let mut programs: Vec<(String, String)> = Vec::new();
    let mut save: Option<PathBuf> = None;
    let mut reduce = false;
    let (mut precision, mut imprecise) = (false, 0);
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--print" => {
                let seed: u64 = it.next().and_then(|s| s.parse().ok()).unwrap_or(1);
                println!("{}", satex::oracle::generate(seed, 6));
                return;
            }
            "--generate" => {
                let range = it.next().cloned().unwrap_or_else(|| "1..100".into());
                let (from, to) = range.split_once("..").unwrap_or(("1", "100"));
                let (from, to): (u64, u64) = (from.parse().unwrap_or(1), to.parse().unwrap_or(100));
                for seed in from..=to {
                    programs.push((format!("generated/{seed}"), satex::oracle::generate(seed, 6)));
                }
            }
            "--save" => save = it.next().map(PathBuf::from),
            "--reduce" => reduce = true,
            "--precision" => precision = true,
            file => programs.push((file.to_string(), std::fs::read_to_string(file).unwrap_or_default())),
        }
    }
    let mut failed = 0;
    for (i, (name, source)) in programs.iter().enumerate() {
        let Some(report) = satex::oracle::verify(source, &seeds, &cfg, &work.join(i.to_string())) else {
            eprintln!("satex-oracle: pdftex did not run");
            std::process::exit(2);
        };
        imprecise += report.imprecise.len();
        if precision && !report.imprecise.is_empty() {
            println!("{name}: imprecise: {}", report.imprecise.join("; "));
        }
        let violations = report.violations;
        if violations.is_empty() {
            continue;
        }
        failed += 1;
        println!("{name}:");
        for v in violations.iter().take(6) {
            println!("  seed {}: {}", v.seed, v.what);
        }
        let source = match reduce {
            true => {
                let small = satex::oracle::reduce(
                    source,
                    &violations[0].what,
                    &[violations[0].seed],
                    &cfg,
                    &work.join("reduce"),
                );
                println!("  reduced: {small}");
                small
            }
            false => source.clone(),
        };
        if let Some(dir) = &save {
            let _ = std::fs::create_dir_all(dir);
            let file = dir.join(format!("{}.tex", name.replace('/', "-")));
            let _ = std::fs::write(file, &source);
        }
    }
    let _ = std::fs::remove_dir_all(&work);
    println!("{failed} of {} programs not covered; {imprecise} facts every run agreed on left open", programs.len());
    std::process::exit(i32::from(failed > 0));
}
