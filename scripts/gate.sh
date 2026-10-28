#!/usr/bin/env bash
# Quick acceptance gate for a change: step budgets, memory bound, determinism
# and termination, all on the fast profile. Run from a satex checkout:
#   scripts/gate.sh [cargo target dir]
# Exits non-zero on the first failing stage.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_TARGET_DIR="${1:-${CARGO_TARGET_DIR:-target/gate}}"
cap=(systemd-run --user --scope -q -p MemoryMax="${GATE_MEMORY:-8G}" -p MemorySwapMax=0)
start=$(date +%s)
stage() { printf '== %s (%ss)\n' "$1" "$(( $(date +%s) - start ))"; }

stage build
"${cap[@]}" cargo build --profile fast --bin satex --tests -q

stage tests
"${cap[@]}" cargo test --profile fast -q --test it -- \
    budgets:: determinism:: limits:: termination:: join:: semantics:: \
    --test-threads="${GATE_THREADS:-4}"

stage documents
bin="$CARGO_TARGET_DIR/fast/satex"
for doc in target/scratch/nb/nb.tex; do
    [ -f "$doc" ] || continue
    (cd "$(dirname "$doc")" && "${cap[@]}" "$OLDPWD/$bin" lint -f "$(basename "$doc")" --stats \
        --set limits.seconds=60 2>&1 >/dev/null | grep -ao '^[0-9.]*s *[0-9]* MiB peak' | sed "s|^|$doc: |")
done

stage done
