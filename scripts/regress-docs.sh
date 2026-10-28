#!/usr/bin/env bash
export LC_ALL=C
# Local document regression check: runs satex on each document of a list and
# compares diagnostics, lint findings, completion and time with a baseline.
# Usage: scripts/regress-docs.sh [--update]
# Env: SATEX_REGRESS (list, default ~/.config/satex/regress.list), SATEX (binary),
# SATEX_JOBS (documents measured at once, default 3).
set -uo pipefail

list="${SATEX_REGRESS:-$HOME/.config/satex/regress.list}"
baseline="$(dirname "$list")/regress.baseline"
root="$(cd "$(dirname "$0")/.." && pwd)"
satex="${SATEX:-$root/target/guard/fast/satex}"
incomplete='budget-exhausted|stalled-loop|limit-reached'
update=0
[[ "${1:-}" == "--update" ]] && update=1
[[ -f "$list" ]] || { echo "no list: $list" >&2; exit 2; }
[[ -x "$satex" ]] || { echo "no binary: $satex (set SATEX)" >&2; exit 2; }
command -v jq >/dev/null || { echo "jq is required" >&2; exit 2; }

guard() { systemd-run --user --scope -q -p MemoryMax=10G -p MemorySwapMax=0 "$@"; }

# Rows: <doc> TAB <key> TAB <value>
# keys: complete, secs, mib, diag:<severity>:<code>, lint:<code>
measure() {
  local doc="$1" out lint start end complete=1
  start=$(date +%s.%N)
  local stats; stats=$(mktemp -p "$root/target/scratch" regress.XXXXXX 2>/dev/null || mktemp)
  out=$(guard "$satex" query diagnostics -f "$doc" --stats --format json 2>"$stats")
  end=$(date +%s.%N)
  lint=$(guard "$satex" lint -f "$doc" --format json 2>/dev/null)
  if ! jq -e . >/dev/null 2>&1 <<<"$out"; then out='[]'; complete=0; fi
  jq -e . >/dev/null 2>&1 <<<"$lint" || lint='[]'
  if jq -e --arg re "^($incomplete)\$" 'any(.[]; .code | test($re))' >/dev/null <<<"$out"; then complete=0; fi
  printf '%s\tcomplete\t%s\n' "$doc" "$complete"
  printf '%s\tmib\t%s\n' "$doc" "$(grep -o '[0-9.]* [MG]iB peak' "$stats" | awk '{print ($2 == "GiB") ? $1 * 1024 : $1}' | tail -1)"
  rm -f "$stats"
  printf '%s\tsecs\t%s\n' "$doc" "$(awk -v a="$start" -v b="$end" 'BEGIN{printf "%.1f", b-a}')"
  jq -r --arg d "$doc" '.[] | [.severity, .code, (.count // 1)] | @tsv' <<<"$out" \
    | awk -F'\t' -v d="$doc" '{c[$1":"$2]+=$3} END {for (k in c) printf "%s\tdiag:%s\t%d\n", d, k, c[k]}'
  jq -r '.[].code' <<<"$lint" \
    | awk -v d="$doc" '{c[$1]++} END {for (k in c) printf "%s\tlint:%s\t%d\n", d, k, c[k]}'
}

current=$(mktemp -p "$root/target/scratch" regress.XXXXXX 2>/dev/null || mktemp)
trap 'rm -f "$current" "$current".*' EXIT
parallel="${SATEX_JOBS:-3}"
n=0 running=0
while IFS= read -r doc; do
  doc="${doc%%#*}"
  doc="$(echo "$doc" | xargs)"
  [[ -z "$doc" ]] && continue
  if [[ ! -f "$doc" ]]; then echo "missing: $doc" >&2; continue; fi
  echo "== $doc" >&2
  n=$((n + 1))
  measure "$doc" >"$current.$n" &
  running=$((running + 1))
  if ((running >= parallel)); then wait -n; running=$((running - 1)); fi
done <"$list"
wait
cat "$current".* 2>/dev/null | sort >"$current"

if ((update)); then
  cp "$current" "$baseline"
  echo "baseline written: $baseline ($(wc -l <"$baseline") rows)"
  exit 0
fi
[[ -f "$baseline" ]] || { echo "no baseline: $baseline (run with --update)" >&2; exit 2; }

awk -F'\t' -v incomplete="^($incomplete)$" '
  NR == FNR { base[$1 "\t" $2] = $3; next }
  {
    k = $1 "\t" $2; v = $3; had = (k in base); b = base[k]; seen[k] = 1
    if ($2 == "complete") { if (v == 0 && (!had || b == 1)) bad("newly incomplete", $1, "") }
    else if ($2 == "secs") { if (had && b > 0.5 && v > b * 1.5) bad("time +50%", $1, b "s -> " v "s") }
    else if ($2 == "mib") { if (had && b > 0 && v > b * 1.5) bad("memory +50%", $1, b "MiB -> " v "MiB") }
    else if (!had) bad("new " (($2 ~ /^lint:/) ? "lint finding" : "diagnostic"), $1, $2 " x" v)
    else if (v > b) bad("higher count", $1, $2 " " b " -> " v)
  }
  function bad(what, doc, detail) { printf "DETERIORATION %s: %s %s\n", what, doc, detail; n++ }
  END { if (n) { printf "%d deterioration(s)\n", n; exit 1 } print "no deterioration" }
' "$baseline" "$current"
