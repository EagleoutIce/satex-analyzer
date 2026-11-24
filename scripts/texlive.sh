# Sourced by scripts/t.sh and scripts/ci-test.sh: picks the TeX Live the suite runs against and
# names the tests that do not apply to it, both from tests/texlive.yaml.
#   TEXLIVE=<year>  the installation tests/texlive.yaml lists for that year
#   TEXLIVE=<dir>   an installation directory (holding bin/<platform> and texmf-dist)
# Without TEXLIVE, whatever TeX Live is on PATH.

# Puts $TEXLIVE's binaries first on PATH.
texlive_use() {
    [[ -n "${TEXLIVE-}" ]] || return 0
    local root=$TEXLIVE
    if [[ "$root" =~ ^[0-9]{4}$ ]]; then
        root=$(sed -n "/^installs:/,/^[^ #]/s/^  $TEXLIVE: *\(.*\)$/\1/p" tests/texlive.yaml | head -1)
        [[ -n "$root" ]] || { echo "TEXLIVE=$TEXLIVE: tests/texlive.yaml lists no such installation" >&2; return 1; }
        root=${root/#\~/$HOME}
    fi
    local bin
    bin=$(ls -d "$root"/bin/*/ 2>/dev/null | head -1)
    [[ -x "${bin}kpsewhich" ]] || { echo "TEXLIVE=$TEXLIVE: no kpsewhich in $root/bin/*/" >&2; return 1; }
    export PATH="$(cd "$bin" && pwd):$PATH"
}

# The year of the TeX Live on PATH (from `tex --version`), empty without one.
texlive_year() {
    tex --version 2>/dev/null | sed -n 's/.*TeX Live \([0-9]\{4\}\).*/\1/p' | head -1
}

# The tests that do not apply to TeX Live year $1, as `test<TAB>why` lines; fails on an entry
# without a why.
texlive_skips() {
    local year=$1
    [[ -n "$year" ]] || return 0
    awk -v year="$year" '
        /^skip:/ { on = 1; next }
        on && /^[^ #]/ { on = 0 }
        on && /^ *- *\{/ {
            t = $0; sub(/.*test: */, "", t); sub(/[, }].*/, "", t)
            b = $0; sub(/.*before: */, "", b); sub(/[^0-9].*/, "", b)
            w = $0; if (w !~ /why: *"/) { print "tests/texlive.yaml: " t " has no why" > "/dev/stderr"; bad = 1; next }
            sub(/.*why: *"/, "", w); sub(/" *\} *$/, "", w)
            if (year + 0 < b + 0) printf "%s\t%s\n", t, w
        }
        END { exit bad }
    ' tests/texlive.yaml
}
