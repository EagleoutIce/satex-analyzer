#!/usr/bin/env bash
# One local command for what CI runs, through the same known-red gate.
#   scripts/regress.sh                full: 8 shards, lib, quick modules, compliance
#   scripts/regress.sh --changed      only the modules tests/changed-map.txt maps the changed files to
#   scripts/regress.sh <filters>      just those test filters (quick, e.g. budgets:: latex::cloud)
#   scripts/regress.sh --record       full, then rewrite tests/green-baseline.txt and tests/steps-baseline.txt
# Options: --profile P (default ci)  --target-dir D  --jobs J (steps at once, default 1)
#          --shards N (default 8)    --base REF (--changed: diff base, default HEAD)
# Heavy commands go through target/scratch/lock2.sh when it exists.
# Prints passed, known red, NEW failures, known reds now passing, regressions
# (known red but green in the baseline) and tests whose steps grew over 20%.
# Exit 1 on a NEW failure or a failing lib/quick step.  --changed is for fast
# cycles only; the full mode stays the source of truth.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."

only=() mode=full profile=ci jobs=1 shards=8 base=HEAD tdir=
while (($#)); do
    case $1 in
        --changed) mode=changed ;;
        --record) mode=record ;;
        --profile) profile=$2; shift ;;
        --target-dir) tdir=$2; shift ;;
        --jobs) jobs=$2; shift ;;
        --shards) shards=$2; shift ;;
        --base) base=$2; shift ;;
        -h | --help) sed -n 2,14p "$0"; exit 0 ;;
        -*) echo "$0: unknown option $1" >&2; exit 2 ;;
        *) only+=("$1") ;;
    esac
    shift
done

export CARGO_PROFILE=$profile
[[ -z $tdir ]] || export CARGO_TARGET_DIR=$tdir
logs=$PWD/${tdir:-target}/regress
rm -rf "$logs"; mkdir -p "$logs"
export CI_LOG_DIR=$logs
wrap="timeout 2400"
[[ -x target/scratch/lock2.sh ]] && wrap="target/scratch/lock2.sh timeout 2400"

step() { # name args...: runs ci-test.sh, output to $logs/out-name.txt
    local name=$1; shift
    $wrap scripts/ci-test.sh "$@" >"$logs/out-$name.txt" 2>&1
    echo "$name exit $?" >>"$logs/exits"
}
export -f step
export wrap logs

if ((${#only[@]})); then
    mode=quick steps=("quick quick ${only[*]}")
elif [[ $mode == changed ]]; then
    mapfile -t files < <({ git diff --name-only "$base"; git ls-files -o --exclude-standard; } | sort -u)
    filters=() all=0
    for f in "${files[@]}"; do
        [[ -n $f ]] || continue
        hit=0
        while read -r glob rest; do
            [[ -z $glob || $glob == '#'* ]] && continue
            # shellcheck disable=SC2053
            if [[ $f == $glob ]]; then hit=1; [[ $rest == all ]] && all=1 || filters+=($rest); break; fi
        done <tests/changed-map.txt
        ((hit)) && continue
        if [[ $f =~ ^tests/it/([a-z0-9_]+)\.rs$ ]]; then filters+=("${BASH_REMATCH[1]}::")
        elif [[ $f == src/* ]]; then all=1; echo "no rule for $f: running everything" >&2; fi
    done
    if ((all)); then mode=full
    elif ((${#filters[@]} == 0)); then echo "no changed file maps to tests"; exit 0
    else
        mapfile -t filters < <(printf '%s\n' "${filters[@]}" | sort -u)
        echo "changed: ${filters[*]}"
        steps=("quick quick ${filters[*]}")
    fi
fi
if [[ $mode != changed && $mode != quick ]]; then
    steps=()
    for ((k = 1; k <= shards; k++)); do steps+=("shard-$k $k/$shards"); done
    steps+=("lib lib" "quick quick latex:: documents:: termination::" "compliance compliance")
fi

: >"$logs/exits"
printf '%s\n' "${steps[@]}" | xargs -P "$jobs" -I{} bash -c 'step {}'

ok=$(cat "$logs"/ci-test-*.log 2>/dev/null | sed -n 's/^test \(.*\) \.\.\. ok$/\1/p' | sort -u)
failed=$(cat "$logs"/ci-test-*.log 2>/dev/null | sed -n 's/^test \(.*\) \.\.\. FAILED$/\1/p' | sort -u)
known=$(grep -v '^#' tests/known-red.txt | sed '/^$/d' | sort -u)
green=$(sort -u tests/green-baseline.txt 2>/dev/null)
sec() { printf '%s\n' "$1" | sed '/^$/d'; }
cnt() { sec "$1" | wc -l; }
new=$(comm -23 <(sec "$failed") <(sec "$known"))
redk=$(comm -12 <(sec "$failed") <(sec "$known"))
fixed=$(comm -12 <(sec "$ok") <(sec "$known"))
regr=$(comm -12 <(sec "$redk") <(sec "$green"))
listed=$(comm -12 <(sec "$known") <(sec "$green"))

# Steps: label value; flag growth over 20% against the baseline.
cat "$logs"/steps-*.txt 2>/dev/null | sort -u >"$logs/steps.txt"
grown=
if [[ -f tests/steps-baseline.txt && $mode != record ]]; then
    grown=$(awk 'NR == FNR { b[$1] = $2; next }
        ($1 in b) && b[$1] > 0 && $2 * 100 > b[$1] * 120 { printf "%s %d -> %d (+%d%%)\n", $1, b[$1], $2, ($2 - b[$1]) * 100 / b[$1] }' \
        tests/steps-baseline.txt "$logs/steps.txt")
fi

bad=0
echo "== regress ($mode, profile $profile) =="
echo "passed: $(cnt "$ok")   known red: $(cnt "$redk") (of $(cnt "$known") listed)   NEW failures: $(cnt "$new")   known reds now passing: $(cnt "$fixed")"
[[ -z $new ]] || { echo "NEW failures:"; sec "$new" | sed 's/^/  /'; bad=1; }
[[ -z $fixed ]] || { echo "known red now passing, drop from tests/known-red.txt:"; sec "$fixed" | sed 's/^/  /'; }
[[ -z $regr ]] || { echo "REGRESSION (green in tests/green-baseline.txt, red now):"; sec "$regr" | sed 's/^/  /'; }
[[ -z $listed ]] || { echo "warning: known-red lists tests the green baseline has as green:"; sec "$listed" | sed 's/^/  /'; }
[[ -z $grown ]] || { echo "warning: steps grew over 20%:"; sec "$grown" | sed 's/^/  /'; }
while read -r name _ code; do
    [[ $code == 0 ]] && continue
    case $name in
        compliance) echo "compliance: red (non-blocking)" ;;
        *) echo "step $name exit $code (see $logs/out-$name.txt)"; bad=1 ;;
    esac
done <"$logs/exits"

if [[ $mode == record ]]; then
    if ((bad)); then echo "not recording: the run has failures"
    else
        sec "$ok" >tests/green-baseline.txt
        sort -u "$logs/steps.txt" >tests/steps-baseline.txt
        echo "recorded tests/green-baseline.txt ($(cnt "$ok")) and tests/steps-baseline.txt ($(wc -l <tests/steps-baseline.txt))"
    fi
fi
exit $bad
