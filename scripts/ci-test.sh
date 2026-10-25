#!/usr/bin/env bash
# Runs one shard of the integration suite (`tests/it`, one binary).
#   scripts/ci-test.sh K/N      shard K of N (1-based)
#   scripts/ci-test.sh compliance
# Tests are dealt round-robin from the sorted list, the long ones of
# tests/slow.txt first, so every shard gets a similar share of both.
# A new test lands in a shard by itself.  Shard 1 also runs the unit tests.
# The compliance ledger fails on purpose while it has open entries, so it
# is a shard of its own, reported non-blocking.
set -euo pipefail
cd "$(dirname "$0")/.."

profile="${CARGO_PROFILE:-ci}"
run() { cargo test --locked --profile "$profile" --features doc --no-fail-fast "$@"; }

if [[ "${1-}" == compliance ]]; then
    set -x
    run --test it -- compliance_matrix::
    exit
fi
[[ "${1-}" =~ ^([1-9][0-9]*)/([1-9][0-9]*)$ ]] || { echo "usage: $0 <K/N|compliance>" >&2; exit 2; }
k=${BASH_REMATCH[1]} n=${BASH_REMATCH[2]}
(( k <= n )) || { echo "$0: shard $k of $n" >&2; exit 2; }

mapfile -t tests < <(
    run --test it -- --list 2>/dev/null | sed -n 's/: test$//p' | LC_ALL=C sort |
        awk -v k="$k" -v n="$n" '
            FILENAME == "tests/slow.txt" { slow[$0] = 1; next }
            /^compliance_matrix::/ { next }
            { c = ($0 in slow) ? "s" : "f"; if (i[c]++ % n + 1 == k) print }
        ' tests/slow.txt -
)
(( ${#tests[@]} > 0 )) || { echo "$0: shard $k/$n is empty" >&2; exit 1; }

set -x
if (( k == 1 )); then run --lib; fi
run --test it -- --exact "${tests[@]}"
