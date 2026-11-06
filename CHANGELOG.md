# Changelog

Newest first. Every entry has a badge: `feat` a feature, `fix` a bug fix, `perf` speed, `docs` documentation.
Commit and pull request links point to the details.
Maintained with `scripts/changelog.py` (`draft`, `check`, `links`, `release`, see [doc/releasing.md](doc/releasing.md)).

## Unreleased

<!-- reviewed: 260c995 -->

### Playground

- `fix` `satex gui` analyzes the open file first and no other file unasked, picks the root like the CLI, and no longer freezes after a big compile. [`260c995`](https://github.com/EagleoutIce/satex-analyzer/commit/260c995)
- `fix` The trace stepper keeps its place while stepping. Table insert buttons stay round. [`ce9713f`](https://github.com/EagleoutIce/satex-analyzer/commit/ce9713f) [`3227afc`](https://github.com/EagleoutIce/satex-analyzer/commit/3227afc)
- `feat` Landing page: a live window that answers from build data before the engine loads, a showcase carousel, plain captions, "local" on a local playground. [`672c02e`](https://github.com/EagleoutIce/satex-analyzer/commit/672c02e) [`75e9e51`](https://github.com/EagleoutIce/satex-analyzer/commit/75e9e51) [`fc867f4`](https://github.com/EagleoutIce/satex-analyzer/commit/fc867f4) [`762cc60`](https://github.com/EagleoutIce/satex-analyzer/commit/762cc60)
- `fix` After a package download every document runs again, so glossaries and figures are right without an edit. A widget opened during a run waits for it. [`de3795e`](https://github.com/EagleoutIce/satex-analyzer/commit/de3795e) [`062f2be`](https://github.com/EagleoutIce/satex-analyzer/commit/062f2be)
- `perf` The first download is 6 MiB instead of 120: the kernel bundle holds only its sample's caches. The files examples look for (mwe's example images) ship too. [`003da0b`](https://github.com/EagleoutIce/satex-analyzer/commit/003da0b) [`8138420`](https://github.com/EagleoutIce/satex-analyzer/commit/8138420)
- `feat` Landing page redesign: try-it first, a showcase of the editor widgets with shots retaken by the build, four live examples. [`d2d6c19`](https://github.com/EagleoutIce/satex-analyzer/commit/d2d6c19) [`217ff78`](https://github.com/EagleoutIce/satex-analyzer/commit/217ff78) [`f5a3057`](https://github.com/EagleoutIce/satex-analyzer/commit/f5a3057) [`3af2880`](https://github.com/EagleoutIce/satex-analyzer/commit/3af2880) [`2004fba`](https://github.com/EagleoutIce/satex-analyzer/commit/2004fba)
- `feat` `satex gui` serves the playground on your own files, no upload. [`dace867`](https://github.com/EagleoutIce/satex-analyzer/commit/dace867)
- `feat` Trace stepper: step through what a macro expands to, with the input ahead, calls, forks and the domains per step. [`50058c3`](https://github.com/EagleoutIce/satex-analyzer/commit/50058c3) [`cdc1835`](https://github.com/EagleoutIce/satex-analyzer/commit/cdc1835) [`77def35`](https://github.com/EagleoutIce/satex-analyzer/commit/77def35)
- `feat` Draw page: find a symbol by sketching it, with filters, zoom, categories and look-alikes. [`cbbaf15`](https://github.com/EagleoutIce/satex-analyzer/commit/cbbaf15) [`85fb7ec`](https://github.com/EagleoutIce/satex-analyzer/commit/85fb7ec) [`618019b`](https://github.com/EagleoutIce/satex-analyzer/commit/618019b)
- `feat` Library and first download: the most used packages, glossaries and the index come with the first download, with progress. [`dbfec82`](https://github.com/EagleoutIce/satex-analyzer/commit/dbfec82) [`b4ed4c4`](https://github.com/EagleoutIce/satex-analyzer/commit/b4ed4c4) [`67750d8`](https://github.com/EagleoutIce/satex-analyzer/commit/67750d8)
- `perf` Phases of a run use all cores the memory allows. Kernel caches survive builds and are built in parallel. [`f587917`](https://github.com/EagleoutIce/satex-analyzer/commit/f587917) [`29042c5`](https://github.com/EagleoutIce/satex-analyzer/commit/29042c5)
- `feat` Settings tabs, outline, cache dialog, timing popup, dev mode and a typeset view of the placed material. [`ef94a13`](https://github.com/EagleoutIce/satex-analyzer/commit/ef94a13) [`e65d145`](https://github.com/EagleoutIce/satex-analyzer/commit/e65d145) [`2ae687e`](https://github.com/EagleoutIce/satex-analyzer/commit/2ae687e)
- `fix` Widgets stop the run they wait for, gutter marks share the finding column, downloads survive reloads. [`5d03d81`](https://github.com/EagleoutIce/satex-analyzer/commit/5d03d81) [`d4da5b0`](https://github.com/EagleoutIce/satex-analyzer/commit/d4da5b0) [`8054f4f`](https://github.com/EagleoutIce/satex-analyzer/commit/8054f4f)

### Editor plugins and widgets

- `feat` Keys layer: labels, citations, glossary and index keys in Typeset and PDF from one kind table, none dropped. TikZ dialog edits node properties and styles. Plugins name the editors they work in. [`260c995`](https://github.com/EagleoutIce/satex-analyzer/commit/260c995)
- `fix` Widget fields check their input, and a package a fix needs is loaded once. [`0fa22f1`](https://github.com/EagleoutIce/satex-analyzer/commit/0fa22f1)
- `feat` Typeset and PDF layers: glue as brackets, a PDF grid in any TeX unit, booktabs rules across the table, choices offer the texts they set. [`7f5458c`](https://github.com/EagleoutIce/satex-analyzer/commit/7f5458c) [`1ec39d8`](https://github.com/EagleoutIce/satex-analyzer/commit/1ec39d8) [`cc697b6`](https://github.com/EagleoutIce/satex-analyzer/commit/cc697b6)
- `fix` Dialogs share one Apply that never drops an edit, ask before discarding changes and undo in one step. Float placement toggles as checkmarks. [`17e9816`](https://github.com/EagleoutIce/satex-analyzer/commit/17e9816) [`66a3e25`](https://github.com/EagleoutIce/satex-analyzer/commit/66a3e25)
- `feat` Type plugins: an editor feature is a plugin that a type of the analysis triggers. Table and image dialogs come first. [`75d96fd`](https://github.com/EagleoutIce/satex-analyzer/commit/75d96fd)
- `feat` TikZ drawing widget with a canvas inspector, layers and matrix, relative nodes placed, keys leading to their definition. [`2a79cfa`](https://github.com/EagleoutIce/satex-analyzer/commit/2a79cfa) [`cde7dd6`](https://github.com/EagleoutIce/satex-analyzer/commit/cde7dd6) [`1bd757d`](https://github.com/EagleoutIce/satex-analyzer/commit/1bd757d) [`d329151`](https://github.com/EagleoutIce/satex-analyzer/commit/d329151)
- `feat` Page layout widget: geometry of the run, ranges and headers in a dialog. [`8c4690c`](https://github.com/EagleoutIce/satex-analyzer/commit/8c4690c) [`2eac3ea`](https://github.com/EagleoutIce/satex-analyzer/commit/2eac3ea)
- `feat` Colors: swatches on color names and arguments, completion only where a color is read, a name dialog. [`b74130c`](https://github.com/EagleoutIce/satex-analyzer/commit/b74130c)
- `feat` Forest tree widget, beamer overlays and frames, biblatex style hovers. [`0c4d45a`](https://github.com/EagleoutIce/satex-analyzer/commit/0c4d45a) [`c923274`](https://github.com/EagleoutIce/satex-analyzer/commit/c923274) [`6eecfa0`](https://github.com/EagleoutIce/satex-analyzer/commit/6eecfa0)
- `feat` Widgets open from the gutter, with submenus and a keys dialog. Lenses are optional. [`0dca5f9`](https://github.com/EagleoutIce/satex-analyzer/commit/0dca5f9) [`7319b01`](https://github.com/EagleoutIce/satex-analyzer/commit/7319b01)
- `fix` Lenses refresh after the run. [`58832d5`](https://github.com/EagleoutIce/satex-analyzer/commit/58832d5)
- `feat` Typed jumps, package docs and image paths by graphics extension. [`de3795e`](https://github.com/EagleoutIce/satex-analyzer/commit/de3795e) [`4fbd4cd`](https://github.com/EagleoutIce/satex-analyzer/commit/4fbd4cd)
- `feat` Completions show their TikZ drawing, and undefined names suggest the nearest one. [`760b810`](https://github.com/EagleoutIce/satex-analyzer/commit/760b810) [`a5f832c`](https://github.com/EagleoutIce/satex-analyzer/commit/a5f832c)

### VS Code and other editors

- `feat` The extension brings every widget, the library and settings. [`72e1053`](https://github.com/EagleoutIce/satex-analyzer/commit/72e1053)
- `feat` Sidebar, status bar, problems view, trace steps and a bounded server. [`20c6d00`](https://github.com/EagleoutIce/satex-analyzer/commit/20c6d00) [`da25261`](https://github.com/EagleoutIce/satex-analyzer/commit/da25261) [`6b21357`](https://github.com/EagleoutIce/satex-analyzer/commit/6b21357)
- `feat` `cargo vscode` builds and installs the extension. [`640f1e5`](https://github.com/EagleoutIce/satex-analyzer/commit/640f1e5)
- `feat` IntelliJ plugin over LSP4IJ. [`05dfeee`](https://github.com/EagleoutIce/satex-analyzer/commit/05dfeee)

### Lints

- `feat` A reference typed by hand becomes `\cref` (`\Cref` after a capital word), loading cleveref when needed. [`c75edd1`](https://github.com/EagleoutIce/satex-analyzer/commit/c75edd1) [`a8fc6d3`](https://github.com/EagleoutIce/satex-analyzer/commit/a8fc6d3)
- `feat` Clone lint for near-duplicate blocks. [`87c3dbc`](https://github.com/EagleoutIce/satex-analyzer/commit/87c3dbc)
- `feat` The `et al.` spelling, biblatex prose and a bibtex-to-biblatex migration. [`2b2727b`](https://github.com/EagleoutIce/satex-analyzer/commit/2b2727b) [`45ceb45`](https://github.com/EagleoutIce/satex-analyzer/commit/45ceb45)
- `feat` Table typography and advice, written references, `undefined-cref-format`. [`572442f`](https://github.com/EagleoutIce/satex-analyzer/commit/572442f) [`abde87a`](https://github.com/EagleoutIce/satex-analyzer/commit/abde87a) [`2039be6`](https://github.com/EagleoutIce/satex-analyzer/commit/2039be6)
- `feat` Rules have stable numbers, projects choose their style, findings group per rule. [`261a0e7`](https://github.com/EagleoutIce/satex-analyzer/commit/261a0e7) [`f3ac51b`](https://github.com/EagleoutIce/satex-analyzer/commit/f3ac51b) [`960d8c0`](https://github.com/EagleoutIce/satex-analyzer/commit/960d8c0)
- `feat` Possible findings are notes, and an error only an approximated path raises is a note. [`cde9439`](https://github.com/EagleoutIce/satex-analyzer/commit/cde9439) [`86359a0`](https://github.com/EagleoutIce/satex-analyzer/commit/86359a0)

### Analysis

- `fix` A read loop's flag stays with its stream place: no false certain terminal-read error in KOMA notecolumn. [`0eb1ce6`](https://github.com/EagleoutIce/satex-analyzer/commit/0eb1ce6)
- `fix` Reading a register in code that may run no longer makes every register unknown (KOMA notecolumn fonts). Lists of names keep their names through loops. [`d08536a`](https://github.com/EagleoutIce/satex-analyzer/commit/d08536a) [`3c99a0b`](https://github.com/EagleoutIce/satex-analyzer/commit/3c99a0b)
- `fix` Typed digits narrow their register, the nonzero bit and digit places never join. Number scans split by digit count. [`b01d4bd`](https://github.com/EagleoutIce/satex-analyzer/commit/b01d4bd) [`3e67c8a`](https://github.com/EagleoutIce/satex-analyzer/commit/3e67c8a) [`5f1e403`](https://github.com/EagleoutIce/satex-analyzer/commit/5f1e403) [`5352e4f`](https://github.com/EagleoutIce/satex-analyzer/commit/5352e4f)
- `fix` Loops widen the lists they grow, a library fault has one refutation rule, waiting paths run on. [`462b2e8`](https://github.com/EagleoutIce/satex-analyzer/commit/462b2e8) [`2bb8868`](https://github.com/EagleoutIce/satex-analyzer/commit/2bb8868) [`2062f6a`](https://github.com/EagleoutIce/satex-analyzer/commit/2062f6a)
- `fix` Deferred writes expand at shipout, footnotes follow their paragraph, page breaks fall where pdflatex breaks. [`9749be3`](https://github.com/EagleoutIce/satex-analyzer/commit/9749be3) [`fb10c57`](https://github.com/EagleoutIce/satex-analyzer/commit/fb10c57) [`857eb17`](https://github.com/EagleoutIce/satex-analyzer/commit/857eb17)
- `feat` Color domain tracking. [`aed0ae1`](https://github.com/EagleoutIce/satex-analyzer/commit/aed0ae1)
- `feat` Memory bound per run and process, and a stop when past it. [`6fde68e`](https://github.com/EagleoutIce/satex-analyzer/commit/6fde68e) [`5832594`](https://github.com/EagleoutIce/satex-analyzer/commit/5832594)
- `perf` Caches: unchanged documents served whole, segment-diff package caches, wild names keyed. [`98da22a`](https://github.com/EagleoutIce/satex-analyzer/commit/98da22a) [`8e378c3`](https://github.com/EagleoutIce/satex-analyzer/commit/8e378c3) [`20d4afd`](https://github.com/EagleoutIce/satex-analyzer/commit/20d4afd)
- `fix` Cache replays carry the diagnostics and builder state they had. [`721220a`](https://github.com/EagleoutIce/satex-analyzer/commit/721220a) [`61814a5`](https://github.com/EagleoutIce/satex-analyzer/commit/61814a5)
- `feat` Builds follow latexmk: output directories, the `latexmkrc` engine and reruns that resume where the document begins. [`816fdad`](https://github.com/EagleoutIce/satex-analyzer/commit/816fdad) [`ce581f1`](https://github.com/EagleoutIce/satex-analyzer/commit/ce581f1) [`a101964`](https://github.com/EagleoutIce/satex-analyzer/commit/a101964)
- `feat` Why-apart report says why two paths do not join. [`bbca942`](https://github.com/EagleoutIce/satex-analyzer/commit/bbca942)
- `fix` Magic comments pick plain TeX, and the engine release decides primitive facts. [`ed98f45`](https://github.com/EagleoutIce/satex-analyzer/commit/ed98f45) [`53c3d56`](https://github.com/EagleoutIce/satex-analyzer/commit/53c3d56)

### CLI

- `feat` `satex gui`, see Playground. [`dace867`](https://github.com/EagleoutIce/satex-analyzer/commit/dace867)
- `feat` `satex compile` builds with latexmk and explains the log. [`28e9bc1`](https://github.com/EagleoutIce/satex-analyzer/commit/28e9bc1)
- `feat` Progress, hotspots and a cache report. [`ddee2ce`](https://github.com/EagleoutIce/satex-analyzer/commit/ddee2ce)
- `feat` Completions for every installed shell. [`f0699ca`](https://github.com/EagleoutIce/satex-analyzer/commit/f0699ca)

### Site and docs

- `docs` Wiki: type lattice, precision limits, why TeX is hard and the compliance matrix, regenerated from the code. [`5083317`](https://github.com/EagleoutIce/satex-analyzer/commit/5083317) [`5c06ed8`](https://github.com/EagleoutIce/satex-analyzer/commit/5c06ed8) [`85e15a6`](https://github.com/EagleoutIce/satex-analyzer/commit/85e15a6)
- `feat` Hover cards for control sequences with one-line descriptions of primitives and kernel macros. [`3b86d4b`](https://github.com/EagleoutIce/satex-analyzer/commit/3b86d4b)
- `docs` This changelog, with a page on the site.

### Build and infrastructure

- `build` Incremental CI, a portable wasm toolchain, Pages publishing from main. [`ff16822`](https://github.com/EagleoutIce/satex-analyzer/commit/ff16822) [`7397e7c`](https://github.com/EagleoutIce/satex-analyzer/commit/7397e7c)
- `build` Benchmarks over releases. [`9a9de54`](https://github.com/EagleoutIce/satex-analyzer/commit/9a9de54)
- `build` Developer binaries behind a `tools` feature. [`26e48a9`](https://github.com/EagleoutIce/satex-analyzer/commit/26e48a9)

## v0.1.0 (2026-09-25)

First public version: the abstract interpreter for (La)TeX with lints, a language server, a CLI for Windows, macOS and Linux, and the wiki. [`5af9583`](https://github.com/EagleoutIce/satex-analyzer/commit/5af9583) [`5570f80`](https://github.com/EagleoutIce/satex-analyzer/commit/5570f80)
