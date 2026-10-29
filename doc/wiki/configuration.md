<!-- generated from doc/src/wiki/configuration.md.in by satex-doc; do not edit -->

<p align="center"><a href="https://eagleoutice.github.io/satex-analyzer/"><img src="https://raw.githubusercontent.com/EagleoutIce/satex-analyzer/main/doc/wiki/img/satex-wide.svg" alt="SaTeX: Static Analysis for (La)TeX and Friends" width="600"></a></p>

# Configuration

SaTeX merges `satex.yaml` layers, later ones winning:

1. the built-in defaults (the file below),
2. your global config (`~/.config/satex/satex.yaml`, `%APPDATA%\satex\satex.yaml` on Windows),
3. every `satex.yaml` from the filesystem root down to the input file's directory,
4. `--config FILE`,
5. `--set PATH=VALUE` (repeatable, e.g. `--set limits.memory=8GiB`).

Mappings merge recursively. A list replaces the outer one unless its key ends in `+` (`load.skip+: [extra]`). A layer that does not fit the schema is skipped. `--no-config` ignores every file but keeps `--set`. `satex --version` lists the merged files.

```yaml
# yaml-language-server: $schema=https://raw.githubusercontent.com/EagleoutIce/satex-analyzer/main/satex.schema.json
# satex configuration. Merged from the global config, then every satex.yaml
# from the filesystem root down to the input file's directory, then --config PATH, then --set.
# `satex --version` can be used to list the files actually merged for a run.

load:  # what to follow from the document
  packages: true  # \usepackage and \RequirePackage
  classes: true  # \documentclass and \LoadClass
  inputs: true  # \input and \include
  format: true  # the installation's latex.ltx
  skip: []  # packages to leave unread
  expect: []  # classes and packages a package or class file expects loaded before it (a beamer theme: [beamer])
  expect_detect: true  # find them from what the packages on disk define, and say so
  build_files: false  # read the .aux, .toc, .bbl, ... an earlier build left; off: absent, the run over-approximates them
  build_extensions: [aux, toc, lof, lot, loa, lol, out, bbl, bcf, run.xml, blg, glo, gls, glg, acn, acr, alg, ist, idx, ind, ilg, nlo, nls, slnc, brf, nav, snm, vrb, fls, fdb_latexmk, xdy, log]  # what counts as such a file
search_paths: []  # searched before the installation
paths:  # kpathsea variables' fallback list: tried after the
                          # environment and the project's latexmkrc/Makefile
  texinputs: []  # TEXINPUTS; a trailing // recurses, as in kpathsea
  bibinputs: []  # BIBINPUTS
  bstinputs: []  # BSTINPUTS
  tfmfonts: []  # TFMFONTS
  encfonts: []  # ENCFONTS
  output_directory: ''  # -output-directory: a relative name opened at run time is looked up here first
at_letter: false  # start with @ a letter, as .sty files do
lint_off: [analysis-imprecision]  # rule codes that never fire; a profile fills this in
project:
  frozen: []  # globs of files read and analyzed, but with no findings and no fixes inside; relative to this file
  freeze_other_repositories: true  # files of another git repository (a submodule, a nested clone) count as frozen too
  ignore_other_configs: false  # true: satex.yaml files above this one and the global one are not read
conventions:  # how TeX is usually written, where the text alone does not say
  internal: ['@', '^__', '_.*:']  # regexes marking internal names (hidden from listings and completion, private to lints)
  definers: []  # more commands whose next name is the one they define; satex finds those the kernel defines with itself
lints:
  unused-definition:
    report_public: true  # also flag a public name
    ignore: []  # regexes on macro names the rule skips
    pgf_keys: false  # also flag a pgf key no-one uses
  wrong-dash:  # how many hyphens belong in each context
    letters: [1, 3]  # chktex HyphDash: a hyphen or an em-dash
    digits: [2]  # chktex NumDash: an en-dash spans a range
    words: [3, 2]  # chktex WordDash: an em-dash breaks a sentence, or Bringhurst's spaced en-dash
    except: [Birch--Swinnerton-Dyer]  # chktex DashExcpt: spellings to leave alone
  user-pattern:  # chktex UserWarn and UserWarnRegex
    patterns: []  # literal strings to report
    regexes: []  # regular expressions to report
  comment-marker:
    markers: [TODO, FIXME, XXX, HACK]  # markers in a comment worth a finding
  build-cost:  # when preamble-cost and includeonly-available suggest saving a part of the build
    tokens: 2000000  # a part below this reads in well under a second
    share: 0.5  # the share of the run's tokens the part must take
    characters: 500000  # includeonly-available: paragraph text the body sets, about 200 pages
  microtype-available:
    characters: 2500  # justified paragraph text the body must set, about a page
  ellipsis-dots:
    center: '=+-<>'  # chktex CenterDots: an ellipsis between these wants \cdots
  table-rules:
    rows: 3  # rows with text, header included, before booktabs rules are asked for
# profile: package          # forces one instead of detecting it from the input

# Sensible defaults per kind of input, chosen automatically by the input's
# extension and, failing that, what it starts with (\ProvidesPackage,
# \documentclass, ...); --profile NAME or profile: above overrides the
# choice.  Each block is a partial config, merged over everything above.
# A profile can reuse another with a YAML anchor and `<<:` merge key
# instead of repeating its keys (`profiles.class: {<<: *library, ...}`).
profiles:
  document: {}  # a LaTeX document: every default above already fits it
  package: &library  # a .sty, read as its own author sees it
    at_letter: true
    lints: {unused-definition: {report_public: false}}  # a public name is the package's own interface
    lint_off: [analysis-imprecision, unused-label, microtype-available, build-shell-escape-missing,
               build-shell-escape-unneeded, build-engine-mismatch,
               build-bibliography-disabled, build-bibliography-unneeded,
               build-missing-custom-dependency, build-unused-custom-dependency,
               build-command-placeholders, build-engine-options]
  class:  # a .cls; the same defaults as a package
    <<: *library
  literate:  # a .dtx/.ins; unpacks to a package or class
    <<: *library
  plain:  # plain TeX: no LaTeX packages, classes or labels
    load: {packages: false, classes: false}
    lint_off: [analysis-imprecision, unused-label, microtype-available, hand-set-quantity]

# Detected when left out.
# engine: pdftex         # tex | pdftex | luatex | xetex
# kernel: latex2e        # latex2e | context (experimental)
# provider: texlive      # texlive | miktex | tectonic
# platform: linux        # linux | macos | windows
# output: pdf            # pdf | dvi | ps | html
# texlive: {root: /usr/local/texlive, year: 2026}

texmf:
  roots: []  # trees to search instead of kpsewhich's
  kpsewhich: true  # may run kpsewhich to find them
  fallback: true  # may fall back to well-known install paths

cache:
  enabled: true  # keep the interpreted kernel and packages between runs
  # dir: ~/.cache/satex     # else $SATEX_CACHE, else the platform default
  tail: 256  # characters before a package's end where every line start is a checkpoint
  index:  # packages `satex cache build` prepares, or, when
                            # empty, whatever ordinary runs already found
                            # expensive to interpret (measured, not a hand-picked list)
    packages: []
    common: [microtype, cleveref, zref-clever, savetrees, tabularray, xurl, glossaries,
      enumitem, nowidow, silence, adjustbox, pdfpages, pgfplotstable, memoize,
      classicthesis]  # prepared first while packages is empty, where installed
    engines: []  # empty: the engine in effect
    class: article  # the class they are loaded after
    auto: true  # every run stores the packages it reads
# preload: preamble.tex     # a preamble to read as this project's format, the
                            # way mylatexformat dumps one; a %& line or -fmt=
                            # in the build configuration names it otherwise
# pretex: '\def\draft{}'   # TeX code read before the document (latexmk -usepretex)

opaque_conditionals: []  # conditionals to leave undecided, both arms analyzed
expand_in_reports: []  # macros to expand when reporting a value, e.g. today

# Primitives the primitive-tex-command rule reports, with what to suggest
# instead; an empty value suggests nothing.  Entries add to what it knows.
latex_alternatives: {}

gaps:
  log: true  # append what a run could not follow to file
  report: true  # and print it
  # file: ~/.cache/satex/gaps.ndjson

analysis:
  join_paths_at:  # where every path still apart is joined, reported as paths-joined-at
    - command: \document  # matched by meaning, however the run reaches it
    # - hook: begindocument  # an lthooks hook, when it is used
  assume_compiles: true  # assumption: drop a path that raises a certain TeX error in library code; the document's own errors are kept
  assume_packages_close_groups: true  # assumption: a package closes the groups it opens
  assume_environment: true  # assumption: the build runs in this environment (Lua's os.getenv reads it)
  assume_forced_page_breaks: true  # assumption: pages break only where a penalty forces it; false also follows breaks where the page may have filled

# shell_escape: restricted # none | restricted | full; unset, what the build files ask for
# TEMPORARY effect summaries: satex does not follow these macros' internals.
# Each is said once per run as an `assumption`. Remove when the internals run.
stubs:
  \includegraphics: { args: "som", result: box }
  \xinttheexpr: { until: \relax, result: { text: number } }

interaction: nonstopmode  # the engine's -interaction: batchmode … errorstopmode
clock: now  # what \year, \month, \day, \time and \pdfcreationdate hold: now | unknown | an ISO date-time

plugins:
  magic: true  # read % !TeX and % arara: lines
  tools: true  # infer biber, makeindex and the rest
  preload: true  # honor %& lines and -fmt=
  discovery: true  # walk the project, skipping what .gitignore excludes
  packages: false  # ask tlmgr which package ships a missing file (network)

trace: false  # record the execution trace
timings: false  # measure where the time goes
record_arguments: false  # keep the argument tokens of every call
arm_joins: true  # arms of a choice inside an \edef or \csname join where they meet in it

# Limits: the termination argument. Reaching one stops that part of the run
# and records a diagnostic. Only choices, site_forks, paths, meaning_splits, widen_after, recursion_depth,
# call_summary_after, meaning_set and value_set trade precision for time; the rest only stop a run.
limits:
  memory: 8GiB  # what one run may hold at once; 0 lifts the limit
  threads: 0  # threads for the project walk; the interpreter runs on one
  cache_size: 2GiB  # caches kept, least recently used evicted; 0 for no bound
  steps: 4000000000  # tokens the document may digest (pdflatex digests ~200M/s; pingus needs ~1.5G real tokens)
  format_steps: 9000000000  # tokens latex.ltx may digest (built once, then cached)
  file_depth: 64  # how deep \input and \usepackage may nest
  expansion_depth: 512  # how often one macro may be nested in itself before its recursion is widened
  site_expansions: 16384  # how often one call site may expand without reading on
  replay_steps: 1000000  # steps a line the run wrote may take where satex replays it for what it names
  seconds: 300  # wall-clock limit per run, 0 for none
  repeat_failures: 8  # times one gap may be met at one place before the expansion is dropped
  choices: 8  # undecided choices one command may make on one path
  site_forks: 4096  # undecided choices one place may make while a line is read before its expansion is cut; 0 = never
  paths: 16  # waiting paths before the two furthest back are cut to the input they share and joined
  alike: 8  # waiting paths before those at one place in the same conditionals and groups have their input widened
  heads_apart: 32  # loop heads at one place kept apart for differing in code still to run, before they are joined
  meaning_splits: 4  # choices a command may have made for a name joined to several meanings to split per meaning
  widen_after: 3  # rounds a loop head reanalyzes a grown state before widening it
  build_runs: 5  # runs of the build (latexmk rounds) to reach the fixpoint of .aux/.toc/...; past them unknown; 1 = one run, no earlier files
  recursion_depth: 100  # levels an undecided recursion is followed before it is assumed to end
  loop_heads: 512  # loop heads and entries one drive keeps with their joined states; the oldest goes first
  call_summary_after: 2  # runs of one call recorded before an unknown-operand call applies their join; 0 = never
  cycle_after: 2  # repeats of one call in the same state before its path ends (the only path is summarized); 0 = never
  meaning_set: 8  # meanings a join keeps as a set before the meaning is unknown
  join_arms: 32  # paths a join keeps paired (which binding goes with which), 0 pairs none
  join_names: 64  # names a join pairs at most; the rest are joined apart
  value_set: 8  # values a join keeps for a count or dimension before only the interval
  interval_set: 4  # members a join keeps for a number once intervals are among them, before only their hull
  degrade_at_percent: 100  # of limits.steps past which the precision limits are halved, step by step; 100 = never
  expansion_tokens: 1048576  # tokens one expansion may add
  conditional_tokens: 1048576  # tokens read ahead to count the arms of an \ifcase
  held_tokens: 4000000  # tokens waiting to be read
  expansion_stack: 4000  # token lists open at once: expansions, arguments, files
  save_stack: 2000000  # assignments a group may have to undo (~200 MB)
  vertices: 400000  # graph nodes recorded; past it the graph stops growing and the analysis goes on
  facts: 2000000  # definitions, uses and occurrences recorded
  trace: 200000  # trace events recorded
```

## What plugins decide

A plugin picks a default from the machine, the installation and the build files. `satex.yaml` can override each. `satex query plugins -f FILE` shows what was chosen and why.

| Plugin | Values | Detected from |
| --- | --- | --- |
| provider | `texlive`, `miktex`, `tectonic` | the locator on `PATH` (`kpsewhich`, `tectonic`), else well-known install paths. `texmf.kpsewhich: false` and `texmf.fallback: false` make the machine look TeX-less (`no-format`, `initex`). |
| platform | `linux`, `macos`, `windows` | the build target |
| engine | `tex`, `pdftex`, `luatex`, or `xetex` | `latexmkrc` (`$pdf_mode`), `% !TeX program` |
| kernel | `latex2e`, `plain`, `context`, or `none` | `latex2e` default; ConTeXt from `% !TeX program = context` or `\starttext` |
| output | `pdf`, `dvi`, `ps`, or `html` | `$pdf_mode`; decides what `\includegraphics` resolves to |
| build system | `latexmk`, `make`, `tectonic`, or `arara` | the build file beside the document |
| tools | `biber`, `bibtex`, `makeindex`, `xindy`, `makeglossaries`, … | the commands the document uses (`plugins.tools`) |
| magic | `% !TeX`, `% !BIB`, `% arara:` | the leading comment block; only narrows a default |
| discovery | root candidates, own classes and packages, build files, bibliographies | a git-style walk (`.gitignore` respected) |

- **Engines.** TeX, e-TeX and pdfTeX are modelled in full. LuaTeX and XeTeX are **experimental**: every primitive is known, but their effects are not modelled. ConTeXt is **experimental** too: its kernel is not interpreted, so `undefined-control-sequence` is off.
- **Lua.** `\directlua`/`\latelua` are read as strings, not run. Literal `require`/`dofile` are dependencies, and unconditional `tex.print` of literals is read back in; anything else is a `lua-output` gap. Catcode tables and `\luaescapestring` are interpreted, so `ltluatex`, `luacode` and expl3's Lua bridge run from source.
- **l3build.** A `build.lua` runs in a sandboxed `texlua` (else its plain assignments are read). Its source dirs are searched, `.ins`/`.dtx` pairs read through docstrip, and `typesetexe`/`stdengine` pick the engine.
- **depp.** Every file read maps to its TeX Live package by TDS path. With depp loaded or its file present, `missing-dependency` and `unused-dependency` compare against it. Rules: `depp.rules`, `depp.renames`, `depp.file`.
- **Preloaded format.** A `%&fmt` line, `-fmt=` or `preload:` names a dumped preamble; SaTeX interprets that preamble instead (up to `\endofdump`). `pretex:` is code read before the first line (latexmk's `-usepretex`).

### Format and package caches

The interpreted kernel is cached after the first run (`satex cache`, `satex cache build`, `satex cache clear`).

A package is cached per **read set**: the meanings, registers, hooks, options and catcodes it consulted, with the values seen. Its **effect** (bindings, hooks, facts, diagnostics) is replayed in any later run that agrees with the read set. Register allocations are renumbered on replay.

- **Rejected** when a file it read changed, a name now resolves to another file, or the state disagrees. `-v` says why; `satex summary` shows `cached`, `stored`, `stale: …`.
- **Not cached**: loads under a conditional or in a group, or that ran out of budget or left a group open.
- **Nested loads** have their own entries; the outer one resumes at the next line at rest.
- **`satex cache build`** prepares the kernel and `cache.index.packages` (or named packages) for `cache.index.engines`, in parallel on `limits.threads`. It reports progress per package and only rebuilds stale ones (`--refresh` forces).
- **Pruning** keeps this build's kernel and four recent ones, then trims by `limits.cache_size` (2 GiB) least-recently used. `satex cache prune` does only that.

```yaml
cache:
  index:
    packages: [tikz, pgfplots, xcolor, hyperref, xparse, forest]  # what cache build prepares
    engines: [pdftex, luatex]   # empty: the engine in effect
    class: article              # the class the packages load after
    auto: true                  # every run stores what it reads
```

## Profiles

A profile is a partial config chosen by the input's kind. A `.sty` is read as its author sees it: `@` a letter, no `\documentclass`, no document-only lints.

| Profile | Chosen for |
| --- | --- |
| `package` | `.sty`, a `.dtx` without `\ProvidesClass`, or a file starting with `\ProvidesPackage` |
| `class` | `.cls`, a `.dtx` with `\ProvidesClass` |
| `literate` | `.ins` |
| `document` | `\documentclass` anywhere |
| `plain` | none of the above |

`--profile NAME` or `profile:` forces one. It merges after the discovered layers and before `--config`/`--set`.

```yaml
profile: package            # forces one; --profile wins
profiles:
  package:
    at_letter: true
    lints: {unused-definition: {report_public: false}}
    lint_off: [unused-label, microtype-available, build-engine-mismatch]
```

`project.frozen` (default `[]`): Globs of files satex still reads and analyzes, so their macros, counters and labels count, but reports nothing in and offers no fixes for, such as `["_config/**", "*.cls"]`. Each glob is relative to the `satex.yaml` that sets it, and `*` crosses directories.

`project.freeze_other_repositories` (default `true`): Whether the files of every git repository other than the main file's, such as a submodule or a nested clone, are frozen too. A file belongs to the repository of its nearest directory with a `.git` in it. Such code is someone else's, and the document only uses it.

`project.ignore_other_configs` (default `false`): Whether the `satex.yaml` files above this one and the global one are ignored, like `root = true` in `.editorconfig`. `--config` and `--set` still apply.

A rule's own settings live under `lints.⟨code⟩`.

`lints.unused-definition.report_public` (default `true`): Whether the rule also reports a name with no `@` and no expl3-style `module_function:signature`. The package and class profiles turn it off, since such a name is the package's public interface, and the caller, not this run, is the one to use it.

`lints.unused-definition.ignore` (default `[]`): Regexes on the name as written (`\\my@hook`). A match is never reported.

`lints.unused-definition.pgf_keys` (default `false`): Whether the rule also reports a pgf key nothing uses. Off by default, since the user sets keys.

`lints.wrong-dash.letters` (default `[1, 3]`): How many hyphens in a row belong between letters, as chktex's `HyphDash`. The first entry is what a fix writes.

`lints.wrong-dash.digits` (default `[2]`): How many belong between numbers, as chktex's `NumDash`. Hyphens inside an identifier such as an ISBN or a date (another hyphen, a letter or a `/` in the same word) are left alone.

`lints.wrong-dash.words` (default `[3, 2]`): How many belong between spaces, as chktex's `WordDash`; two is Bringhurst's spaced en-dash.

`lints.wrong-dash.except` (default `[Birch--Swinnerton-Dyer]`): Whole spellings to leave alone, as chktex's `DashExcpt`.

`lints.user-pattern.patterns` (default `[]`): Literal strings reported wherever the document's own text has them, as chktex's `UserWarn`.

`lints.user-pattern.regexes` (default `[]`): Regular expressions reported the same way, as chktex's `UserWarnRegex`.

`lints.comment-marker.markers` (default `[TODO, FIXME, XXX, HACK]`): The markers in a comment worth a finding. This is satex's reading of chktex's warning 22, which prints every comment.

## Search paths

`TEXINPUTS` and `BIBINPUTS` (plus `BSTINPUTS`, `TFMFONTS`, `ENCFONTS`, passed on to `kpsewhich`) come from, first wins: the environment, the `latexmkrc` (`ensure_path`, `$ENV{…}`), the Makefile (`export`, inline), then `paths.*` in `satex.yaml`. kpathsea syntax applies (`:`, `//`, an empty component for the default). Relative entries resolve against the build's directory. `satex --version` shows each value and its source.

**Output directory.** With `$aux_dir`/`$out_dir`, `-outdir=` or `paths.output_directory`, relative names open there first (web2c `open_input`), so `build/main.aux` is read, not a stale copy.

## The clock

`clock` sets `\year`, `\month`, `\day`, `\time` and `\pdfcreationdate` ([tex.web § 241](https://github.com/TeX-Live/texlive-source/blob/fd728dfcdeda36e6180c75121b35aa1b128771f0/texk/web2c/tex.web#L5240)).

| Value | Meaning |
| --- | --- |
| `now` (default) | the analysis time, reported once as `clock-assumed` |
| `2026-09-29`, `2026-09-29T14:30+02:00` | a pinned date |
| `unknown` | every digit unknown: date-dependent code is analyzed for every outcome |

Package caches that read the clock are kept per setting.

## Where paths join

Undecided tests part the run into paths ([Domains](Domains)). They join where they stand alike, and at the points `analysis.join_paths_at` names.

```yaml
analysis:
  join_paths_at:
    - command: \document
    - hook: begindocument
```

`analysis.join_paths_at` (default `[{command: \document}]`): Points where every path still apart is joined, whatever else differs. A `command` is matched by its meaning: `\document` joins wherever the run starts that meaning, from `\begin{document}`, an alias or an `\expandafter`. A `hook` is an lthooks name and joins when the hook is used. A slot the paths disagree on becomes the join of their values, widened when needed, and no path is dropped. Each join is reported once as the `imprecision` diagnostic `paths-joined-at`, with how many paths arrived and what kept them apart.

`analysis.assume_compiles` (default `true`): A soundiness assumption: the document compiles without errors. A path that raises a certain TeX error in library code (a package, the kernel) is then no run of the document, and is dropped. An error is certain when it arises on state the run knows exactly, before any approximation. An error in the document's own text is always kept, to be reported and fixed. Each drop is reported once as an `assumption` diagnostic, and the last path is never dropped. `false` keeps every path.

`analysis.assume_packages_close_groups` (default `true`): A soundiness assumption: a package closes the groups it opens, so two paths at the same line of a file, one in more groups, join with those closed. `false` keeps such paths apart.

`analysis.assume_environment` (default `true`): A soundiness assumption: the build runs in the environment SaTeX runs in, which Lua's `os.getenv` reads. `false` makes every variable unknown.

`analysis.assume_forced_page_breaks` (default `true`): A soundiness assumption: a page breaks only where a penalty forces it (`\clearpage`, `\newpage`, `\end`). `false` also fires the output routine wherever the page may have filled, one path each way.

## Stubs

Stubs are a temporary exception; the goal is none. A stub summarizes a macro SaTeX cannot yet run. Each use is an `assumption`.

```yaml
stubs:
  \includegraphics: { args: "som", result: box }
  \xinttheexpr: { until: \relax, result: { text: number } }
```

`args`: `s` star, `o` optional, `m` mandatory; or `until` a delimiter. `result` is an element of the [unknown-text lattice](Domains#unknown-text-the-textclass-lattice): `nothing`, `number` / `{ number: [lo, hi] }`, `dimen`, `text` / `{ text: digits | number | dimension | cs-free | any }`, `box` / `{ box: { width, height, depth } }`. `sets` names what it assigns (`sets: { \pgfmathresult: number }`). A document that redefines the name gets its own definition.

## What a run may use

| Key | Default | Effect |
| --- | --- | --- |
| `limits.memory` | `8GiB` | stops the run with `budget-exhausted` (`--max-memory`); library users get no limit |
| `limits.seconds` | `300` | stops the run; lints after it share the same budget |
| `limits.degrade_at_percent` | `100` (off) | past that share of `limits.steps`, halves the time-only limits, reported as `precision-degraded` |
| `limits.threads` | `0` (`min(4, cores)`) | file walks only; the interpreter is one thread |

Steps are counted alike on every machine, so results never depend on speed; seconds and memory only stop a run. The installation's answers (`kpsewhich --expand-var`, engine version) are cached against the program's path and timestamp.
