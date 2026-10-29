#!/usr/bin/env bash
# Prints which CI job groups the changed files concern, as `name=true|false`
# lines for $GITHUB_OUTPUT.
#   scripts/ci-changes.sh <base>    changes from <base> to HEAD
#   scripts/ci-changes.sh           no base known: everything runs
# Sound over precise: a file not known to be irrelevant to the Rust build
# counts as relevant.
set -euo pipefail
cd "$(dirname "$0")/.."

rust=false links=false citations=false
emit() { printf 'rust=%s\nlinks=%s\ncitations=%s\n' "$rust" "$links" "$citations"; }

if [[ -z "${1-}" || "$1" =~ ^0+$ ]] || ! files=$(git diff --name-only "$1" HEAD 2>/dev/null); then
    rust=true links=true citations=true
    emit
    exit
fi

# A Markdown page is a test input only if satex-doc generates it from
# doc/src/<page>.in (tests/it/docs.rs checks those are up to date).
generated() { local p=${1#doc/}; [[ -e "doc/src/$p.in" ]]; }

while IFS= read -r f; do
    [[ -n "$f" ]] || continue
    case "$f" in
        *.md | lychee.toml) links=true ;;
    esac
    case "$f" in
        .github/workflows/ci.yml | scripts/ci-changes.sh) rust=true links=true citations=true ;;
        SEMANTICS.md | doc/src/tex-web-*) citations=true ;;
    esac
    case "$f" in
        editors/* | scripts/ci-await-artifact.py | LICENSE | CITATION.cff | lychee.toml | SEMANTICS.md | \
            .github/dependabot.yml | .github/codeql-config.yml | \
            .github/workflows/vscode.yml | .github/workflows/pages.yml | \
            .github/workflows/wiki.yml | .github/workflows/release.yml | \
            .github/workflows/codeql.yml) ;;
        *.md) generated "$f" && rust=true ;;
        *) rust=true ;;
    esac
done <<<"$files"
[[ "$rust" == true ]] && citations=true
emit
