# Changelog

Newest first, fixes first in each group. Every entry has a badge: `feat` a feature, `fix` a bug fix, `perf` speed, `docs` documentation.
Commit and pull request links point to the details.
Maintained with `scripts/changelog.py` (`draft`, `check`, `links`, `release`, see [doc/releasing.md](doc/releasing.md)).

## Unreleased

<!-- reviewed: 477934d1 -->

### Analysis

- `fix` **Drawing the progress bar changes nothing.** The lines it counts were kept with the analysis, so a package cache read and its replay could differ in them. They are display only now, and each run draws at its own steps.
- `fix` **No false missing end after a library scan.** A library argument over unknown text that read past the end of the main file at `\end{document}` now ends the analysis as such, not as a document without `\end{document}`.
- `fix` **A runaway library argument no longer ends the run.** A delimited argument begun in library code that ran on through the document is dropped only when another path goes on. As the last path it continues with the document text given back, so a widened value read back after a cut reaches the end.
- `fix` **Big diagrams are no longer cut.** `limits.line_steps` is now 500M (was 100M): one line with a TikZ diagram of many nodes runs 100M to 200M steps, and a cut there cost far more than it saved.
- `fix` **Cold and warm caches agree.** Calls open across the start or end of a cached package load are no longer recorded. A replayed load runs none of them, so a run with an empty package cache and one with a full cache now learn the same call effects and report the same.
- `fix` **An unknown number equals itself.** `\int_case:nn` ends its list with the value as its own sentinel. Comparing an unknown number with itself was undecided, so the false arm read past the list: false library errors for `\int_case:nn`, `\int_to_alph:n` and their kin on unknown values. A test whose two sides read the same register or number now decides by the constants added to it.
- `fix` **Preamble lines no longer pile up open calls.** To learn what a preamble line defines, satex runs it in a side machine. A line its step budget stopped left its calls counted as open. After 512 such lines every call of the same macro (often `\hook_use:n`) was cut as a runaway recursion and defined nothing.
- `fix` **No false runaway in every `longtable`.** longtable walks its last row back with `\lastbox`, and satex cannot see that list: the arm with one box more than the columns ran `\LT@max@sel` on nothing, a path TeX never takes. An undecided box test on a box of unknown kind now marks its path, and the runaway there is refuted as in other library code.
- `perf` **Joins in linear time.** Joining two paths merged the loops each had met by searching one list per entry. A thesis meets thousands of loops, so joins took half its time. ma-henry-schuler's first run now ends in 277 s, where it did not end in 600 s.
- `fix` **Exponentials and powers of l3fp.** `exp`, `ln` and `^` in `\fp_eval:n` give TeX's values: dividing digits that repeat (`1/10`) was cut as an endless recursion, though its nested scans unwind. tikz-sankey diagrams run in a tenth of the steps, and a thesis with two of them reaches its end.
- `fix` **No false missing end after a skipped line.** When the guard skipped the rest of a stuck line, the end of the analysis is reported as such, not as a document without `\end{document}`.
- `fix` **Raised boxes in math.** `\raise` and `\lower` in inline math keep the formula's size: a TikZ picture inside a math node has its width, and a graph of such nodes no longer forks.
- `feat` **Unread parts are errors.** The new lint `unread-document` (A003) reports every file the document names that the run never read, and a scan that read past the document's end. Before, they were silent in `satex lint`.
- `perf` **Written lines and fonts.** A line read back from the `.aux` file that stops at its budget leaves nothing behind for the next line, and files are looked up by path in a map: a thesis with many labels runs in a third of the time.
- `perf` **Call summaries of large tables.** A summary looks up each run's writes by name, and a call's shape is checked once, not at every meeting: pgfplotstable no longer stalls a run.
- `fix` **Rounded corners at unknown points.** A loop summary no longer gives a waiting path the code of another program point: `\pgfprocessround` raises no false errors.
- `fix` **Joins of code.** Paths whose code is bound apart are no longer joined into one, and a call summary no longer carries an old call's numbers. `\let` to text that is one of several takes the meanings of its first tokens. pgfmath on random counts is exact again and cheaper.
- `fix` **Paired lists.** An `\edef` of names a join kept paired (a float on the top list or deferred) holds their one text where every arm agrees.
- `fix` **Cuts inside a library call.** A line cut inside library code that holds a document token as data (a name or number passed down into l3fp) no longer reads on in the middle of that code: only a list that called the document's text (`\include`) stays. A tikz-sankey diagram past the line budget ran l3fp over the cut's leftovers for 160k forks and 20 false library errors.
- `fix` **Cuts inside a chapter.** A line cut inside `\include` drops the loop it stands in, even when its lists hold the document's text: the later chapters are read, with no extra `\fi` and no garbage file names.
- `fix` **A stuck path inside a command.** It ends that path only: the paths waiting outside go on (`\pgfmathparse{\the\wd2}` reached no end).
- `fix` **Colors of unknown mix.** An undelimited argument read from text that is one of several takes the first argument of each and puts the rest back. `\color{blue!\w!red}` with an unknown `\w` no longer swallows the rest of the document. `\meaning` of text that is one of several prints each, so `\colorlet` in a loop keeps the color exact.
- `fix` **Calls by unknown names.** A macro reached by a name of unknown spelling no longer takes the code behind its call as arguments. That code could redefine `\let` on a path and turn the rest of the document into data.
- `fix` **Long exact loops.** A loop over known values is no longer cut as spinning. Its cut made names look undefined later on.
- `fix` **LuaTeX's PDF commands.** `\pdfvariable` reads as a quantity, `\pdfextension` reads its operands, and `\pdffeedback` is a query.
- `fix` **Box sizes.** `\ ` in math keeps a box's width, and `\unhbox` keeps whether the last node is a ligature.
- `perf` **Long bibliographies.** A walk over a key list of unknown text ends after `limits.rest_splits` splits, and a path save no longer copies the list of wildcard names.
- `fix` **A lost formula ends.** A `$` that finds no formula to close leaves math mode, so the `\par` behind it no longer loops.
- `fix` **Note columns clear.** A call summary no longer stands for a call that reads another value than its first runs did, so `\clearnotecolumns` ends instead of setting hundreds of pages. A package loaded inside a paragraph stays out of the package cache.
- `feat` **Long documents reach their end.** Where floats and page parts land is now assumed past a budget instead of split per arm (`analysis.assume_page_one_arm`, reported as `page-one-arm`), so a thesis no longer runs the output routine once per float arm.
- `fix` **A cut no longer forgets a chapter.** A nesting cut inside `\include` widened all the chapter defined, and macros with arguments stayed known through a widening.
- `fix` **The line guard reads on.** A line that overruns its budget again is skipped and the run goes on; it stops only with no line left, or after `limits.line_overruns` cuts.
- `perf` **Much less work per path switch and written line.** Replaying a written line is capped at 50 000 steps, value scans over unknown text fork a bounded number of times, and paths meet without building their whole key.
- `fix` **Undecided mode stays one mode.** Arms of a mode split meet again in one mode, and token list sets survive widening, so a joined `\fi` no longer starts false loops. [`0da79ed`](https://github.com/EagleoutIce/satex-analyzer/commit/0da79ed) [`71c673d`](https://github.com/EagleoutIce/satex-analyzer/commit/71c673d)
- `fix` **Typed numbers stay typed through pgfmath.** Digits, signs and printed dimensions keep their type, and copies of one number compare equal. [`4536f7f`](https://github.com/EagleoutIce/satex-analyzer/commit/4536f7f) [`cde8a60`](https://github.com/EagleoutIce/satex-analyzer/commit/cde8a60) [`4084ec7`](https://github.com/EagleoutIce/satex-analyzer/commit/4084ec7)
- `fix` **Float lists keep their names, and value scans over unknown text no longer cut paths early.** [`29bd2bb`](https://github.com/EagleoutIce/satex-analyzer/commit/29bd2bb) [`93e8242`](https://github.com/EagleoutIce/satex-analyzer/commit/93e8242) [`9336d57`](https://github.com/EagleoutIce/satex-analyzer/commit/9336d57) [`601f0a7`](https://github.com/EagleoutIce/satex-analyzer/commit/601f0a7)
- `fix` **A build that falls back to an earlier run says so** in its report. [`57a6b6b`](https://github.com/EagleoutIce/satex-analyzer/commit/57a6b6b)
- `fix` **A number parser over an unknown value stops walking its digits.** A macro's runs on known values teach what it leaves; a run that keeps forking ends with that effect, each value of its type, named `call-learned-effect` (`analysis.assume_call_learned_effect`, `limits.call_learn_after`, `limits.call_learn_forks`). `\\pgfmathparse` needs no rule or stub. Rounded nodes and arrows of unknown size end clean.
- `fix` **A penalty in a formula keeps the formula's size.** siunitx 3.6 sets `\nobreak` inside math, so every `\qty` had an unknown width, and pictures around one raised false pgfmath errors. [`da70671`](https://github.com/EagleoutIce/satex-analyzer/commit/da70671)
- `fix` **No crash when every arm of a choice is refuted.** The run counted its fork off twice. [`bf50fd5`](https://github.com/EagleoutIce/satex-analyzer/commit/bf50fd5)

### CLI

- `fix` **The progress bars no longer flicker.** Each frame is drawn at once over the last one, inside a synchronized update, and the bar counts the package caches loaded.
- `fix` **The right root document.** Only the project's top-level build files (`latexmkrc`, `Makefile`, `build.lua`, …) hint at the root: a vendored bundle's `build.lua` no longer makes its example the root. `project.root` in `satex.yaml` names the root over every guess.
- `fix` **Progress that moves.** The bar redraws every 2048 steps, not on their exact multiples, which a jump over skipped; counts read `1.2B`; a package stored in parts is reported cached once.

### Lints

- `feat` **Quiet arguments.** `lints.quiet-arguments.commands` names commands whose arguments the typographic rules skip, as they skip verbatim text. Defaults: `\todo`, `\missingfigure`, `\href`, `\lstinputlisting`, `\lstinline`, `\inputminted`, `\mintinline`.
- `fix` **Precision in `S` columns.** `inconsistent-precision` on a siunitx `S` column names the one option that rounds it (`round-mode = places`), not every cell.
- `fix` **Braced names in the generated bibliography.** A corporate author (`{{GCC Development Team}}`) or a braced particle (`Alexander {von Rhein}`) is split only outside braces, as BibTeX splits it: the `.bbl` satex writes no longer leaves a `\name` argument open.
- `fix` **No tie asked before a citation that prints a name.** `missing-nonbreaking-space` leaves `\citeauthor`, `\textcite`, `\citet` and their kin alone: they print words of the sentence.
- `fix` **Month constants with biblatex.** `undefined-bibliography-string` no longer reports `jun`: biber predefines the months, and the style lookup now finds `plain.bst` where BibTeX keeps it.
- `fix` **Names a package builds per call are no dead definitions.** A sankey node's flags or a citation package's per-key macros were reported at every call.
- `fix` **Only clones worth a macro.** `clones` reports a block only when a macro saves more text than its arguments cost (`min_saved`, `parameter_cost`), and never one needing more than `max_parameters`.
- `feat` **Clearer `already defined` findings.** `already-defined` names the definition a `\newcommand` ran into, by package and line. The same definition again is fixed with `\providecommand`, safely, and a package's command gets the warning that overriding it changes the package's own uses.
- `feat` **Lists that pick the wrong environment.** `itemize-numbered` finds an `itemize` whose items count by hand (`\item[1.]`, `\item[(b)]`) and turns it into an `enumerate`, with enumitem's `label`, `start` or `resume` where the default labels differ. `itemize-described` turns one labelled with words into a `description`.
- `feat` **Packages a successor continues.** `package-successor` names the successor of a loaded package and what moving changes, and its fix loads the successor in its place: `wrapfig` to `wrapfig2`. It stays quiet where another package loads the old one. The pairs are data in `lints.package-successor.successors`.
- `fix` **Registers declared twice.** `new-redefines` no longer reports a register declared again as one of its kind (`\newcount\rows` in two tables): LaTeX takes it silently.
- `fix` **False errors after `\newif` and next to colors are gone.** `new-redefines` no longer blames a `\new…` call for a plain `\def` after it, and `undefined-color` needs an error that names the color.
- `fix` **An editor never offers to turn off `clones`.** Copied code is a finding to act on, not the project's style. [`1cdd3cc`](https://github.com/EagleoutIce/satex-analyzer/commit/1cdd3cc)
- `fix` **Prose rules skip what undefined commands leave behind.** A TeX file opened without its preamble no longer gets spacing findings on `\node (a)at(b)` or inside an undefined environment. [`5d61ec1`](https://github.com/EagleoutIce/satex-analyzer/commit/5d61ec1)

### VS Code

- `fix` **A server the client gave up on restarts.** [`096dc93`](https://github.com/EagleoutIce/satex-analyzer/commit/096dc93)
- `fix` **Findings no longer flicker while an analysis runs.** Those a later pass or more precision can change stay hidden until the run ends, then swap in once. Findings over the source text show at once and stay. `satex.findings.whileAnalyzing` is `hide` (default), `keep` or `mark`. A run a budget stopped never replaces findings, its own join them, marked as unfinished.

### Playground

- `feat` **Nodes kept relative.** In the TikZ widget a node can be locked relative to its neighbours: a drag writes a shift from the nearest node, never `at (x,y)`. Turning on relative placement makes every node relative at once.
- `feat` **One layout for list managers.** The bibliography, glossary and index views share a resizable list and detail, and every entry shows its uses in one search view. Removing an entry moved into a fold.
- `feat` **Tips in the console bar.** The closed console's line shows tips while the page is visible.
- `feat` **The draw pad can be resized**, is centred, and its size stays in the link. [`190bf16`](https://github.com/EagleoutIce/satex-analyzer/commit/190bf16)
- `feat` **Hover lists fetched packages, and dialogs have an another-tip button.** [`149d779`](https://github.com/EagleoutIce/satex-analyzer/commit/149d779) [`ef9b92c`](https://github.com/EagleoutIce/satex-analyzer/commit/ef9b92c)
- `fix` **The header keeps its menu until the links fit.** Lists use their own icon, and go-to-definition works after downloads. [`28837d2`](https://github.com/EagleoutIce/satex-analyzer/commit/28837d2) [`b55e95f`](https://github.com/EagleoutIce/satex-analyzer/commit/b55e95f)
- `fix` **`\begin` completion keeps the arguments already written.** [`0ba4807`](https://github.com/EagleoutIce/satex-analyzer/commit/0ba4807)
- `fix` **The style prompt moved under lints.** [`03ad504`](https://github.com/EagleoutIce/satex-analyzer/commit/03ad504)
- `feat` **Compile works on the web playground.** pdfTeX runs in the browser, with TeX Live 2026 files fetched on demand and kept. The hosts are set under Settings, Compile, or turned off. `satex gui` keeps using your own TeX.
- `feat` **Loading banners show tips**, a new one every 20 s, from a list checked against the docs and the code.
- `fix` **The version loses its star when only docs or CI changed since the release.** It follows what the site ships, not the commit count.
- `feat` **One layout for the bibliography, glossary, index and theorem managers.** Search, filter chips with counts, a list that fills its pane, a draggable divider (arrow keys too, 35% by default, remembered) and the entry's detail. The glossary's chips separate entry commands (`\newglossaryentry`, `\newacronym`) and glossary types.
- `feat` **Inspect every use of an entry.** The detail of an entry lists each use with file, line and source line, and a click opens it. Delete moved behind the uses, in a fold.
- `perf` **Package caches replay in every document.** A package loaded at `\begin{document}`, like graphics' epstopdf-base, was rebuilt in the browser for every example. [`1fbd322`](https://github.com/EagleoutIce/satex-analyzer/commit/1fbd322)

### Site and docs

- `fix` **Before and after shots no longer cover each other.** The quick-fix card shows both windows at two thirds of its width.
- `fix` **Showcase cards show whole captures.** A card's picture is no longer cut at its bottom or right edge, and on a phone the pictures no longer move up and down while the page scrolls.
- `feat` **Typed site scripts.** The site's widgets, dialogs, draw pad and TeX runner are strict TypeScript, built by esbuild for the pages, the VS Code views and `cargo playground --dev`.
- `fix` **The benchmark page shows summaries.** Each release runs its benchmark, totals compare the documents both releases ran, and pdfTeX comes first. [`9322ee0`](https://github.com/EagleoutIce/satex-analyzer/commit/9322ee0) [`5e39b65`](https://github.com/EagleoutIce/satex-analyzer/commit/5e39b65) [`4c5e651`](https://github.com/EagleoutIce/satex-analyzer/commit/4c5e651)
- `fix` **The site deploys again**, with its playground tests running. [`d2c8c13`](https://github.com/EagleoutIce/satex-analyzer/commit/d2c8c13) [`6702be3`](https://github.com/EagleoutIce/satex-analyzer/commit/6702be3) [`7ab2503`](https://github.com/EagleoutIce/satex-analyzer/commit/7ab2503) [`37a5812`](https://github.com/EagleoutIce/satex-analyzer/commit/37a5812)

### Build and infrastructure

- `fix` **The site builds in CI again.** The Pages job installs esbuild, which turns the site's TypeScript into scripts.

## v0.2.0 (2026-11-07)

Sound and usable on real documents: joins, loops and caches that keep the analysis precise and ending, the playground and `satex gui`, editor widgets and managers, VS Code and IntelliJ plugins, latexmk builds.

### Playground

- `fix` **Maybe text reads clearly.** One case picker per undecided conditional, glue specs over the glue, numbers and parameters colored alike. [`1403031`](https://github.com/EagleoutIce/satex-analyzer/commit/1403031)
- `fix` **A waiting widget says so** until it opens. [`55f7986`](https://github.com/EagleoutIce/satex-analyzer/commit/55f7986)
- `fix` **Showcase shots line up.** One height, centered on small screens, the live window at full width, the stepper holding still. [`45ba047`](https://github.com/EagleoutIce/satex-analyzer/commit/45ba047) [`1c31cb6`](https://github.com/EagleoutIce/satex-analyzer/commit/1c31cb6) [`b95ee96`](https://github.com/EagleoutIce/satex-analyzer/commit/b95ee96) [`d190837`](https://github.com/EagleoutIce/satex-analyzer/commit/d190837) [`6867c3d`](https://github.com/EagleoutIce/satex-analyzer/commit/6867c3d)
- `feat` **Landing page.** A title page on the first screen, the showcase on a 3D ring that turns with the page, the page layout widget in it. [`22939fd`](https://github.com/EagleoutIce/satex-analyzer/commit/22939fd) [`b711b33`](https://github.com/EagleoutIce/satex-analyzer/commit/b711b33) [`e553530`](https://github.com/EagleoutIce/satex-analyzer/commit/e553530) [`884dcba`](https://github.com/EagleoutIce/satex-analyzer/commit/884dcba) [`0de0584`](https://github.com/EagleoutIce/satex-analyzer/commit/0de0584) [`e6a0098`](https://github.com/EagleoutIce/satex-analyzer/commit/e6a0098) [`9128f0d`](https://github.com/EagleoutIce/satex-analyzer/commit/9128f0d)
- `fix` **`satex gui` keeps serving when its terminal closes** or its output is piped to `head`. [`bf8c194`](https://github.com/EagleoutIce/satex-analyzer/commit/bf8c194)
- `fix` **`satex gui` analyzes the open file first** and nothing unasked, and no longer freezes after a big compile. [`ff00464`](https://github.com/EagleoutIce/satex-analyzer/commit/ff00464)
- `fix` **Never stuck.** Dead workers, busy caches and bad stored state no longer stall the start. Stuck runs recover. [`5a702f4`](https://github.com/EagleoutIce/satex-analyzer/commit/5a702f4) [`82d7c20`](https://github.com/EagleoutIce/satex-analyzer/commit/82d7c20) [`c0ff841`](https://github.com/EagleoutIce/satex-analyzer/commit/c0ff841) [`5e722f1`](https://github.com/EagleoutIce/satex-analyzer/commit/5e722f1)
- `fix` **Everything reruns after a package download.** Widgets wait for the analysis. [`6ee8c88`](https://github.com/EagleoutIce/satex-analyzer/commit/6ee8c88) [`d1aa312`](https://github.com/EagleoutIce/satex-analyzer/commit/d1aa312)
- `fix` **The trace stepper keeps its place.** Phone toolbar, claims and table buttons fit. [`0a45669`](https://github.com/EagleoutIce/satex-analyzer/commit/0a45669) [`11edabd`](https://github.com/EagleoutIce/satex-analyzer/commit/11edabd) [`f832d13`](https://github.com/EagleoutIce/satex-analyzer/commit/f832d13) [`88f8f79`](https://github.com/EagleoutIce/satex-analyzer/commit/88f8f79)
- `feat` **`satex gui`** serves the playground on your own files, no upload. See [`satex gui`](doc/wiki/cli.md#the-playground-on-your-machine). [`73f101b`](https://github.com/EagleoutIce/satex-analyzer/commit/73f101b)
- `feat` **Landing page.** Try it first, a live window before the engine loads, a carousel of the widgets. [`d2d6c19`](https://github.com/EagleoutIce/satex-analyzer/commit/d2d6c19) [`d820715`](https://github.com/EagleoutIce/satex-analyzer/commit/d820715) [`ad58d41`](https://github.com/EagleoutIce/satex-analyzer/commit/ad58d41) [`c149ca2`](https://github.com/EagleoutIce/satex-analyzer/commit/c149ca2)
- `feat` **Trace stepper.** Step through what a macro expands to, with input, calls, forks and domains. [`50058c3`](https://github.com/EagleoutIce/satex-analyzer/commit/50058c3) [`cdc1835`](https://github.com/EagleoutIce/satex-analyzer/commit/cdc1835) [`30955ff`](https://github.com/EagleoutIce/satex-analyzer/commit/30955ff)
- `feat` **Draw page.** Find a symbol by sketching it. [`cbbaf15`](https://github.com/EagleoutIce/satex-analyzer/commit/cbbaf15) [`85fb7ec`](https://github.com/EagleoutIce/satex-analyzer/commit/85fb7ec) [`618019b`](https://github.com/EagleoutIce/satex-analyzer/commit/618019b)
- `feat` **Library and first download** with the most used packages, glossaries and the index. [`dbfec82`](https://github.com/EagleoutIce/satex-analyzer/commit/dbfec82) [`ae5e2f0`](https://github.com/EagleoutIce/satex-analyzer/commit/ae5e2f0) [`67750d8`](https://github.com/EagleoutIce/satex-analyzer/commit/67750d8)
- `feat` **Settings, outline, cache dialog, timing popup** and a typeset view. [`ef94a13`](https://github.com/EagleoutIce/satex-analyzer/commit/ef94a13) [`e65d145`](https://github.com/EagleoutIce/satex-analyzer/commit/e65d145) [`2ae687e`](https://github.com/EagleoutIce/satex-analyzer/commit/2ae687e)
- `perf` **First download 6 MiB instead of 120.** [`6528842`](https://github.com/EagleoutIce/satex-analyzer/commit/6528842) [`d38a268`](https://github.com/EagleoutIce/satex-analyzer/commit/d38a268)
- `perf` **Runs use every core** the memory allows. Kernel caches survive builds. [`f587917`](https://github.com/EagleoutIce/satex-analyzer/commit/f587917) [`29042c5`](https://github.com/EagleoutIce/satex-analyzer/commit/29042c5)

### Editor plugins and widgets

- `fix` **Hovers name the real owner and fall back** through its manual, an alias's target, any loaded manual, the kernel and the source comment, and say what was tried. [`eeeb418`](https://github.com/EagleoutIce/satex-analyzer/commit/eeeb418)
- `fix` **TikZ arrows draw only their tips**, so a dashed or short arrow keeps its look. [`3fad770`](https://github.com/EagleoutIce/satex-analyzer/commit/3fad770)
- `fix` **TikZ arrows touch their nodes** in the drawing widget. [`10431bc`](https://github.com/EagleoutIce/satex-analyzer/commit/10431bc)
- `fix` **Paper sizes known without geometry**, so letter is no longer custom. [`06e4a47`](https://github.com/EagleoutIce/satex-analyzer/commit/06e4a47)
- `fix` **Requests never wait for a run**, and exit stops a running analysis. [`fc5dda3`](https://github.com/EagleoutIce/satex-analyzer/commit/fc5dda3) [`30b2124`](https://github.com/EagleoutIce/satex-analyzer/commit/30b2124) [`40ae0c9`](https://github.com/EagleoutIce/satex-analyzer/commit/40ae0c9)
- `fix` **Dialogs never drop an edit**, ask before discarding and undo in one step. [`91b5239`](https://github.com/EagleoutIce/satex-analyzer/commit/91b5239) [`c6a80ef`](https://github.com/EagleoutIce/satex-analyzer/commit/c6a80ef)
- `fix` **Colors complete only in color arguments**, with swatches on the right line. [`5e6ecba`](https://github.com/EagleoutIce/satex-analyzer/commit/5e6ecba) [`59e9afa`](https://github.com/EagleoutIce/satex-analyzer/commit/59e9afa) [`d05aa00`](https://github.com/EagleoutIce/satex-analyzer/commit/d05aa00)
- `fix` **Lenses and widget marks** refresh after the run and survive stale positions. [`7518997`](https://github.com/EagleoutIce/satex-analyzer/commit/7518997) [`4431378`](https://github.com/EagleoutIce/satex-analyzer/commit/4431378) [`c48fea9`](https://github.com/EagleoutIce/satex-analyzer/commit/c48fea9) [`ff01740`](https://github.com/EagleoutIce/satex-analyzer/commit/ff01740)
- `fix` **Hovers lead with the signature.** [`4c2984d`](https://github.com/EagleoutIce/satex-analyzer/commit/4c2984d) [`9aa8577`](https://github.com/EagleoutIce/satex-analyzer/commit/9aa8577)
- `feat` **Drawing canvas: every node shape** pgf knows, libraries not loaded too, each drawn from a TeX sample, and arrows take clicks. [`e1b165a`](https://github.com/EagleoutIce/satex-analyzer/commit/e1b165a)
- `feat` **Bibliography as a reference manager.** A list with search, type badges and filters beside a checked editor, `@string` constants, import from DOI, arXiv, RIS, CSL-JSON and Zotero, a dblp check, duplicate and missing-field lints. [`eb65d13`](https://github.com/EagleoutIce/satex-analyzer/commit/eb65d13)
- `feat` **Definition history.** Every redefinition and augment of a command, newest first, with `\let` copies followed and definitions on other paths. Bodies on hover are pretty-printed, a toggle shows them raw. [`b29128f`](https://github.com/EagleoutIce/satex-analyzer/commit/b29128f)
- `feat` **Drag the margins** in the page layout graphic. Under KOMA-Script the drag sets `DIV`. [`3fad770`](https://github.com/EagleoutIce/satex-analyzer/commit/3fad770)
- `feat` **Table, image, placement, tree, color and TikZ widgets write only what changed.** Packages they need come from the installation, not a list. [`d3c4880`](https://github.com/EagleoutIce/satex-analyzer/commit/d3c4880) [`c5be515`](https://github.com/EagleoutIce/satex-analyzer/commit/c5be515) [`decf572`](https://github.com/EagleoutIce/satex-analyzer/commit/decf572)
- `feat` **Page layout per class.** KOMA-Script keeps `DIV` and `BCOR`, memoir and the standard classes get geometry suggested. Edits keep every untouched byte. See [type plugins](doc/wiki/type-plugins.md). [`cf4243a`](https://github.com/EagleoutIce/satex-analyzer/commit/cf4243a)
- `feat` **Index and bibliography managers** open from `\printindex`, `\cite` and their kin. Every length field takes any unit, like `2cm` or `0.5\linewidth`, the TikZ grid size too. See [type plugins](doc/wiki/type-plugins.md). [`06e4a47`](https://github.com/EagleoutIce/satex-analyzer/commit/06e4a47)
- `feat` **Managers add, edit and search** glossaries, index terms, bibliographies (also `thebibliography`, with a convert to `.bib`) and theorems. See [type plugins](doc/wiki/type-plugins.md). [`e22a899`](https://github.com/EagleoutIce/satex-analyzer/commit/e22a899)
- `feat` **Keys layer.** Labels, citations, glossary and index keys in Typeset and PDF. [`ff00464`](https://github.com/EagleoutIce/satex-analyzer/commit/ff00464)
- `feat` **Type plugins.** Each editor feature is a plugin that a type of the analysis triggers. See [type plugins](doc/wiki/type-plugins.md). [`75d96fd`](https://github.com/EagleoutIce/satex-analyzer/commit/75d96fd)
- `feat` **TikZ widget** with a canvas, properties, styles, layers and relative positions. [`2a79cfa`](https://github.com/EagleoutIce/satex-analyzer/commit/2a79cfa) [`5e87296`](https://github.com/EagleoutIce/satex-analyzer/commit/5e87296) [`990c691`](https://github.com/EagleoutIce/satex-analyzer/commit/990c691) [`ff00464`](https://github.com/EagleoutIce/satex-analyzer/commit/ff00464)
- `feat` **Table, image, page layout and color dialogs.** [`3c2a595`](https://github.com/EagleoutIce/satex-analyzer/commit/3c2a595) [`d07e6b7`](https://github.com/EagleoutIce/satex-analyzer/commit/d07e6b7) [`bcadebc`](https://github.com/EagleoutIce/satex-analyzer/commit/bcadebc) [`b74130c`](https://github.com/EagleoutIce/satex-analyzer/commit/b74130c)
- `feat` **Typeset and PDF layers.** Glue, a PDF grid in TeX units, booktabs rules. [`d16d209`](https://github.com/EagleoutIce/satex-analyzer/commit/d16d209) [`71cbd9f`](https://github.com/EagleoutIce/satex-analyzer/commit/71cbd9f)
- `feat` **Forest trees, beamer overlays, biblatex style hovers.** [`0c4d45a`](https://github.com/EagleoutIce/satex-analyzer/commit/0c4d45a) [`c923274`](https://github.com/EagleoutIce/satex-analyzer/commit/c923274) [`6eecfa0`](https://github.com/EagleoutIce/satex-analyzer/commit/6eecfa0)
- `feat` **Widgets from the gutter.** Lenses are optional. [`4e60fbf`](https://github.com/EagleoutIce/satex-analyzer/commit/4e60fbf) [`7319b01`](https://github.com/EagleoutIce/satex-analyzer/commit/7319b01)
- `feat` **Typed jumps** to options, colors and column types. Package docs in completion. [`6ee8c88`](https://github.com/EagleoutIce/satex-analyzer/commit/6ee8c88) [`4fbd4cd`](https://github.com/EagleoutIce/satex-analyzer/commit/4fbd4cd)
- `feat` **Completions** show TikZ drawings and suggest the nearest name. [`760b810`](https://github.com/EagleoutIce/satex-analyzer/commit/760b810) [`a5f832c`](https://github.com/EagleoutIce/satex-analyzer/commit/a5f832c)

### VS Code and other editors

- `feat` **VS Code** gets every widget, the library and settings. See [VS Code](doc/wiki/vscode.md). [`40174d7`](https://github.com/EagleoutIce/satex-analyzer/commit/40174d7) [`20c6d00`](https://github.com/EagleoutIce/satex-analyzer/commit/20c6d00) [`da25261`](https://github.com/EagleoutIce/satex-analyzer/commit/da25261)
- `feat` **`cargo vscode`** builds and installs the extension. [`640f1e5`](https://github.com/EagleoutIce/satex-analyzer/commit/640f1e5)
- `feat` **IntelliJ plugin** over LSP4IJ. See [IntelliJ](doc/wiki/intellij.md). [`05dfeee`](https://github.com/EagleoutIce/satex-analyzer/commit/05dfeee)

### Lints

- `fix` **A missing image names its folder.** It suggests `\graphicspath` and is no longer also reported as a missing package. [`5cca876`](https://github.com/EagleoutIce/satex-analyzer/commit/5cca876)
- `fix` **Fewer false positives.** Own text only, stated definitions, aliases, zero-width spaces, code in parentheses. [`be400e6`](https://github.com/EagleoutIce/satex-analyzer/commit/be400e6) [`cd0354b`](https://github.com/EagleoutIce/satex-analyzer/commit/cd0354b) [`0909180`](https://github.com/EagleoutIce/satex-analyzer/commit/0909180) [`bedf0ef`](https://github.com/EagleoutIce/satex-analyzer/commit/bedf0ef)
- `fix` **Possible findings are notes**, never certain errors. [`cde9439`](https://github.com/EagleoutIce/satex-analyzer/commit/cde9439) [`86359a0`](https://github.com/EagleoutIce/satex-analyzer/commit/86359a0) [`90f5393`](https://github.com/EagleoutIce/satex-analyzer/commit/90f5393)
- `fix` **Typed references become `\cref`**, loading cleveref when needed. [`442ee8e`](https://github.com/EagleoutIce/satex-analyzer/commit/442ee8e) [`1a1da0b`](https://github.com/EagleoutIce/satex-analyzer/commit/1a1da0b)
- `fix` **One finding per `\cite` call.** Duplicate labels only where LaTeX warns. [`aead4e8`](https://github.com/EagleoutIce/satex-analyzer/commit/aead4e8) [`3d5d4b7`](https://github.com/EagleoutIce/satex-analyzer/commit/3d5d4b7) [`e548a61`](https://github.com/EagleoutIce/satex-analyzer/commit/e548a61)
- `fix` **Undefined colors only where xcolor raises.** [`8bf8a22`](https://github.com/EagleoutIce/satex-analyzer/commit/8bf8a22)
- `feat` **Clone lint** for near-duplicate blocks. [`b28c7c1`](https://github.com/EagleoutIce/satex-analyzer/commit/b28c7c1)
- `feat` **`et al.`, biblatex prose** and a bibtex-to-biblatex migration. [`f60be55`](https://github.com/EagleoutIce/satex-analyzer/commit/f60be55) [`45ceb45`](https://github.com/EagleoutIce/satex-analyzer/commit/45ceb45)
- `feat` **Table typography**, written references, `undefined-cref-format`. [`572442f`](https://github.com/EagleoutIce/satex-analyzer/commit/572442f) [`abde87a`](https://github.com/EagleoutIce/satex-analyzer/commit/abde87a) [`3617ffa`](https://github.com/EagleoutIce/satex-analyzer/commit/3617ffa)
- `feat` **Stable rule numbers**, project styles, findings grouped per rule. See [all rules](doc/wiki/lints.md). [`261a0e7`](https://github.com/EagleoutIce/satex-analyzer/commit/261a0e7) [`f3ac51b`](https://github.com/EagleoutIce/satex-analyzer/commit/f3ac51b) [`960d8c0`](https://github.com/EagleoutIce/satex-analyzer/commit/960d8c0)

### Analysis

- `fix` **No crash on a box of unbounded size.** A length past what TeX can hold is any length. [`199ef55`](https://github.com/EagleoutIce/satex-analyzer/commit/199ef55)
- `fix` **TeX behaviours checked against pdfTeX.** `\meaning` of a `\noexpand`ed macro, `\toks0=\bgroup`, the always global `\deadcycles`, `\ifx` of different parameter characters, `^^` after a control word and `\scantokens` now give what pdfTeX gives. [`15e2c6b`](https://github.com/EagleoutIce/satex-analyzer/commit/15e2c6b)
- `fix` **A catcode set for an unknown character leaves braces alone.** `{`, `}` and `\` keep their category, a named and configurable assumption. [`9327f51`](https://github.com/EagleoutIce/satex-analyzer/commit/9327f51)
- `fix` **Names built from unknown text stay few.** A family written once per record widens to one typed text instead of one path per record. See [domains](doc/wiki/domains.md). [`5703f45`](https://github.com/EagleoutIce/satex-analyzer/commit/5703f45)
- `fix` **Class options no longer stale every package cache.** Only the class replays them. [`7c6fe9c`](https://github.com/EagleoutIce/satex-analyzer/commit/7c6fe9c)
- `fix` **No false errors in KOMA notecolumn.** Read flags stay with their stream, and reading a register no longer makes every register unknown. [`a3be187`](https://github.com/EagleoutIce/satex-analyzer/commit/a3be187) [`7eda937`](https://github.com/EagleoutIce/satex-analyzer/commit/7eda937)
- `fix` **Here-float captions** stay in the typeset text. See [deferred material](doc/wiki/domains.md#files-written-in-the-run). [`c510435`](https://github.com/EagleoutIce/satex-analyzer/commit/c510435)
- `fix` **Numbers stay typed.** Digits narrow their register, digit places never join, scans split by digit count. See [typed unknown text](doc/wiki/domains.md#unknown-text-the-textclass-lattice). [`e6255b9`](https://github.com/EagleoutIce/satex-analyzer/commit/e6255b9) [`52f1c7a`](https://github.com/EagleoutIce/satex-analyzer/commit/52f1c7a) [`5352e4f`](https://github.com/EagleoutIce/satex-analyzer/commit/5352e4f) [`a796fe0`](https://github.com/EagleoutIce/satex-analyzer/commit/a796fe0)
- `fix` **Joins keep structure.** Paired arms, maybe-empty text, equal length bounds and name lists survive. See [the join](doc/wiki/domains.md#the-join-in-general). [`f827f03`](https://github.com/EagleoutIce/satex-analyzer/commit/f827f03) [`f47ac1b`](https://github.com/EagleoutIce/satex-analyzer/commit/f47ac1b) [`3495a7d`](https://github.com/EagleoutIce/satex-analyzer/commit/3495a7d) [`0e47ccb`](https://github.com/EagleoutIce/satex-analyzer/commit/0e47ccb)
- `fix` **Loops end.** A round that rewrites its state is a cycle, and summaries widen against their entry. See [call summaries](doc/wiki/domains.md#call-summaries-what-a-call-did-twice). [`d4b6fa3`](https://github.com/EagleoutIce/satex-analyzer/commit/d4b6fa3) [`af60255`](https://github.com/EagleoutIce/satex-analyzer/commit/af60255) [`1adff24`](https://github.com/EagleoutIce/satex-analyzer/commit/1adff24) [`6b50406`](https://github.com/EagleoutIce/satex-analyzer/commit/6b50406)
- `fix` **Approximations raise no certain errors.** What only an approximated path reaches is a possible error. See [soundness invariant](doc/wiki/domains.md#soundness-invariant). [`8ff098e`](https://github.com/EagleoutIce/satex-analyzer/commit/8ff098e) [`0d387ac`](https://github.com/EagleoutIce/satex-analyzer/commit/0d387ac) [`5712ab7`](https://github.com/EagleoutIce/satex-analyzer/commit/5712ab7)
- `fix` **Groups and output routines** restore what they saved and end as a unit. [`59d1ae1`](https://github.com/EagleoutIce/satex-analyzer/commit/59d1ae1) [`cf04851`](https://github.com/EagleoutIce/satex-analyzer/commit/cf04851) [`e5d53f0`](https://github.com/EagleoutIce/satex-analyzer/commit/e5d53f0) [`04b0533`](https://github.com/EagleoutIce/satex-analyzer/commit/04b0533)
- `fix` **Typesetting closer to TeX.** Alignments, `\parshape`, math styles, `\unskip` on vertical lists, ligature hyphenation. [`e38b36f`](https://github.com/EagleoutIce/satex-analyzer/commit/e38b36f) [`484f4d6`](https://github.com/EagleoutIce/satex-analyzer/commit/484f4d6) [`884f40f`](https://github.com/EagleoutIce/satex-analyzer/commit/884f40f) [`3b8ae10`](https://github.com/EagleoutIce/satex-analyzer/commit/3b8ae10) [`4331fe3`](https://github.com/EagleoutIce/satex-analyzer/commit/4331fe3) [`27d3cf4`](https://github.com/EagleoutIce/satex-analyzer/commit/27d3cf4)
- `fix` **Page breaks where pdflatex breaks.** Deferred writes expand at shipout, `\vadjust` breaks reach the page. See [deferred material](doc/wiki/domains.md#files-written-in-the-run). [`b3abcbc`](https://github.com/EagleoutIce/satex-analyzer/commit/b3abcbc) [`7e5acb5`](https://github.com/EagleoutIce/satex-analyzer/commit/7e5acb5) [`0d4c944`](https://github.com/EagleoutIce/satex-analyzer/commit/0d4c944) [`f49169a`](https://github.com/EagleoutIce/satex-analyzer/commit/f49169a)
- `fix` **Exact scans over unknown text.** One-of texts split at delimiters, unknown characters have unknown codes. See [one-of values](doc/wiki/domains.md#the-set-lift-oneof). [`a6a7830`](https://github.com/EagleoutIce/satex-analyzer/commit/a6a7830) [`d9e4316`](https://github.com/EagleoutIce/satex-analyzer/commit/d9e4316) [`490a0f5`](https://github.com/EagleoutIce/satex-analyzer/commit/490a0f5) [`5b62065`](https://github.com/EagleoutIce/satex-analyzer/commit/5b62065)
- `fix` **A missing package no longer ends the run.** Messages print as TeX prints them. [`bc78fff`](https://github.com/EagleoutIce/satex-analyzer/commit/bc78fff) [`18d31c8`](https://github.com/EagleoutIce/satex-analyzer/commit/18d31c8)
- `fix` **Magic comments pick plain TeX**, and the installed engine decides primitive facts. [`ed98f45`](https://github.com/EagleoutIce/satex-analyzer/commit/ed98f45) [`53c3d56`](https://github.com/EagleoutIce/satex-analyzer/commit/53c3d56)
- `fix` **Every written file reaches the next run.** Only a file not found reruns the whole build. [`94760be`](https://github.com/EagleoutIce/satex-analyzer/commit/94760be) [`9875e23`](https://github.com/EagleoutIce/satex-analyzer/commit/9875e23) [`06d6afe`](https://github.com/EagleoutIce/satex-analyzer/commit/06d6afe) [`49d686e`](https://github.com/EagleoutIce/satex-analyzer/commit/49d686e)
- `fix` **Cache replays are complete**: facts, diagnostics and builder state. [`54bfbc2`](https://github.com/EagleoutIce/satex-analyzer/commit/54bfbc2) [`7ea0631`](https://github.com/EagleoutIce/satex-analyzer/commit/7ea0631) [`fbc7634`](https://github.com/EagleoutIce/satex-analyzer/commit/fbc7634)
- `fix` **Cache keys** follow what a step reads, the same in every profile and on Windows. See [caches](doc/wiki/configuration.md#format-and-package-caches). [`b65d1b1`](https://github.com/EagleoutIce/satex-analyzer/commit/b65d1b1) [`c85d25f`](https://github.com/EagleoutIce/satex-analyzer/commit/c85d25f) [`a686f08`](https://github.com/EagleoutIce/satex-analyzer/commit/a686f08) [`4973535`](https://github.com/EagleoutIce/satex-analyzer/commit/4973535)
- `fix` **Caches work without file locks**, as on network drives. [`a8c7530`](https://github.com/EagleoutIce/satex-analyzer/commit/a8c7530) [`a0d9fc8`](https://github.com/EagleoutIce/satex-analyzer/commit/a0d9fc8) [`e1220cb`](https://github.com/EagleoutIce/satex-analyzer/commit/e1220cb)
- `fix` **Clean stops.** Memory and time limits end a build with its last complete run. Time is only a backstop. See [limits](doc/wiki/configuration.md#configurable-limits-for-the-analysis). [`5832594`](https://github.com/EagleoutIce/satex-analyzer/commit/5832594) [`e8362b4`](https://github.com/EagleoutIce/satex-analyzer/commit/e8362b4) [`71a431b`](https://github.com/EagleoutIce/satex-analyzer/commit/71a431b) [`26fa2db`](https://github.com/EagleoutIce/satex-analyzer/commit/26fa2db)
- `fix` **`satex compile` honours the latexmkrc engine.** [`ce581f1`](https://github.com/EagleoutIce/satex-analyzer/commit/ce581f1)
- `fix` **Sandboxed `build.lua`.** Names from unknown text never reach kpsewhich. [`29ae6e3`](https://github.com/EagleoutIce/satex-analyzer/commit/29ae6e3) [`4354840`](https://github.com/EagleoutIce/satex-analyzer/commit/4354840)
- `feat` **Color domain.** See [colors](doc/wiki/domains.md#colors-color-and-colors). [`aed0ae1`](https://github.com/EagleoutIce/satex-analyzer/commit/aed0ae1)
- `feat` **Memory bound** per run and process. See [limits](doc/wiki/configuration.md#configurable-limits-for-the-analysis). [`6fde68e`](https://github.com/EagleoutIce/satex-analyzer/commit/6fde68e)
- `feat` **Builds follow latexmk**, with output directories and resumed reruns. [`816fdad`](https://github.com/EagleoutIce/satex-analyzer/commit/816fdad) [`a101964`](https://github.com/EagleoutIce/satex-analyzer/commit/a101964)
- `feat` **Why-apart report** says why two paths do not join. [`303f218`](https://github.com/EagleoutIce/satex-analyzer/commit/303f218)
- `perf` **Caches serve unchanged documents whole** and diff package caches by segment. [`d2dbb95`](https://github.com/EagleoutIce/satex-analyzer/commit/d2dbb95) [`8e378c3`](https://github.com/EagleoutIce/satex-analyzer/commit/8e378c3) [`46842cf`](https://github.com/EagleoutIce/satex-analyzer/commit/46842cf)

### CLI

- `feat` **`satex compile`** builds with latexmk and explains the log. [`28e9bc1`](https://github.com/EagleoutIce/satex-analyzer/commit/28e9bc1)
- `feat` **Progress, hotspots and a cache report.** [`ddee2ce`](https://github.com/EagleoutIce/satex-analyzer/commit/ddee2ce)
- `feat` **Shell completions** for every installed shell. [`f0699ca`](https://github.com/EagleoutIce/satex-analyzer/commit/f0699ca)

### Site and docs

- `fix` **Site scripts type-check again**, so the pages deploy.
- `fix` **Every in-page link reaches its section.** [`8028fda`](https://github.com/EagleoutIce/satex-analyzer/commit/8028fda)
- `fix` **Semantics pages match tex.web and the code.** Theorems state their limits, the main theorem has no free hypotheses. [`2e43003`](https://github.com/EagleoutIce/satex-analyzer/commit/2e43003) [`566fb52`](https://github.com/EagleoutIce/satex-analyzer/commit/566fb52) [`74777e5`](https://github.com/EagleoutIce/satex-analyzer/commit/74777e5) [`6d534b2`](https://github.com/EagleoutIce/satex-analyzer/commit/6d534b2) [`23e02be`](https://github.com/EagleoutIce/satex-analyzer/commit/23e02be) [`2be133d`](https://github.com/EagleoutIce/satex-analyzer/commit/2be133d) [`8757fc4`](https://github.com/EagleoutIce/satex-analyzer/commit/8757fc4)
- `feat` **Command output on the docs pages** is highlighted, and its cards link to the docs. [`b0d090f`](https://github.com/EagleoutIce/satex-analyzer/commit/b0d090f)
- `feat` **Landing page.** The first cards show on the first screen, plugin groups show at a glance, lint rules get their own shot, quick fixes and settings join the ring, the build runs as stacked `.aux` cards, before and after overlap. [`2f8a357`](https://github.com/EagleoutIce/satex-analyzer/commit/2f8a357) [`fd50308`](https://github.com/EagleoutIce/satex-analyzer/commit/fd50308) [`3e8a887`](https://github.com/EagleoutIce/satex-analyzer/commit/3e8a887) [`641737d`](https://github.com/EagleoutIce/satex-analyzer/commit/641737d)
- `docs` **Wiki** on the type lattice, precision limits and why TeX is hard, regenerated from the code. See [why TeX is hard](doc/wiki/challenges.md). [`5083317`](https://github.com/EagleoutIce/satex-analyzer/commit/5083317) [`3a04299`](https://github.com/EagleoutIce/satex-analyzer/commit/3a04299) [`85e15a6`](https://github.com/EagleoutIce/satex-analyzer/commit/85e15a6)
- `feat` **Hover cards** for control sequences. [`3b86d4b`](https://github.com/EagleoutIce/satex-analyzer/commit/3b86d4b)
- `docs` **This changelog**, with a page on the site. [`4e72dd7`](https://github.com/EagleoutIce/satex-analyzer/commit/4e72dd7)

### Build and infrastructure

- `build` **Sharded CI** builds once and its shards reuse prebuilt tests and cached kernels. [`03858a7`](https://github.com/EagleoutIce/satex-analyzer/commit/03858a7) [`61992f1`](https://github.com/EagleoutIce/satex-analyzer/commit/61992f1) [`31d41cf`](https://github.com/EagleoutIce/satex-analyzer/commit/31d41cf) [`b8031fc`](https://github.com/EagleoutIce/satex-analyzer/commit/b8031fc)
- `build` **Incremental CI**, a portable wasm toolchain, Pages from main. [`ff16822`](https://github.com/EagleoutIce/satex-analyzer/commit/ff16822) [`7397e7c`](https://github.com/EagleoutIce/satex-analyzer/commit/7397e7c)
- `build` **Benchmarks over releases.** [`9a9de54`](https://github.com/EagleoutIce/satex-analyzer/commit/9a9de54)
- `build` **Developer binaries** behind a `tools` feature. [`8bfbc07`](https://github.com/EagleoutIce/satex-analyzer/commit/8bfbc07)

## v0.1.0 (2026-09-25)

First public version: the abstract interpreter for (La)TeX with lints, a language server, a CLI for Windows, macOS and Linux, and the wiki. [`5af9583`](https://github.com/EagleoutIce/satex-analyzer/commit/5af9583) [`5570f80`](https://github.com/EagleoutIce/satex-analyzer/commit/5570f80)
