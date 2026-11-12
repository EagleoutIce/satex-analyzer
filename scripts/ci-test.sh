#!/usr/bin/env bash
# Runs one shard of the integration suite (`tests/it`, one binary).
#   scripts/ci-test.sh K/N      shard K of N (1-based)
#   scripts/ci-test.sh compliance
#   scripts/ci-test.sh lib
#   scripts/ci-test.sh reds     the known-red tests, at most ${REDS_SECONDS:-1200} s
#   scripts/ci-test.sh quick <filters>   the given modules, without the doc feature
# Tests are dealt round-robin from the sorted list, the long ones of
# tests/slow.txt first, so every shard gets a similar share of both.
# A new test lands in a shard by itself.  The compliance ledger fails on
# purpose while it has open entries, so it is a shard of its own, reported
# non-blocking; being short, it also runs the unit tests (`lib`), blocking.
# A shard fails on any failing test; the ones tests/known-red.txt lists run
# only in `reds` (non-blocking, time-boxed: an open one may hang), where one
# that passes is named so the list shrinks.  A known-red test that tests/green-baseline.txt records
# as once green is flagged "REGRESSION": it broke, it was not always open.
# Logs go to ${CI_LOG_DIR:-target}; passing document and budget tests append
# their analysis steps to steps-<name>.txt there (scripts/regress.sh reads them).
set -euo pipefail
cd "$(dirname "$0")/.."

profile="${CARGO_PROFILE:-ci}"
# The integration and unit test binaries: prebuilt ones when CI's build job handed them over
# ($IT_BIN, $LIB_BIN), else built here.
# A prebuilt binary runs without cargo, so without .cargo/config.toml's [env]: take its test-thread stack from
# there (a kernel build nests deeper than the 2 MiB default holds).
export RUST_MIN_STACK="${RUST_MIN_STACK:-$(sed -n 's/^RUST_MIN_STACK = "\(.*\)"/\1/p' .cargo/config.toml)}"
if [[ -n "${IT_BIN-}" ]]; then IT=("$IT_BIN"); else IT=(cargo test --locked --profile "$profile" --features doc --no-fail-fast --test it --); fi
if [[ -n "${LIB_BIN-}" ]]; then LIB=("$LIB_BIN"); else LIB=(cargo test --locked --profile "$profile" --features doc --no-fail-fast --lib --); fi
it() { "${IT[@]}" "$@"; }
lib() { "${LIB[@]}" "$@"; }

# Runs a test command; fails on a failing test tests/known-red.txt does not list.
gated() {
    local logs=${CI_LOG_DIR:-target} name=$1; shift
    local log=$logs/ci-test-$name.log
    mkdir -p "$logs"
    export SATEX_STEPS_LOG=$logs/steps-$name.txt
    : >"$SATEX_STEPS_LOG"
    set +e
    (set -x; "$@") 2>&1 | tee "$log"
    set -e
    local failed known new
    failed=$(sed -n 's/^test \(.*\) \.\.\. FAILED$/\1/p' "$log" | LC_ALL=C sort -u)
    known=$(grep -v '^#' tests/known-red.txt | LC_ALL=C sort -u)
    new=$(comm -23 <(printf '%s\n' "$failed" | sed '/^$/d') <(printf '%s\n' "$known"))
    while read -r t; do
        if [[ -n "$t" ]] && grep -q "^test $t \.\.\. ok$" "$log"; then echo "known red passes now, drop it from tests/known-red.txt: $t"; fi
    done <<<"$known"
    [[ -z "$failed" ]] || echo "known red: $(comm -12 <(printf '%s\n' "$failed") <(printf '%s\n' "$known") | tr '\n' ' ')"
    if [[ -f tests/green-baseline.txt ]]; then
        local reg
        reg=$(comm -12 <(comm -12 <(printf '%s\n' "$failed") <(printf '%s\n' "$known")) <(LC_ALL=C sort -u tests/green-baseline.txt))
        [[ -z "$reg" ]] || { echo "REGRESSION, green in tests/green-baseline.txt, now red (fix it, do not list it):"; printf '  %s\n' $reg; }
    fi
    grep -q '^test result: ' "$log" || { echo "$0: the test binary did not finish" >&2; return 1; }
    [[ -z "$new" ]] || { echo "failing, not known red:"; printf '  %s\n' $new; return 1; }
}

if [[ "${1-}" == quick ]]; then
    shift
    # Known reds run in `reds` only: an open one may hang.
    skips=(); while read -r t; do skips+=(--skip "$t"); done < <(grep -v '^#' tests/known-red.txt | sed '/^$/d')
    gated quick cargo test --locked --profile "$profile" --no-fail-fast --test it -- "$@" "${skips[@]}"
    exit
fi
if [[ "${1-}" == compliance ]]; then
    set -x
    it compliance_matrix::
    exit
fi
if [[ "${1-}" == reds ]]; then
    mapfile -t reds < <(grep -v '^#' tests/known-red.txt | sed '/^$/d')
    gated reds timeout "${REDS_SECONDS:-1200}" "${IT[@]}" --exact "${reds[@]}"
    exit
fi
if [[ "${1-}" == lib ]]; then
    set -x
    lib
    exit
fi
[[ "${1-}" =~ ^([1-9][0-9]*)/([1-9][0-9]*)$ ]] || { echo "usage: $0 <K/N|compliance>" >&2; exit 2; }
k=${BASH_REMATCH[1]} n=${BASH_REMATCH[2]}
(( k <= n )) || { echo "$0: shard $k of $n" >&2; exit 2; }

mapfile -t tests < <(
    it --list 2>/dev/null | sed -n 's/: test$//p' | LC_ALL=C sort |
        awk -v k="$k" -v n="$n" '
            FILENAME == "tests/slow.txt" { slow[$0] = 1; next }
            FILENAME == "tests/known-red.txt" { red[$0] = 1; next }
            $0 in red { next }
            /^compliance_matrix::/ { next }
            { c = ($0 in slow) ? "s" : "f"; if (i[c]++ % n + 1 == k) print }
        ' tests/slow.txt tests/known-red.txt -
)
(( ${#tests[@]} > 0 )) || { echo "$0: shard $k/$n is empty" >&2; exit 1; }

gated "shard-$k" it --exact "${tests[@]}"
