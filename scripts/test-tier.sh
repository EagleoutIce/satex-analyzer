#!/usr/bin/env bash
# The integration suite in tiers: `fast` skips the tests listed in
# tests/slow.txt and the known reds of tests/known-red.txt (the daily loop),
# `slow` runs only the slow ones, `reds` runs each known red alone under
# RED_SECONDS (default 300) and says which turned green. `measure` runs the
# whole suite with timings and rewrites tests/slow.txt (every test of
# SLOW_SECONDS or more, default 20). Each run first checks that every listed
# test still exists, so the lists cannot rot.
#   scripts/test-tier.sh fast|slow|reds|measure [libtest args]
# Builds under the build lock only, tests run unlocked (scripts/t.sh; same env).
set -euo pipefail
cd "$(dirname "$0")/.."
tier=${1:?usage: $0 fast|slow|measure [args]}; shift
if [ "$tier" = measure ]; then
    log=$(mktemp -p "${TMPDIR:-target}" tiers.XXXXXX)
    TIMES=1 scripts/t.sh "$@" | tee "$log" || true
    sed -nE 's/^test (\S+) \.\.\. \S+ <([0-9.]+)s>$/\2 \1/p' "$log" |
        awk -v s="${SLOW_SECONDS:-20}" '$1 >= s {print $2}' | sort > tests/slow.txt
    rm -f "$log"; echo "tests/slow.txt: $(wc -l < tests/slow.txt) tests"; exit
fi
mapfile -t slow < <(grep -v '^\s*$' tests/slow.txt)
mapfile -t reds < <(grep -v '^\s*\(#\|$\)' tests/known-red.txt)
stale=$(comm -23 <(printf '%s\n' "${slow[@]}" "${reds[@]}" | sort -u) <(scripts/t.sh --list 2>/dev/null | sed -n 's/: test$//p' | sort))
[ -z "$stale" ] || { echo "tests/slow.txt or tests/known-red.txt names tests that no longer exist:" >&2; echo "$stale" >&2; exit 1; }
if [ "$tier" = reds ]; then
    # Each alone, so one that runs away costs RED_SECONDS, not the run.
    for t in "${reds[@]}"; do
        if timeout "${RED_SECONDS:-300}" scripts/t.sh --exact "$t" >/dev/null 2>&1; then echo "GREEN $t"
        elif [ $? = 124 ]; then echo "TIMEOUT $t"; else echo "red $t"; fi
    done
    exit
fi
case $tier in
    fast) args=(); for t in "${slow[@]}" "${reds[@]}"; do args+=(--skip "$t"); done ;;
    slow) args=("${slow[@]}") ;;
    *) echo "usage: $0 fast|slow|reds|measure [args]" >&2; exit 2 ;;
esac
rc=0
scripts/t.sh --exact "${args[@]}" "$@" || rc=$?
# The unit tests in src/ (CI runs them in shard 1): part of the daily loop.
if [ "$tier" = fast ]; then
    CARGO_BUILD_JOBS=${JOBS:-12} cargo test --profile "${PROFILE:-fast}" --lib -- --test-threads "${THREADS:-12}" || rc=$?
fi
exit $rc
