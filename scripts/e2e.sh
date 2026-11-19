#!/bin/sh
# The playground's browser e2e suite (doc/site/e2e/README.md) against a dev server of its own:
#   scripts/e2e.sh quick|full|perf [run.mjs options: --only a,b --area x --repeat 3 --out DIR]
# Starts target/playground/satex-playground --dev on the first free port of 8472-8479, waits until it answers,
# runs doc/site/e2e/run.mjs, and stops the server by its PID. Build the server first with `cargo playground`.
set -eu

root=$(git rev-parse --show-toplevel)
cd "$root"
tier=${1:-quick}
[ $# -gt 0 ] && shift
server=${E2E_SERVER:-target/playground/satex-playground}
[ -x "$server" ] || { echo "e2e: no $server (run cargo playground once)" >&2; exit 2; }

port=
for p in 8472 8473 8474 8475 8476 8477 8478 8479; do
    # curl's 7: nothing listens there
    rc=0; curl -s -o /dev/null --max-time 2 "http://127.0.0.1:$p/" || rc=$?
    if [ "$rc" = 7 ]; then port=$p; break; fi
done
[ -n "$port" ] || { echo "e2e: ports 8472-8479 all taken" >&2; exit 2; }

mkdir -p target/e2e
log=target/e2e/server-$port.log
"$server" --dev --port "$port" >"$log" 2>&1 &
pid=$!
trap 'kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true' EXIT
trap 'exit 130' INT TERM

up=
for _ in $(seq 1 120); do
    if curl -sf -o /dev/null "http://127.0.0.1:$port/play/"; then up=1; break; fi
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.5
done
[ -n "$up" ] || { echo "e2e: the server on $port did not answer (log $log):" >&2; cat "$log" >&2; exit 2; }

node doc/site/e2e/run.mjs --port "$port" --tier "$tier" "$@"
