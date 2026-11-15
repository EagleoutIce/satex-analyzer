# Changelog

Newest first, fixes first in each group. Every entry has a badge: `feat` a feature, `fix` a bug fix, `perf` speed, `docs` documentation.
Commit and pull request links point to the details.
Maintained with `scripts/changelog.py` (`draft`, `check`, `links`, `release`, see [doc/releasing.md](doc/releasing.md)).

## Unreleased

### Lints

- `feat` **Four new lints for references and sentences.** `bare-reference` (S059) flags a `\ref` with no name before it. `reference-name-mismatch` (C054) flags a name that belongs to another kind than the label, such as "Figure" before a table. `doubled-punctuation` (S060) flags two punctuation marks in a row. `sentence-capital` (S061) flags a sentence that starts with a lowercase letter. [#44](https://github.com/EagleoutIce/satex-analyzer/pull/44)

### Site and docs

- `feat` **Links to showcase entries.** Each entry on the front page has an anchor, such as `#shot-table`, and its title links to it. Opening the link scrolls to the entry and turns the card ring to it. [#44](https://github.com/EagleoutIce/satex-analyzer/pull/44)

### Build and infrastructure

- `perf` **Benchmark results for 0.2.3.** The benchmark page shows the 0.2.3 release. [`bcf2cf4`](https://github.com/EagleoutIce/satex-analyzer/commit/bcf2cf4)

### Analysis

- `perf` **Faster analysis.** Fewer copies and lookups in hot paths: deferred writes, recent-write lookups, token counting, definition comparison, font lookup and widening. [#44](https://github.com/EagleoutIce/satex-analyzer/pull/44)

## v0.2.3 (2026-11-13)

### Playground

- `feat` **Trace steps show the text set so far.** After an `\else`, for example, you see what that branch typeset (`Hello, abab`). `satex trace` reports it as `set_so_far`. [`b92d718`](https://github.com/EagleoutIce/satex-analyzer/commit/b92d718)
- `fix` **The definition history is back.** Its dialog opened empty in the playground and in VS Code. [`0ddb302`](https://github.com/EagleoutIce/satex-analyzer/commit/0ddb302)

### VS Code

- `fix` **The server starts in devcontainers and WSL.** Without a systemd user session the server failed with "Failed to connect to user scope bus" and the log showed only `write EPIPE`. The extension now caps memory with `ulimit -d` there, or runs without a hard cap. [`786e1e0`](https://github.com/EagleoutIce/satex-analyzer/commit/786e1e0) [`3f34cf0`](https://github.com/EagleoutIce/satex-analyzer/commit/3f34cf0)
- `fix` **Start problems are explained.** If `satex.path` or the bundled binary cannot run (wrong C library, musl, missing executable bit), the next binary is tried. A server that stops is reported with its reason and buttons to download satex, set the path, show the output or restart. [`786e1e0`](https://github.com/EagleoutIce/satex-analyzer/commit/786e1e0) [`3f34cf0`](https://github.com/EagleoutIce/satex-analyzer/commit/3f34cf0)
- `fix` **The Windows package is published again.** Packaging failed on Windows, so 0.2.1 and 0.2.2 shipped no Windows extension. [`802cbdf`](https://github.com/EagleoutIce/satex-analyzer/commit/802cbdf)

### Site and docs

- `docs` **Fairer benchmark comparisons.** Charts and deltas count only documents that every compared release analyzed to the end. A new table compares each engine with the previous release. [`899a58b`](https://github.com/EagleoutIce/satex-analyzer/commit/899a58b)

## v0.2.2 (2026-11-12)

### VS Code

- `fix` **No restart loop when memory runs out.** The memory cap was set to exactly the server's own budget, so the kernel killed the server before it could stop cleanly, and the extension restarted it every minute with a "memory shortage" notice. The cap now has 50% headroom, and a crashing server is restarted at most three times. [`28cfe0d`](https://github.com/EagleoutIce/satex-analyzer/commit/28cfe0d)

### Build and infrastructure

- `fix` **No stack overflow on Windows.** The Windows binary had 1 MiB of stack, too little to build the LaTeX kernel cache. It now has 8 MiB, as on Linux. [`9029e53`](https://github.com/EagleoutIce/satex-analyzer/commit/9029e53)
- `fix` **The site shows the new version after a release.** The site is now built from `main`, since GitHub Pages refused builds from the tag. [`e6b2357`](https://github.com/EagleoutIce/satex-analyzer/commit/e6b2357)

### Site and docs

- `docs` **Clearer front page.** The rotating claims are full sentences, the option card no longer looks like a menu, and the build runs appear one by one. [`d8cb335`](https://github.com/EagleoutIce/satex-analyzer/commit/d8cb335)

## v0.2.1 (2026-11-12)

### Analysis

- `fix` **Progress output no longer changes results.** The progress counters were stored with the analysis, so a cached and an uncached package load could differ.
- `fix` **No false "missing `\end{document}`" after a library scan** that read past the end of the main file.
- `fix` **Large TikZ diagrams are analyzed completely.** `limits.line_steps` is raised from 100M to 500M steps. A diagram with many nodes needs 100M to 200M steps on one line.
- `fix` **Cold and warm caches give the same results.** Calls that span the start or end of a cached package load are no longer recorded.
- `fix` **No false errors from `\int_case:nn` and `\int_to_alph:n` on unknown values.** Comparing an unknown number with itself is now true.
- `fix` **Long preambles no longer lose definitions.** After 512 preamble lines that hit their step limit, calls such as `\hook_use:n` were wrongly cut as runaway recursion.
- `fix` **No false runaway error in every `longtable`.**
- `perf` **Much faster joins.** Merging two paths took half the run time on a thesis with thousands of loops. Such a thesis now finishes in 277 s instead of not finishing within 600 s.
- `fix` **l3fp `exp`, `ln` and `^` give TeX's values.** Values like `1/10` were cut as endless recursion. tikz-sankey diagrams need a tenth of the steps.
- `fix` **No false "missing `\end{document}`" after a skipped line.**
- `fix` **`\raise` and `\lower` in inline math keep the formula's size.** A TikZ picture inside a math node no longer splits the analysis.
- `feat` **New lint `unread-document` (A003).** It reports files the document names but the analysis never read, and scans that read past the end of the document.
- `perf` **Documents with many labels run three times faster.** Lines read back from the `.aux` file are cheaper, and files are looked up by path.
- `perf` **pgfplotstable no longer stalls the analysis.**
- `fix` **No false errors from `\pgfprocessround`.**
- `fix` **More precise joins.** pgfmath on random numbers is exact again and faster.
- `fix` **Floats keep their exact text where all branches agree.**
- `fix` **A line cut inside `\include` no longer breaks the following chapters.** They are read without a stray `\fi` or garbage file names.
- `fix` **A stuck path inside a command ends only that path.** `\pgfmathparse{\the\wd2}` now finishes.
- `fix` **`\color{blue!\w!red}` with an unknown `\w` no longer swallows the rest of the document**, and `\colorlet` in a loop keeps the exact color.
- `fix` **A macro called through an unknown name no longer eats the code after it.**
- `fix` **Long loops over known values are no longer cut**, which made names look undefined later.
- `fix` **LuaTeX's `\pdfvariable`, `\pdfextension` and `\pdffeedback` are read correctly.**
- `fix` **`\ ` in math and `\unhbox` keep box sizes correct.**
- `perf` **Faster with long bibliographies.** Splits over unknown key lists are limited by `limits.rest_splits`.
- `fix` **A `$` with no formula to close leaves math mode**, so a following `\par` no longer loops.
- `fix` **`\clearnotecolumns` no longer sets hundreds of pages.** A package loaded inside a paragraph is not cached.
- `feat` **Long documents reach their end.** Past a budget, float placement is assumed instead of explored per branch (`analysis.assume_page_one_arm`, reported as `page-one-arm`).
- `fix` **A cut inside `\include` no longer forgets the chapter's definitions.**
- `fix` **A line that hits its step limit is skipped and the analysis continues.** It stops after `limits.line_overruns` such lines.
- `perf` **Less work per path switch and per written line.** Replaying a written line is capped at 50 000 steps.
- `fix` **No false loops after a joined `\fi`.** [`0da79ed`](https://github.com/EagleoutIce/satex-analyzer/commit/0da79ed) [`71c673d`](https://github.com/EagleoutIce/satex-analyzer/commit/71c673d)
- `fix` **Numbers keep their type through pgfmath**, and copies of one number compare equal. [`4536f7f`](https://github.com/EagleoutIce/satex-analyzer/commit/4536f7f) [`cde8a60`](https://github.com/EagleoutIce/satex-analyzer/commit/cde8a60) [`4084ec7`](https://github.com/EagleoutIce/satex-analyzer/commit/4084ec7)
- `fix` **Float lists keep their names, and scans over unknown text no longer end paths early.** [`29bd2bb`](https://github.com/EagleoutIce/satex-analyzer/commit/29bd2bb) [`93e8242`](https://github.com/EagleoutIce/satex-analyzer/commit/93e8242) [`9336d57`](https://github.com/EagleoutIce/satex-analyzer/commit/9336d57) [`601f0a7`](https://github.com/EagleoutIce/satex-analyzer/commit/601f0a7)
- `fix` **A build that falls back to an earlier run says so in its report.** [`57a6b6b`](https://github.com/EagleoutIce/satex-analyzer/commit/57a6b6b)
- `fix` **Number parsers on unknown values finish.** After `limits.call_learn_after` runs, a macro's effect is assumed from its earlier calls (`analysis.assume_call_learned_effect`, reported as `call-learned-effect`). `\pgfmathparse` needs no special rule.
- `fix` **No false pgfmath errors around siunitx `\qty`.** A penalty in math no longer makes the formula's width unknown. [`da70671`](https://github.com/EagleoutIce/satex-analyzer/commit/da70671)
- `fix` **No crash when every branch of a choice is ruled out.** [`bf50fd5`](https://github.com/EagleoutIce/satex-analyzer/commit/bf50fd5)

### CLI

- `fix` **Progress bars no longer flicker**, and they count the package caches loaded.
- `fix` **Better root document detection.** Only top-level build files (`latexmkrc`, `Makefile`, `build.lua` and so on) count, so a vendored `build.lua` no longer picks the wrong root. `project.root` in `satex.yaml` overrides detection.
- `fix` **Progress updates steadily**, counts read like `1.2B`, and a package stored in parts counts once.

### Lints

- `feat` **Commands whose arguments prose rules skip.** Set them in `lints.quiet-arguments.commands`. Defaults: `\todo`, `\missingfigure`, `\href`, `\lstinputlisting`, `\lstinline`, `\inputminted`, `\mintinline`.
- `fix` **`inconsistent-precision` on siunitx `S` columns** suggests the one option to set (`round-mode = places`) instead of reporting every cell.
- `fix` **Braced names in generated bibliographies.** `{{GCC Development Team}}` and `Alexander {von Rhein}` are split as BibTeX splits them.
- `fix` **`missing-nonbreaking-space` ignores `\citeauthor`, `\textcite`, `\citet` and similar**, which print names.
- `fix` **`undefined-bibliography-string` no longer reports month constants** with biblatex, and `plain.bst` is found.
- `fix` **Macros that packages create per call are no longer reported as dead definitions.**
- `fix` **`clones` reports only blocks worth a macro**, set by `min_saved`, `parameter_cost` and `max_parameters`.
- `feat` **`already-defined` names the earlier definition** by package and line, and fixes a repeated one with `\providecommand`.
- `feat` **New lints `itemize-numbered` and `itemize-described`.** An `itemize` with hand-numbered items becomes an `enumerate`, one with word labels a `description`.
- `feat` **New lint `package-successor`.** It suggests successors for outdated packages (`wrapfig` to `wrapfig2`, `here` to `float`, `fancyheadings` to `fancyhdr`, `caption2` to `caption`, `scrpage2` to `scrlayer-scrpage`) and fixes the `\usepackage`. The pairs are in `lints.package-successor.successors`.
- `fix` **`new-redefines` accepts a register declared twice**, as LaTeX does.
- `fix` **Fewer false errors after `\newif` and around colors.**
- `fix` **Editors no longer offer to turn off `clones`.** [`1cdd3cc`](https://github.com/EagleoutIce/satex-analyzer/commit/1cdd3cc)
- `fix` **No spacing findings in TikZ code or undefined environments** of a file opened without its preamble. [`5d61ec1`](https://github.com/EagleoutIce/satex-analyzer/commit/5d61ec1)

### VS Code

- `fix` **The server restarts after the client gave up on it.** [`096dc93`](https://github.com/EagleoutIce/satex-analyzer/commit/096dc93)
- `fix` **Findings no longer flicker during an analysis.** Findings that may still change appear when the run ends. Set `satex.findings.whileAnalyzing` to `hide` (default), `keep` or `mark`.

### Playground

- `feat` **Relative node placement in the TikZ widget.** A dragged node gets a shift from its nearest neighbour instead of `at (x,y)`.
- `feat` **The bibliography, glossary and index views share one layout**, with a resizable list and a search over each entry's uses.
- `feat` **Tips in the closed console bar.**
- `feat` **The draw pad can be resized**, and its size is kept in the link. [`190bf16`](https://github.com/EagleoutIce/satex-analyzer/commit/190bf16)
- `feat` **Hovers list downloaded packages**, and dialogs can show another tip. [`149d779`](https://github.com/EagleoutIce/satex-analyzer/commit/149d779) [`ef9b92c`](https://github.com/EagleoutIce/satex-analyzer/commit/ef9b92c)
- `fix` **The header menu stays until the links fit**, and go-to-definition works after downloads. [`28837d2`](https://github.com/EagleoutIce/satex-analyzer/commit/28837d2) [`b55e95f`](https://github.com/EagleoutIce/satex-analyzer/commit/b55e95f)
- `fix` **`\begin` completion keeps arguments already written.** [`0ba4807`](https://github.com/EagleoutIce/satex-analyzer/commit/0ba4807)
- `fix` **The style prompt moved to the lint settings.** [`03ad504`](https://github.com/EagleoutIce/satex-analyzer/commit/03ad504)
- `feat` **Compile in the browser.** pdfTeX runs in the web playground and fetches TeX Live 2026 files on demand. Set the hosts or turn it off under Settings, Compile. `satex gui` uses your own TeX.
- `feat` **Loading screens show tips.**
- `fix` **The version is starred only when the site's code changed since the release**, not for docs or CI changes.
- `feat` **Bibliography, glossary, index and theorem managers** with search, filters, a resizable divider and an entry detail view.
- `feat` **See every use of an entry**, with file and line, and jump to it.
- `perf` **Package caches are reused across examples.** Packages loaded at `\begin{document}` were rebuilt for every example. [`1fbd322`](https://github.com/EagleoutIce/satex-analyzer/commit/1fbd322)

### Site and docs

- `fix` **Choosing a case of uncertain text works.** The dropdown closed immediately and is now a row of buttons.
- `fix` **Before and after screenshots no longer overlap.**
- `fix` **Showcase pictures are no longer cut off**, and on phones they no longer jump while scrolling.
- `feat` **The site scripts are TypeScript.**
- `fix` **The benchmark page shows totals per release.** [`9322ee0`](https://github.com/EagleoutIce/satex-analyzer/commit/9322ee0) [`5e39b65`](https://github.com/EagleoutIce/satex-analyzer/commit/5e39b65) [`4c5e651`](https://github.com/EagleoutIce/satex-analyzer/commit/4c5e651)
- `fix` **The site deploys again.** [`d2c8c13`](https://github.com/EagleoutIce/satex-analyzer/commit/d2c8c13) [`6702be3`](https://github.com/EagleoutIce/satex-analyzer/commit/6702be3) [`7ab2503`](https://github.com/EagleoutIce/satex-analyzer/commit/7ab2503) [`37a5812`](https://github.com/EagleoutIce/satex-analyzer/commit/37a5812)

### Build and infrastructure

- `fix` **The site builds in CI again.**

## v0.2.0 (2026-11-07)

Usable on real documents: faster and more precise analysis, the playground and `satex gui`, editor widgets, VS Code and IntelliJ plugins, and latexmk builds.

### Playground

- `fix` **Clearer display of uncertain text.** [`1403031`](https://github.com/EagleoutIce/satex-analyzer/commit/1403031)
- `fix` **A loading widget shows that it is loading.** [`55f7986`](https://github.com/EagleoutIce/satex-analyzer/commit/55f7986)
- `fix` **Showcase screenshots line up.** [`45ba047`](https://github.com/EagleoutIce/satex-analyzer/commit/45ba047) [`1c31cb6`](https://github.com/EagleoutIce/satex-analyzer/commit/1c31cb6) [`b95ee96`](https://github.com/EagleoutIce/satex-analyzer/commit/b95ee96) [`d190837`](https://github.com/EagleoutIce/satex-analyzer/commit/d190837) [`6867c3d`](https://github.com/EagleoutIce/satex-analyzer/commit/6867c3d)
- `feat` **New front page** with the showcase on a rotating ring. [`22939fd`](https://github.com/EagleoutIce/satex-analyzer/commit/22939fd) [`b711b33`](https://github.com/EagleoutIce/satex-analyzer/commit/b711b33) [`e553530`](https://github.com/EagleoutIce/satex-analyzer/commit/e553530) [`884dcba`](https://github.com/EagleoutIce/satex-analyzer/commit/884dcba) [`0de0584`](https://github.com/EagleoutIce/satex-analyzer/commit/0de0584) [`e6a0098`](https://github.com/EagleoutIce/satex-analyzer/commit/e6a0098) [`9128f0d`](https://github.com/EagleoutIce/satex-analyzer/commit/9128f0d)
- `fix` **`satex gui` keeps running when its terminal closes** or its output is piped. [`bf8c194`](https://github.com/EagleoutIce/satex-analyzer/commit/bf8c194)
- `fix` **`satex gui` analyzes the open file first** and no longer freezes after a large compile. [`ff00464`](https://github.com/EagleoutIce/satex-analyzer/commit/ff00464)
- `fix` **The playground no longer hangs at start.** [`5a702f4`](https://github.com/EagleoutIce/satex-analyzer/commit/5a702f4) [`82d7c20`](https://github.com/EagleoutIce/satex-analyzer/commit/82d7c20) [`c0ff841`](https://github.com/EagleoutIce/satex-analyzer/commit/c0ff841) [`5e722f1`](https://github.com/EagleoutIce/satex-analyzer/commit/5e722f1)
- `fix` **Everything reruns after a package download.** [`6ee8c88`](https://github.com/EagleoutIce/satex-analyzer/commit/6ee8c88) [`d1aa312`](https://github.com/EagleoutIce/satex-analyzer/commit/d1aa312)
- `fix` **The trace stepper keeps its place**, and the phone toolbar fits. [`0a45669`](https://github.com/EagleoutIce/satex-analyzer/commit/0a45669) [`11edabd`](https://github.com/EagleoutIce/satex-analyzer/commit/11edabd) [`f832d13`](https://github.com/EagleoutIce/satex-analyzer/commit/f832d13) [`88f8f79`](https://github.com/EagleoutIce/satex-analyzer/commit/88f8f79)
- `feat` **`satex gui`** runs the playground on your own files, without uploading them. See [`satex gui`](doc/wiki/cli.md#the-playground-on-your-machine). [`73f101b`](https://github.com/EagleoutIce/satex-analyzer/commit/73f101b)
- `feat` **A live example on the front page** and a carousel of the widgets. [`d2d6c19`](https://github.com/EagleoutIce/satex-analyzer/commit/d2d6c19) [`d820715`](https://github.com/EagleoutIce/satex-analyzer/commit/d820715) [`ad58d41`](https://github.com/EagleoutIce/satex-analyzer/commit/ad58d41) [`c149ca2`](https://github.com/EagleoutIce/satex-analyzer/commit/c149ca2)
- `feat` **Trace stepper.** Step through a macro's expansion. [`50058c3`](https://github.com/EagleoutIce/satex-analyzer/commit/50058c3) [`cdc1835`](https://github.com/EagleoutIce/satex-analyzer/commit/cdc1835) [`30955ff`](https://github.com/EagleoutIce/satex-analyzer/commit/30955ff)
- `feat` **Draw page.** Find a symbol by drawing it. [`cbbaf15`](https://github.com/EagleoutIce/satex-analyzer/commit/cbbaf15) [`85fb7ec`](https://github.com/EagleoutIce/satex-analyzer/commit/85fb7ec) [`618019b`](https://github.com/EagleoutIce/satex-analyzer/commit/618019b)
- `feat` **Library**, and a first download with the most used packages. [`dbfec82`](https://github.com/EagleoutIce/satex-analyzer/commit/dbfec82) [`ae5e2f0`](https://github.com/EagleoutIce/satex-analyzer/commit/ae5e2f0) [`67750d8`](https://github.com/EagleoutIce/satex-analyzer/commit/67750d8)
- `feat` **Settings, outline, cache dialog, timings** and a typeset view. [`ef94a13`](https://github.com/EagleoutIce/satex-analyzer/commit/ef94a13) [`e65d145`](https://github.com/EagleoutIce/satex-analyzer/commit/e65d145) [`2ae687e`](https://github.com/EagleoutIce/satex-analyzer/commit/2ae687e)
- `perf` **First download is 6 MiB instead of 120 MiB.** [`6528842`](https://github.com/EagleoutIce/satex-analyzer/commit/6528842) [`d38a268`](https://github.com/EagleoutIce/satex-analyzer/commit/d38a268)
- `perf` **Builds use every core that memory allows.** [`f587917`](https://github.com/EagleoutIce/satex-analyzer/commit/f587917) [`29042c5`](https://github.com/EagleoutIce/satex-analyzer/commit/29042c5)

### Editor plugins and widgets

- `fix` **Hovers name the right package**, and say where they looked when no documentation is found. [`eeeb418`](https://github.com/EagleoutIce/satex-analyzer/commit/eeeb418)
- `fix` **TikZ arrows keep their dash pattern.** [`3fad770`](https://github.com/EagleoutIce/satex-analyzer/commit/3fad770)
- `fix` **TikZ arrows touch their nodes in the drawing widget.** [`10431bc`](https://github.com/EagleoutIce/satex-analyzer/commit/10431bc)
- `fix` **Paper sizes are known without geometry.** [`06e4a47`](https://github.com/EagleoutIce/satex-analyzer/commit/06e4a47)
- `fix` **Requests no longer wait for a running analysis.** [`fc5dda3`](https://github.com/EagleoutIce/satex-analyzer/commit/fc5dda3) [`30b2124`](https://github.com/EagleoutIce/satex-analyzer/commit/30b2124) [`40ae0c9`](https://github.com/EagleoutIce/satex-analyzer/commit/40ae0c9)
- `fix` **Dialogs keep every edit**, ask before discarding, and undo in one step. [`91b5239`](https://github.com/EagleoutIce/satex-analyzer/commit/91b5239) [`c6a80ef`](https://github.com/EagleoutIce/satex-analyzer/commit/c6a80ef)
- `fix` **Colors are completed only in color arguments.** [`5e6ecba`](https://github.com/EagleoutIce/satex-analyzer/commit/5e6ecba) [`59e9afa`](https://github.com/EagleoutIce/satex-analyzer/commit/59e9afa) [`d05aa00`](https://github.com/EagleoutIce/satex-analyzer/commit/d05aa00)
- `fix` **Code lenses and widget marks update after each run.** [`7518997`](https://github.com/EagleoutIce/satex-analyzer/commit/7518997) [`4431378`](https://github.com/EagleoutIce/satex-analyzer/commit/4431378) [`c48fea9`](https://github.com/EagleoutIce/satex-analyzer/commit/c48fea9) [`ff01740`](https://github.com/EagleoutIce/satex-analyzer/commit/ff01740)
- `fix` **Hovers start with the signature.** [`4c2984d`](https://github.com/EagleoutIce/satex-analyzer/commit/4c2984d) [`9aa8577`](https://github.com/EagleoutIce/satex-analyzer/commit/9aa8577)
- `feat` **The drawing widget knows every pgf node shape.** [`e1b165a`](https://github.com/EagleoutIce/satex-analyzer/commit/e1b165a)
- `feat` **Bibliography manager** with search, filters, import from DOI, arXiv, RIS, CSL-JSON and Zotero, a dblp check, and lints for duplicates and missing fields. [`eb65d13`](https://github.com/EagleoutIce/satex-analyzer/commit/eb65d13)
- `feat` **Definition history.** Every redefinition of a command, newest first. [`b29128f`](https://github.com/EagleoutIce/satex-analyzer/commit/b29128f)
- `feat` **Drag the margins** in the page layout widget. [`3fad770`](https://github.com/EagleoutIce/satex-analyzer/commit/3fad770)
- `feat` **Widgets change only what you edited.** [`d3c4880`](https://github.com/EagleoutIce/satex-analyzer/commit/d3c4880) [`c5be515`](https://github.com/EagleoutIce/satex-analyzer/commit/c5be515) [`decf572`](https://github.com/EagleoutIce/satex-analyzer/commit/decf572)
- `feat` **Page layout per class** (KOMA-Script, memoir, standard classes). See [type plugins](doc/wiki/type-plugins.md). [`cf4243a`](https://github.com/EagleoutIce/satex-analyzer/commit/cf4243a)
- `feat` **Index and bibliography managers**, and length fields that take any unit. See [type plugins](doc/wiki/type-plugins.md). [`06e4a47`](https://github.com/EagleoutIce/satex-analyzer/commit/06e4a47)
- `feat` **Managers for glossaries, index terms, bibliographies and theorems.** See [type plugins](doc/wiki/type-plugins.md). [`e22a899`](https://github.com/EagleoutIce/satex-analyzer/commit/e22a899)
- `feat` **Keys layer** for labels, citations, glossary and index keys. [`ff00464`](https://github.com/EagleoutIce/satex-analyzer/commit/ff00464)
- `feat` **Type plugins.** Each editor feature is a plugin, triggered by what the analysis finds. See [type plugins](doc/wiki/type-plugins.md). [`75d96fd`](https://github.com/EagleoutIce/satex-analyzer/commit/75d96fd)
- `feat` **TikZ widget** with canvas, properties, styles and layers. [`2a79cfa`](https://github.com/EagleoutIce/satex-analyzer/commit/2a79cfa) [`5e87296`](https://github.com/EagleoutIce/satex-analyzer/commit/5e87296) [`990c691`](https://github.com/EagleoutIce/satex-analyzer/commit/990c691) [`ff00464`](https://github.com/EagleoutIce/satex-analyzer/commit/ff00464)
- `feat` **Table, image, page layout and color dialogs.** [`3c2a595`](https://github.com/EagleoutIce/satex-analyzer/commit/3c2a595) [`d07e6b7`](https://github.com/EagleoutIce/satex-analyzer/commit/d07e6b7) [`bcadebc`](https://github.com/EagleoutIce/satex-analyzer/commit/bcadebc) [`b74130c`](https://github.com/EagleoutIce/satex-analyzer/commit/b74130c)
- `feat` **Typeset and PDF layers.** [`d16d209`](https://github.com/EagleoutIce/satex-analyzer/commit/d16d209) [`71cbd9f`](https://github.com/EagleoutIce/satex-analyzer/commit/71cbd9f)
- `feat` **Forest trees, beamer overlays and biblatex style hovers.** [`0c4d45a`](https://github.com/EagleoutIce/satex-analyzer/commit/0c4d45a) [`c923274`](https://github.com/EagleoutIce/satex-analyzer/commit/c923274) [`6eecfa0`](https://github.com/EagleoutIce/satex-analyzer/commit/6eecfa0)
- `feat` **Open widgets from the gutter.** Code lenses are optional. [`4e60fbf`](https://github.com/EagleoutIce/satex-analyzer/commit/4e60fbf) [`7319b01`](https://github.com/EagleoutIce/satex-analyzer/commit/7319b01)
- `feat` **Go to options, colors and column types**, and package documentation in completions. [`6ee8c88`](https://github.com/EagleoutIce/satex-analyzer/commit/6ee8c88) [`4fbd4cd`](https://github.com/EagleoutIce/satex-analyzer/commit/4fbd4cd)
- `feat` **Completions preview TikZ drawings** and suggest the closest name. [`760b810`](https://github.com/EagleoutIce/satex-analyzer/commit/760b810) [`a5f832c`](https://github.com/EagleoutIce/satex-analyzer/commit/a5f832c)

### VS Code and other editors

- `feat` **VS Code** gets every widget, the library and settings. See [VS Code](doc/wiki/vscode.md). [`40174d7`](https://github.com/EagleoutIce/satex-analyzer/commit/40174d7) [`20c6d00`](https://github.com/EagleoutIce/satex-analyzer/commit/20c6d00) [`da25261`](https://github.com/EagleoutIce/satex-analyzer/commit/da25261)
- `feat` **`cargo vscode`** builds and installs the extension. [`640f1e5`](https://github.com/EagleoutIce/satex-analyzer/commit/640f1e5)
- `feat` **IntelliJ plugin** based on LSP4IJ. See [IntelliJ](doc/wiki/intellij.md). [`05dfeee`](https://github.com/EagleoutIce/satex-analyzer/commit/05dfeee)

### Lints

- `fix` **A missing image suggests `\graphicspath`** instead of reporting a missing package. [`5cca876`](https://github.com/EagleoutIce/satex-analyzer/commit/5cca876)
- `fix` **Fewer false positives.** [`be400e6`](https://github.com/EagleoutIce/satex-analyzer/commit/be400e6) [`cd0354b`](https://github.com/EagleoutIce/satex-analyzer/commit/cd0354b) [`0909180`](https://github.com/EagleoutIce/satex-analyzer/commit/0909180) [`bedf0ef`](https://github.com/EagleoutIce/satex-analyzer/commit/bedf0ef)
- `fix` **Uncertain findings are notes, not errors.** [`cde9439`](https://github.com/EagleoutIce/satex-analyzer/commit/cde9439) [`86359a0`](https://github.com/EagleoutIce/satex-analyzer/commit/86359a0) [`90f5393`](https://github.com/EagleoutIce/satex-analyzer/commit/90f5393)
- `fix` **Typed references become `\cref`**, loading cleveref when needed. [`442ee8e`](https://github.com/EagleoutIce/satex-analyzer/commit/442ee8e) [`1a1da0b`](https://github.com/EagleoutIce/satex-analyzer/commit/1a1da0b)
- `fix` **One finding per `\cite`**, and duplicate labels only where LaTeX warns. [`aead4e8`](https://github.com/EagleoutIce/satex-analyzer/commit/aead4e8) [`3d5d4b7`](https://github.com/EagleoutIce/satex-analyzer/commit/3d5d4b7) [`e548a61`](https://github.com/EagleoutIce/satex-analyzer/commit/e548a61)
- `fix` **Undefined colors only where xcolor reports them.** [`8bf8a22`](https://github.com/EagleoutIce/satex-analyzer/commit/8bf8a22)
- `feat` **New lint `clones`** for near-duplicate blocks. [`b28c7c1`](https://github.com/EagleoutIce/satex-analyzer/commit/b28c7c1)
- `feat` **Lints for `et al.` and biblatex**, and a BibTeX to biblatex migration. [`f60be55`](https://github.com/EagleoutIce/satex-analyzer/commit/f60be55) [`45ceb45`](https://github.com/EagleoutIce/satex-analyzer/commit/45ceb45)
- `feat` **Table typography lints**, and `undefined-cref-format`. [`572442f`](https://github.com/EagleoutIce/satex-analyzer/commit/572442f) [`abde87a`](https://github.com/EagleoutIce/satex-analyzer/commit/abde87a) [`3617ffa`](https://github.com/EagleoutIce/satex-analyzer/commit/3617ffa)
- `feat` **Stable rule numbers**, project styles and findings grouped by rule. See [all rules](doc/wiki/lints.md). [`261a0e7`](https://github.com/EagleoutIce/satex-analyzer/commit/261a0e7) [`f3ac51b`](https://github.com/EagleoutIce/satex-analyzer/commit/f3ac51b) [`960d8c0`](https://github.com/EagleoutIce/satex-analyzer/commit/960d8c0)

### Analysis

- `fix` **No crash on boxes of huge size.** [`199ef55`](https://github.com/EagleoutIce/satex-analyzer/commit/199ef55)
- `fix` **Matches pdfTeX** for `\meaning` of a `\noexpand`ed macro, `\toks0=\bgroup`, `\deadcycles`, `\ifx` on parameter characters, `^^` after a control word and `\scantokens`. [`15e2c6b`](https://github.com/EagleoutIce/satex-analyzer/commit/15e2c6b)
- `fix` **A catcode change for an unknown character leaves `{`, `}` and `\` alone.** This is a named, configurable assumption. [`9327f51`](https://github.com/EagleoutIce/satex-analyzer/commit/9327f51)
- `fix` **Fewer paths for names built from unknown text.** See [domains](doc/wiki/domains.md). [`5703f45`](https://github.com/EagleoutIce/satex-analyzer/commit/5703f45)
- `fix` **Class options no longer invalidate every package cache.** [`7c6fe9c`](https://github.com/EagleoutIce/satex-analyzer/commit/7c6fe9c)
- `fix` **No false errors with KOMA-Script's notecolumn.** [`a3be187`](https://github.com/EagleoutIce/satex-analyzer/commit/a3be187) [`7eda937`](https://github.com/EagleoutIce/satex-analyzer/commit/7eda937)
- `fix` **Captions of `[h]` floats stay in the typeset text.** See [deferred material](doc/wiki/domains.md#files-written-in-the-run). [`c510435`](https://github.com/EagleoutIce/satex-analyzer/commit/c510435)
- `fix` **More precise numbers.** See [typed unknown text](doc/wiki/domains.md#unknown-text-the-textclass-lattice). [`e6255b9`](https://github.com/EagleoutIce/satex-analyzer/commit/e6255b9) [`52f1c7a`](https://github.com/EagleoutIce/satex-analyzer/commit/52f1c7a) [`5352e4f`](https://github.com/EagleoutIce/satex-analyzer/commit/5352e4f) [`a796fe0`](https://github.com/EagleoutIce/satex-analyzer/commit/a796fe0)
- `fix` **More precise joins.** See [the join](doc/wiki/domains.md#the-join-in-general). [`f827f03`](https://github.com/EagleoutIce/satex-analyzer/commit/f827f03) [`f47ac1b`](https://github.com/EagleoutIce/satex-analyzer/commit/f47ac1b) [`3495a7d`](https://github.com/EagleoutIce/satex-analyzer/commit/3495a7d) [`0e47ccb`](https://github.com/EagleoutIce/satex-analyzer/commit/0e47ccb)
- `fix` **Loops end.** See [call summaries](doc/wiki/domains.md#call-summaries-what-a-call-did-twice). [`d4b6fa3`](https://github.com/EagleoutIce/satex-analyzer/commit/d4b6fa3) [`af60255`](https://github.com/EagleoutIce/satex-analyzer/commit/af60255) [`1adff24`](https://github.com/EagleoutIce/satex-analyzer/commit/1adff24) [`6b50406`](https://github.com/EagleoutIce/satex-analyzer/commit/6b50406)
- `fix` **Approximations never produce certain errors.** See [soundness invariant](doc/wiki/domains.md#soundness-invariant). [`8ff098e`](https://github.com/EagleoutIce/satex-analyzer/commit/8ff098e) [`0d387ac`](https://github.com/EagleoutIce/satex-analyzer/commit/0d387ac) [`5712ab7`](https://github.com/EagleoutIce/satex-analyzer/commit/5712ab7)
- `fix` **Groups and output routines restore what they saved.** [`59d1ae1`](https://github.com/EagleoutIce/satex-analyzer/commit/59d1ae1) [`cf04851`](https://github.com/EagleoutIce/satex-analyzer/commit/cf04851) [`e5d53f0`](https://github.com/EagleoutIce/satex-analyzer/commit/e5d53f0) [`04b0533`](https://github.com/EagleoutIce/satex-analyzer/commit/04b0533)
- `fix` **Typesetting closer to TeX**: alignments, `\parshape`, math styles, `\unskip`, hyphenation of ligatures. [`e38b36f`](https://github.com/EagleoutIce/satex-analyzer/commit/e38b36f) [`484f4d6`](https://github.com/EagleoutIce/satex-analyzer/commit/484f4d6) [`884f40f`](https://github.com/EagleoutIce/satex-analyzer/commit/884f40f) [`3b8ae10`](https://github.com/EagleoutIce/satex-analyzer/commit/3b8ae10) [`4331fe3`](https://github.com/EagleoutIce/satex-analyzer/commit/4331fe3) [`27d3cf4`](https://github.com/EagleoutIce/satex-analyzer/commit/27d3cf4)
- `fix` **Page breaks match pdflatex.** See [deferred material](doc/wiki/domains.md#files-written-in-the-run). [`b3abcbc`](https://github.com/EagleoutIce/satex-analyzer/commit/b3abcbc) [`7e5acb5`](https://github.com/EagleoutIce/satex-analyzer/commit/7e5acb5) [`0d4c944`](https://github.com/EagleoutIce/satex-analyzer/commit/0d4c944) [`f49169a`](https://github.com/EagleoutIce/satex-analyzer/commit/f49169a)
- `fix` **More exact scans over unknown text.** See [one-of values](doc/wiki/domains.md#the-set-lift-oneof). [`a6a7830`](https://github.com/EagleoutIce/satex-analyzer/commit/a6a7830) [`d9e4316`](https://github.com/EagleoutIce/satex-analyzer/commit/d9e4316) [`490a0f5`](https://github.com/EagleoutIce/satex-analyzer/commit/490a0f5) [`5b62065`](https://github.com/EagleoutIce/satex-analyzer/commit/5b62065)
- `fix` **A missing package no longer ends the analysis**, and messages print as TeX prints them. [`bc78fff`](https://github.com/EagleoutIce/satex-analyzer/commit/bc78fff) [`18d31c8`](https://github.com/EagleoutIce/satex-analyzer/commit/18d31c8)
- `fix` **Magic comments can select plain TeX.** [`ed98f45`](https://github.com/EagleoutIce/satex-analyzer/commit/ed98f45) [`53c3d56`](https://github.com/EagleoutIce/satex-analyzer/commit/53c3d56)
- `fix` **Every written file reaches the next run.** [`94760be`](https://github.com/EagleoutIce/satex-analyzer/commit/94760be) [`9875e23`](https://github.com/EagleoutIce/satex-analyzer/commit/9875e23) [`06d6afe`](https://github.com/EagleoutIce/satex-analyzer/commit/06d6afe) [`49d686e`](https://github.com/EagleoutIce/satex-analyzer/commit/49d686e)
- `fix` **Cache replays are complete.** [`54bfbc2`](https://github.com/EagleoutIce/satex-analyzer/commit/54bfbc2) [`7ea0631`](https://github.com/EagleoutIce/satex-analyzer/commit/7ea0631) [`fbc7634`](https://github.com/EagleoutIce/satex-analyzer/commit/fbc7634)
- `fix` **Cache keys are the same on every platform.** See [caches](doc/wiki/configuration.md#format-and-package-caches). [`b65d1b1`](https://github.com/EagleoutIce/satex-analyzer/commit/b65d1b1) [`c85d25f`](https://github.com/EagleoutIce/satex-analyzer/commit/c85d25f) [`a686f08`](https://github.com/EagleoutIce/satex-analyzer/commit/a686f08) [`4973535`](https://github.com/EagleoutIce/satex-analyzer/commit/4973535)
- `fix` **Caches work without file locks**, for example on network drives. [`a8c7530`](https://github.com/EagleoutIce/satex-analyzer/commit/a8c7530) [`a0d9fc8`](https://github.com/EagleoutIce/satex-analyzer/commit/a0d9fc8) [`e1220cb`](https://github.com/EagleoutIce/satex-analyzer/commit/e1220cb)
- `fix` **Memory and time limits stop a build cleanly** with its last complete run. See [limits](doc/wiki/configuration.md#configurable-limits-for-the-analysis). [`5832594`](https://github.com/EagleoutIce/satex-analyzer/commit/5832594) [`e8362b4`](https://github.com/EagleoutIce/satex-analyzer/commit/e8362b4) [`71a431b`](https://github.com/EagleoutIce/satex-analyzer/commit/71a431b) [`26fa2db`](https://github.com/EagleoutIce/satex-analyzer/commit/26fa2db)
- `fix` **`satex compile` uses the engine set in latexmkrc.** [`ce581f1`](https://github.com/EagleoutIce/satex-analyzer/commit/ce581f1)
- `fix` **`build.lua` runs sandboxed.** [`29ae6e3`](https://github.com/EagleoutIce/satex-analyzer/commit/29ae6e3) [`4354840`](https://github.com/EagleoutIce/satex-analyzer/commit/4354840)
- `feat` **Colors are tracked.** See [colors](doc/wiki/domains.md#colors-color-and-colors). [`aed0ae1`](https://github.com/EagleoutIce/satex-analyzer/commit/aed0ae1)
- `feat` **Memory limit** per run and per process. See [limits](doc/wiki/configuration.md#configurable-limits-for-the-analysis). [`6fde68e`](https://github.com/EagleoutIce/satex-analyzer/commit/6fde68e)
- `feat` **Builds follow latexmk**, with output directories and resumed reruns. [`816fdad`](https://github.com/EagleoutIce/satex-analyzer/commit/816fdad) [`a101964`](https://github.com/EagleoutIce/satex-analyzer/commit/a101964)
- `feat` **Report why two paths were not joined.** [`303f218`](https://github.com/EagleoutIce/satex-analyzer/commit/303f218)
- `perf` **Unchanged documents come straight from the cache.** [`d2dbb95`](https://github.com/EagleoutIce/satex-analyzer/commit/d2dbb95) [`8e378c3`](https://github.com/EagleoutIce/satex-analyzer/commit/8e378c3) [`46842cf`](https://github.com/EagleoutIce/satex-analyzer/commit/46842cf)

### CLI

- `feat` **`satex compile`** builds with latexmk and explains the log. [`28e9bc1`](https://github.com/EagleoutIce/satex-analyzer/commit/28e9bc1)
- `feat` **Progress, hotspots and a cache report.** [`ddee2ce`](https://github.com/EagleoutIce/satex-analyzer/commit/ddee2ce)
- `feat` **Shell completions.** [`f0699ca`](https://github.com/EagleoutIce/satex-analyzer/commit/f0699ca)

### Site and docs

- `fix` **The site deploys again.**
- `fix` **Every in-page link works.** [`8028fda`](https://github.com/EagleoutIce/satex-analyzer/commit/8028fda)
- `fix` **The semantics pages match tex.web and the code.** [`2e43003`](https://github.com/EagleoutIce/satex-analyzer/commit/2e43003) [`566fb52`](https://github.com/EagleoutIce/satex-analyzer/commit/566fb52) [`74777e5`](https://github.com/EagleoutIce/satex-analyzer/commit/74777e5) [`6d534b2`](https://github.com/EagleoutIce/satex-analyzer/commit/6d534b2) [`23e02be`](https://github.com/EagleoutIce/satex-analyzer/commit/23e02be) [`2be133d`](https://github.com/EagleoutIce/satex-analyzer/commit/2be133d) [`8757fc4`](https://github.com/EagleoutIce/satex-analyzer/commit/8757fc4)
- `feat` **Highlighted command output on the docs pages.** [`b0d090f`](https://github.com/EagleoutIce/satex-analyzer/commit/b0d090f)
- `feat` **Front page updates**: more of the showcase on the first screen, plugin groups and new screenshots. [`2f8a357`](https://github.com/EagleoutIce/satex-analyzer/commit/2f8a357) [`fd50308`](https://github.com/EagleoutIce/satex-analyzer/commit/fd50308) [`3e8a887`](https://github.com/EagleoutIce/satex-analyzer/commit/3e8a887) [`641737d`](https://github.com/EagleoutIce/satex-analyzer/commit/641737d)
- `docs` **Wiki pages** on the type lattice, precision limits and why TeX is hard. See [why TeX is hard](doc/wiki/challenges.md). [`5083317`](https://github.com/EagleoutIce/satex-analyzer/commit/5083317) [`3a04299`](https://github.com/EagleoutIce/satex-analyzer/commit/3a04299) [`85e15a6`](https://github.com/EagleoutIce/satex-analyzer/commit/85e15a6)
- `feat` **Hover cards** for control sequences. [`3b86d4b`](https://github.com/EagleoutIce/satex-analyzer/commit/3b86d4b)
- `docs` **This changelog**, also on the site. [`4e72dd7`](https://github.com/EagleoutIce/satex-analyzer/commit/4e72dd7)

### Build and infrastructure

- `build` **Faster CI** with shards that reuse one build. [`03858a7`](https://github.com/EagleoutIce/satex-analyzer/commit/03858a7) [`61992f1`](https://github.com/EagleoutIce/satex-analyzer/commit/61992f1) [`31d41cf`](https://github.com/EagleoutIce/satex-analyzer/commit/31d41cf) [`b8031fc`](https://github.com/EagleoutIce/satex-analyzer/commit/b8031fc)
- `build` **Incremental CI**, a portable wasm toolchain, and Pages built from `main`. [`ff16822`](https://github.com/EagleoutIce/satex-analyzer/commit/ff16822) [`7397e7c`](https://github.com/EagleoutIce/satex-analyzer/commit/7397e7c)
- `build` **Benchmarks across releases.** [`9a9de54`](https://github.com/EagleoutIce/satex-analyzer/commit/9a9de54)
- `build` **Developer binaries** behind a `tools` feature. [`8bfbc07`](https://github.com/EagleoutIce/satex-analyzer/commit/8bfbc07)

## v0.1.0 (2026-09-25)

First public version: the abstract interpreter for (La)TeX with lints, a language server, a CLI for Windows, macOS and Linux, and the wiki. [`5af9583`](https://github.com/EagleoutIce/satex-analyzer/commit/5af9583) [`5570f80`](https://github.com/EagleoutIce/satex-analyzer/commit/5570f80)
