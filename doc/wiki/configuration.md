<!-- generated from doc/src/wiki/configuration.md.in by satex-doc; do not edit -->

# Configuration

SaTeX merges its configuration from several `satex.yaml` files, in this order: the built-in defaults (the file below, with every key at its default). The user's own global config (`$XDG_CONFIG_HOME/satex/satex.yaml`, or `~/.config/satex/satex.yaml`. `%APPDATA%\satex\satex.yaml` on Windows). Every `satex.yaml` from the filesystem root down to the input file's directory, outermost first, so a directory's own file wins over an ancestor's. Then the file given with `--config`, on top of all of them. Every key is optional in every layer.

A mapping key merges recursively, so a layer only needs to name the keys it changes. A list replaces the outer layer's value entirely, unless the key is spelled with a trailing `+` (`skip_packages+: [extra]`), which appends to the outer list instead of replacing it. A layer whose merged result does not fit the schema below (an unknown key, a value of the wrong type) is left out, so one bad file does not blank out the ones under it. `satex --version` lists the files actually merged for a run, in order.

Any key can be overridden for one run with `--set PATH=VALUE`, where `PATH` is the key's dotted path in the file and `VALUE` is written as it would be there: `satex lint --set limits.memory=8GiB --set cache_index.auto=false`. The option can be repeated. `--set` is applied after every file, so it always wins.

`--no-config` ignores every `satex.yaml` file, discovered or named with `--config`, so a run starts from the built-in defaults below. `--set` still applies on top of it, so `satex lint --no-config --set limits.memory=8GiB` runs with every other default untouched. Useful for reproducing a result without a project's own configuration in the way, or in a script that should not be affected by whichever directory it runs in.

```yaml
# satex configuration. Merged from the global config, then every satex.yaml
# from the filesystem root down to the input file's directory, then --config PATH, then --set. 
# `satex --version` can be used to list the files actually merged for a run.

load_packages: true       # follow \usepackage and \RequirePackage
load_classes: true        # follow \documentclass and \LoadClass
load_inputs: true         # follow \input and \include
skip_packages: []         # packages to leave unread
search_paths: []          # searched before the installation
paths:                    # kpathsea variables' fallback list: tried after the
                          # environment and the project's latexmkrc/Makefile
  texinputs: []           # TEXINPUTS; a trailing // recurses, as in kpathsea
  bibinputs: []           # BIBINPUTS
  bstinputs: []           # BSTINPUTS
  tfmfonts: []            # TFMFONTS
  encfonts: []            # ENCFONTS
at_letter: false          # start with @ a letter, as .sty files do
lint_off: [analysis-imprecision]  # rule codes that never fire; a profile fills this in
report_public_definitions: true  # unused-definition also flags a public name
# profile: package          # forces one instead of detecting it from the input

# Sensible defaults per kind of input, chosen automatically by the input's
# extension and, failing that, what it starts with (\ProvidesPackage,
# \documentclass, ...); --profile NAME or profile: above overrides the
# choice.  Each block is a partial config, merged over everything above.
profiles:
  document: {}            # a LaTeX document: every default above already fits it
  package:                 # a .sty, read as its own author sees it
    at_letter: true
    report_public_definitions: false  # a public name is the package's own interface
    lint_off: [analysis-imprecision, unused-label, microtype-available, build-shell-escape-missing,
               build-shell-escape-unneeded, build-engine-mismatch,
               build-bibliography-disabled, build-bibliography-unneeded,
               build-missing-custom-dependency, build-unused-custom-dependency,
               build-command-placeholders, build-engine-options]
  class:                    # a .cls; the same defaults as a package
    at_letter: true
    report_public_definitions: false
    lint_off: [analysis-imprecision, unused-label, microtype-available, build-shell-escape-missing,
               build-shell-escape-unneeded, build-engine-mismatch,
               build-bibliography-disabled, build-bibliography-unneeded,
               build-missing-custom-dependency, build-unused-custom-dependency,
               build-command-placeholders, build-engine-options]
  literate:                 # a .dtx/.ins; unpacks to a package or class
    at_letter: true
    report_public_definitions: false
    lint_off: [analysis-imprecision, unused-label, microtype-available, build-shell-escape-missing,
               build-shell-escape-unneeded, build-engine-mismatch,
               build-bibliography-disabled, build-bibliography-unneeded,
               build-missing-custom-dependency, build-unused-custom-dependency,
               build-command-placeholders, build-engine-options]
  plain:                     # plain TeX: no LaTeX packages, classes or labels
    load_packages: false
    load_classes: false
    lint_off: [analysis-imprecision, unused-label, microtype-available]

# Detected when left out: engine from the build files, provider from the
# locator on PATH, platform from this machine, output from $pdf_mode.
# engine: pdftex          # tex, pdftex, luatex, xetex
# kernel: latex2e        # latex2e, or context (experimental)
# provider: texlive       # texlive, miktex, tectonic
# platform: linux         # linux, macos, windows
# output: pdf             # pdf, dvi, ps, html
# texlive_root: /usr/local/texlive
# texlive_year: 2026
texmf_roots: []           # trees to search instead of kpsewhich's
use_kpsewhich: true       # may run kpsewhich to find them
use_fallback_roots: true  # may fall back to well-known install paths

load_format: true         # interpret the installation's latex.ltx
cache: true               # keep the interpreted kernel between runs
# cache_dir: ~/.cache/satex   # else $SATEX_CACHE, else the platform default
cache_index:              # package caches; `satex cache build` prepares the
                          # packages listed (none by default; this list is ours)
  packages: [tikz, pgfplots, xcolor, graphicx, hyperref, amsmath, siunitx, babel,
             fontspec, biblatex, xparse, forest, tcolorbox, listings]
  engines: []             # empty: the engine in effect
  class: article          # the class they are loaded after
  auto: true              # every run stores the packages it reads
# preload: preamble.tex     # a preamble to read as this project's format, the
                            # way mylatexformat dumps one; a %& line or -fmt=
                            # in the build configuration names it otherwise

opaque_conditionals: []   # conditionals to leave undecided, both arms analyzed
expand_in_reports: []     # macros to expand when reporting a value, e.g. today

# Commands that record an occurrence, by kind, and the environments whose body
# is read as text rather than as code.
# Primitives the primitive-tex-command rule reports, with what to suggest
# instead; an empty value suggests nothing.  Entries add to what it knows.
latex_alternatives: {}

log_gaps: true            # append what a run could not follow to the gap log
report_gaps: true         # and print it
# gaps_log: ~/.cache/satex/gaps.ndjson


shell_escape: restricted  # none, restricted or full, as the engine was started
interaction: nonstopmode  # the engine's -interaction: batchmode … errorstopmode

plugins:
  magic: true             # read % !TeX and % arara: lines
  tools: true             # infer biber, makeindex and the rest
  preload: true           # honor %& lines and -fmt=
  discovery: true         # walk the project, skipping what .gitignore excludes

trace: false              # record the execution trace
timings: false            # measure where the time goes
record_arguments: false   # keep the argument tokens of every call

# Limits: the termination argument. Reaching one stops that part of the run
# and records a diagnostic. Raising one costs time, never accuracy.
limits:
  memory: 4GiB            # what one run may hold at once; 0 lifts the limit
  threads: 0              # threads for the project walk; the interpreter runs on one
  cache_size: 1GiB        # package caches kept, least recently used evicted; 0 for no bound
  steps: 900000000        # tokens the document may digest
  format_steps: 1000000000  # tokens latex.ltx may digest (built once, then cached)
  file_depth: 64          # how deep \input and \usepackage may nest
  expansion_depth: 24     # how often a macro may re-enter itself
  site_expansions: 16384   # how often one call site may expand without reading on
  seconds: 300            # wall-clock limit per run, 0 for none
  stall_tokens: 50000000  # tokens without reading the file before a loop is dropped
  branch_depth: 8         # nesting of undecided conditions
  join_tokens: 65536      # tokens the arms of one may read before they meet
  meaning_splits: 1       # path nesting up to which a name joined to several meanings splits per meaning
  widen_after: 3          # rounds a loop head reanalyzes a grown state before widening it
  meaning_set: 5          # meanings a join keeps as a set before the meaning is unknown
  value_set: 5            # values a join keeps for a count or dimension before only the interval
  expansion_tokens: 1048576   # tokens one expansion or argument may span
  conditional_tokens: 1048576 # tokens one conditional may skip over
  held_tokens: 4000000    # tokens waiting to be read
  expansion_stack: 4000   # token lists open at once: expansions, arguments, files
  save_stack: 2000000     # assignments a group may have to undo (~200 MB)
  vertices: 400000        # graph nodes recorded
  facts: 400000           # definitions, uses and occurrences recorded
  trace: 200000           # trace events recorded
```

## What plugins decide

A plugin is a piece of context that SaTeX would otherwise have to be told: it looks at the machine, the installation, and the document's build files, and picks a default that `satex.yaml` can still override. `satex query plugins -f FILE` shows what was chosen for a given run and why.

### provider

The TeX distribution: `texlive`, `miktex`, or `tectonic`. Detected by finding which locator program (`kpsewhich`, `tectonic`) is on `PATH`, and its reported version. Set with `provider:` in `satex.yaml`, or turn off detection with `use_kpsewhich: false`. When neither that nor an explicit `provider:`/`texmf_roots:`/`texlive_root:` resolves one, SaTeX falls back to well-known install paths (`/usr/local/texlive/...`, `$HOME/texmf`, ...). `--set use_kpsewhich=false --set use_fallback_roots=false` turns that off too, so a machine with TeX installed can be made to behave as if it had none — the run reports `no-format` once and analyzes the document as `initex` would, from the engine's own primitives.

### platform

The operating system SaTeX is running on: `linux`, `macos`, or `windows`. Detected from the build target SaTeX itself was compiled for. This decides path conventions and which build files (for example `.bat` wrappers) are recognized.

### engine

The TeX engine: `tex`, `pdftex`, `luatex`, or `xetex`. Detected from the document's build configuration, for example `latexmkrc`'s `$pdf_mode`, or a `% !TeX program` magic comment. The engine decides which primitives SaTeX interprets and which format file it looks for.

TeX, e-TeX and pdfTeX are what SaTeX models in full. LuaTeX and XeTeX are **experimental**: their primitive catalogs were probed against the installed engines, so every primitive is known and `\ifx` tests against them answer correctly, but SaTeX does not run Lua and does not model what the primitives do. `\directlua{…}` and `\latelua{…}` are read as Lua, not TeX: see [Lua](#lua) for what SaTeX reads from them without running them.

### kernel

The format the engine is started with: `latex2e` or `context`. `latex2e` is the default and the only one SaTeX interprets. ConTeXt is **experimental**: a document is recognized as ConTeXt from `% !TeX program = context` or from `\starttext`, `\stoptext`, `\startcomponent` or `\usemodule` in the source, `satex summary` then reports it as a ConTeXt document instead of mislabeling it LaTeX2e, and `latex.ltx` is not loaded for it. Because the ConTeXt kernel is not interpreted, only the engine primitives are known in such a run, and `undefined-control-sequence` is not reported. Set with `kernel:` in `satex.yaml`.

### output

The document's output format: `pdf`, `dvi`, `ps`, or `html`. Detected the same way as the engine, mainly from `$pdf_mode` in a `latexmkrc`. This decides which files a `\includegraphics` call can actually resolve to, and so whether `missing-graphic` fires.

### build system

The build tool driving the compilation: `latexmk`, `make`, `tectonic`, or `arara`, detected from which build file sits beside the document (`latexmkrc`, `Makefile`, `Tectonic.toml`, or an `% arara:` magic comment). This is what `satex query project` reports as the build configuration, and what the engine and output plugins read their defaults from.

### l3build

A `build.lua` in the document's directory or above it (within the repository) is an l3build bundle. SaTeX runs it: `texlua` evaluates it in a sandbox that cannot write files, start programs or load code, with l3build's own `l3build-variables.lua` supplying the defaults, as `l3build` does after `dofile("build.lua")`. Anything the file calls that only l3build defines (`uploadconfig`, `options`, `target_list`) is stubbed. When the run stops early, or no `texlua` is installed, the plain assignments (`x = "…"`, `x = {"a", "b"}`, `x = x or …`, `..`) are read from the file instead. `satex query project` says which happened.

What it configures is used:

* `sourcefiledir`, `supportdir`, `testsuppdir` and the directories `sourcefiles` reaches into are searched for packages, so the package under development is found from its sources;
* every file the `.ins` batch files in `unpackfiles` generate is read from its `.dtx` the way docstrip extracts it, with the guards the `.ins` names;
* a document in `typesetfiles` runs with `typesetexe`'s engine, anything else (the `.lvt` tests in `testfiledir`) with `stdengine`, the first of `checkengines` by default;
* when no `-f` names a file, the first of `typesetfiles` is the document.

`satex.yaml`'s `engine:` and a `% !TeX program` line still win.

### Lua

Under LuaTeX, `\directlua` and `\latelua` are read as LuaTeX reads them: the argument is expanded and turned into a string, which is recorded as a `lua` occurrence. SaTeX does not run Lua, but it reads what it can without doing so: a `require`, `dofile` or `loadfile` of a literal name is a dependency (and a local `.lua` file is read for the modules it loads in turn), and a `tex.print` or `tex.sprint` of string literals that the chunk runs unconditionally is read back into the input, with the catcode table its first argument names. A chunk that prints anything else is a `lua-output` gap. `tex.enableprimitives` with a literal list defines the prefixed primitives.

The catcode tables (`\initcatcodetable`, `\savecatcodetable`, `\catcodetable`) are interpreted, and `\luaescapestring` escapes its expanded argument, so packages built on them — `ltluatex`, `luatexbase`, `luacode`'s environments, expl3's `\lua_now:n` and `\cctab_select:N` — run from their own source. `\luatexversion` is the installed engine's, from `luatex --version`.

### TeX Live packages (depp)

[depp](https://gitlab.com/islandoftex/texmf/depp) names the TeX Live package of every file a LaTeX run reads by where it sits in the TDS tree, and writes the packages to a dependency file in TeX Live's format (`hard`/`soft` lines, `#` comments). SaTeX applies the same rule to every file the run reads — also the ones read before `\usepackage{depp}` — and, when depp is loaded or the dependency file is beside the document, holds the two against each other: `missing-dependency` for a package the run reads from that the file does not list, `unused-dependency` for one it lists that nothing comes from. depp's options `dependency-file` (`CTAN` for `DEPENDS.txt`, `jobname` for `\jobname-DEPENDS.txt`, or a name), `ignore`, `package`, `add-binaries` and `set-binaries` are honored. The summary shows the count on its `TeX Live packages` line, and `satex query dependencies` the package of every file.

The rule is configurable:

```yaml
depp:
  # the last capture of the first pattern that matches the file's directory
  rules: ['.+/tex/[^/]+/([^/]+)', '.+/fonts/.+/([^/]+)']
  renames: { base: latex }
  file: DEPENDS.txt   # instead of what depp's options name
```

### tools

Auxiliary programs a build calls beyond the engine itself: `biber`, `bibtex`, `makeindex`, `xindy`, `makeglossaries`, `dvips`, `ps2pdf`. Inferred from which commands the document's macros imply are needed, for example `\bibliography` implying `bibtex` or `biber`. Controlled with `plugins.tools` in `satex.yaml`.

### magic

TeXworks/TeXShop `% !TeX` comments (`program`, `root`, `encoding`, `spellcheck`), `% !BIB program`, and `% arara:` directives. Read from the leading comment block of each file. Controlled with `plugins.magic` in `satex.yaml`. Magic comments only ever narrow a default, never override an explicit `satex.yaml` key.

### preloaded format

The `.fmt` a build starts from instead of the stock `latex`, such as the ones `mylatexformat` dumps from a document's own preamble. SaTeX finds it in the `%&⟨format⟩` line TeX reads before anything else, in a `-fmt=` the build configuration passes to the engine, or in `preload:` in `satex.yaml`. Since a `.fmt` is a memory dump SaTeX cannot read, it interprets the preamble that was dumped instead: the file of the project named after the format, up to `\endofdump`, `\dump` or `\begin{document}`. The result is cached with the kernel, so a document that starts from its own format gets its own cache entry.

`satex cache build --preamble preamble.tex` builds that cache from a preamble directly. Controlled with `plugins.preload` in `satex.yaml`.

### discovery

What the project directory holds, found by walking it with the rules git uses: `.gitignore`, `.ignore`, the global excludes and hidden files are all skipped. It is what `satex` starts from when no `-f` names a file, and what `satex query project` lists: the documents that could be the root, the project's own classes and packages, its build configurations and its bibliographies. Controlled with `plugins.discovery` in `satex.yaml`.

### format

The interpreted LaTeX2e kernel (`latex.ltx`), cached after the first run so later runs do not re-interpret it. `satex cache` shows the cache's state. `satex cache build` replaces it. `satex cache clear` deletes it along with the package caches. Controlled with `load_format:` and `cache:` in `satex.yaml`.

### package caches

A package is a function of what it reads, so SaTeX caches what a `\usepackage` or `\documentclass` of the document did and replays it in any later run whose state agrees with everything the package looked at — whatever else that run's preamble does. While the package is read, SaTeX records its *read set*: every meaning and register value it consulted before assigning it, the hooks it looked at, the packages it asked about with `\@ifpackageloaded`, the options it was given and passed on, the category codes and `\endlinechar` it was read under. It records its *effect* too: the bindings it left, the hooks it changed, and every definition, occurrence, diagnostic and dependency-graph vertex it recorded, in order. A cache entry is the read set with the values seen, and the effect. A package keeps several entries, one per read set that differed, such as one per option set.

Register allocation is an effect, not a read: `\newcount` and its relatives advance `\count10`–`\count19`, and a replay allocates from the counters of the run it is replayed in, renumbering every register the package allocated — the meanings, the register slots, `\allocationnumber` and the `\count24` the log messages print. A register number a package keeps as plain text elsewhere is not renumbered.

A cache entry is rejected, and the package read again, when a file the run had read by then changed size or modification time, when a name looked up then now resolves to a different file, or when the run disagrees with the read set. With `-v` SaTeX logs why, and `satex summary` shows each package as `cached`, `stored`, `stale: …` or `not stored: …`. Loads under a conditional, inside a group or an expansion that is still reading its arguments, and loads that ran out of budget or left a group or conditional open, are not cached.

`satex cache build` prepares these caches up front: it interprets the kernel, replacing its cache, then interprets `\documentclass{⟨class⟩}\usepackage{⟨package⟩}` for every package named on the command line — or, when none are named, every package in `cache_index.packages` — and every engine in `cache_index.engines` (the one in effect when empty). It prints its plan first — each package and whether a cache is missing or will be checked — then one line per item as it goes (`[3/17] tikz (pdftex) … built in 2.1 s, 4.2 MiB`, `fresh, skipped`, `rejected: pgfkeys.code.tex changed; rebuilt …`, `failed: …`), all on standard error, and a summary of what was built, fresh and failed, the total size and the time on standard output. `--format json` prints only the result, as JSON. Packages are built only when missing or stale; `--refresh` rebuilds them too. `--engine NAME` overrides the configured engine list. `satex cache` shows how many package caches are stored and their size, and what it pruned: every store and `satex cache` delete the caches no build can read any more — another encoding, or package caches of a satex build that has no kernel cache left (a kernel family keeps this build's and the four most recent others) — and then the least recently used package caches beyond `limits.cache_size` (1 GiB by default, `0` for no bound. `--set limits.cache_size=4GiB`); `satex cache prune` applies that pruning on its own. The kernel and the configured class are built first. The packages are then analyzed in parallel, on `limits.threads` workers (every processor when 0), each with its own interpreter. The memory budget counts the whole process, so the worker count is also capped at one per GiB of `limits.memory`. Cache files are written to a temporary file and renamed, so concurrent writers and readers never see half a file.

```yaml
cache_index:
  packages: [tikz, pgfplots, xcolor, hyperref, xparse, forest]  # what cache build prepares; none by default
  engines: [pdftex, luatex]   # empty: the engine in effect
  class: article              # the class the packages are loaded after
  auto: true                  # every run stores the packages it reads
```

## Profiles

A `.sty` is read as its author sees it, not as a document that happens to load it: `@` is a letter, no `\documentclass` is required, and the lints that only make sense for a document with a build to check (`unused-label`, `microtype-available`, the `build-*` rules) do not fire on it. A document project turns those back on, discovers its main file the way [discovery](#discovery) always does, and reads its `latexmkrc`/Makefile for the engine and output format. SaTeX picks between defaults like these — `profiles.document`, `.package`, `.class`, `.literate` and `.plain` in `satex.yaml` below — from the kind of input it was given, and each one is itself just a partial config, merged over the general settings the same way a `satex.yaml` layer is ([Configuration](#configuration)).

The profile is chosen by the input's extension first: `.sty` is `package`, `.cls` is `class`, `.ins` is `literate`, and a `.dtx`/`.fdd` is `class` when the code docstrip extracts from it has a `\ProvidesClass` line and `package` otherwise. Where the extension says nothing — a `.tex` file, stdin, an unknown extension — SaTeX reads what the source starts with: `\ProvidesClass`/`\ProvidesPackage` before the first substantial line make it `package`/`class`, `\documentclass`/`\documentstyle` make it `document`. Failing that, `document` if `\documentclass` turns up anywhere and `plain` (plain TeX, no LaTeX) if it never does. The run reads a `.sty`, `.cls` or `.dtx` main file as that same kind of file, with `@` a letter.

`--profile NAME` (`document`, `package`, `class`, `literate` or `plain`) forces one outright, skipping the detection. `profile:` in `satex.yaml` does the same from a file. The profile's block is merged in after every discovered `satex.yaml` layer and before `--config`/`--set`, so either of those still overrides a key the profile set. `satex --version` and `satex summary` show which profile was in force.

```yaml
profile: package            # forces one instead of detecting it; --profile wins over this

profiles:
  package:                  # merged over the settings above when this profile applies
    at_letter: true
    report_public_definitions: false
    lint_off: [unused-label, microtype-available, build-engine-mismatch]
```

`report_public_definitions: false` (the package and class profiles' default) keeps `unused-definition` for a name that reads as the package's own internals — `@` in it, or an expl3 `module_function:signature` — but not for one with neither, since that is the package's public interface and the caller, not this run, is the one to use it.

## Search paths

Where SaTeX looks for a package, class or `\input` file beyond the document's own directory and the installation's trees, and where `\bibliography` looks for a `.bib` database beyond the project itself: the kpathsea search-path variables `TEXINPUTS` and `BIBINPUTS` (`BSTINPUTS`, `TFMFONTS` and `ENCFONTS` are resolved the same way and passed to `kpsewhich`, though SaTeX does not yet search them itself). Each is resolved in the order a real build would set it up in, the first source that sets it winning: the process environment. Then the project's `latexmkrc` (`ensure_path('TEXINPUTS', …)`, `$ENV{TEXINPUTS} = …`, read from the directory's own file and from `~/.latexmkrc`). Then its Makefile (`export TEXINPUTS=…`, or an inline `TEXINPUTS=… pdflatex`). Then `paths.texinputs` (and the other `paths.*` keys) in `satex.yaml`, itself overridable with `--set paths.texinputs=[…]`.

A value follows kpathsea's own path syntax (kpathsea manual, "Path searching"): `:`-separated (`;` on Windows), an empty component for the installation's own default path, a trailing `//` to search every subdirectory, and a leading `!!` to trust the installation's `ls-R` listing (accepted, though SaTeX always checks a candidate file directly rather than trusting a listing). Relative directories are resolved against the build's working directory — the directory the `latexmkrc` or Makefile sits in, which is not necessarily the document's own.

`kpsewhich` itself is started with whichever of these variables SaTeX resolved set in its own environment, so a lookup it makes agrees with SaTeX's. `satex --version` shows each variable's effective value, where it came from, and how many directories it expanded to.

## What a run may use

`limits.memory` bounds what one run may hold at once, written the way `ulimit` takes it (`512M`, `4GiB`, `0` for no limit, default `4GiB`). The `satex` binary counts every allocation it makes, so reaching the limit stops the run with the same `budget-exhausted` diagnostic as the other limits instead of letting the machine swap. `--max-memory SIZE` sets it for one run. A program that uses SaTeX as a library installs its own allocator, so it gets no limit.

`limits.threads` bounds the file walks: `0` (the default) lets SaTeX choose `min(4, cores)`, `1` keeps everything on one thread, any other number is the cap. `--threads N` sets it for one run. The interpreter itself always runs on one thread, because TeX's state is one machine: catcodes, the save stack and the macro table are global, and every token can change them.

Startup asks the installation two questions, `kpsewhich --expand-var` for the texmf roots and the engine for its version. Both answers are cached against the program's own path and timestamp, so only the first run after an installation changes pays for them.
