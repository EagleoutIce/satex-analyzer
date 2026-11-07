//! The kernels every engine's tests start from, built in one place.
//!
//! Interpreting `latex.ltx` or `plain.tex` takes minutes, so its result is cached (a format cache) and every
//! later run starts from it. CI builds these once per change (the `build` job runs this test), keeps them in
//! GitHub's cache and hands them to every shard: a shard then only reads them, never builds one again.

use satex::analysis::machine::Machine;
use satex::plugins::Kernel;
use satex::project::config::Engine;

/// Each engine's LaTeX kernel and the plain kernel build, and a run on each starts from a format.
#[test]
fn the_suite_s_kernels_build() {
    if !crate::support::installed() {
        return;
    }
    let kernels = [
        (Engine::PdfTeX, Kernel::Latex),
        (Engine::XeTeX, Kernel::Latex),
        (Engine::LuaTeX, Kernel::Latex),
        (Engine::PdfTeX, Kernel::Plain),
    ];
    for (engine, kernel) in kernels {
        let mut cfg = crate::support::config();
        cfg.engine = Some(engine);
        cfg.kernel = Some(kernel);
        let analysis = Machine::analyze("\\relax\n", None, &cfg);
        assert!(analysis.format.is_some(), "{engine:?} {kernel:?}: no kernel format");
    }
    // the kernel `seed_kernel` copies into each budget's own cache directory
    let mut seeded = crate::support::config();
    seeded.cache.dir = Some(crate::support::test_cache().join("seed"));
    crate::support::seed_kernel(&seeded);
}
