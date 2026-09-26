#!/usr/bin/env bash
# Runs one shard of the integration test suite.  The shards are balanced by
# how long their targets take; `rest` is every target the other shards do not
# claim, so a new test file lands in a shard instead of being dropped.
set -euo pipefail

usage() {
    echo "usage: $0 <cache|slow|rest>" >&2
    exit 2
}

# `package_cache` interprets the kernel twice; the `slow` targets each drive
# the engine or a whole installation.  Together they are half the wall time.
CACHE_SHARD=(package_cache)
SLOW_SHARD=(no_hang l3build build_lints mutation corpus)

cd "$(dirname "$0")/.."

all=()
for file in tests/*.rs; do
    all+=("$(basename "$file" .rs)")
done

claimed=("${CACHE_SHARD[@]}" "${SLOW_SHARD[@]}")
for target in "${claimed[@]}"; do
    [[ " ${all[*]} " == *" $target "* ]] || { echo "$0: no tests/$target.rs; fix the shards" >&2; exit 1; }
done

args=()
case "${1-}" in
    cache) targets=("${CACHE_SHARD[@]}") ;;
    slow) targets=("${SLOW_SHARD[@]}") ;;
    rest)
        targets=()
        for target in "${all[@]}"; do
            [[ " ${claimed[*]} " == *" $target "* ]] || targets+=("$target")
        done
        # The unit tests are quick and ride along with the small targets.
        args+=(--lib)
        ;;
    *) usage ;;
esac

for target in "${targets[@]}"; do
    args+=(--test "$target")
done

set -x
cargo test --locked --profile "${CARGO_PROFILE:-ci}" --features doc --no-fail-fast "${args[@]}"
