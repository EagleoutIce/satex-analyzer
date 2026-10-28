# Developing satex

This page covers working on satex itself: building, testing, debugging, and documenting it.
For using satex, see the [README](../README.md). For contribution guidelines (hooks, commit messages), see [CONTRIBUTING](../CONTRIBUTING.md).
For what satex computes and why, see [SEMANTICS](../SEMANTICS.md) and the [domains](wiki/domains.md) page.

## Setup

You need a Rust toolchain and a TeX distribution (TeX Live or MiKTeX). Most
tests analyze real LaTeX documents and skip themselves when `latex.ltx` cannot
be found. Check that `kpsewhich latex.ltx` prints a path.

Run once per clone:

```sh
scripts/setup-hooks.sh
```

## Build profiles

| profile | command | use it for | rebuild after an edit |
| --- | --- | --- | --- |
| `fast` | `cargo build --profile fast` | experiments, local test runs | seconds |
| `ci` | `cargo build --profile ci` | what CI tests with | about a minute |
| `release` | `cargo build --release` | the shipped binary, timing | minutes (thin LTO, one codegen unit) |

`fast` is optimized like `release` but without link-time optimization, 32 codegen units, and incremental builds.
The binary is at `target/fast/satex`. Analysis runs about as fast as `release`, so measurements remain meaningful.
Use `release` only when a few percent matters.

Give parallel work its own target directory (`--target-dir target/<name>`) so running tests don't block builds and builds don't wait for each other's locks.

## Tests

```sh
cargo test --profile fast                            # everything
cargo test --profile fast --test it -- semantics::    # one module
cargo test --profile fast --test it -- latex::cloud   # tests whose name contains "cloud"
```

Integration tests live in `tests/it/<area>.rs`, one module per area, collected by `tests/it/main.rs` into a single `it` binary.
`cargo test` links one test binary and libtest schedules every test across all cores instead of running 49 small binaries sequentially.
CI splits the run into shards by module (`scripts/ci-test.sh`) to keep any one job under its time limit.

`compliance_matrix` fails on purpose while its ledger of known divergences
from TeX has entries. CI reports it without blocking.

### Regression tests

Every bug gets a minimal example (MWE) as a test in the module that covers its area.
Use `tests/it/latex.rs` for documents loading packages, `tests/it/semantics.rs` for primitives, `tests/it/counterexamples.rs` for soundness.
Keep examples anonymous: generic text, the `article` class, no names or paths from the original document.
`broken_gaps` and `document_gaps` in `tests/it/latex.rs` collect gaps a test can assert are absent.

### Memory

The test binary counts its allocations like `satex` does, and every test
config sets `limits.memory` and the hard ceiling (see `tests/it/support.rs`), so a
runaway test stops or aborts itself below the cgroup cap instead of taking the
machine's memory. For experiments, run
heavy commands in a capped cgroup that sits above satex's own limit:

```sh
systemd-run --user --scope -q -p MemoryMax=10G -p MemorySwapMax=0 cargo test --profile fast --test it
```

## Caches

The first run interprets the LaTeX kernel and stores it as a format in the cache directory.
Every package gets its own cache. Both are tied to the satex build, so the first run after a rebuild takes 20–30 seconds longer.
satex shows a progress bar to the terminal while the cache builds.

`SATEX_CACHE=<dir>` points a run at another cache directory, for example an
empty one to measure a cold start.

## Debugging an analysis

Collect evidence before changing code: which construct, path, and limit.

| question | command |
| --- | --- |
| Which gaps and limits did a run hit? | `satex query gaps -f doc.tex --stats` |
| Every diagnostic, machine-readable | `satex query diagnostics -f doc.tex --format json` |
| What happens step by step? | `satex -f doc.tex --format csv trace --include-internal` |
| Only some lines or steps | `satex trace --lines 10:20` or `--steps 1000:2000` |
| Why are paths kept apart, which loops keep growing? | `satex -vvvv query gaps -f doc.tex` |
| What does a macro look like to satex? | `satex explain <name> -f doc.tex` |
| Does a larger limit help? | `--set limits.<name>=<value>` |

To shrink a failing document, remove half and check if the gap stays, then repeat.
A package-level bug usually shrinks to a few lines, which become the regression test.

Keep traces and scratch files in `target/scratch/` (git ignores `target/`) and delete large traces when done.
Don't use `/tmp`: traces reach hundreds of megabytes.

Don't use `git stash`, `git checkout`, or `git reset` to compare with an older state while others work in the tree.
Use a separate worktree instead:

```sh
git worktree add target/scratch/base HEAD
```

## Documentation

Wiki pages in `doc/wiki/` and parts of the README are generated from templates in `doc/src/` and from the code (lint table, `limits` table, primitive coverage).

```sh
cargo run --profile fast --features doc --bin satex-doc              # regenerate
cargo run --profile fast --features doc --bin satex-doc -- --check   # CI: is everything current?
```

Citations of TeX's sources name a section, as in `tex.web § 366`. Links point to a fixed upstream commit and line so they keep working when upstream changes.

```sh
satex-doc --refresh-citations   # resolve every citation against the current upstream
satex-doc --check-citations     # fail if a citation moved or no longer resolves
```

CI also checks every link in the documentation with
[lychee](https://lychee.cli.rs) (configured in `lychee.toml`).

Write the documentation in American English, in short sentences.

### The site and the playground

`.github/workflows/pages.yml` builds the site from scratch on every push to `main`.
`doc/site/build.py` does the work. It reads only the tree:

- The pages are the README, `SEMANTICS.md`, `doc/chktex-coverage.md`, this file and `doc/wiki/*.md`, rendered by pandoc into `doc/site/template.html`.
  The side navigation is `doc/wiki/_Sidebar.md`. The version comes from `Cargo.toml`.
- The logo is `doc/logo/logo.tex`, typeset and converted with `pdftocairo`, its white page removed. Page 3 is the wordmark.
  Pages 4 and 5 are the tagline's lines. The landing page sets them beside the wordmark, as page 2 does, and animates them. Page 6 is the plain name for the header and the favicon.
  In dark mode a CSS filter inverts the logo's lightness and keeps its hues.
- `doc/site/examples/*.tex` are the landing page's examples. `examples/commands.txt` names the command of each one that does not run `lint`.
  The build runs it with the native `satex` and shows the output as it is. The examples are also the playground's first files.
  `site.js` colors them with the editor's tokenizer, CodeMirror's stex mode.
- `site.js` holds the light/dark toggle (kept in `localStorage`) and the running table of contents of every page.
- The playground runs the `satex` binary itself, built for `wasm32-unknown-emscripten`.
  Emscripten compiles the vendored Lua. `.cargo/config.toml` holds the link flags.
- Its TeX files are what `satex query files` reports for each sample in `samples/`, once per kernel (pdfTeX, XeTeX, LuaTeX, plain TeX), copied into a `texmf.bundle`.
  The samples are then linted against that bundle alone with each kernel, and `satex cache export` packs the caches each one built.
  Built against the installation, the package caches would not serve the bundle: their key names the roots.
- pdfTeX's files, its caches and the samples are one gzip file. Each other kernel is a gzip file of what it adds, fetched when it is chosen.
  Every asset name carries its content hash, so browsers cache it for good.
- Further packages are a gzip file each: those `doc/site/packages.txt` names, then `cache.index.common` of satex.yaml.
  The build runs satex on a document loading each one, once per kernel, and keeps the files that kernel's download lacks.
  The playground fetches a package when a finding or a run says it is not in this installation, then starts over.
- The page also carries data from satex itself: `satex query --list`, the rules with their explanations (`satex lint --rules`, `satex lint --explain`),
  and `satex completions json` (the command tree, the config keys and the rule codes) for the argument line's completion.

In the browser, `doc/site/worker.js` fetches the wasm and the bundle and imports the caches once.
Every run is a fresh instance over an in-memory file system, as a process would be.
The language server is one more instance that stays alive. Under Emscripten `satex lsp` has no threads and no stdio.
It sets up the engine and returns, and `src/lsp/wasm.rs` exports `satex_lsp`, which takes one JSON-RPC message and returns the replies.
`doc/site/playground.js` sends the edits, debounced, and asks for completion, hover, code actions and rename. The editor is CodeMirror 6 from esm.sh.
The command line is the one way to act. `lint`, `explain`, `query` and the rest are runs. `rename` and `fix` are language server edits applied in the editor.
Plain `lint` runs only on Enter: the findings panel already shows the language server's.
F2, the context menu and Ctrl+click rename, explain and go to a definition. A definition in a bundle file opens read-only.
The layout follows flowR's playground. The project's files are a tree left of the editor, the open ones are tabs, the command line and its output sit under the editor, the analysis is on the right.
A project is an example (`examples/projects.txt` names the files it reads besides itself) or an upload. One file is its root. Runs and the language server start there, so an `\input` file is analyzed with it.
The tree adds, renames and deletes files and makes another file the root. A rename also renames the `\input`, `\include` and `\bibliography` arguments that name the file.
Uploaded files join the open project. A folder or a zip is unpacked in the page and goes to `/work/upload`. A permalink is the project's changed files, the root, the open file, the arguments, the kernel and the selection, deflated into the URL's hash.
`doc/site/playground.yaml` is the satex.yaml of those runs.

`cargo playground` does all of it and serves the result at <http://127.0.0.1:8000/play/> (`--port`, `--out`). It builds with the incremental `playground` profile, so a rebuild after an edit takes seconds; `--release` builds what the published site ships. A last build serves at once while the new one builds, and replaces it when done.
To build and test it step by step you need the native and the wasm build, the
[Emscripten SDK](https://emscripten.org/docs/getting_started/downloads.html) (the version in the workflow), pandoc and pdftocairo:

```sh
cargo build --release
cargo build --release --target wasm32-unknown-emscripten --bin satex
python3 doc/site/build.py --satex target/release/satex --wasm target/wasm32-unknown-emscripten/release --out site-out
python3 -m http.server -d site-out 8000 &
node doc/site/test.mjs http://localhost:8000/   # lints every sample through the served playground, then asks the language server
```

Both builds must use the same profile: a cache is keyed on the build that wrote it.

## Regression checks

Two local safeguards keep fixed problems from returning.

`tests/it/budgets.rs` holds step budgets for the kernel build and for documents that were once slow. Steps are tokens, never seconds. Each test fails at 20% above the value measured when the problem was fixed. When a change lowers a count on purpose, lower the constant too.

`scripts/regress-docs.sh` checks real documents. It reads a list of `.tex`, `.sty` or `.cls` paths from `${SATEX_REGRESS:-~/.config/satex/regress.list}`, one per line, with `#` comments. It runs `satex query diagnostics` and `satex lint` on each document, one at a time, under `systemd-run` with a 10G memory limit. It needs `jq` and a built `satex` (`SATEX` overrides the default `target/guard/fast/satex`).

```sh
scripts/regress-docs.sh --update   # record the baseline (regress.baseline, next to the list)
scripts/regress-docs.sh            # compare with it
```

The script prints every deterioration and exits non-zero. It reports a document that is newly incomplete (`budget-exhausted`, `stalled-loop`, `limit-reached`), a new diagnostic code or a higher count, a time more than 50% above the baseline, and a new lint finding or a higher count.
