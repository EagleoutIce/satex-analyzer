#!/usr/bin/env bash
# Edit-compile-test loop: builds under the build lock, runs the tests without it.
#   scripts/t.sh [libtest args: filter, --skip x, --exact, ...]
# Env: PROFILE (fast), THREADS (12), JOBS (12), BUILD_LOCK (target/scratch/.build.lock),
#      CARGO_TARGET_DIR, CAP=0 to skip the systemd-run memory/CPU cap,
#      TIMES=1 to print every test's seconds (libtest --report-time).
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_BUILD_JOBS=${JOBS:-12} RUST_TEST_THREADS=${THREADS:-12}
cap=()
[ "${CAP:-1}" = 0 ] || cap=(systemd-run --user --scope -q -p MemoryMax=20G -p MemorySwapMax=0 -p CPUQuota=1200% nice -n 10)
lock=${BUILD_LOCK:-target/scratch/.build.lock}
mkdir -p "$(dirname "$lock")"
# Only the build holds the lock: it is gone when this pipeline ends.
bin=$(flock "$lock" "${cap[@]}" cargo test --profile "${PROFILE:-fast}" --test it --no-run --message-format=json |
    sed -n 's/.*"executable":"\([^"]*\)".*/\1/p' | grep '/it-' | tail -1)
[ -n "$bin" ] || { echo "no test binary built" >&2; exit 1; }
# The bootstrap variable would rebuild everything, so only the binary sees it.
[ -z "${TIMES:-}" ] || set -- -Z unstable-options --report-time "$@"
exec "${cap[@]}" env ${TIMES:+RUSTC_BOOTSTRAP=1} "$bin" "$@"
