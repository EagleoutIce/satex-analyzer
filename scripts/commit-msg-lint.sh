#!/bin/sh
# Checks one commit message against this project's conventional-commit rules,
# inspired by flowR's .github/.commitlintrc.json and written in shell because
# satex has no npm to run commitlint with.
#
# Reads the message from the file given as $1, or from stdin with `-`.
# Errors fail the commit; warnings only print.
set -eu

# Note there is no bare `fix`: a fix carries the type of what it fixes,
# `feat-fix`, `doc-fix`, `ci-fix` and so on.
TYPES="build build-fix builds builds-fix
ci ci-fix
doc doc-fix docs docs-fix
feat feat-fix
perf perf-fix
refactor
test tests test-fix tests-fix
dep dep-fix deps deps-fix
lint lint-fix
wip
meta meta-fix"

HELP_URL="https://github.com/EagleoutIce/satex-analyzer/blob/main/CONTRIBUTING.md"

if [ $# -ne 1 ]; then
    echo "usage: $0 <message-file|->" >&2
    exit 2
fi

if [ "$1" = "-" ]; then
    message=$(cat)
else
    message=$(cat "$1")
fi

# What git itself drops before it stores the message: its own comments, and
# everything below the scissors line `git commit --verbose` adds.
message=$(printf '%s\n' "$message" | sed -e '/^# -* >8 -*$/,$d' -e '/^#/d')
header=$(printf '%s\n' "$message" | sed -n '1p')

# git writes these itself, so they are not ours to shape.
case "$header" in
    Merge\ * | merge\ * | Revert\ * | revert\ * | fixup!* | squash!* | Automatic\ merge* | Auto-merged\ *)
        exit 0
        ;;
esac

errors=""
warnings=""
fail() { errors="$errors  - $1
"; }
warn() { warnings="$warnings  - $1
"; }

kebab() {
    printf '%s\n' "$1" | grep -qE '^[a-z0-9]+(-[a-z0-9]+)*$'
}

if [ -z "$header" ]; then
    echo "commit-msg: the message is empty" >&2
    exit 1
fi

# `type(scope)!: subject`, with the scope and the `!` optional.  A `:` in the
# subject is fine; the first `: ` is the one that separates.
if ! printf '%s\n' "$header" | grep -qE '^[^():]*(\([^()]*\))?!?: .'; then
    printf 'commit-msg: the subject line is not `type(scope): subject`:\n\n  %s\n\nSee %s\n' \
        "$header" "$HELP_URL" >&2
    exit 1
fi

prefix=${header%%: *}
subject=${header#*: }
prefix=${prefix%!}
case "$prefix" in
    *'('*')')
        type=${prefix%%\(*}
        scope=${prefix#*\(}
        scope=${scope%\)}
        ;;
    *)
        type=$prefix
        scope=""
        ;;
esac

# Type: present, kebab-case, and one this project uses.
if [ -z "$type" ]; then
    fail "the type is empty"
elif ! kebab "$type"; then
    fail "the type \`$type\` is not kebab-case"
elif ! printf '%s\n' "$TYPES" | tr ' ' '\n' | grep -qxF -- "$type"; then
    fail "the type \`$type\` is not one of: $(printf '%s' "$TYPES" | tr '\n' ' ')"
fi

# Scope: optional, but kebab-case and telling when it is there.
case "$prefix" in
    *'('*')')
        if [ -z "$scope" ]; then
            warn "the scope is empty; leave the parentheses out instead"
        elif ! kebab "$scope"; then
            fail "the scope \`$scope\` is not kebab-case"
        elif [ ${#scope} -lt 3 ]; then
            warn "the scope \`$scope\` is shorter than 3 characters"
        fi
        ;;
    *) warn "no scope; \`$type(what-it-touches): …\` says more" ;;
esac

# Subject: a lowercase phrase, no full stop, and not itself the breaking mark.
case "$subject" in
    !*) fail "the subject starts with \`!\`; mark a breaking change as \`$type!: …\`" ;;
esac
case "$subject" in
    *.) fail "the subject ends with a full stop" ;;
esac
if printf '%s\n' "$subject" | grep -qE '^[A-Z]'; then
    fail "the subject starts with a capital"
fi
if [ ${#subject} -lt 6 ]; then
    warn "the subject is shorter than 6 characters"
elif [ ${#subject} -gt 42 ]; then
    warn "the subject is longer than 42 characters"
fi
if [ ${#header} -gt 72 ]; then
    warn "the subject line is longer than 72 characters"
fi

# Body: git's own shape, and no sign-off, which this project does not use.
if printf '%s\n' "$message" | sed -n '2p' | grep -qE '.'; then
    warn "the body does not start with a blank line"
fi
if printf '%s\n' "$message" | grep -qE '^.{101,}$'; then
    warn "a line of the body is longer than 100 characters"
fi
if printf '%s\n' "$message" | grep -qiE '^signed-off-by:'; then
    fail "the message carries a \`Signed-off-by:\` line"
fi

if [ -n "$warnings" ]; then
    printf 'commit-msg: warnings for `%s`:\n%s' "$header" "$warnings" >&2
fi
if [ -n "$errors" ]; then
    printf 'commit-msg: `%s` is not a valid commit message:\n%s\nSee %s\n' \
        "$header" "$errors" "$HELP_URL" >&2
    exit 1
fi
