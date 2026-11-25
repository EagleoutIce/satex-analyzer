// Count allocations as the binary does, so `limits.memory` stops a runaway
// analysis in a test instead of the machine running out.
#[global_allocator]
static ALLOCATOR: satex::analysis::budget::Counted = satex::analysis::budget::Counted;

// Integration test crate: every module below was a standalone file in
// tests/ before the merge into one binary, so libtest schedules every
// test across all cores and cargo links one test binary instead of 49.

mod assertions;
mod bibdb;
mod budgets;
mod build_lints;
mod cache_agreement;
mod cache_background;
mod cache_cli;
mod cache_concurrency;
mod cache_semantics;
mod chktex;
mod classes;
mod cli_rules;
mod clones;
mod coldwarm;
mod colors;
mod compile;
#[cfg(unix)]
mod completions;
mod compliance_matrix;
mod corpus;
mod counterexamples;
mod crash;
mod depp;
mod determinism;
mod differential;
mod distribution_guard;
mod spacing_cycle;
// `satex-doc`, this module's target, is only built with the `doc` feature.
#[cfg(feature = "doc")]
mod docs;
mod document_cache;
mod documents;
mod dropped_runs;
mod early;
mod encoding;
mod endinput;
mod engine;
mod engine_trace;
mod engine_truth;
mod explain;
mod fix;
mod fonts;
mod format_flags;
mod gui;
mod incremental;
mod install;
mod join;
mod l3build;
mod latex;
mod layering;
mod layout;
mod libraries;
mod limits;
mod lint_contexts;
mod lists;
mod literate;
mod lsp;
mod lua;
mod manuals;
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
mod quick_fixes;
mod rename;
mod roots;
mod semantics;
mod semantics_coverage;
mod slice;
mod slice_build;
mod stubs;
mod style_prompt;
mod successors;
mod support;
mod tables;
mod tagging;
mod templates;
mod termination;
mod tex_compliance;
mod tex_semantics;
mod trace_range;
mod typed_mistakes;
mod units;
mod updates;
mod warm;
mod write_set;
mod xref;
