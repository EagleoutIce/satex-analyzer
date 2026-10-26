#!/usr/bin/env bash
# The integration suite in two tiers: `fast` skips the tests listed in
# tests/slow.txt (the daily loop), `slow` runs only those; together they are
# the whole suite. `measure` runs the whole suite with timings and rewrites
# tests/slow.txt (every test of SLOW_SECONDS or more, default 20). Each run
# first checks that every listed test still exists, so the list cannot rot.
#   scripts/test-tier.sh fast|slow|measure [libtest args]
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
stale=$(comm -23 <(printf '%s\n' "${slow[@]}" | sort) <(scripts/t.sh --list 2>/dev/null | sed -n 's/: test$//p' | sort))
[ -z "$stale" ] || { echo "tests/slow.txt names tests that no longer exist (run measure):" >&2; echo "$stale" >&2; exit 1; }
case $tier in
    fast) args=(); for t in "${slow[@]}"; do args+=(--skip "$t"); done ;;
    slow) args=("${slow[@]}") ;;
    *) echo "usage: $0 fast|slow|measure [args]" >&2; exit 2 ;;
esac
scripts/t.sh --exact "${args[@]}" "$@"
