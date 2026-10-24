// Count allocations as the binary does, so `limits.memory` stops a runaway
// analysis in a test instead of the machine running out.
#[global_allocator]
static ALLOCATOR: satex::budget::Counted = satex::budget::Counted;

// Integration test crate: every module below was a standalone file in
// tests/ before the merge into one binary, so libtest schedules every
// test across all cores and cargo links one test binary instead of 49.

mod budgets;
mod build_lints;
mod cache_agreement;
mod cache_concurrency;
mod cache_semantics;
mod chktex;
mod classes;
mod colors;
mod compile;
#[cfg(unix)]
mod completions;
mod compliance_matrix;
mod corpus;
mod counterexamples;
mod depp;
mod determinism;
mod differential;
mod distribution_guard;
// `satex-doc`, this module's target, is only built with the `doc` feature.
#[cfg(feature = "doc")]
mod docs;
mod documents;
mod dropped_runs;
mod encoding;
mod endinput;
mod engine;
mod engine_trace;
mod engine_truth;
mod explain;
mod fix;
mod fonts;
mod format_flags;
mod incremental;
mod install;
mod join;
mod l3build;
mod latex;
mod libraries;
mod limits;
mod literate;
mod lsp;
mod lua;
mod monotonicity;
mod mutation;
mod no_hang;
mod no_installation;
mod observed;
mod oracle;
mod package_cache;
mod packages;
mod paths;
mod positions;
mod primitives;
mod produces;
mod profiles;
mod progressive;
mod rename;
mod semantics;
mod slice;
mod slice_build;
mod stubs;
mod support;
mod tables;
mod termination;
mod tex_compliance;
mod trace_range;
mod units;
mod write_set;
