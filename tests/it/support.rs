//! Shared infrastructure for modules below, not a module of tests itself.
#![allow(dead_code)]

use satex::config::{Config, Load, Texmf};
use satex::machine::Analysis;
use std::path::{Path, PathBuf};

/// `Config::default()` with `limits.seconds = 0` (no wall-clock budget), so
/// tests are bounded only by deterministic step limits and stay independent
/// of machine load. Use this instead of `Config::default()` for any config
/// that drives an actual analysis.
pub fn config() -> Config {
    bounded(Config::default())
}

/// `Config::discover(dir)`, bounded the same way.
pub fn discover(dir: &Path) -> Config {
    bounded(Config::discover(dir))
}

/// No wall-clock limit, so a loaded machine cannot fail a test, and a step
/// and memory budget no test document comes near, so a runaway run fails
/// alone instead of taking the machine's memory.
fn bounded(mut cfg: Config) -> Config {
    cfg.limits.seconds = 0;
    // One fixed clock: runs a second apart, cached or not, agree.
    cfg.clock = satex::config::Clock::At(time::OffsetDateTime::from_unix_timestamp(1_767_268_800).expect("a date"));
    cfg.limits.steps = 300_000_000;
    // Counted for the whole test process (tests/it/main.rs): the run stops
    // itself at 4 GiB, and an allocation past 6 GiB aborts the process, so
    // the test cgroup (10 GB) never has to kill it.
    cfg.limits.memory = bytesize::ByteSize::gib(4);
    satex::budget::set_limit(cfg.limits.memory.as_u64());
    cfg.limits.facts = 1_000_000;
    cfg.limits.vertices = 400_000;
    cfg
}

/// Whether a real TeX installation (`kpsewhich`) is on `PATH`. Tests that
/// need `texmf.kpsewhich` or package/class loading skip themselves when this
/// is `false`.
pub fn installed() -> bool {
    which::which("kpsewhich").is_ok()
}

/// `config()` with package, class, input and format loading all off, and
/// `kpsewhich` off too: nothing but the modeled primitives, so no TeX
/// installation is needed to run the analysis.
pub fn bare() -> Config {
    Config {
        load: Load { packages: false, classes: false, inputs: false, format: false, ..Default::default() },
        texmf: Texmf { kpsewhich: false, ..Default::default() },
        ..config()
    }
}

/// Recreate `dir` empty. Used to give each test its own scratch directory.
pub fn scratch_dir(dir: PathBuf) -> PathBuf {
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

/// Puts the kernel cache `cfg` would read into `cfg.cache.dir`, built at most
/// once per engine and load per test process (and kept across runs, keyed by
/// the interpreter's digest) in `target/scratch/test-kernel`. Package caches
/// are not copied: a test that measures them starts with none.
pub fn seed_kernel(cfg: &Config) {
    static BUILT: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let dir = cfg.cache.dir.clone().expect("a cache directory to seed");
    let shared = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/scratch/test-kernel");
    let key = format!("{:?}", (cfg.engine, &cfg.load, &cfg.kernel, &cfg.texmf));
    {
        let mut built = BUILT.lock().unwrap_or_else(|e| e.into_inner());
        if !built.contains(&key) {
            std::fs::create_dir_all(&shared).expect("the shared kernel directory");
            let mut warm = cfg.clone();
            warm.cache.dir = Some(shared.clone());
            warm.cache.rebuild = false;
            satex::machine::Machine::analyze("\\relax\n", Some(&shared.join("kernel.tex")), &warm);
            built.push(key);
        }
    }
    std::fs::create_dir_all(&dir).expect("a cache directory");
    for entry in std::fs::read_dir(&shared).expect("the shared kernel cache").flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with("format-") && name.to_string_lossy().ends_with(".postcard") {
            let _ = std::fs::copy(entry.path(), dir.join(&name));
        }
    }
}

/// The first definition of `name` in analysis order.
pub fn definition<'a>(analysis: &'a Analysis, name: &str) -> Option<&'a satex::facts::Definition> {
    analysis.facts.defs.iter().find(|d| analysis.interner.name(d.name) == name)
}

/// The replacement text of the first definition of `name`, detokenized.
pub fn body(analysis: &Analysis, name: &str) -> String {
    definition(analysis, name)
        .and_then(|d| d.mac.as_ref())
        .map(|m| satex::tex::detokenize(&m.replacement_text, &analysis.interner))
        .unwrap_or_default()
}

/// Whether `name` has any meaning at all in the environment `analysis` ended
/// with.
pub fn defined(analysis: &Analysis, name: &str) -> bool {
    analysis.interner.lookup(name).is_some_and(|sym| analysis.env.is_defined(sym))
}

/// A table row: the case's name, then its data.
pub type BodyRow = (&'static str, &'static str, &'static [(&'static str, &'static str)]);
pub type PairRow = (&'static str, &'static [(&'static str, &'static str)]);

/// Checks every row, then fails once, naming each row that failed and why.
/// A row that panics counts as failed; the rows after it still run.
pub fn table<R>(rows: &[R], name: impl Fn(&R) -> &str, check: impl Fn(&R) -> Option<String>) {
    let failures: Vec<String> = rows
        .iter()
        .filter_map(|row| {
            let why = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(row))) {
                Ok(why) => why,
                Err(p) => Some(format!(
                    "panicked: {}",
                    p.downcast_ref::<String>().map(String::as_str).or(p.downcast_ref::<&str>().copied()).unwrap_or("?")
                )),
            };
            why.map(|why| format!("{}: {why}", name(row)))
        })
        .collect();
    assert!(failures.is_empty(), "{} of {} rows failed:\n{}", failures.len(), rows.len(), failures.join("\n"));
}

/// Each row's source through `analyze`, then each named macro's body
/// against the expected text.
pub fn body_rows(rows: &[BodyRow], analyze: fn(&str) -> Analysis) {
    table(
        rows,
        |r| r.0,
        |&(_, source, expected)| {
            let analysis = analyze(source);
            let wrong: Vec<String> = expected
                .iter()
                .filter_map(|&(name, want)| {
                    let got = body(&analysis, name);
                    (got != want).then(|| format!("\\{name} is {got:?}, expected {want:?}"))
                })
                .collect();
            (!wrong.is_empty()).then(|| wrong.join("; "))
        },
    );
}

/// Each row's (input, expected) pairs through `check`, which says what is
/// wrong with a pair or `None`.
pub fn pair_rows(rows: &[PairRow], check: impl Fn(&str, &str) -> Option<String>) {
    table(
        rows,
        |r| r.0,
        |&(_, pairs)| {
            let wrong: Vec<String> = pairs.iter().filter_map(|&(input, expected)| check(input, expected)).collect();
            (!wrong.is_empty()).then(|| wrong.join("; "))
        },
    );
}
