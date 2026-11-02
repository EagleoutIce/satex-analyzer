#!/usr/bin/env bash
# Runs one shard of the integration suite (`tests/it`, one binary).
#   scripts/ci-test.sh K/N      shard K of N (1-based)
#   scripts/ci-test.sh compliance
#   scripts/ci-test.sh lib
# Tests are dealt round-robin from the sorted list, the long ones of
# tests/slow.txt first, so every shard gets a similar share of both.
# A new test lands in a shard by itself.  The compliance ledger fails on
# purpose while it has open entries, so it is a shard of its own, reported
# non-blocking; being short, it also runs the unit tests (`lib`), blocking.
# A shard fails on a failing test that tests/known-red.txt does not list;
# the listed ones run and are reported, and one that passes is named so
# the list shrinks.
set -euo pipefail
cd "$(dirname "$0")/.."

profile="${CARGO_PROFILE:-ci}"
run() { cargo test --locked --profile "$profile" --features doc --no-fail-fast "$@"; }

if [[ "${1-}" == compliance ]]; then
    set -x
    run --test it -- compliance_matrix::
    exit
fi
if [[ "${1-}" == lib ]]; then
    set -x
    run --lib
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

log=target/ci-test-shard-$k.log; mkdir -p target
set +e
(set -x; run --test it -- --exact "${tests[@]}") 2>&1 | tee "$log"
set -e
failed=$(sed -n 's/^test \(.*\) \.\.\. FAILED$/\1/p' "$log" | LC_ALL=C sort -u)
known=$(grep -v '^#' tests/known-red.txt | LC_ALL=C sort -u)
new=$(comm -23 <(printf '%s\n' "$failed" | sed '/^$/d') <(printf '%s\n' "$known"))
for t in "${tests[@]}"; do
    if grep -qxF "$t" <<<"$known" && ! grep -qxF "$t" <<<"$failed"; then echo "known red passes now, drop it from tests/known-red.txt: $t"; fi
done
[[ -z "$failed" ]] || echo "known red: $(comm -12 <(printf '%s\n' "$failed") <(printf '%s\n' "$known") | tr '\n' ' ')"
grep -q '^test result: ' "$log" || { echo "$0: the test binary did not finish" >&2; exit 1; }
[[ -z "$new" ]] || { echo "failing, not known red:"; printf '  %s\n' $new; exit 1; }
