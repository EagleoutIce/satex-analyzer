#!/bin/sh
# Points git at the hooks this repository carries.  Run once per clone.
# `git config --unset core.hooksPath` undoes it.
set -eu

cd "$(git rev-parse --show-toplevel)"
git config core.hooksPath .githooks
echo "core.hooksPath = .githooks"
echo "commit-msg checks the message, pre-commit the staged files, pre-push fmt and clippy."
