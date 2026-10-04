#!/usr/bin/env bash
# Runs one shard of the integration test suite.  All integration tests build
# into one binary (`tests/it/main.rs`, one `mod` per former test file), so
# shards select by module path instead of by `--test` target.  The shards are
# balanced by how long their modules take; `rest` is every module the other
# shards do not claim, so a new test file lands in a shard instead of being
# dropped.
set -euo pipefail

usage() {
    echo "usage: $0 <cache|slow|compliance|rest>" >&2
    exit 2
}

# `package_cache` interprets the kernel twice; the `slow` modules each drive
# the engine or a whole installation.  Together they are half the wall time.
CACHE_SHARD=(package_cache)
SLOW_SHARD=(no_hang l3build build_lints mutation corpus)
# Held apart from `rest`: it fails on purpose while the known-divergence
# ledger has entries, and CI reports it non-blocking instead of hiding it.
COMPLIANCE_SHARD=(compliance_matrix)

cd "$(dirname "$0")/.."

all=()
for file in tests/it/*.rs; do
    name="$(basename "$file" .rs)"
    [[ "$name" == "main" ]] && continue
    all+=("$name")
done

claimed=("${CACHE_SHARD[@]}" "${SLOW_SHARD[@]}" "${COMPLIANCE_SHARD[@]}")
for target in "${claimed[@]}"; do
    [[ " ${all[*]} " == *" $target "* ]] || { echo "$0: no tests/it/$target.rs; fix the shards" >&2; exit 1; }
done

run_lib=0
case "${1-}" in
    cache) modules=("${CACHE_SHARD[@]}") ;;
    slow) modules=("${SLOW_SHARD[@]}") ;;
    compliance) modules=("${COMPLIANCE_SHARD[@]}") ;;
    rest)
        modules=()
        for target in "${all[@]}"; do
            [[ " ${claimed[*]} " == *" $target "* ]] || modules+=("$target")
        done
        # The unit tests are quick and ride along with the small modules.
        # A filter after `--` applies to every target in the invocation, so
        # `--lib` gets its own unfiltered `cargo test` call.
        run_lib=1
        ;;
    *) usage ;;
esac

filters=()
for module in "${modules[@]}"; do
    filters+=("${module}::")
done

set -x
if [[ "$run_lib" == 1 ]]; then
    cargo test --locked --profile "${CARGO_PROFILE:-ci}" --features doc --no-fail-fast --lib
fi
cargo test --locked --profile "${CARGO_PROFILE:-ci}" --features doc --no-fail-fast --test it -- "${filters[@]}"
