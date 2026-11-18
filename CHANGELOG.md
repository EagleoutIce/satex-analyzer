# Changelog

Newest first, fixes first in each group. Every entry has a badge: `feat` a feature, `fix` a bug fix, `perf` speed, `docs` documentation.
Commit and pull request links point to the details.
Maintained with `scripts/changelog.py` (`draft`, `check`, `links`, `release`, see [doc/releasing.md](doc/releasing.md)).

## Unreleased

### Compile

- `feat` **`satex compile` picks the build system.** `% arara:` runs arara, a `Tectonic.toml` Tectonic, else latexmk or the engine. `--system` or `compile.system` names one (latexmk, arara, tectonic, make, l3build, engine). `--view` opens the PDF in `compile.viewer`.
- `perf` **`satex compile --system satex`: incremental build.** The preamble is dumped once as a format and reused, the engine, BibTeX, Biber and makeindex rerun only on changed input. A body edit of the thesis example in the browser takes 0.57 s → 0.19 s (−67%). The playground's Compile uses it on `satex gui`.

### Caches

- `feat` **Cache reports and background setup.** `satex lint`, `query` and `explain` say in one line how many package caches they reused and built (`-v` lists them). `satex cache show` lists a package's cache files, sizes and variants. On first use satex asks to analyze kernels and packages in the background within `limits.cache_size`. `satex cache` shows its state, `satex cache stop` stops it (`cache.background`, `SATEX_NO_SETUP`).
- `perf` **Smaller, faster package caches.** datatool-base 55 → 24 MB (−56%), scrbook 40 → 25 MB (−38%), warm glossary replay 20% faster. `satex cache show` breaks bytes down by section.

### Lints

- `feat` **`misspelled-magic-comment`.** `% !rex program = tex` or `% !TeX progam` is reported early, with a fix to `!TeX` or `program`.
- `feat` **`low-contrast-color`.** Text below WCAG's 4.5:1 contrast (3:1 for large text) against its background is reported. The fix adjusts your `\definecolor` (`lints.low-contrast-color`).
- `feat` **`indistinct-colors`.** Colors side by side that look alike under protanopia, deuteranopia or tritanopia are reported. The fix suggests an Okabe-Ito color (`lints.indistinct-colors`).
- `feat` **Package rules.** `package-load-order` finds a later package that redefines an earlier one's command, the fix moves its `\usepackage` up. `redundant-package-load` deletes a load the class or an earlier package made already. `unknown-package-name` suggests an installed package a letter or two away (`graphix` gives `graphicx`).
- `feat` **PDF tagging lints and accessibility manager.** With `\DocumentMetadata{tagging=on}`: `tagging-incompatible-package`, `graphic-without-alt`, `pdfua-without-title`, `tagging-without-language`, `metadata-after-class`, `tagging-unsupported-engine`, `tagged-math-without-mathml`. `satex query tagging` lists the setup. "Accessibility…" edits tagging, PDF standard, language, title, alternative texts and table headers (`type_plugins.tagging`).
- `feat` **Early findings.** Text-only rules report on a sketch before the run: `comment-marker`, `user-pattern`, `clones`, two latexmkrc rules, `duplicate-package`, `unresolved-file`, `missing-graphic`. `satex lint` prints them at once as provisional, the run confirms or retracts each (`lints.early`).
- `feat` **Overfull preflight.** `overfull-line`, `overfull-box` and `wider-than-line` report pdflatex's `Overfull \hbox` by the same amount. A too wide `\includegraphics` gets the fix `width=\linewidth`.
- `feat` **Rule metadata.** `satex lint --explain`, `--rules`, `satex query --list`, the editor and the wiki show `since`. Summaries are short, rule numbers never change once released.
- `feat` **Package augmentations.** Per-package lint knowledge is switchable in `satex.yaml`. siunitx comes first: `inconsistent-precision` reads its rounding of `S` columns.
- `feat` **Spacing and punctuation fixes.** `primitive-tex-command` rewrites `\vskip` to `\vspace` and `\hskip` to `\hspace`. `command-terminated-space` covers `\TeX file` in TikZ nodes and `\fbox`. `scripts/spacing-sweep.py` checks every spacing fix on real documents. `math-punctuation` handles `$use?$ edge`.
- `fix` **A `\ref` inside an argument counts.** `\ref` in `\footnote`, `\emph`, `\textbf` or `\caption` now marks its label as used: no false `unused-label` on a first run.
- `fix` **Prose rules skip math, code, keys and URLs.** 37 false positives in 20 rules fixed. A trigger, or the word it is judged by, in `$…$`, `\verb`, `\texttt`, `\url` or a listing is no prose: `?` before `$x$` starts no sentence. `scripts/lint-sweep.py` sets every prose rule's trigger in 23 contexts.
- `fix` **No findings from a run stopped before the body.** Such a run shows only why it stopped.

### Playground

- `feat` **Type mode in the PDF and Typeset views.** On by default: a click puts a caret in the source, a tint shows where text comes from. Text from a macro edits its definition, with Undo and "Type in the argument instead". Enter starts a paragraph, `\` opens completion, `$` starts math (one "math" button: inline at once, display or `align*` from its menu), `x_42` types `x_{42}`. Inline math is written as `\(…\)` (`playground.smart_scripts`, `playground.math_inline`: `\(`, `$` or `auto`). A drag selects words, Ctrl+C copies their source, Home, End, Up and Down follow typeset lines, also in two columns and beamer. A click on a macro's own text (the `\TeX` logo in a `\title`) opens completion at the click with that command first. Words of a stored argument (`\title` set by `\maketitle`) type into that argument.
- `feat` **Type mode reaches generated and special text.** A contents entry without a short title gets one ("Heading too" offers the heading the same text). List headings, caption labels and other names (`Contents`, `Figure`) rename through a preamble `\renewcommand`, in `\captions<language>` under babel. A click on dot leaders offers no dots, wider or closer dots, or no page numbers. Typing into an undecided choice (`[not]` of `\tleapy`) edits the definition that spells it, a click on `⟨?⟩` explains the unknown value. Listings, `verbatim`, `minted`, `\verb` and `\lstinline` take characters as they are. A printed `\ref` or `\cite` opens a fuzzy-searched list of labels and citations and writes the chosen key. A forest node's text in the PDF types into its brackets.
- `feat` **Tables in Type mode.** Tab and Shift+Tab move between cells, Enter adds a row, typeset cells get TeX's padding. Hovering shows column letters and widths. Dragging a column edge in the PDF or the table widget writes `p{…}`, keeping `c`/`r` alignment, snapped to 0.5 mm or `\linewidth` shares, as one undo step. The table widget's selection tools sit in their own row above the grid.
- `feat` **TikZ nodes: type, drag, snap, shape.** A node's text types again and the drag starts at its frame (or with Alt). A drag writes the new `at (x,y)`, an `xshift`/`yshift` for a placed node or `pos=` for a path label, on a 1 mm grid with a ghost showing the coordinates. Drags snap to other nodes with guide lines and equal gaps (Shift: off). A right click switches shape, frame, fill and text, or adds a label and arrow tips to a path. A `forest` node in the PDF gets the same menu: shape, frame, add a child or sibling, delete, edit its text. Typeset sketches fit each text in its node and type into it. Typeset sketches set all their texts at one size and tint filled shapes. Typeset sketches draw `forest` trees (and pictures other environments' code draws), set each node's text where the run put its box, and use the run's fill and stroke colours.
- `feat` **Optional words read inline.** An undecided macro that sets a word or nothing shows as "is [not] a leap year", with a hover that says why.
- `feat` **Layers menu.** The Typeset view has the PDF view's layers button and menu instead of its chips row, with the same double-click toggle-all. The glue layer shows stretched glue at both ends of a line, such as `\centering`'s skips. The menu wraps and the Typeset view starts with `maybe` off. Index terms show as small marks. Two page breaks at one line show as one. Its bar's buttons and menus share the PDF bar's height and font (22–23 px mixed → 25.6 px). A narrow PDF bar (the PDF beside the editor) shows Layers and Fold as icons, so the build's state reads "6 pages · the…" instead of "6…". Contents lines set their page number on the line, not 6 px above it. A section's contents line sets its page number at the right edge too. Another file's paragraphs take no extra rail, so a click that opens that file moves no text sideways (9 px before).
- `feat` **Cards, hovers and go-to.** Resting on a finding shows its message over the rule's full card (explanation, number, category, severity, fix kind, since). The type card shows the hovered type with parents and children, rule explanations link, since badges link to the changelog, hovers show a body as TeX prints it (`type_plugins.pretty.enabled`, off by default). A definition lookup shows each target's code with its line marked, on hover or arrow keys. Go to definition, Find references and the definition history share one Navigate group. A hover asked during a run says it waits for the run instead of showing nothing. Findings and widget marks share one gutter column, a finding's marker shown on its line.
- `feat` **Definition history as a timeline.** Each definition is a dot on one line, the one in force filled, undone ones dashed. A macro in a body opens its own history in place.
- `feat` **Settings: YAML as YAML.** The YAML tab and the export write block lists, nested maps, unquoted text and folded long text. Search and `@changed` work there (hits with their parent keys, Enter and Shift+Enter jump). Settings open folded with counts, plugins sit in categories, search finds values, cache rows fold to their files. The first group of a tab and groups with changes start open. Editor, delay, widget, Type, Typeset, PDF and tip settings are keys under `playground:`, carried by a shared link.
- `feat` **Status line and run stats.** The run pop-up shows one header line, six equal tiles and folded sections, file times marked as carried over from the cold run. The status line is one thin row with a 2px bar and fixed slots. It shows how incremental a run was: line N → end, its own steps against a full run's, and the time (`satex/runInfo`). Tips name a key, command or setting. Package fetching no longer holds it at "Fetching 9 packages", the offline copy is opt-in, a first compile names each phase with its size. On phones the time shrinks to fit the row. The status text keeps room for a few words with an ellipsis, the pills and the time give way, a narrow row keeps the pills' counts, never a cut word. It names every wait: a question the language server is on past 400 ms ("Listing the commands in force, then analyzing thesis.tex…"), a run the page did not start, each phase in words with its file and line ("Analyzing the document · chapter.tex:33") and the build run ("run 2") when the share starts again. The first run's line no longer says "Ready" while it works, and its end no longer repeats "3 packages fetched". The phone's bottom tabs share one size and spacing. The Findings count holds every file's findings once the first run ends (the thesis example showed 2 of 11 until the next edit). Stop shows once a run lasts 0.6 s and fits the row: typing no longer flashes it or moves the bar by 1 px. Short of room, the steps and time give way before the status text. The status text has a fixed slot and digits one width: the pills no longer shift as it changes (typing CLS 0.009 → 0.002). In the preamble the line shows the time spent, not a time left (a trace said 5 → 25 s left and took 6 s). The cache table is drawn only while open (about 100 hidden rows were rebuilt per run).
- `feat` **Examples reset on a new build.** Edits to the examples made on an older build are cleared once, with the note "Examples reset for the new build" and an Undo reset button. Settings and uploads stay.
- `feat` **Set the root.** A file's or tab's menu sets the root, a tab marks it, the example keeps it, and `% !TeX root = …` is honoured. A new file typed as `x.tex` is no longer named `x.tex.tex`. A LaTeX file uploaded onto a root without a class (a plain TeX example) becomes the root, with its engine. Uploaded images show in the file tree and can be deleted there.
- `feat` **Tree widget: form layout and feature models.** The tree dialog lays out as a form (labels in one column, a row of actions), and add child, add sibling, move, delete and outline edits write brackets as the tree spaces them. A feature-model preset marks nodes mandatory or optional and abstract, groups alternative or or, lists cross-tree constraints below, and writes its `\forestset` styles (forest and TikZ only). Example `feature-model.tex`. A form taller than the dialog scrolls under its footer, Apply stays in sight. Widget fields and segmented buttons are as tall as the buttons beside them (four heights in the TikZ toolbar → one). Picking the feature-model preset shows its look in the preview at once. Undo and redo sit in every widget's header, as in the table (the tree had them in its footer, the drawing in its toolbar). Managers show one primary button per entry and drop the kind column when every entry has the same kind (a cut `\newgloss…` chip in each row). The trace's JSON and CSV exports wait for the steps.
- `feat` **Circuit widget.** "Edit circuit…" edits a `circuitikz` environment on a grid (parts, wires, labels) and adds `\usepackage{circuitikz}`. "New circuit…" starts one on a blank line.
- `feat` **Page layout.** The Page tab sets page numbering per part, restyles chapter to subsubsection headings (KOMA-Script or titlesec, named style sets), page color, columns and binding. Clicking a page number in the PDF or a page break in the Typeset view opens it with the numbering in force there. A click in a page's header or footer types into the text the document writes there, else opens the header and footer at that field. The Typeset view draws each break's footer and next header the same way.
- `feat` **Completion finds by what a name is.** `\usepackage{…}`, `\documentclass{…}`, `\cite{…}` and `\ref{…}` show each item's catalogue line, citation author, year and title, or label target, searchable with typos forgiven (`\usepackage{network}` offers tikz-network). The language server does the same. Commands such as `\emph` come with braces.
- `feat` **Preview cache contents.** The previews.json row counts its arrow tips, patterns and shapes, and shows each kind as a grid.
- `feat-fix` **Typing in the PDF: keys land at once, in the right place and font.** A key reaches the editor in the same frame (was 11–15 s behind the check), a heading click types the long title, a caption or listing click lands in it, the echo uses the page's embedded font (width 0.57–0.66 → 0.98–1.0) and wraps at the line. The caret follows the source offset on each new build, sits on a glyph boundary measured from the page's pixels at any zoom, and the view stays put when a line moves to the next page. Over 300 keys across a page break, caret jumps went from 60 to 4 and the longest stall from 5 keys to 3. The PDF no longer dims or flickers, the wait bar and slice badge no longer push pages down, origin tints match between PDF and Typeset (per glyph, back after typing in the PDF), and prose typing opens no completion. A word selection dragged on the page goes with one Backspace or Delete, or a typed character replaces it, as one gated edit, painted over before the rebuild. The echo's size is fitted to the page's own word width (within 5%), a file included with `\input` echoes in the page's font and wraps at the line, and rows the echo adds push the text below down instead of covering it. A click on inline math before a display formula (`$n$`) places the caret, and so does a click in a display (`equation`, an `align` row). A contents entry keeps the caret on its own row, a click by a footnote mark lands in the footnote, typing in a footnote echoes there (not after the text's line), a centered row (a caption, a TikZ node) stays centered while typed into, and the echo wraps at the line's width, not at the paragraph's widest row, from the margin. Letters the page's embedded font subset lacks are measured at that font's width, so a landing build no longer moves the text (bold and heading jumps of 7 px gone); a contents line's fill takes up what is typed; a click in the space after a word types after it, before a command that follows (`Inside \texttt{…}`); a listing's caret keeps its column; typing at a new paragraph's start echoes on its row. The echo re-flows the paragraph as you type: overflow runs on through its rows, a deletion takes words up, Enter starts a paragraph with the page's own indent and gap, and an echo draw costs 1.5–2 ms (p95 2–5 ms, was 3–5 and 7–17 ms). The page typed on keeps its echo until typing rests for 400 ms and while the build lacks a key and a newer one is coming, then crossfades in 120 ms. The Type button reads "Typing" and the pages get a thin accent frame while a caret is in them; Esc ends it. Typed math shows as math before the build lands: `$x_1$`, `\(a+b\)`, `x^2`, `\frac{a}{b}`, `\alpha` with letters in math italic, scripts at 0.7 lowered or raised, a binary operator's spacing and fractions stacked; display math stands centered on a row of its own, the paragraph pushed down. The glyphs come from the page's own math fonts where it has them, else from Latin Modern at the page's x-height; the landed build puts the word after a formula within 1 px of the preview, and the preview costs 0.07 ms a key. Overlays (tints, chips, layers, caret, echo) show only over a page whose image is drawn: a page pdf.js fails to draw is tried three times and keeps no tint over a blank page. Type-mode e2e 2/23 → 46/52 pass. In the Typeset view, words typed after a space stay beside the clicked word ("Inside quickly" showed "quickly"), the caret no longer jumps back to the word's start on a space, and a click that opens an `\input` file keeps the clicked paragraph where it was on screen (it jumped 30 px). A Type-mode click between lines or in the margin says "No text here. Click a word to type". Ctrl+Backspace and Ctrl+Delete take a whole word (they took one letter). Completion after `\` opens however fast you type, and Tab or Enter pressed before the list shows waits for it (`\emp` Tab gave `\empQuokka`, now `\emph{Quokka}`): the first key typed into a field replaces its hint (`\emph{arg}`). A title page (`report`, `book`, KOMA, beamer) types into `\title` and `\author` instead of opening the page layout widget. A build from the cache no longer leaves its pages undrawn. A widget's write, a rename or a completion sends the text first, so the server never refuses it as "a place outside the text".
- `perf` **Edits, typing and completion keep up.** Edit → findings 0.40 → 0.18 s (−55%) on a small hyperref document, 0.66 → 0.45 s (−32%) on the thesis example. Steps per body edit 415k → 85k (−80%), a state copy 20 → 4 ms (−80%), debounce 300 → 100 ms. Thesis example edit to run and Typeset view 5 s → 0.2 s (−96%), an edit right after load waited 6–11 s behind the command list asked after every run (now asked at the first `\` and after a run only while it answers within 1 s): typing → findings 9.2 → 1.0 s on the thesis example, an edited row in the browser 4.5–10 s → 0.6 s (−87 to −94%). The command list a `\` completes from 5.0 s / 5.4 MB → 0.05 s / 1.8 MB (lean `scope`: name, tag, shape, package; usage and docs built only for the shown items). A file switch no longer leaves the status line on "Analyzing" for 10 s. Opening the trace 6.9 → 0.3 s (it runs from the open run's last checkpoint before the traced line), each traced call showing its own definition and arguments. A package bundle landing keeps the checkpoints whose lookups it leaves alone: the rerun after it 2.3 → 0.3 s on the thesis example instead of the whole preamble. A run with nothing to do no longer sends the page into a loop of re-analyses. Key to new page about 0.4 s on a short document and 0.5 to 0.6 s on 100 pages. Source editor key → paint p95 104 → 40 ms, long tasks while typing 21 → 0, dropped frames 202 → 1. The completion list follows each letter 90 → 20 ms and asks the server again only in a new place or a cut-short list. A return visit to the thesis example fetches the packages its last run lacked before the first run: first findings 10.0 → 7.3 s (−27%). The browser's TeX keeps the files no host has (`.vf` probes, `lstlang0.sty`), so a warm Compile at load asks no host: 4.0 → 1.2–1.5 s (−65%). A first visit fetches the bundles the site's build saw an example read, so its first run is its only one: thesis first findings 13.4 → 9.2–10.1 s (−28%). A first Compile fetches the TeX files the analysis saw side by side, while the engine downloads: 8.7 → 2.0–2.6 s (−73%), the preamble's format 6.0 → 0.4 s. The command list completion reads takes 11 → 5.7 s (−48%) on the thesis example.
- `perf` **`cargo playground` shows a change fast.** First pass after a Rust change 10:00 → 0:07 (−99%): file listings, the bundle, kernel caches, rule facts and symbols come from the last build until the second pass remakes them. Full result after a Rust change 24:44 → 5:43 (−77%), a CSS change 6:42 → 2:37 (−61%) on 4 cores. Satex builds once.
- `fix` **Type mode keeps the source's structure.** One check gates every keystroke: braces, command names, environments, math, comments and verbatim stay intact, Enter in a title is refused (Shift+Enter breaks the line), Backspace after a formula's `\)` takes its last character (a formula of one character whole) and Delete before `\(` its first, contents entries edit their short title, a failed build shows its first TeX error with Undo. Enter at a source line's end, Backspace after a closing `$` and Delete before an opening one work, a command's `[…]` option stays with its command and its argument, and a forest node dragged beside the root stays. A fuzz test checks 48,506 edits. The old code broke 3,669 of its 9,600 key runs, in 8 classes. The playground's unit tests (117, under 4 s) live in themed `doc/site/tests/*.test.mjs` with seeded property tests of the gate, the forest, table and page models.
- `fix` **Macro text in TikZ nodes is tinted.** Letters a macro sets from code outside your files (`\TeX` in a node) are tinted as macro text. In Type mode a click on them opens the completion at the macro's call.
- `fix` **Tables in the views.** The PDF's column chips show each `l`/`c`/`r` column's width from its cells, one chip per column above it, alignment choices behind its ⋯, applied like typing on press (the old page and chips stay undimmed, one swap when the build lands), and a click on a width handle no longer writes a `p{…}`. A cell's `|` is drawn on the cell's edge so rules line up. No blank row 0 shows after `\end{document}`. The + and − buttons are round and never clipped. `\ref` and `\cite` an earlier .aux decides read `reference x` or `citation key`. A table whose package calls `\@array` by another name gets "Edit table…".
- `fix` **Keys layer follows the shown project.** A failed or outdated query for labels, references and citations is asked again, and another project's keys never stay on the pages.
- `fix` **Compile beside the analysis.** The PDF pane says when an analysis runs alongside a build. The browser's TeX builds again. A build's success pulse plays only on a shown panel.
- `fix` **Numbers and expl3 names highlighted whole.** `0.05pt`, `-3.5pt` and `2fill` are one number token, `\dim_eval:n` and `\l_tmpa_dim` one command.
- `fix` **Final page hints win.** Preview rows never replace the final rows of the same text, so page hints match the finished run.
- `fix` **Trace panes.** The input pane grows with its tokens instead of scrolling, and thin scroll bars keep the corners round.
- `fix` **Explain and rename at `\begin`.** They take that command.
- `fix` **Crash recovery.** A crashed, hung or out-of-memory analyzer restarts by itself with the open files, root, settings and typed text, its last findings dimmed until fresh ones arrive, and one line says how long the restart took (1.2–4 s). Three crashes in a minute stop it with Restart and Report (copies the crash log). A crashed browser pdfTeX restarts and builds once more. A failed start keeps drafts, settings and layout and clears caches only when one is the cause.

### CLI

- `fix` **The verdict names where the run gave up.** `satex lint` says "incomplete" with the first places instead of "complete".
- `fix` **One rule for the input.** An empty stdin (`</dev/null`) reads the folder's root documents, `-f -` reads stdin, an open stdin says so on stderr. `satex cache` never reads stdin.
- `fix` **Flags and config.** `--format` is checked everywhere. `satex gui` passes `--set` and `--config` on. A broken `satex.yaml` is named once by full path.
- `fix` **Pages only some paths ship read `maybe`.** `query typeset` marks such rows `maybe` and counts later pages from one less.

### Site and docs

- `feat` **Landing page.** A typing animation shows the Type mode and becomes the live editor on click. In "Try it here" a click on a word of the typeset text puts a caret there and keys write into the source, with the playground's Type rules. Showcase cards turn in a carousel on a ring, a card every 8 s, each frame hugging its capture, phones keep the stacked grid. Hovering the version shows the build time to the minute. The typing demo is taken again with today's toolbar and a drawn mouse pointer: a sentence typed into the page, Enter for a new paragraph, then a TikZ node retyped, dragged along its alignment guide and switched to a circle from its right-click menu. Short compiles show no wait bar over the page. The Settings card shows one crisp dialog that fills it.
- `feat` **A fuller thesis example.** Title page, table of contents, lists of figures and tables, short titles and captions, equations with `\eqref` and an `align`.
- `perf` **More showcase recordings at once.** Up to 6 browsers record in parallel (was 3), one per two idle cores and 3 GB.
- `fix` **Landing page polish.** The typing demo loops over its first frame, keeps the clip's aspect ratio and wraps lines so its editor never scrolls sideways. Sentences fade in at one steady pace without reflow. The live window draws no inner border, the front card is redrawn at its scale once the ring rests.
- `fix` **Dialog and settings polish.** Tabs, plugin cards and buttons share one flat hover, no card edge is cut, the footer's controls sit flush right. A list with its details stacks only when the dialog itself is too narrow. Small buttons, toolbar pills and selects share one height (`--btn-h-sm`), a dialog's close button matches its header icons (`--btn-h`). The phone library puts each description on its own line.
- `docs` **The license line joins the rotating claims.** "…that it is yours to read, run offline and change, MIT-licensed on GitHub", and an "MIT license" button sits beside Try it, Install and Guide.
- `docs` **Plainer docs and a clearer front page.** Guide, wiki and README use plain words, the lint page shows each rule's `S` or `U` badge. The front page explains why satex interprets the code, names the MIT license and the flowR inspiration (not used, not required).

### Analysis

- `feat` **Manuals for TikZ, pgf, primitives and more packages.** Hovers and Explain show pgf's manual, the pdfTeX manual, TeX by Topic and package docs in more formats (`docCommand`, `\DescribeMacro`, `desc` blocks, tcolorbox, enumitem, titlesec, xstring). Set by `docs.named_environments` and `docs.usage_blocks`.
- `feat-fix` **Option completion in listings and floats.** A body's `\csname lstlisting@\endcsname` is a call in the call graph, so `\begin{lstlisting}[` completes listings' keys. A float's `[…]` offers the placement letters not yet typed.
- `perf` **Far fewer path splits.** Four pgfmath divides 803 → 439 splits (−45%). l3fp over an aux position 112.6k splits and 37 s → 104 and 0.6 s (−99.9%, −98%). Tagged `\maketitle` and tables 281 s → 6 s (−98%). xcolor mix 88 → 4 splits (−95%).
- `perf` **Faster steps and long documents.** Figure-heavy documents save up to 25% of the time, other documents about 7%. 1000-page article 33.6 → 22.1 s (−34%), 1000-page scrbook −26%, tikz_200 peak 2.95 → 2.63 GB (−11%). A 20k-line paragraph 2.73 → 1.84 s (−33%), 1e5 `\loop` steps 0.66 → 0.53 s (−19%). Same steps and findings.
- `perf` **Kept call effects.** Calls of 200 steps or more replay when arguments and state match (`limits.call_memo_steps`, 0 turns it off, `limits.call_memo_verify: 1` checks each replay).
- `fix` **Build files give no entries or pictures.** The `.aux` read back no longer makes `\contentsline` arguments index entries, and `.ind`/`.bbl` environments no longer show as empty pictures.
- `fix` **A stray `#` in a definition is reported there.** `\def\ck#1{The year # is}` now gives TeX's "Illegal parameter number in definition" at the `#`, not only the later error at the use.
- `fix` **`limits.memory` counts what the process really holds.** The XeTeX thesis example peaked at 17.6 GB under an 8 GiB limit and was cut, it now reaches its end at 2.7 GB. Allocator overhead and resident memory count, and a loop under undecided conditionals no longer keeps every macro body it built.
- `fix` **Runs no longer stall or loop.** Joining and resuming waiting paths counts against `limits.walk_per_step`, so such a line is cut and reported instead of running for minutes. Fixed: `\DocumentMetadata` with graphicx at `\begin{document}`, `\@tryfcolumn` recursing 512 deep, `\clearnotecolumns`, captions looking endless at shipout, tcolorbox drawing code cut as a loop. A run waits at most `limits.lock_seconds` (60 s) for another build, then builds itself, and never waits on a lock it holds (chemfig's `\input chemfig.tex`).
- `fix` **pgfmath, stack and unknown-number precision.** Negative and positive parses stay apart, summaries no longer replace calls that rewrite a name they read and keep no text of other arguments (a false missing-number on a beamer title page), pgfmath stacks pop cleanly. TikZ bends to unknown width raise no false missing `\endcsname`, a signed random count now reaches the end, and TikZ pictures of unknown size end again.
- `fix` **Text colors after an early character.** Later text and page colors reach the Typeset view and color lints.
- `fix` **Sizes in math and on the page.** Big operators, delimiters, accents and scripts pick the right variant and size (the sample paper breaks its pages where pdflatex does). Displays add skips, so a here-float after one lands on its page.

## v0.2.4 (2026-11-15)

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
