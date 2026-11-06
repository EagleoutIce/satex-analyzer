# Changelog

Newest first, fixes first in each group. Every entry has a badge: `feat` a feature, `fix` a bug fix, `perf` speed, `docs` documentation.
Commit and pull request links point to the details.
Maintained with `scripts/changelog.py` (`draft`, `check`, `links`, `release`, see [doc/releasing.md](doc/releasing.md)).

## Unreleased

<!-- reviewed: 228b584 -->

### Playground

- `fix` **`satex gui` analyzes the open file first** and nothing unasked, and no longer freezes after a big compile. [`260c995`](https://github.com/EagleoutIce/satex-analyzer/commit/260c995)
- `fix` **Never stuck.** Dead workers, busy caches and bad stored state no longer stall the start. Stuck runs recover. [`5a702f4`](https://github.com/EagleoutIce/satex-analyzer/commit/5a702f4) [`82d7c20`](https://github.com/EagleoutIce/satex-analyzer/commit/82d7c20) [`c0ff841`](https://github.com/EagleoutIce/satex-analyzer/commit/c0ff841) [`5e722f1`](https://github.com/EagleoutIce/satex-analyzer/commit/5e722f1)
- `fix` **Everything reruns after a package download.** Widgets wait for the analysis. [`de3795e`](https://github.com/EagleoutIce/satex-analyzer/commit/de3795e) [`062f2be`](https://github.com/EagleoutIce/satex-analyzer/commit/062f2be)
- `fix` **The trace stepper keeps its place.** Phone toolbar, claims and table buttons fit. [`ce9713f`](https://github.com/EagleoutIce/satex-analyzer/commit/ce9713f) [`11edabd`](https://github.com/EagleoutIce/satex-analyzer/commit/11edabd) [`f832d13`](https://github.com/EagleoutIce/satex-analyzer/commit/f832d13) [`3227afc`](https://github.com/EagleoutIce/satex-analyzer/commit/3227afc)
- `feat` **`satex gui`** serves the playground on your own files, no upload. See [`satex gui`](doc/wiki/cli.md#the-playground-on-your-machine). [`dace867`](https://github.com/EagleoutIce/satex-analyzer/commit/dace867)
- `feat` **Landing page.** Try it first, a live window before the engine loads, a carousel of the widgets. [`d2d6c19`](https://github.com/EagleoutIce/satex-analyzer/commit/d2d6c19) [`f5a3057`](https://github.com/EagleoutIce/satex-analyzer/commit/f5a3057) [`672c02e`](https://github.com/EagleoutIce/satex-analyzer/commit/672c02e) [`75e9e51`](https://github.com/EagleoutIce/satex-analyzer/commit/75e9e51)
- `feat` **Trace stepper.** Step through what a macro expands to, with input, calls, forks and domains. [`50058c3`](https://github.com/EagleoutIce/satex-analyzer/commit/50058c3) [`cdc1835`](https://github.com/EagleoutIce/satex-analyzer/commit/cdc1835) [`77def35`](https://github.com/EagleoutIce/satex-analyzer/commit/77def35)
- `feat` **Draw page.** Find a symbol by sketching it. [`cbbaf15`](https://github.com/EagleoutIce/satex-analyzer/commit/cbbaf15) [`85fb7ec`](https://github.com/EagleoutIce/satex-analyzer/commit/85fb7ec) [`618019b`](https://github.com/EagleoutIce/satex-analyzer/commit/618019b)
- `feat` **Library and first download** with the most used packages, glossaries and the index. [`dbfec82`](https://github.com/EagleoutIce/satex-analyzer/commit/dbfec82) [`b4ed4c4`](https://github.com/EagleoutIce/satex-analyzer/commit/b4ed4c4) [`67750d8`](https://github.com/EagleoutIce/satex-analyzer/commit/67750d8)
- `feat` **Settings, outline, cache dialog, timing popup** and a typeset view. [`ef94a13`](https://github.com/EagleoutIce/satex-analyzer/commit/ef94a13) [`e65d145`](https://github.com/EagleoutIce/satex-analyzer/commit/e65d145) [`2ae687e`](https://github.com/EagleoutIce/satex-analyzer/commit/2ae687e)
- `perf` **First download 6 MiB instead of 120.** [`003da0b`](https://github.com/EagleoutIce/satex-analyzer/commit/003da0b) [`8138420`](https://github.com/EagleoutIce/satex-analyzer/commit/8138420)
- `perf` **Runs use every core** the memory allows. Kernel caches survive builds. [`f587917`](https://github.com/EagleoutIce/satex-analyzer/commit/f587917) [`29042c5`](https://github.com/EagleoutIce/satex-analyzer/commit/29042c5)

### Editor plugins and widgets

- `fix` **Requests never wait for a run**, and exit stops a running analysis. [`fc5dda3`](https://github.com/EagleoutIce/satex-analyzer/commit/fc5dda3) [`30b2124`](https://github.com/EagleoutIce/satex-analyzer/commit/30b2124) [`40ae0c9`](https://github.com/EagleoutIce/satex-analyzer/commit/40ae0c9)
- `fix` **Dialogs never drop an edit**, ask before discarding and undo in one step. [`17e9816`](https://github.com/EagleoutIce/satex-analyzer/commit/17e9816) [`0fa22f1`](https://github.com/EagleoutIce/satex-analyzer/commit/0fa22f1)
- `fix` **Colors complete only in color arguments**, with swatches on the right line. [`5e6ecba`](https://github.com/EagleoutIce/satex-analyzer/commit/5e6ecba) [`59e9afa`](https://github.com/EagleoutIce/satex-analyzer/commit/59e9afa) [`d05aa00`](https://github.com/EagleoutIce/satex-analyzer/commit/d05aa00)
- `fix` **Lenses and widget marks** refresh after the run and survive stale positions. [`58832d5`](https://github.com/EagleoutIce/satex-analyzer/commit/58832d5) [`d4da5b0`](https://github.com/EagleoutIce/satex-analyzer/commit/d4da5b0) [`c681132`](https://github.com/EagleoutIce/satex-analyzer/commit/c681132) [`0f749a1`](https://github.com/EagleoutIce/satex-analyzer/commit/0f749a1)
- `fix` **Hovers lead with the signature.** [`9eadbbc`](https://github.com/EagleoutIce/satex-analyzer/commit/9eadbbc) [`0362f90`](https://github.com/EagleoutIce/satex-analyzer/commit/0362f90)
- `feat` **Managers add, edit and search** glossaries, index terms, bibliographies (also `thebibliography`, with a convert to `.bib`) and theorems. See [type plugins](doc/wiki/type-plugins.md). [`ce5ebdd`](https://github.com/EagleoutIce/satex-analyzer/commit/ce5ebdd)
- `feat` **Keys layer.** Labels, citations, glossary and index keys in Typeset and PDF. [`260c995`](https://github.com/EagleoutIce/satex-analyzer/commit/260c995)
- `feat` **Type plugins.** Each editor feature is a plugin that a type of the analysis triggers. See [type plugins](doc/wiki/type-plugins.md). [`75d96fd`](https://github.com/EagleoutIce/satex-analyzer/commit/75d96fd)
- `feat` **TikZ widget** with a canvas, properties, styles, layers and relative positions. [`2a79cfa`](https://github.com/EagleoutIce/satex-analyzer/commit/2a79cfa) [`cde7dd6`](https://github.com/EagleoutIce/satex-analyzer/commit/cde7dd6) [`1bd757d`](https://github.com/EagleoutIce/satex-analyzer/commit/1bd757d) [`260c995`](https://github.com/EagleoutIce/satex-analyzer/commit/260c995)
- `feat` **Table, image, page layout and color dialogs.** [`1ec39d8`](https://github.com/EagleoutIce/satex-analyzer/commit/1ec39d8) [`8c4690c`](https://github.com/EagleoutIce/satex-analyzer/commit/8c4690c) [`2eac3ea`](https://github.com/EagleoutIce/satex-analyzer/commit/2eac3ea) [`b74130c`](https://github.com/EagleoutIce/satex-analyzer/commit/b74130c)
- `feat` **Typeset and PDF layers.** Glue, a PDF grid in TeX units, booktabs rules. [`7f5458c`](https://github.com/EagleoutIce/satex-analyzer/commit/7f5458c) [`cc697b6`](https://github.com/EagleoutIce/satex-analyzer/commit/cc697b6)
- `feat` **Forest trees, beamer overlays, biblatex style hovers.** [`0c4d45a`](https://github.com/EagleoutIce/satex-analyzer/commit/0c4d45a) [`c923274`](https://github.com/EagleoutIce/satex-analyzer/commit/c923274) [`6eecfa0`](https://github.com/EagleoutIce/satex-analyzer/commit/6eecfa0)
- `feat` **Widgets from the gutter.** Lenses are optional. [`0dca5f9`](https://github.com/EagleoutIce/satex-analyzer/commit/0dca5f9) [`7319b01`](https://github.com/EagleoutIce/satex-analyzer/commit/7319b01)
- `feat` **Typed jumps** to options, colors and column types. Package docs in completion. [`de3795e`](https://github.com/EagleoutIce/satex-analyzer/commit/de3795e) [`4fbd4cd`](https://github.com/EagleoutIce/satex-analyzer/commit/4fbd4cd)
- `feat` **Completions** show TikZ drawings and suggest the nearest name. [`760b810`](https://github.com/EagleoutIce/satex-analyzer/commit/760b810) [`a5f832c`](https://github.com/EagleoutIce/satex-analyzer/commit/a5f832c)

### VS Code and other editors

- `feat` **VS Code** gets every widget, the library and settings. See [VS Code](doc/wiki/vscode.md). [`72e1053`](https://github.com/EagleoutIce/satex-analyzer/commit/72e1053) [`20c6d00`](https://github.com/EagleoutIce/satex-analyzer/commit/20c6d00) [`da25261`](https://github.com/EagleoutIce/satex-analyzer/commit/da25261)
- `feat` **`cargo vscode`** builds and installs the extension. [`640f1e5`](https://github.com/EagleoutIce/satex-analyzer/commit/640f1e5)
- `feat` **IntelliJ plugin** over LSP4IJ. See [IntelliJ](doc/wiki/intellij.md). [`05dfeee`](https://github.com/EagleoutIce/satex-analyzer/commit/05dfeee)

### Lints

- `fix` **Fewer false positives.** Own text only, stated definitions, aliases, zero-width spaces, code in parentheses. [`b99ad56`](https://github.com/EagleoutIce/satex-analyzer/commit/b99ad56) [`cd0354b`](https://github.com/EagleoutIce/satex-analyzer/commit/cd0354b) [`0909180`](https://github.com/EagleoutIce/satex-analyzer/commit/0909180) [`bedf0ef`](https://github.com/EagleoutIce/satex-analyzer/commit/bedf0ef)
- `fix` **Possible findings are notes**, never certain errors. [`cde9439`](https://github.com/EagleoutIce/satex-analyzer/commit/cde9439) [`86359a0`](https://github.com/EagleoutIce/satex-analyzer/commit/86359a0) [`90f5393`](https://github.com/EagleoutIce/satex-analyzer/commit/90f5393)
- `fix` **Typed references become `\cref`**, loading cleveref when needed. [`c75edd1`](https://github.com/EagleoutIce/satex-analyzer/commit/c75edd1) [`a8fc6d3`](https://github.com/EagleoutIce/satex-analyzer/commit/a8fc6d3)
- `fix` **One finding per `\cite` call.** Duplicate labels only where LaTeX warns. [`aead4e8`](https://github.com/EagleoutIce/satex-analyzer/commit/aead4e8) [`3d5d4b7`](https://github.com/EagleoutIce/satex-analyzer/commit/3d5d4b7) [`e548a61`](https://github.com/EagleoutIce/satex-analyzer/commit/e548a61)
- `fix` **Undefined colors only where xcolor raises.** [`8bf8a22`](https://github.com/EagleoutIce/satex-analyzer/commit/8bf8a22)
- `feat` **Clone lint** for near-duplicate blocks. [`87c3dbc`](https://github.com/EagleoutIce/satex-analyzer/commit/87c3dbc)
- `feat` **`et al.`, biblatex prose** and a bibtex-to-biblatex migration. [`2b2727b`](https://github.com/EagleoutIce/satex-analyzer/commit/2b2727b) [`45ceb45`](https://github.com/EagleoutIce/satex-analyzer/commit/45ceb45)
- `feat` **Table typography**, written references, `undefined-cref-format`. [`572442f`](https://github.com/EagleoutIce/satex-analyzer/commit/572442f) [`abde87a`](https://github.com/EagleoutIce/satex-analyzer/commit/abde87a) [`2039be6`](https://github.com/EagleoutIce/satex-analyzer/commit/2039be6)
- `feat` **Stable rule numbers**, project styles, findings grouped per rule. See [all rules](doc/wiki/lints.md). [`261a0e7`](https://github.com/EagleoutIce/satex-analyzer/commit/261a0e7) [`f3ac51b`](https://github.com/EagleoutIce/satex-analyzer/commit/f3ac51b) [`960d8c0`](https://github.com/EagleoutIce/satex-analyzer/commit/960d8c0)

### Analysis

- `fix` **No false errors in KOMA notecolumn.** Read flags stay with their stream, and reading a register no longer makes every register unknown. [`0eb1ce6`](https://github.com/EagleoutIce/satex-analyzer/commit/0eb1ce6) [`d08536a`](https://github.com/EagleoutIce/satex-analyzer/commit/d08536a)
- `fix` **Here-float captions** stay in the typeset text. See [deferred material](doc/wiki/domains.md#files-written-in-the-run). [`2b8137f`](https://github.com/EagleoutIce/satex-analyzer/commit/2b8137f)
- `fix` **Numbers stay typed.** Digits narrow their register, digit places never join, scans split by digit count. See [typed unknown text](doc/wiki/domains.md#unknown-text-the-textclass-lattice). [`b01d4bd`](https://github.com/EagleoutIce/satex-analyzer/commit/b01d4bd) [`5f1e403`](https://github.com/EagleoutIce/satex-analyzer/commit/5f1e403) [`5352e4f`](https://github.com/EagleoutIce/satex-analyzer/commit/5352e4f) [`3e67c8a`](https://github.com/EagleoutIce/satex-analyzer/commit/3e67c8a)
- `fix` **Joins keep structure.** Paired arms, maybe-empty text, equal length bounds and name lists survive. See [the join](doc/wiki/domains.md#the-join-in-general). [`f827f03`](https://github.com/EagleoutIce/satex-analyzer/commit/f827f03) [`f47ac1b`](https://github.com/EagleoutIce/satex-analyzer/commit/f47ac1b) [`3495a7d`](https://github.com/EagleoutIce/satex-analyzer/commit/3495a7d) [`3c99a0b`](https://github.com/EagleoutIce/satex-analyzer/commit/3c99a0b)
- `fix` **Loops end.** A round that rewrites its state is a cycle, and summaries widen against their entry. See [call summaries](doc/wiki/domains.md#call-summaries-what-a-call-did-twice). [`d4b6fa3`](https://github.com/EagleoutIce/satex-analyzer/commit/d4b6fa3) [`af60255`](https://github.com/EagleoutIce/satex-analyzer/commit/af60255) [`1adff24`](https://github.com/EagleoutIce/satex-analyzer/commit/1adff24) [`6b50406`](https://github.com/EagleoutIce/satex-analyzer/commit/6b50406)
- `fix` **Approximations raise no certain errors.** What only an approximated path reaches is a possible error. See [soundness invariant](doc/wiki/domains.md#soundness-invariant). [`8ff098e`](https://github.com/EagleoutIce/satex-analyzer/commit/8ff098e) [`0d387ac`](https://github.com/EagleoutIce/satex-analyzer/commit/0d387ac) [`5712ab7`](https://github.com/EagleoutIce/satex-analyzer/commit/5712ab7)
- `fix` **Groups and output routines** restore what they saved and end as a unit. [`59d1ae1`](https://github.com/EagleoutIce/satex-analyzer/commit/59d1ae1) [`cf04851`](https://github.com/EagleoutIce/satex-analyzer/commit/cf04851) [`e5d53f0`](https://github.com/EagleoutIce/satex-analyzer/commit/e5d53f0) [`04b0533`](https://github.com/EagleoutIce/satex-analyzer/commit/04b0533)
- `fix` **Typesetting closer to TeX.** Alignments, `\parshape`, math styles, `\unskip` on vertical lists, ligature hyphenation. [`e38b36f`](https://github.com/EagleoutIce/satex-analyzer/commit/e38b36f) [`484f4d6`](https://github.com/EagleoutIce/satex-analyzer/commit/484f4d6) [`884f40f`](https://github.com/EagleoutIce/satex-analyzer/commit/884f40f) [`3b8ae10`](https://github.com/EagleoutIce/satex-analyzer/commit/3b8ae10) [`4331fe3`](https://github.com/EagleoutIce/satex-analyzer/commit/4331fe3) [`7e9c41c`](https://github.com/EagleoutIce/satex-analyzer/commit/7e9c41c)
- `fix` **Page breaks where pdflatex breaks.** Deferred writes expand at shipout, `\vadjust` breaks reach the page. See [deferred material](doc/wiki/domains.md#files-written-in-the-run). [`857eb17`](https://github.com/EagleoutIce/satex-analyzer/commit/857eb17) [`9749be3`](https://github.com/EagleoutIce/satex-analyzer/commit/9749be3) [`0d4c944`](https://github.com/EagleoutIce/satex-analyzer/commit/0d4c944) [`f49169a`](https://github.com/EagleoutIce/satex-analyzer/commit/f49169a)
- `fix` **Exact scans over unknown text.** One-of texts split at delimiters, unknown characters have unknown codes. See [one-of values](doc/wiki/domains.md#the-set-lift-oneof). [`a6a7830`](https://github.com/EagleoutIce/satex-analyzer/commit/a6a7830) [`d9e4316`](https://github.com/EagleoutIce/satex-analyzer/commit/d9e4316) [`490a0f5`](https://github.com/EagleoutIce/satex-analyzer/commit/490a0f5) [`5b62065`](https://github.com/EagleoutIce/satex-analyzer/commit/5b62065)
- `fix` **A missing package no longer ends the run.** Messages print as TeX prints them. [`bc78fff`](https://github.com/EagleoutIce/satex-analyzer/commit/bc78fff) [`18d31c8`](https://github.com/EagleoutIce/satex-analyzer/commit/18d31c8)
- `fix` **Magic comments pick plain TeX**, and the installed engine decides primitive facts. [`ed98f45`](https://github.com/EagleoutIce/satex-analyzer/commit/ed98f45) [`53c3d56`](https://github.com/EagleoutIce/satex-analyzer/commit/53c3d56)
- `fix` **Every written file reaches the next run.** Only a file not found reruns the whole build. [`94760be`](https://github.com/EagleoutIce/satex-analyzer/commit/94760be) [`9875e23`](https://github.com/EagleoutIce/satex-analyzer/commit/9875e23) [`06d6afe`](https://github.com/EagleoutIce/satex-analyzer/commit/06d6afe) [`49d686e`](https://github.com/EagleoutIce/satex-analyzer/commit/49d686e)
- `fix` **Cache replays are complete**: facts, diagnostics and builder state. [`9c111ba`](https://github.com/EagleoutIce/satex-analyzer/commit/9c111ba) [`721220a`](https://github.com/EagleoutIce/satex-analyzer/commit/721220a) [`61814a5`](https://github.com/EagleoutIce/satex-analyzer/commit/61814a5)
- `fix` **Cache keys** follow what a step reads, the same in every profile and on Windows. See [caches](doc/wiki/configuration.md#format-and-package-caches). [`b65d1b1`](https://github.com/EagleoutIce/satex-analyzer/commit/b65d1b1) [`c85d25f`](https://github.com/EagleoutIce/satex-analyzer/commit/c85d25f) [`a686f08`](https://github.com/EagleoutIce/satex-analyzer/commit/a686f08) [`4973535`](https://github.com/EagleoutIce/satex-analyzer/commit/4973535)
- `fix` **Caches work without file locks**, as on network drives. [`a8c7530`](https://github.com/EagleoutIce/satex-analyzer/commit/a8c7530) [`a0d9fc8`](https://github.com/EagleoutIce/satex-analyzer/commit/a0d9fc8) [`e1220cb`](https://github.com/EagleoutIce/satex-analyzer/commit/e1220cb)
- `fix` **Clean stops.** Memory and time limits end a build with its last complete run. Time is only a backstop. See [limits](doc/wiki/configuration.md#configurable-limits-for-the-analysis). [`5832594`](https://github.com/EagleoutIce/satex-analyzer/commit/5832594) [`e8362b4`](https://github.com/EagleoutIce/satex-analyzer/commit/e8362b4) [`71a431b`](https://github.com/EagleoutIce/satex-analyzer/commit/71a431b) [`26fa2db`](https://github.com/EagleoutIce/satex-analyzer/commit/26fa2db)
- `fix` **`satex compile` honours the latexmkrc engine.** [`ce581f1`](https://github.com/EagleoutIce/satex-analyzer/commit/ce581f1)
- `fix` **Sandboxed `build.lua`.** Names from unknown text never reach kpsewhich. [`29ae6e3`](https://github.com/EagleoutIce/satex-analyzer/commit/29ae6e3) [`01cc767`](https://github.com/EagleoutIce/satex-analyzer/commit/01cc767)
- `feat` **Color domain.** See [colors](doc/wiki/domains.md#colors-color-and-colors). [`aed0ae1`](https://github.com/EagleoutIce/satex-analyzer/commit/aed0ae1)
- `feat` **Memory bound** per run and process. See [limits](doc/wiki/configuration.md#configurable-limits-for-the-analysis). [`6fde68e`](https://github.com/EagleoutIce/satex-analyzer/commit/6fde68e)
- `feat` **Builds follow latexmk**, with output directories and resumed reruns. [`816fdad`](https://github.com/EagleoutIce/satex-analyzer/commit/816fdad) [`a101964`](https://github.com/EagleoutIce/satex-analyzer/commit/a101964)
- `feat` **Why-apart report** says why two paths do not join. [`bbca942`](https://github.com/EagleoutIce/satex-analyzer/commit/bbca942)
- `perf` **Caches serve unchanged documents whole** and diff package caches by segment. [`98da22a`](https://github.com/EagleoutIce/satex-analyzer/commit/98da22a) [`8e378c3`](https://github.com/EagleoutIce/satex-analyzer/commit/8e378c3) [`20d4afd`](https://github.com/EagleoutIce/satex-analyzer/commit/20d4afd)

### CLI

- `feat` **`satex compile`** builds with latexmk and explains the log. [`28e9bc1`](https://github.com/EagleoutIce/satex-analyzer/commit/28e9bc1)
- `feat` **Progress, hotspots and a cache report.** [`ddee2ce`](https://github.com/EagleoutIce/satex-analyzer/commit/ddee2ce)
- `feat` **Shell completions** for every installed shell. [`f0699ca`](https://github.com/EagleoutIce/satex-analyzer/commit/f0699ca)

### Site and docs

- `docs` **Wiki** on the type lattice, precision limits and why TeX is hard, regenerated from the code. See [why TeX is hard](doc/wiki/challenges.md). [`5083317`](https://github.com/EagleoutIce/satex-analyzer/commit/5083317) [`5c06ed8`](https://github.com/EagleoutIce/satex-analyzer/commit/5c06ed8) [`85e15a6`](https://github.com/EagleoutIce/satex-analyzer/commit/85e15a6)
- `feat` **Hover cards** for control sequences. [`3b86d4b`](https://github.com/EagleoutIce/satex-analyzer/commit/3b86d4b)
- `docs` **This changelog**, with a page on the site. [`c37742f`](https://github.com/EagleoutIce/satex-analyzer/commit/c37742f)

### Build and infrastructure

- `build` **Incremental CI**, a portable wasm toolchain, Pages from main. [`ff16822`](https://github.com/EagleoutIce/satex-analyzer/commit/ff16822) [`7397e7c`](https://github.com/EagleoutIce/satex-analyzer/commit/7397e7c)
- `build` **Benchmarks over releases.** [`9a9de54`](https://github.com/EagleoutIce/satex-analyzer/commit/9a9de54)
- `build` **Developer binaries** behind a `tools` feature. [`26e48a9`](https://github.com/EagleoutIce/satex-analyzer/commit/26e48a9)

## v0.1.0 (2026-09-25)

First public version: the abstract interpreter for (La)TeX with lints, a language server, a CLI for Windows, macOS and Linux, and the wiki. [`5af9583`](https://github.com/EagleoutIce/satex-analyzer/commit/5af9583) [`5570f80`](https://github.com/EagleoutIce/satex-analyzer/commit/5570f80)
