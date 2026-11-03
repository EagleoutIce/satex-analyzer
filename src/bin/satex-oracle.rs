//! satex-oracle: checks SaTeX's analysis of plain-TeX programs against
//! real `pdftex -ini -etex` runs ([`satex::oracle`]).
//!
//! `satex-oracle FILE.tex…` checks the files; `satex-oracle --generate
//! FROM..TO` checks generated programs by seed, `--print SEED` prints one,
//! `--save DIR` writes each program found uncovered there, and `--reduce`
//! first cuts it down to what still shows its first violation.
//!
//! `satex-oracle --slice [--jobs N] [--sample N] [--seed S] [--trim] [--work
//! DIR] PATH…` checks slices instead ([`satex::oracle::slice`]): every main
//! file under each PATH is sliced at each line, and each standalone slice
//! has to typeset its line exactly as the original.  The report goes to
//! DIR/report.txt, and a second run resumes where one stopped.

use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--slice") {
        std::process::exit(slices(&args[1..]));
    }
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
            println!("{name}: imprecise: {}", report.imprecise.join(", "));
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
    println!("{failed} of {} programs not covered, {imprecise} facts every run agreed on left open", programs.len());
    std::process::exit(i32::from(failed > 0));
}

/// The main files under `path`: a `.tex` file that starts a document.
fn main_files(path: &std::path::Path) -> Vec<PathBuf> {
    let starts = |p: &std::path::Path| {
        let text = std::fs::read_to_string(p).unwrap_or_default();
        text.contains("\\documentclass") || text.contains("\\bye") || text.contains("program = tex")
    };
    if path.is_file() {
        return vec![path.to_path_buf()];
    }
    let mut out: Vec<PathBuf> =
        walk(path).into_iter().filter(|p| p.extension().is_some_and(|e| e == "tex") && starts(p)).collect();
    out.sort();
    out
}

fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() { out.extend(walk(&path)) } else { out.push(path) }
    }
    out
}

fn slices(args: &[String]) -> i32 {
    let (mut jobs, mut sample, mut seed, mut trim) = (8usize, 0usize, 1u64, false);
    let mut work = PathBuf::from("target/scratch/slice-oracle");
    let mut paths = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut number = || it.next().and_then(|n| n.parse::<u64>().ok()).unwrap_or(0);
        match arg.as_str() {
            "--jobs" => jobs = number() as usize,
            "--sample" => sample = number() as usize,
            "--seed" => seed = number(),
            "--trim" => trim = true,
            "--work" => work = it.next().map(PathBuf::from).unwrap_or(work),
            path => paths.extend(main_files(std::path::Path::new(path))),
        }
    }
    let mut report = String::new();
    let mut bad = 0;
    for main in &paths {
        let dir = main.parent().unwrap_or(std::path::Path::new("."));
        let name = main.file_name().unwrap_or_default().to_string_lossy().to_string();
        let key = main.to_string_lossy().replace(['/', '.'], "_");
        let doc = match satex::oracle::slice::Document::prepare(dir, &name, &work.join(&key)) {
            Ok(doc) => doc,
            Err(why) => {
                report.push_str(&format!("{}: skipped, {why}\n", main.display()));
                continue;
            }
        };
        let mut criteria = doc.criteria();
        // A sample by seed: the same seed picks the same lines.
        if sample > 0 && criteria.len() > sample {
            let mut state = seed;
            let mut picked = Vec::new();
            while picked.len() < sample {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                picked.push(criteria.swap_remove((state >> 33) as usize % criteria.len()));
            }
            picked.sort();
            criteria = picked;
        }
        let sliced = doc.slice_all(&criteria, trim);
        let verdicts = satex::oracle::slice::check_all(&doc, &sliced, jobs);
        let count = |w: &str| verdicts.iter().filter(|v| v.word() == w).count();
        let line = format!(
            "{}: {} pass, {} empty, {} mismatch, {} fail\n",
            main.display(),
            count("pass"),
            count("empty"),
            count("mismatch"),
            count("fail")
        );
        print!("{line}");
        report.push_str(&line);
        for ((file, at, slice), v) in sliced.iter().zip(&verdicts) {
            if let satex::oracle::slice::Verdict::Mismatch(why) | satex::oracle::slice::Verdict::Fail(why) = v {
                bad += 1;
                report.push_str(&format!(
                    "  {} {file}:{at}: {} ({})\n",
                    v.word(),
                    why.lines().next().unwrap_or_default(),
                    slice.display()
                ));
            }
        }
    }
    let _ = std::fs::create_dir_all(&work);
    let _ = std::fs::write(work.join("report.txt"), &report);
    println!("report: {}", work.join("report.txt").display());
    i32::from(bad > 0)
}
