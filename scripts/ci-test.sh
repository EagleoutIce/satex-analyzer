#!/usr/bin/env bash
# Runs one shard of the integration suite (`tests/it`, one binary).
#   scripts/ci-test.sh K/N      shard K of N (1-based)
#   scripts/ci-test.sh compliance
#   scripts/ci-test.sh lib
#   scripts/ci-test.sh reds     the known-red tests, at most ${REDS_SECONDS:-1200} s
#   scripts/ci-test.sh quick <filters>   the given modules, without the doc feature
# Tests are dealt longest first onto the least loaded shard by the seconds in
# tests/times.txt, so the shards end together.  The compliance ledger fails on
# purpose while it has open entries, so it is a shard of its own, reported
# non-blocking; being short, it also runs the unit tests (`lib`), blocking.
# A shard fails on any failing test; the ones tests/known-red.txt lists run
# only in `reds` (non-blocking, time-boxed: an open one may hang), where one
# that passes is named so the list shrinks.  A red that ends the test binary (stack overflow,
# abort) is a crash: `reds` then exits 2, which the workflow does not forgive, and no
# `crash::` test may be listed at all.  A known-red test that tests/green-baseline.txt records
# as once green is flagged "REGRESSION": it broke, it was not always open.
# tests/known-red-tl<year>.txt lists reds of one TeX Live only; they count as known there.
# Logs go to ${CI_LOG_DIR:-target}; passing document and budget tests append
# their analysis steps to steps-<name>.txt there (scripts/regress.sh reads them).
set -euo pipefail
cd "$(dirname "$0")/.."
# TEXLIVE=<year|dir> picks the TeX Live; tests/texlive.yaml names the tests that do not apply to it.
source scripts/texlive.sh
texlive_use
tl=$(texlive_year)
skipped=$(texlive_skips "$tl")
while IFS=$'\t' read -r t why; do [[ -z "$t" ]] || echo "not on TeX Live $tl (tests/texlive.yaml): $t: $why"; done <<<"$skipped"
skipped=$(cut -f1 <<<"$skipped" | sed '/^$/d')

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
    # Plus the reds of this TeX Live only (tests/known-red-tl<year>.txt).
    known=$(cat tests/known-red.txt ${tl:+$(ls tests/known-red-tl"$tl".txt 2>/dev/null)} | grep -v '^#' | LC_ALL=C sort -u)
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
    # A crash is never a known red: the crash tests are not listed, and a binary that died
    # (stack overflow, abort, out of memory) fails the job whatever it was running.
    if grep -q '^crash::' <<<"$known"; then echo "$0: tests/known-red.txt lists a crash:: test: fix the crash" >&2; return 1; fi
    if grep -qE 'has overflowed its stack|\(signal: (4|6|7|8|11),|memory allocation of [0-9]+ bytes failed' "$log"; then
        echo "$0: CRASH: the test binary died (see $log): the test it ran last is a crash, not a red" >&2
        return 2
    fi
    grep -q '^test result: ' "$log" || { echo "$0: the test binary did not finish" >&2; return 1; }
    [[ -z "$new" ]] || { echo "failing, not known red:"; printf '  %s\n' $new; return 1; }
}

if [[ "${1-}" == quick ]]; then
    shift
    # Known reds run in `reds` only: an open one may hang.
    skips=(); while read -r t; do skips+=(--skip "$t"); done < <({ grep -v '^#' tests/known-red.txt; echo "$skipped"; } | sed '/^$/d')
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

# Longest first onto the least loaded shard, by tests/times.txt (an unmeasured test counts
# 60 s if tests/slow.txt names it, else one); every shard computes the same deal.
mapfile -t tests < <(
    it --list 2>/dev/null | sed -n 's/: test$//p' | LC_ALL=C sort |
        awk '
            FILENAME == "tests/times.txt" { t[$2] = $1; next }
            FILENAME == "tests/slow.txt" { slow[$0] = 1; next }
            FILENAME == "tests/known-red.txt" || FILENAME == "/dev/fd/3" { red[$0] = 1; next }
            $0 in red { next }
            /^compliance_matrix::/ { next }
            { print ($0 in t ? t[$0] : ($0 in slow ? 60 : 1)), $0 }
        ' tests/times.txt tests/slow.txt tests/known-red.txt /dev/fd/3 - 3<<<"$skipped" |
        LC_ALL=C sort -k1,1gr -k2,2 |
        awk -v k="$k" -v n="$n" '
            { b = 1; for (i = 2; i <= n; i++) if (s[i] < s[b]) b = i; s[b] += $1; if (b == k) print $2 }
        '
)
(( ${#tests[@]} > 0 )) || { echo "$0: shard $k/$n is empty" >&2; exit 1; }

gated "shard-$k" it --exact "${tests[@]}"
