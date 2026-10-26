#!/usr/bin/env bash
# Before a commit: the whole suite, fast tier then slow tier, one build.
# The build lock is held only while compiling (scripts/t.sh), never while testing.
#   scripts/precommit.sh [libtest args]
set -uo pipefail
cd "$(dirname "$0")/.."
rc=0
scripts/test-tier.sh fast "$@" || rc=$?
scripts/test-tier.sh slow "$@" || rc=$?
exit $rc
