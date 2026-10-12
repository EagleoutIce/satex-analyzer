# Primitive facts: what the engines register, measured

Ground truth for turning satex's primitive registration (`src/builtins.rs`: `initial_meanings`, `docstrip_meanings`, the per-engine `if` blocks, the `Primitive` enum) into one static data table per engine. Everything below was measured on real engines, not read off a manual; satex's side was dumped by calling `builtins::initial_meanings` for each `Engine`.

## Summary of mismatches

| category | count |
|---|---:|
| satex registers, `tex` engine lacks (eTeX names; satex `tex` is eTeX-extended) | 67 |
| satex registers for pdftex, real pdfTeX 1.40.25 lacks | 2 |
| satex registers for xetex, real XeTeX lacks | 1 |
| satex registers for luatex, real LuaTeX 1.17 lacks | 8 |
| engine has, satex lacks (pdftex / xetex / luatex) | 13 / 11 / 1 |
| engine-membership pattern mismatches (names, all engines) | 95 |
| kind mismatches (satex variant vs measured class) | 15 |
| initial-value mismatches (excluding clock, interactionmode, pagegoal) | 39 |

## Method

Engines (TeX Live 2023, Debian): `TeX 3.141592653 (TeX Live 2023/Debian)`, `pdfTeX 3.141592653-2.6-1.40.25 (TeX Live 2023/Debian)`, `XeTeX 3.141592653-2.6-0.999995 (TeX Live 2023/Debian)`, `This is LuaTeX, Version 1.17.0 (TeX Live 2023/Debian)`.

1. **Candidate names**: every `[A-Za-z]+` string in the four engine binaries (`strings`), every name satex registers, and every name `tex.primitives()` lists in LuaTeX (15.9k names). A first pass (`\ifx\name\undefined`) in `-ini` mode keeps the ones the engine defines: in iniTeX every defined control sequence is a primitive. The three non-letter primitives (`\ `, `\/`, `\-`) exist in every engine and are not probed.
2. **Per-name probe** (one engine process per name, so a primitive that derails the input cannot hide its neighbours): `\meaning` (a primitive prints as `\name`; a different print name marks an alias, `\XeTeXdelcode` → `\Udelcode`), expandability (`\expandafter\ifx\noexpand\n\n` is false exactly for expandable commands) and `\showthe\name 0 ` for the non-expandable ones, whose reply (`0`, `0.0pt`, `0pt plus 1fil`, a token list) gives the value class and the iniTeX initial value. Names that are not internal quantities answer "You can't use … after \the" and have no value.
3. **Command code** (`command` column): LuaTeX only, from `token.create(name).cmdname` after `tex.enableprimitives('', tex.extraprimitives())` (these are LuaTeX's own `cmdname`s, e.g. `assign_int`, `if_test`; they follow tex.web's command codes). The other engines do not expose command codes to the document; their column is filled from LuaTeX where the same name exists there and is blank for engine-specific names (`pdf*`, `XeTeX*`).
4. **LaTeX values**: the same probe in the body of a document (`\documentclass{article}\begin{document}`, no packages) on formats built from TeX Live's own `pdflatex.ini`, `xelatex.ini`, `lualatex.ini`. There is no LaTeX format for Knuth's `tex`, so its LaTeX columns are empty. `meaning_latex` shows where LaTeX replaced a primitive (`\everypar` is `\toks12`, `\par` is a macro).

Engines probed (`doc/primitive-facts/<engine>.tsv`):

| file | how it was run | satex `Engine` it is compared to |
|---|---|---|
| `tex.tsv` | `tex -ini` (Knuth TeX 3.14159265) | `tex` |
| `pdftex.tsv` | `pdftex -ini` (compatibility mode, no eTeX) | none (informative) |
| `etex.tsv` | `pdftex -ini -etex` (what `pdflatex` runs) | `pdftex` |
| `xetex.tsv` | `xetex -ini -etex` | `xetex` |
| `luatex.tsv` | `luatex -ini`, extra primitives enabled | `luatex` |

TSV columns: `name`, `command` (LuaTeX cmdname, see above), `value_class` (`int`, `dimen`, `glue`, `muglue`, `toks`, `toks/other`, `expandable`, `nonvalue`, `crash`), `expandable`, `meaning_ini`, `initial_ini` (value in iniTeX; dimens in sp, glue as the engine prints it), `meaning_latex`, `initial_latex` (empty when not measured or not a value).

## Primitive counts

| engine | primitives (letters only) | satex registers (letters only) | satex-only | engine-only |
|---|---:|---:|---:|---:|
| tex | 322 | 389 | 67 | 0 |
| etex | 545 | 534 | 2 | 13 |
| xetex | 498 | 488 | 1 | 11 |
| luatex | 721 | 728 | 8 | 1 |
| pdftex (compat, no eTeX) | 476 | (n/a) | 71 | 13 |

The three non-letter primitives (`\ `, `\/`, `\-`) are registered by satex for every engine and exist in every engine.

## Names satex registers that the engine lacks

**`tex`**: satex's `tex` engine is eTeX-extended (`Engine::has_etex` is always `true`), so all 67 eTeX primitives below are registered although Knuth's TeX has none of them. If `tex` is meant to be Knuth's TeX they have to be dropped from its table; if it is "TeX with eTeX" the engine should be called `etex`.

`TeXXeTstate`, `beginL`, `beginR`, `botmarks`, `clubpenalties`, `currentgrouplevel`, `currentgrouptype`, `currentifbranch`, `currentiflevel`, `currentiftype`, `detokenize`, `dimexpr`, `displaywidowpenalties`, `eTeXrevision`, `eTeXversion`, `endL`, `endR`, `everyeof`, `firstmarks`, `fontchardp`, `fontcharht`, `fontcharic`, `fontcharwd`, `glueexpr`, `glueshrink`, `glueshrinkorder`, `gluestretch`, `gluestretchorder`, `gluetomu`, `ifcsname`, `ifdefined`, `iffontchar`, `interactionmode`, `interlinepenalties`, `lastlinefit`, `lastnodetype`, `marks`, `middle`, `muexpr`, `mutoglue`, `numexpr`, `pagediscards`, `parshapedimen`, `parshapeindent`, `parshapelength`, `predisplaydirection`, `protected`, `readline`, `savinghyphcodes`, `savingvdiscards`, `scantokens`, `showgroups`, `showifs`, `showtokens`, `splitbotmarks`, `splitdiscards`, `splitfirstmarks`, `topmarks`, `tracingassigns`, `tracinggroups`, `tracingifs`, `tracingnesting`, `tracingscantokens`, `tracingstacklevels`, `unexpanded`, `unless`, `widowpenalties`

**pdftex** (measured: etex): `ignoreprimitiveerror`, `pdfptexuseunderscore`

**xetex** (measured: xetex): `ignoreprimitiveerror`

**luatex** (measured: luatex): `TeXXeTstate`, `beginL`, `beginR`, `endL`, `endR`, `ignoreprimitiveerror`, `mathemptydisplaymode`, `tracingstacklevels`

Version-dependent registrations (the measured engines are TeX Live 2023): `\ignoreprimitiveerror` is in none of pdfTeX 1.40.25, XeTeX 0.999995 or LuaTeX 1.17; `\pdfptexuseunderscore` is not in pdfTeX 1.40.25; `\mathemptydisplaymode` and `\tracingstacklevels` are not in LuaTeX 1.17 (`\tracingstacklevels` is also not in Knuth's TeX, where it is just one of the eTeX names above, but it is in pdfTeX and XeTeX). They are mismatches only against this release. `\TeXXeTstate`, `\beginL`, `\beginR`, `\endL` and `\endR` are registered for LuaTeX, which has none of them (it has `\textdir`-style directions instead); they exist in pdfTeX and XeTeX.

## Names the engine has that satex lacks

**etex**: `pdfadjustinterwordglue`, `pdfforcepagebox`, `pdfinclusionerrorlevel`, `pdflastximagecolordepth`, `pdfmovechars`, `pdfomitinfodict`, `pdfomitprocset`, `pdfoptionalwaysusepdfpagebox`, `pdfoptionpdfinclusionerrorlevel`, `pdfoptionpdfminorversion`, `pdfsuppresswarningdupdest`, `pdfsuppresswarningdupmap`, `pdfsuppresswarningpagegroup`

**xetex**: `XeTeXdelcode`, `XeTeXdelcodenum`, `XeTeXdelimiter`, `XeTeXmathaccent`, `XeTeXmathchar`, `XeTeXmathchardef`, `XeTeXmathcharnum`, `XeTeXmathcharnumdef`, `XeTeXmathcode`, `XeTeXmathcodenum`, `XeTeXradical`

**luatex**: `glet`

The eleven `\XeTeX…` names are aliases: `\meaning\XeTeXdelcode` prints `\Udelcode`. satex registers the `\U…` spelling only. `\glet` is a LuaTeX addition (`\global\let`) that satex lacks.

## Engine assignment mismatches

Membership over (tex, etex, xetex, luatex), `1` = present. "engine" is measured, "satex" is what `initial_meanings` registers for the matching `Engine`.

| engine | satex | count | names |
|---|---|---:|---|
| 0000 | 0001 | 1 | `mathemptydisplaymode` |
| 0000 | 0100 | 1 | `pdfptexuseunderscore` |
| 0000 | 0111 | 1 | `ignoreprimitiveerror` |
| 0001 | 0000 | 1 | `glet` |
| 0010 | 0000 | 11 | `XeTeXdelcode`, `XeTeXdelcodenum`, `XeTeXdelimiter`, `XeTeXmathaccent`, `XeTeXmathchar`, `XeTeXmathchardef`, `XeTeXmathcharnum`, `XeTeXmathcharnumdef`, `XeTeXmathcode`, `XeTeXmathcodenum`, `XeTeXradical` |
| 0100 | 0000 | 13 | `pdfadjustinterwordglue`, `pdfforcepagebox`, `pdfinclusionerrorlevel`, `pdflastximagecolordepth`, `pdfmovechars`, `pdfomitinfodict`, `pdfomitprocset`, `pdfoptionalwaysusepdfpagebox`, `pdfoptionpdfinclusionerrorlevel`, `pdfoptionpdfminorversion`, `pdfsuppresswarningdupdest`, `pdfsuppresswarningdupmap`, `pdfsuppresswarningpagegroup` |
| 0110 | 1111 | 6 | `TeXXeTstate`, `beginL`, `beginR`, `endL`, `endR`, `tracingstacklevels` |
| 0111 | 1111 | 61 | `botmarks`, `clubpenalties`, `currentgrouplevel`, `currentgrouptype`, `currentifbranch`, `currentiflevel`, `currentiftype`, `detokenize`, `dimexpr`, `displaywidowpenalties`, `eTeXrevision`, `eTeXversion`, `everyeof`, `firstmarks`, `fontchardp`, `fontcharht`, `fontcharic`, `fontcharwd`, `glueexpr`, `glueshrink`, `glueshrinkorder`, `gluestretch`, `gluestretchorder`, `gluetomu`, `ifcsname`, `ifdefined`, `iffontchar`, `interactionmode`, `interlinepenalties`, `lastlinefit`, `lastnodetype`, `marks`, `middle`, `muexpr`, `mutoglue`, `numexpr`, `pagediscards`, `parshapedimen`, `parshapeindent`, `parshapelength`, `predisplaydirection`, `protected`, `readline`, `savinghyphcodes`, `savingvdiscards`, `scantokens`, `showgroups`, `showifs`, `showtokens`, `splitbotmarks`, `splitdiscards`, `splitfirstmarks`, `topmarks`, `tracingassigns`, `tracinggroups`, `tracingifs`, `tracingnesting`, `tracingscantokens`, `unexpanded`, `unless`, `widowpenalties` |

## Kind mismatches (satex variant vs engine)

| engine | name | satex variant | LuaTeX command | measured class |
|---|---|---|---|---|
| tex | setlanguage | Command | assign_int | int |
| tex | spacefactor | IntegerParameter | set_aux | nonvalue |
| etex | setlanguage | Command | assign_int | int |
| etex | spacefactor | IntegerParameter | set_aux | nonvalue |
| xetex | setlanguage | Command | assign_int | int |
| xetex | spacefactor | IntegerParameter | set_aux | nonvalue |
| luatex | bodydirection | Unmodeled | assign_direction | int |
| luatex | catcodetable | Lua | assign_int | int |
| luatex | linedirection | Unmodeled | assign_direction | int |
| luatex | mathdirection | Unmodeled | assign_direction | int |
| luatex | pagedirection | Unmodeled | assign_direction | int |
| luatex | pardirection | Unmodeled | assign_direction | int |
| luatex | setlanguage | Command | assign_int | int |
| luatex | spacefactor | IntegerParameter | set_aux | nonvalue |
| luatex | textdirection | Unmodeled | assign_direction | int |

The `LastItem`/`Special`/`Register` rows with class `int`/`dimen` that the table above does not list are consistent: a read-only or specially stored quantity answers `\showthe`.

## Initial-value mismatches (iniTeX)

satex's initial value is what `Machine::install_parameters` yields (zero unless `builtins::limit(..).start()` or the explicit list says otherwise). Excluded as not facts about the engine: `\year`, `\month`, `\day`, `\time` (clock), `\interactionmode` (the probe runs in `\scrollmode`), `\pagegoal` (page builder reads `\maxdimen` while the page is empty; satex models that as `Owned::PageSoFar`).

| engine | name | satex variant | satex initial | measured initial |
|---|---|---|---|---|
| tex | errorcontextlines | IntegerParameter | -1 | 0 |
| etex | errorcontextlines | IntegerParameter | -1 | 0 |
| etex | pdfcompresslevel | IntegerParameter | 0 | 9 |
| etex | pdfdecimaldigits | IntegerParameter | 0 | 3 |
| etex | pdfeachlinedepth | DimenParameter | 0sp | -65536000sp |
| etex | pdfeachlineheight | DimenParameter | 0sp | -65536000sp |
| etex | pdffirstlineheight | DimenParameter | 0sp | -65536000sp |
| etex | pdfgamma | IntegerParameter | 0 | 1000 |
| etex | pdfhorigin | DimenParameter | 0sp | 4736287sp |
| etex | pdfignoreddimen | DimenParameter | 0sp | -65536000sp |
| etex | pdfimagegamma | IntegerParameter | 0 | 2200 |
| etex | pdfimagehicolor | IntegerParameter | 0 | 1 |
| etex | pdfimageresolution | IntegerParameter | 0 | 72 |
| etex | pdflastlinedepth | DimenParameter | 0sp | -65536000sp |
| etex | pdfmajorversion | IntegerParameter | 0 | 1 |
| etex | pdfminorversion | IntegerParameter | 0 | 4 |
| etex | pdfpxdimen | DimenParameter | 0sp | 65782sp |
| etex | pdfvorigin | DimenParameter | 0sp | 4736287sp |
| etex | showstream | IntegerParameter | 0 | -1 |
| xetex | XeTeXhyphenatablelength | IntegerParameter | 0 | 63 |
| xetex | errorcontextlines | IntegerParameter | -1 | 0 |
| xetex | showstream | IntegerParameter | 0 | -1 |
| luatex | compoundhyphenmode | IntegerParameter | 0 | 1 |
| luatex | errorcontextlines | IntegerParameter | -1 | 0 |
| luatex | exhyphenchar | IntegerParameter | 0 | 45 |
| luatex | mathdefaultsmode | IntegerParameter | 1 | 0 |
| luatex | matheqnogapstep | IntegerParameter | 0 | 1000 |
| luatex | mathflattenmode | IntegerParameter | 0 | 1 |
| luatex | pagebottomoffset | DimenParameter | 0sp | 4736287sp |
| luatex | pageleftoffset | DimenParameter | 0sp | 4736287sp |
| luatex | pagerightoffset | DimenParameter | 0sp | 4736287sp |
| luatex | pagetopoffset | DimenParameter | 0sp | 4736287sp |
| luatex | prebinoppenalty | IntegerParameter | 0 | 10000 |
| luatex | predisplaygapfactor | IntegerParameter | 0 | 2000 |
| luatex | prehyphenchar | IntegerParameter | 0 | 45 |
| luatex | prerelpenalty | IntegerParameter | 0 | 10000 |
| luatex | pxdimen | DimenParameter | 0sp | 65781sp |
| luatex | showstream | IntegerParameter | 0 | -1 |
| luatex | variablefam | IntegerParameter | 0 | -1 |

## Under the LaTeX format

Primitives whose meaning is not the primitive any more in the pdfLaTeX/XeLaTeX/LuaLaTeX body (`meaning_latex` column):

- **etex**: `\end` → `macro:->\romannumeral \ifx \protect \@typeset@prot`, `\eqno` → `\protected macro:->\@kernel@eqno \aftergroup \igno`, `\everydisplay` → `\toks14`, `\everymath` → `\toks13`, `\everypar` → `\toks12`, `\input` → `macro:->\@ifnextchar \bgroup \@iinput \@@input`, `\leqno` → `\protected macro:->\@kernel@leqno \aftergroup \ign`, `\par` → `\protected\long macro:->\scan_stop: \mode_if_horiz`, `\shipout` → `\protected\long macro:->\tl_set:Nx \l__shipout_gro`, `\underline` → `macro:->\protect \underline`
- **xetex**: `\end` → `macro:->\romannumeral \ifx \protect \@typeset@prot`, `\eqno` → `\protected macro:->\@kernel@eqno \aftergroup \igno`, `\everydisplay` → `\toks14`, `\everymath` → `\toks13`, `\everypar` → `\toks12`, `\input` → `macro:->\@ifnextchar \bgroup \@iinput \@@input`, `\leqno` → `\protected macro:->\@kernel@leqno \aftergroup \ign`, `\par` → `\protected\long macro:->\scan_stop: \mode_if_horiz`, `\shipout` → `\protected\long macro:->\tl_set:Nx \l__shipout_gro`, `\underline` → `macro:->\protect \underline`
- **luatex**: `\end` → `macro:->\romannumeral \ifx \protect \@typeset@prot`, `\eqno` → `\protected macro:->\@kernel@eqno \aftergroup \igno`, `\everydisplay` → `\toks14`, `\everymath` → `\toks13`, `\everypar` → `\toks12`, `\input` → `macro:->\@ifnextchar \bgroup \@iinput \@@input`, `\leqno` → `\protected macro:->\@kernel@leqno \aftergroup \ign`, `\par` → `\protected\long macro:->\scan_stop: \mode_if_horiz`, `\shipout` → `\protected\long macro:->\tl_set:Nx \l__shipout_gro`, `\tracingfonts` → `macro:->\@font@warning {Command \noexpand \tracing`, `\underline` → `macro:->\protect \underline`

Initial values that LaTeX changes are in the `initial_ini` vs `initial_latex` columns (e.g. `\tolerance` 10000 → 200, `\hsize` 0 → 345pt, `\baselineskip` 0 → 12pt).

## Notes for a static table

- **A parameter satex hard-codes that the engines do not start with**: `\errorcontextlines` starts at 0 in all four engines (tex.web § 240 does not set it; `-1` is not an iniTeX value).
- **LuaTeX math spacing**: `\Umath…spacing` (64 names) crash LuaTeX 1.17 on `\showthe` in iniTeX; they are reported as `value_class=crash` and carry no value. The command code (`set_math_param`) comes from `tex.primitives()` and is reliable.
- **List- and page-owned quantities** (`\prevdepth`, `\spacefactor`, `\prevgraf`, `\deadcycles`, the page dimens, `\insertpenalties`) answer "Improper …"/no value from the probe's vertical-mode context, so their `value_class` is `nonvalue`; satex models them through `builtins::owned`. They appear as kind mismatches above for that reason only.
- **Aliases**: XeTeX's `\XeTeX…math…` names print as `\U…`. A static table should record the print name separately from the lookup name.
- **Date-dependent**: `\year`, `\month`, `\day`, `\time` in the TSV hold the date of this run.

## Reproducing

Candidate names: `strings` of the four binaries ∪ satex names ∪ `tex.primitives()`. Defined-name filter: `\ifx\name\undefined` in `-ini`. Probe per name: `\def\sh{\showthe\name 0 \relax}`, `\expandafter\ifx\noexpand\name\name` (expandability), `\meaning`, one process per name; LaTeX columns use `-fmt=` on `pdflatex.ini`/`xelatex.ini`/`lualatex.ini` formats built with `-ini -etex`, then `\documentclass{article}\begin{document}`. satex's side: a scratch crate calling `satex::builtins::initial_meanings(&mut Interner, Engine)` for each `Engine::ALL` and `builtins::limit(..)`/`owned(..)` for initial values.


## How satex's tables follow these facts

`tests/it/primitives.rs` reads the TSV files: every engine registers the measured names (`every_engine_registers_the_measured_primitives`), parameters have the measured value class (`parameter_kinds_match_the_measured_value_classes`) and start at the measured iniTeX value (`parameters_start_where_the_measured_engines_start_them`). What satex deliberately keeps apart from the TeX Live 2023 measurement:

- **Newer than TeX Live 2023** (satex targets current engines): `\ignoreprimitiveerror` (pdfTeX, XeTeX, LuaTeX) and `\pdfptexuseunderscore` (pdfTeX), new in TeX Live 2025; LuaTeX's `\mathemptydisplaymode`, new in TeX Live 2025; LuaTeX's `\mathdefaultsmode`, writable and 0 in LuaTeX 1.17, a fixed 1 in the newer LuaTeX `builtins::limit` was measured on.
- **Probe artifacts, not mismatches**: `\setlanguage` outside LuaTeX ("You can't use `\setlanguage' after \the"; its `assign_int` is LuaTeX's), `\spacefactor` (owned by the current list), `\catcodetable` (a Lua op in satex that reads back as the table in force, initially 0).
- **Set by the run, not by iniTeX**: `\pdfoutput` and `\outputmode` come from the output plugin.
- **Alias**: `\pdfoptionpdfminorversion` is `\pdfminorversion` under a second name; satex leaves it unmodeled rather than keep a second value.
