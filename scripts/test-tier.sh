#!/usr/bin/env bash
# The integration suite in tiers, chosen by the seconds in tests/times.txt:
#   smoke    the per-edit loop: quick with QUICK_SECONDS 0.3, QUICK_MIN 0 and
#            QUICK_NEW 0 (tests not in tests/times.txt left out: some take minutes)
#   quick    the edit loop: tests under QUICK_SECONDS (1), the QUICK_MIN (1)
#            cheapest of every module, and tests not measured yet
#   changed  the per-edit loop: the tests tests/changed-map.txt maps the files of
#            git diff HEAD (and untracked ones) to, cheapest first; a src/ path
#            without a rule adds the quick set, a rule `all` the fast set
#   fast     the gate before a commit: all but tests/slow.txt and
#            tests/known-red.txt, plus the unit tests in src/
#   slow     only tests/slow.txt
#   reds     each known red alone under RED_SECONDS (300): which turned green
#   measure  fast and slow with timings: rewrites tests/times.txt and
#            tests/slow.txt (SLOW_SECONDS or more, default 20)
# Every run records what failed in target/scratch/last-failed.txt (one name per
# line); the next run starts with those tests, and FAIL_FAST=1 (always with
# changed) runs the rest in cheap-then-dear batches and stops after the first
# batch with a failure.
#   scripts/test-tier.sh TIER [filter...] [-- libtest args]
# A filter keeps the tests whose name contains it. One build, then SHARDS
# processes (quick 2, else 4) of THREADS test threads (3), balanced by time.
# Each shard takes its own target/scratch/lock2.sh slot when that exists and
# this script does not already run in one (cap.sh sets SATEX_SLOT). A shard
# whose process is killed (OOM) reruns its unfinished tests, then one at a
# time; what still does not finish is reported. All shards share the
# suite's caches (target/scratch/test-cache), which lock their builds.
# Env as scripts/t.sh (PROFILE, JOBS, CARGO_TARGET_DIR), and SHARD_SECONDS
# (1800), the most one shard may run.
set -euo pipefail
cd "$(dirname "$0")/.."
export LC_ALL=C
tier=${1:?usage: $0 smoke|quick|changed|fast|slow|reds|measure [filter...] [-- libtest args]}; shift
[ "$tier" != smoke ] || { tier=quick; QUICK_SECONDS=${QUICK_SECONDS:-0.3} QUICK_MIN=${QUICK_MIN:-0} QUICK_NEW=${QUICK_NEW:-0}; }
filters=(); while [ $# -gt 0 ] && [ "$1" != -- ]; do filters+=("$1"); shift; done
[ "${1:-}" != -- ] || shift
pass=("$@")
names() { grep -v '^\s*\(#\|$\)' "$1" || true; }
mapfile -t slow < <(names tests/slow.txt)
mapfile -t reds < <(names tests/known-red.txt)
if [ "$tier" = reds ]; then
    # Each alone, so one that runs away costs RED_SECONDS, not the run.
    for t in "${reds[@]}"; do
        if timeout "${RED_SECONDS:-300}" scripts/t.sh --exact "$t" >/dev/null 2>&1; then echo "GREEN $t"
        elif [ $? = 124 ]; then echo "TIMEOUT $t"; else echo "red $t"; fi
    done
    exit
fi
# The main checkout's slots, from a linked worktree too.
lock2=$(git rev-parse --path-format=absolute --git-common-dir)/../target/scratch/lock2.sh
slot=()
[ -n "${SATEX_SLOT:-}" ] || [ ! -x "$lock2" ] || slot=("$lock2")
in_slot() { CAP_SECONDS=${SHARD_SECONDS:-1800} "${slot[@]}" "$@"; }
# One build, in a slot: the integration binary and the unit tests.
mapfile -t bins < <(CAP=0 BUILD_ONLY=1 in_slot scripts/t.sh)
it=$(printf '%s\n' "${bins[@]}" | grep '/it-' || true); lib=$(printf '%s\n' "${bins[@]}" | grep -v '/it-' || true)
[ -n "$it" ] || { echo "no test binary built" >&2; exit 1; }
export RUST_MIN_STACK=${RUST_MIN_STACK:-$(sed -n 's/^RUST_MIN_STACK *= *"\([0-9]*\)"/\1/p' .cargo/config.toml)}
all=$("$it" --list 2>/dev/null | sed -n 's/: test$//p' | sort)
stale=$(comm -23 <(printf '%s\n' "${slow[@]}" "${reds[@]}" | sort -u) <(echo "$all"))
[ -z "$stale" ] || { echo "tests/slow.txt or tests/known-red.txt names tests that no longer exist:" >&2; echo "$stale" >&2; exit 1; }
skip=$(printf '%s\n' "${slow[@]}" "${reds[@]}" | sort -u)
# The quick tier's set of the tests on stdin.
quick_set() {
    local timed
    timed=$(join -a1 -1 1 -2 2 -o 0,2.1 -e - <(sort) <(sort -k2,2 tests/times.txt 2>/dev/null))
    { awk -v q="${QUICK_SECONDS:-1}" -v new="${QUICK_NEW:-1}" '$2 == "-" ? new : $2 < q { print $1 }' <<<"$timed"
      awk '$2 != "-" { m = $1; sub(/::.*/, "", m); print m, $2, $1 }' <<<"$timed" |
          sort -k1,1 -k2,2g | awk -v k="${QUICK_MIN:-1}" 'n[$1]++ < k { print $3 }'; } | sort -u
}
# The tests the changed files map to: stdout the filters, or `all`; `quick` when a src/ path has no rule.
changed_filters() {
    local f glob rest hit all=0 quick=0 re='^tests/it/([a-z0-9_]+)\.rs$'
    while read -r f; do
        [ -n "$f" ] || continue
        hit=0
        while read -r glob rest; do
            [[ -z $glob || $glob == '#'* ]] && continue
            # shellcheck disable=SC2053
            if [[ $f == $glob ]]; then hit=1; if [ "$rest" = all ]; then all=1; else printf '%s\n' $rest; fi; break; fi
        done <tests/changed-map.txt
        [ "$hit" = 0 ] || continue
        if [[ $f =~ $re ]]; then echo "${BASH_REMATCH[1]}::"
        elif [[ $f == src/* ]]; then quick=1; echo "no rule for $f: adding the quick tier" >&2; fi
    done < <({ git diff --name-only HEAD; git ls-files -o --exclude-standard; } | sort -u)
    [ "$all" = 0 ] || echo all
    [ "$quick" = 0 ] || echo quick
}
case $tier in
    quick|changed|fast|measure) set_=$(comm -23 <(echo "$all") <(echo "$skip")) ;;&
    measure) set_=$(printf '%s\n' "$set_" "${slow[@]}" | sort -u) ;;
    slow) set_=$(printf '%s\n' "${slow[@]}") ;;
    quick) set_=$(quick_set <<<"$set_") ;;
    changed) mapfile -t cf < <(changed_filters | sort -u)
        if printf '%s\n' "${cf[@]}" | grep -qx all; then :
        else
            sel=$(for f in "${cf[@]}"; do [ "$f" = quick ] || grep -F -- "$f" <<<"$set_" || true; done)
            [ "${#cf[@]}" -gt 0 ] && ! printf '%s\n' "${cf[@]}" | grep -qx quick || sel=$({ echo "$sel"; quick_set <<<"$set_"; } | grep . || true)
            set_=$(sort -u <<<"$sel")
            [ -n "$set_" ] || { echo "no changed file maps to tests"; exit 0; }
            echo "changed: ${cf[*]}" >&2
        fi ;;
    fast) ;;
    *) echo "usage: $0 smoke|quick|changed|fast|slow|reds|measure [filter...] [-- libtest args]" >&2; exit 2 ;;
esac
for f in "${filters[@]}"; do set_=$(grep -F -- "$f" <<<"$set_" || true); done
[ -n "$set_" ] || { echo "no tests selected" >&2; exit 1; }
[ "$tier" != quick ] || [ "${#filters[@]}" -gt 0 ] || [ -s tests/times.txt ] || echo "tests/times.txt is empty: run measure" >&2
shards=${SHARDS:-$([ "$tier" = quick ] && echo 2 || echo 4)}
threads=${THREADS:-3}
mkdir -p "${CARGO_TARGET_DIR:-target}"; out=$(mktemp -d -p "${CARGO_TARGET_DIR:-target}" tier.XXXXXX)
# Runs the listed tests; reruns the unfinished ones of a killed process,
# first with the same threads, then one at a time.
shard() {
    local list=$1 log=${1%.list}.log t rc
    : >"$log"
    for t in "$threads" "$threads" 1; do
        rc=0
        mapfile -t todo < <(comm -23 <(sort "$list") <(sed -n 's/^test \(.*\) \.\.\. \(ok\|FAILED\|ignored\).*/\1/p' "$log" | sort))
        [ "${#todo[@]}" -gt 0 ] || return 0
        in_slot env RUSTC_BOOTSTRAP=1 "$it" --exact --test-threads "$t" -Z unstable-options --report-time "${pass[@]}" "${todo[@]}" >>"$log" 2>&1 || rc=$?
        # 137: SIGKILL, the OOM killer. Any other end (a failure, a timeout) is final.
        [ "$rc" = 137 ] || return 0
        echo "shard $(basename "$list" .list): killed (OOM?), rerunning its unfinished tests" >&2
    done
}
# Runs the tests on stdin as one phase: longest first onto the least loaded
# shard (unmeasured tests count a second). Returns 1 if a test failed.
libstarted=
phase() {
    local d="$out/$1" list tests
    tests=$(cat); [ -n "$tests" ] || return 0
    mkdir -p "$d"
    join -a1 -1 1 -2 2 -o 0,2.1 -e 1 <(sort <<<"$tests") <(sort -k2,2 tests/times.txt 2>/dev/null) | sort -k2 -rn |
        awk -v n="$shards" -v d="$d" '{ b = 1; for (i = 2; i <= n; i++) if (s[i] < s[b]) b = i; s[b] += $2; print $1 > (d "/" b ".list") }'
    for list in "$d"/*.list; do shard "$list" & done
    if [ "$tier" = fast ] && [ -n "$lib" ] && [ "${#filters[@]}" = 0 ] && [ -z "$libstarted" ]; then
        libstarted=1; in_slot "$lib" --test-threads "$threads" >"$out/lib.out" 2>&1 &
    fi
    wait
    ! grep -qs '^test .* \.\.\. FAILED' "$d"/*.log
}
start=$(date +%s)
lastfile=target/scratch/last-failed.txt
prev=$(sort -u "$lastfile" 2>/dev/null || true)
first=$(comm -12 <(echo "$set_" | sort) <(echo "$prev"))
rest=$(comm -23 <(echo "$set_" | sort) <(echo "$first"))
ok=1
if [ -n "$first" ]; then echo "$(echo "$first" | wc -l) failed last time first" >&2; phase 1 <<<"$first" || ok=0; fi
if [ "$ok" = 0 ] && [ -n "${FAIL_FAST:-}" ]; then :
elif [ "$tier" = changed ] || [ -n "${FAIL_FAST:-}" ]; then
    # Cheap batch, then the dear one; unmeasured tests count as dear.
    cheap=$(join -1 1 -2 2 -o 0,2.1 <(echo "$rest" | sort) <(sort -k2,2 tests/times.txt 2>/dev/null) | awk -v q="${CHEAP_SECONDS:-1}" '$2 < q { print $1 }')
    dear=$(comm -23 <(echo "$rest" | sort) <(echo "$cheap" | sort))
    phase 2 <<<"$cheap" || ok=0
    if [ "$ok" = 1 ] || [ -z "${FAIL_FAST:-}" ]; then phase 3 <<<"$dear" || ok=0; fi
else
    phase 2 <<<"$rest" || ok=0
fi
# Merged result: one line per test, like libtest's, then the verdict.
cat "$out"/*/*.log | grep -E '^test .* \.\.\. (ok|FAILED|ignored)' | sort -u >"$out/results"
failed=$(sed -n 's/^test \(.*\) \.\.\. FAILED.*/\1/p' "$out/results")
ran=$(sed -n 's/^test \(.*\) \.\.\. .*/\1/p' "$out/results" | sort -u)
# With FAIL_FAST the skipped batches are not unfinished.
[ -z "${FAIL_FAST:-}" ] || [ "$ok" = 1 ] || set_=$ran
unfinished=$(comm -23 <(echo "$set_" | sort) <(sed -n 's/^test \(.*\) \.\.\. .*/\1/p' "$out/results" | sort -u))
rc=0
# What failed or did not finish stays in last-failed.txt; the rest of this run's tests leave it.
mkdir -p target/scratch
{ comm -23 <(echo "$prev") <(echo "$set_" | sort); echo "$failed"; echo "$unfinished"; } | grep . | sort -u >"$lastfile" || : >"$lastfile"
for t in $failed; do sed -n "/^---- $t stdout ----\$/,/^\(---- \|failures:\$\)/p" "$out"/*/*.log | sed '$d'; done
echo "$(grep -c ' ok' "$out/results" || true) passed, $(echo -n "$failed" | grep -c . || true) failed, $(echo -n "$unfinished" | grep -c . || true) unfinished of $(echo "$set_" | wc -l) ($tier, $shards shards x $threads threads, $(($(date +%s) - start)) s)"
[ -z "$failed" ] || { echo "FAILED:"; echo "$failed" | sed 's/^/  /'; rc=1; }
[ -z "$unfinished" ] || { echo "UNFINISHED (killed or timed out):"; echo "$unfinished" | sed 's/^/  /'; rc=1; }
if [ -f "$out/lib.out" ]; then
    grep -q '^test result: ok' "$out/lib.out" || { grep -E '^test .* FAILED|^test result' "$out/lib.out"; echo "unit tests FAILED"; rc=1; }
    grep '^test result' "$out/lib.out" | sed 's/^/unit tests: /'
fi
if [ "$tier" = measure ]; then
    # This run's seconds, then the old ones of tests it did not run.
    sed -nE 's/^test (\S+) \.\.\. \S+ <([0-9.]+)s>$/\2 \1/p' "$out/results" >"$out/new"
    { cat "$out/new"; join -v2 -1 2 -2 2 -o 2.1,2.2 <(sort -k2 "$out/new") <(sort -k2,2 tests/times.txt 2>/dev/null); } | sort -k2 >tests/times.txt
    awk -v s="${SLOW_SECONDS:-20}" '$1 >= s {print $2}' tests/times.txt | sort >tests/slow.txt
    echo "tests/times.txt: $(wc -l <tests/times.txt) tests, tests/slow.txt: $(wc -l <tests/slow.txt)"
fi
[ -n "${KEEP:-}" ] && echo "logs: $out" || rm -rf "$out"
exit $rc
