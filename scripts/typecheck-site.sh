#!/bin/sh
# Type-checks the site's scripts as the Pages build does (TypeScript pinned here,
# the one version CI and the hooks use). $TSC names a tsc to use; otherwise the
# pinned one is installed once into target/tsc.
set -eu
cd "$(git rev-parse --show-toplevel)"
version=5.9.3
if [ -z "${TSC:-}" ]; then
    TSC=target/tsc/node_modules/.bin/tsc
    if ! "$TSC" --version 2>/dev/null | grep -qx "Version $version"; then
        npm install --silent --no-save --prefix target/tsc "typescript@$version" >/dev/null
    fi
fi
for config in tsconfig.json jsconfig.json tsconfig.worker.json; do
    "$TSC" -p "doc/site/$config"
done
