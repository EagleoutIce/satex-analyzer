<!-- generated from doc/src/wiki/primitive-coverage.md.in by satex-doc; do not edit -->

<p align="center"><a href="https://eagleoutice.github.io/satex-analyzer/"><img src="https://raw.githubusercontent.com/EagleoutIce/satex-analyzer/main/doc/wiki/img/satex-wide.svg" alt="SaTeX: Static Analysis for (La)TeX and Friends" width="600"></a></p>

# Primitive coverage

[tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) parts 24-49 define TeX82's primitives. The [eTeX manual](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf), the [pdfTeX manual](https://mirror.ctan.org/systems/doc/pdftex/manual/pdftex-a.pdf), the [XeTeX reference](https://tug.org/xetex/) and the [LuaTeX manual](https://mirror.ctan.org/systems/doc/luatex/base/luatex.pdf) add the rest.

`src/builtins.rs` maps every one of these names to a `Primitive`, and `builtins::verdict` judges each `Primitive` from what `src/exec.rs`, `src/mode.rs` and `src/commands.rs` do with it. This page is generated from both, so it cannot drift from the interpreter. Every **modeled as** cell links to the arm of `builtins::verdict` that judges the primitive, built from `file!()` and `line!()` at that arm rather than from a table typed by hand. See [Domains](Domains) for the values a primitive's operands and results are tracked as, and [Lints](Lints) and [Chktex-Coverage](Chktex-Coverage) for what a lint rule builds on top of a primitive's real effect.

Every primitive gets one of three verdicts:

- **exact** — satex reproduces the engine's behaviour.
- **approximated** — a sound over-approximation. The table says how.
- **unsupported + diagnostic** — not modelled, but everything it could affect is widened and the run says so.

Five answers in the **how** column repeat over many rows. They are written out once here and abbreviated in the table:

- **widens the mode** — `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else.
- **the stored parameter table** — the value is read and written from the parameter table satex stores.
- **scanned, not built** — the operand is scanned exactly. The effect is typesetting or PDF output satex does not build, so nothing reads the result back.
- **scanned, answer unknown** — operands are scanned per the manual. The answer needs state satex does not build (regex captures, PDF pages, the page builder's marks, a clock, an RNG), so it widens to Unknown.
- **the font's real tables** — read from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved.

A primitive that scans its operands and drops their effect with neither of the last two is a bug, not a fourth verdict. satex never rests at "unsupported" silently ([Domains § Soundness invariant](Domains#soundness-invariant)). Any found this way are fixed, not documented as sound, and listed separately below until they are.

### TeX82 & eTeX

Source: [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) parts 24-49. eTeX is in every format satex analyzes (`Engine::has_etex`), so its primitives are listed with TeX82's, not apart.

| primitive | modeled as | verdict | how |
| --- | --- | --- | --- |
| `\ ` (control space) | [`Mode(Horizontal(Space))`][builtins-2941] | exact | a real item, appended to the list satex tracks |
| `\-` | [`Mode(Horizontal(Hyphen))`][builtins-2948] | approximated | widens to Unknown. A rarely-taken [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) path this pass does not distinguish further |
| `\/` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\TeXXeTstate` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\above` | [`Command(Dimen)`][builtins-2869] | approximated | scanned, not built |
| `\abovedisplayshortskip` | [`GlueParameter { mu: false }`][builtins-2676] | exact | the stored parameter table |
| `\abovedisplayskip` | [`GlueParameter { mu: false }`][builtins-2676] | exact | the stored parameter table |
| `\abovewithdelims` | [`Command(Delimiters { dimen: true })`][builtins-2869] | approximated | scanned, not built |
| `\accent` | [`Accent`][builtins-2716] | approximated | widens to Unknown when the accented character does not resolve in horizontal mode |
| `\adjdemerits` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\advance` | [`Arith(Advance)`][builtins-2671] | approximated | exact for known operands. Interval arithmetic and [tex.web § 222](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s zero default otherwise |
| `\afterassignment` | [`Defer(Assignment)`][builtins-2657] | exact | [tex.web §§ 1211, 1](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) (semi-simple groups, `\aftergroup`/`\afterassignment`) |
| `\aftergroup` | [`Defer(Group)`][builtins-2657] | exact | [tex.web §§ 1211, 1](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) (semi-simple groups, `\aftergroup`/`\afterassignment`) |
| `\atop` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\atopwithdelims` | [`Command(Delimiters { dimen: false })`][builtins-2869] | approximated | scanned, not built |
| `\badness` | [`LastItem(Badness)`][builtins-2796] | approximated | always Unknown. Badness needs the line-breaking satex does not run |
| `\baselineskip` | [`GlueParameter { mu: false }`][builtins-2676] | exact | the stored parameter table |
| `\batchmode` | [`Command(Interaction(0))`][builtins-2845] | exact | sets `\interactionmode` |
| `\beginL` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\beginR` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\begingroup` | [`BeginGroup`][builtins-2657] | exact | [tex.web §§ 1211, 1](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) (semi-simple groups, `\aftergroup`/`\afterassignment`) |
| `\belowdisplayshortskip` | [`GlueParameter { mu: false }`][builtins-2676] | exact | the stored parameter table |
| `\belowdisplayskip` | [`GlueParameter { mu: false }`][builtins-2676] | exact | the stored parameter table |
| `\binoppenalty` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\botmark` | [`Convert(Mark)`][builtins-2898] | approximated | scanned, answer unknown |
| `\botmarks` | [`Convert(Marks)`][builtins-2898] | approximated | scanned, answer unknown |
| `\box` | [`Register(Box)`][builtins-2662] | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\boxmaxdepth` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\brokenpenalty` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\catcode` | [`CatcodeAssign`][builtins-2682] | unsupported + diagnostic | exact for one known character and category. Diagnosed (`unknown-catcode`) and widened when either can't be pinned down |
| `\char` | [`Mode(Horizontal(Char))`][builtins-2941] | exact | a real item, appended to the list satex tracks |
| `\chardef` | [`RegisterDef(Char)`][builtins-2662] | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\cleaders` | [`Mode(Leaders)`][builtins-2915] | approximated | the box or rule is scanned exactly. [tex.web § 1078](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s repeated copies are not laid out, since satex tracks natural size, not a page |
| `\closein` | [`CloseIn`][builtins-2667] | approximated | [tex.web § 1351](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web). Exact for a known stream number; one satex cannot pin down leaves every stream's state unknown |
| `\closeout` | [`Command(CloseOut)`][builtins-2849] | approximated | tracks the stream state a later `\input` of the written file reads back; a stream number satex cannot pin down widens every stream |
| `\clubpenalties` | [`Special(Penalties)`][builtins-2833] | approximated | a table satex stores and reads back exactly for a known entry; one it cannot pin down widens every entry the write may have reached |
| `\clubpenalty` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\copy` | [`Mode(Box(Copy))`][builtins-2961] | exact | the real box machinery |
| `\count` | [`Register(Count)`][builtins-2662] | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\countdef` | [`RegisterDef(Count)`][builtins-2662] | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\cr` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\crcr` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\csname` | [`Csname`][builtins-2639] | approximated | exact once the name is resolved. A bounded join of candidate meanings when it is built from unknown digits (`one_of_names`) |
| `\currentgrouplevel` | [`LastItem(GroupLevel)`][builtins-2798] | exact | read from state satex already tracks |
| `\currentgrouptype` | [`LastItem(GroupType)`][builtins-2798] | exact | read from state satex already tracks |
| `\currentifbranch` | [`LastItem(IfBranch)`][builtins-2801] | approximated | exact once the conditional stack has settled. Unknown while a branch is undecided |
| `\currentiflevel` | [`LastItem(IfLevel)`][builtins-2801] | approximated | exact once the conditional stack has settled. Unknown while a branch is undecided |
| `\currentiftype` | [`LastItem(IfType)`][builtins-2801] | approximated | exact once the conditional stack has settled. Unknown while a branch is undecided |
| `\day` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\deadcycles` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\def` | [`Def { global: false, expand: false }`][builtins-2630] | exact | [tex.web § 476](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web). An unterminated definition is diagnosed, not read past the file's end |
| `\defaulthyphenchar` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\defaultskewchar` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\delcode` | [`CharCode(Delimiter)`][builtins-2686] | unsupported + diagnostic | exact for one known character. Diagnosed (`unknown-char-code`) and the whole table widened when the candidates can't be enumerated at all |
| `\delimiter` | [`Command(Number)`][builtins-2869] | approximated | scanned, not built |
| `\delimiterfactor` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\delimitershortfall` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\detokenize` | [`Text(Detokenize)`][builtins-2736] | approximated | exact for a known operand. Widens when the token's meaning or a register's digits are unresolved |
| `\dimen` | [`Register(Dimen)`][builtins-2662] | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\dimendef` | [`RegisterDef(Dimen)`][builtins-2662] | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\dimexpr` | [`Expr(Dimen)`][builtins-2709] | exact | [eTeX manual § 3.5](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf) |
| `\discretionary` | [`Mode(Horizontal(Discretionary))`][builtins-2941] | exact | a real item, appended to the list satex tracks |
| `\displayindent` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\displaylimits` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\displaystyle` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\displaywidowpenalties` | [`Special(Penalties)`][builtins-2833] | approximated | a table satex stores and reads back exactly for a known entry; one it cannot pin down widens every entry the write may have reached |
| `\displaywidowpenalty` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\displaywidth` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\divide` | [`Arith(Divide)`][builtins-2671] | approximated | exact for known operands. Interval arithmetic and [tex.web § 222](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s zero default otherwise |
| `\doublehyphendemerits` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\dp` | [`Special(BoxDimen)`][builtins-2831] | approximated | round-trips through the register consistently. The box behind it is never built |
| `\dump` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\eTeXrevision` | [`Convert(EtexRevision)`][builtins-2878] | exact | a real constant or stored name |
| `\eTeXversion` | [`LastItem(EtexVersion)`][builtins-2798] | exact | read from state satex already tracks |
| `\edef` | [`Def { global: false, expand: true }`][builtins-2630] | exact | [tex.web § 476](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web). An unterminated definition is diagnosed, not read past the file's end |
| `\else` | [`Else`][builtins-2650] | exact | [tex.web § 510](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web), including `insert_relax` for an else or fi met mid-test |
| `\emergencystretch` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\end` | [`End`][builtins-2703] | exact | [tex.web § 1054](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) (`end_job`) |
| `\endL` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\endR` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\endcsname` | [`Endcsname`][builtins-2643] | exact | [LuaTeX manual, "Macros"](https://ctan.org/pkg/luatex) |
| `\endgroup` | [`EndGroup`][builtins-2657] | exact | [tex.web §§ 1211, 1](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) (semi-simple groups, `\aftergroup`/`\afterassignment`) |
| `\endinput` | [`Endinput`][builtins-2699] | unsupported + diagnostic | exact when unconditional. Diagnosed (`conditional-endinput`) and the file read on when reached under an undecided conditional |
| `\endlinechar` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\eqno` | [`Mode(EqNo)`][builtins-2907] | exact | [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own mode-transition rules |
| `\errhelp` | [`TokensParameter`][builtins-2678] | approximated | the stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\errmessage` | [`Message { error: true }`][builtins-2704] | exact | [tex.web § 1279](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) |
| `\errorcontextlines` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\errorstopmode` | [`Command(Interaction(3))`][builtins-2845] | exact | sets `\interactionmode` |
| `\escapechar` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\everycr` | [`TokensParameter`][builtins-2678] | approximated | the stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\everydisplay` | [`TokensParameter`][builtins-2678] | approximated | the stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\everyeof` | [`TokensParameter`][builtins-2678] | approximated | the stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\everyhbox` | [`TokensParameter`][builtins-2678] | approximated | the stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\everyjob` | [`TokensParameter`][builtins-2678] | approximated | the stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\everymath` | [`TokensParameter`][builtins-2678] | approximated | the stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\everypar` | [`TokensParameter`][builtins-2678] | approximated | the stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\everyvbox` | [`TokensParameter`][builtins-2678] | approximated | the stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\exhyphenpenalty` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\expandafter` | [`ExpandAfter`][builtins-2634] | exact | [tex.web § 367](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) |
| `\fam` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\fi` | [`Fi`][builtins-2650] | exact | [tex.web § 510](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web), including `insert_relax` for an else or fi met mid-test |
| `\finalhyphendemerits` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\firstmark` | [`Convert(Mark)`][builtins-2898] | approximated | scanned, answer unknown |
| `\firstmarks` | [`Convert(Marks)`][builtins-2898] | approximated | scanned, answer unknown |
| `\floatingpenalty` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\font` | [`FontDef`][builtins-2666] | exact | [tex.web § 1257](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) (`\font`) |
| `\fontchardp` | [`LastItem(FontCharDp)`][builtins-2807] | approximated | exact for a TFM font ([etex_man § 3.10](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf)). A native font's height, depth and italic correction are not read, so it widens to Unknown |
| `\fontcharht` | [`LastItem(FontCharHt)`][builtins-2807] | approximated | exact for a TFM font ([etex_man § 3.10](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf)). A native font's height, depth and italic correction are not read, so it widens to Unknown |
| `\fontcharic` | [`LastItem(FontCharIc)`][builtins-2807] | approximated | exact for a TFM font ([etex_man § 3.10](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf)). A native font's height, depth and italic correction are not read, so it widens to Unknown |
| `\fontcharwd` | [`LastItem(FontCharWd)`][builtins-2806] | exact | a TFM's or a native font's real glyph advance |
| `\fontdimen` | [`FontParam(Dimen)`][builtins-2691] | approximated | reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\fontname` | [`Convert(FontName)`][builtins-2878] | exact | a real constant or stored name |
| `\futurelet` | [`Let { global: false, future: true }`][builtins-2632] | exact | [tex.web § 1221](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web), including the two-token `\futurelet` lookahead |
| `\gdef` | [`Def { global: true, expand: false }`][builtins-2630] | exact | [tex.web § 476](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web). An unterminated definition is diagnosed, not read past the file's end |
| `\global` | [`Prefix(Global)`][builtins-2633] | exact | [tex.web § 1211](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) |
| `\globaldefs` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\glueexpr` | [`Expr(Skip)`][builtins-2709] | exact | [eTeX manual § 3.5](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf) |
| `\glueshrink` | [`LastItem(GlueShrink)`][builtins-2804] | exact | computed from a known glue spec |
| `\glueshrinkorder` | [`LastItem(GlueShrinkOrder)`][builtins-2804] | exact | computed from a known glue spec |
| `\gluestretch` | [`LastItem(GlueStretch)`][builtins-2804] | exact | computed from a known glue spec |
| `\gluestretchorder` | [`LastItem(GlueStretchOrder)`][builtins-2804] | exact | computed from a known glue spec |
| `\gluetomu` | [`LastItem(GlueToMu)`][builtins-2804] | exact | computed from a known glue spec |
| `\halign` | [`Mode(Vertical(Align))`][builtins-2953] | approximated | cells run in the alignment's own list, not distinguished from the preamble |
| `\hangafter` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\hangindent` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\hbadness` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\hbox` | [`Mode(Box(HBox))`][builtins-2961] | exact | the real box machinery |
| `\hfil` | [`Mode(Horizontal(Fil))`][builtins-2941] | exact | a real item, appended to the list satex tracks |
| `\hfill` | [`Mode(Horizontal(Fil))`][builtins-2941] | exact | a real item, appended to the list satex tracks |
| `\hfilneg` | [`Mode(Horizontal(Fil))`][builtins-2941] | exact | a real item, appended to the list satex tracks |
| `\hfuzz` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\hoffset` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\holdinginserts` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\hrule` | [`Mode(Vertical(Rule))`][builtins-2941] | exact | a real item, appended to the list satex tracks |
| `\hsize` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\hskip` | [`Mode(Horizontal(Glue))`][builtins-2941] | exact | a real item, appended to the list satex tracks |
| `\hss` | [`Mode(Horizontal(Fil))`][builtins-2941] | exact | a real item, appended to the list satex tracks |
| `\ht` | [`Special(BoxDimen)`][builtins-2831] | approximated | round-trips through the register consistently. The box behind it is never built |
| `\hyphenation` | [`Command(GeneralText)`][builtins-2869] | approximated | scanned, not built |
| `\hyphenchar` | [`FontParam(HyphenChar)`][builtins-2691] | approximated | reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\hyphenpenalty` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\if` | [`If(Chars)`][builtins-2760] | approximated | exact for known characters. Undecided when either operand is an unresolved character |
| `\ifcase` | [`If(Case)`][builtins-2763] | approximated | [tex.web § 509](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web). Every arm is analyzed as its own path when the selector is unknown |
| `\ifcat` | [`If(Catcodes)`][builtins-2760] | approximated | exact for known characters. Undecided when either operand is an unresolved character |
| `\ifcsname` | [`If(Csname)`][builtins-2755] | exact | builds the name and looks it up |
| `\ifdefined` | [`If(Defined)`][builtins-2753] | approximated | exact for a known name. Narrowed by which names could still be undefined otherwise |
| `\ifdim` | [`If(Dim)`][builtins-2757] | approximated | exact for known operands. Narrows the interval on each branch otherwise |
| `\ifeof` | [`If(Eof)`][builtins-2765] | approximated | satex resolves the real file, so end-of-file is known for a known stream number; one it cannot pin down leaves the test undecided |
| `\iffalse` | [`If(False)`][builtins-2747] | exact | a constant |
| `\iffontchar` | [`If(FontChar)`][builtins-2781] | exact | [eTeX manual § 3.10](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf): read from the font's real character table |
| `\ifhbox` | [`If(Box)`][builtins-2774] | approximated | exact for a known box state (void, `\hbox`, `\vbox`). Undecided once the register is Unknown |
| `\ifhmode` | [`If(HMode)`][builtins-2770] | approximated | decided from the mode satex tracks through the run. Open groups the run couldn't follow leave it undecided |
| `\ifinner` | [`If(InnerMode)`][builtins-2770] | approximated | decided from the mode satex tracks through the run. Open groups the run couldn't follow leave it undecided |
| `\ifmmode` | [`If(MathMode)`][builtins-2770] | approximated | decided from the mode satex tracks through the run. Open groups the run couldn't follow leave it undecided |
| `\ifnum` | [`If(Num)`][builtins-2757] | approximated | exact for known operands. Narrows the interval on each branch otherwise |
| `\ifodd` | [`If(Odd)`][builtins-2757] | approximated | exact for known operands. Narrows the interval on each branch otherwise |
| `\iftrue` | [`If(True)`][builtins-2747] | exact | a constant |
| `\ifvbox` | [`If(Box)`][builtins-2774] | approximated | exact for a known box state (void, `\hbox`, `\vbox`). Undecided once the register is Unknown |
| `\ifvmode` | [`If(VMode)`][builtins-2770] | approximated | decided from the mode satex tracks through the run. Open groups the run couldn't follow leave it undecided |
| `\ifvoid` | [`If(Box)`][builtins-2774] | approximated | exact for a known box state (void, `\hbox`, `\vbox`). Undecided once the register is Unknown |
| `\ifx` | [`If(IfX)`][builtins-2748] | approximated | exact for known meanings ([tex.web § 507](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s `original` handles a redefined primitive). Member-by-member across a set of possible meanings otherwise, decided only when they agree |
| `\ignorespaces` | [`IgnoreSpaces`][builtins-2711] | exact | [tex.web § 1045](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) |
| `\immediate` | [`Prefix(Immediate)`][builtins-2633] | exact | [tex.web § 1211](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) |
| `\indent` | [`Mode(StartPar { indent: true })`][builtins-2907] | exact | [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own mode-transition rules |
| `\input` | [`Load(Input)`][builtins-2698] | exact | [tex.web § 537](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) (`\input`) |
| `\inputlineno` | [`LastItem(InputLineNo)`][builtins-2798] | exact | read from state satex already tracks |
| `\insert` | [`Mode(Insert { vadjust: false })`][builtins-2921] | approximated | opens the insert's own list. Not told apart from the vertical list around it |
| `\insertpenalties` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\interactionmode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\interlinepenalties` | [`Special(Penalties)`][builtins-2833] | approximated | a table satex stores and reads back exactly for a known entry; one it cannot pin down widens every entry the write may have reached |
| `\interlinepenalty` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\jobname` | [`Text(JobName)`][builtins-2735] | exact | the run's real main file name |
| `\kern` | [`Typeset(Dimen)`][builtins-2975] | approximated | `\kern` is exact. `\mkern`'s math-unit dimension always widens to Unknown, since it needs math-font setup satex does not run |
| `\language` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\lastbox` | [`Mode(Box(LastBox))`][builtins-2961] | exact | the real box machinery |
| `\lastkern` | [`LastItem(Kern)`][builtins-2790] | approximated | reads the tail of the list satex tracks. Unknown once the tail itself is |
| `\lastlinefit` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\lastnodetype` | [`LastItem(NodeType)`][builtins-2792] | approximated | reads the tail of the list: a character, a ligature, glue, a kern or a penalty, and -1 for the empty main list (etex.ch). Unknown otherwise |
| `\lastpenalty` | [`LastItem(Penalty)`][builtins-2790] | approximated | reads the tail of the list satex tracks. Unknown once the tail itself is |
| `\lastskip` | [`LastItem(Skip)`][builtins-2790] | approximated | reads the tail of the list satex tracks. Unknown once the tail itself is |
| `\lccode` | [`CharCode(Lower)`][builtins-2686] | unsupported + diagnostic | exact for one known character. Diagnosed (`unknown-char-code`) and the whole table widened when the candidates can't be enumerated at all |
| `\leaders` | [`Mode(Leaders)`][builtins-2915] | approximated | the box or rule is scanned exactly. [tex.web § 1078](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s repeated copies are not laid out, since satex tracks natural size, not a page |
| `\left` | [`Mode(Left)`][builtins-2928] | approximated | documented: satex keeps no group for it, so the display may read as inner from here on |
| `\lefthyphenmin` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\leftskip` | [`GlueParameter { mu: false }`][builtins-2676] | exact | the stored parameter table |
| `\leqno` | [`Mode(EqNo)`][builtins-2907] | exact | [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own mode-transition rules |
| `\let` | [`Let { global: false, future: false }`][builtins-2632] | exact | [tex.web § 1221](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web), including the two-token `\futurelet` lookahead |
| `\limits` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\linepenalty` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\lineskip` | [`GlueParameter { mu: false }`][builtins-2676] | exact | the stored parameter table |
| `\lineskiplimit` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\long` | [`Prefix(Long)`][builtins-2633] | exact | [tex.web § 1211](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) |
| `\looseness` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\lower` | [`Mode(Shift { vertical: true })`][builtins-2907] | exact | [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own mode-transition rules |
| `\lowercase` | [`CaseShift(Lower)`][builtins-2690] | exact | [tex.web § 1288](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web). Widens a code an unknown token was assigned |
| `\mag` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\mark` | [`Command(GeneralText)`][builtins-2869] | approximated | scanned, not built |
| `\marks` | [`Command(NumberText)`][builtins-2869] | approximated | scanned, not built |
| `\mathaccent` | [`Command(Number)`][builtins-2869] | approximated | scanned, not built |
| `\mathbin` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\mathchar` | [`Command(Number)`][builtins-2869] | approximated | scanned, not built |
| `\mathchardef` | [`RegisterDef(MathChar)`][builtins-2662] | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\mathchoice` | [`Mode(MathChoice)`][builtins-2926] | exact | [tex.web § 1172](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web): opens all four styles in sequence |
| `\mathclose` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\mathcode` | [`CharCode(Math)`][builtins-2686] | unsupported + diagnostic | exact for one known character. Diagnosed (`unknown-char-code`) and the whole table widened when the candidates can't be enumerated at all |
| `\mathinner` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\mathop` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\mathopen` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\mathord` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\mathpunct` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\mathrel` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\mathsurround` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\maxdeadcycles` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\maxdepth` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\meaning` | [`Text(Meaning)`][builtins-2736] | approximated | exact for a known operand. Widens when the token's meaning or a register's digits are unresolved |
| `\medmuskip` | [`GlueParameter { mu: true }`][builtins-2676] | exact | the stored parameter table |
| `\message` | [`Message { error: false }`][builtins-2704] | exact | [tex.web § 1279](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) |
| `\middle` | [`Typeset(Delimiter)`][builtins-2979] | exact | [tex.web § 1161](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web): the delimiter is the one token the primitive itself consumes |
| `\mkern` | [`Typeset(Dimen)`][builtins-2975] | approximated | `\kern` is exact. `\mkern`'s math-unit dimension always widens to Unknown, since it needs math-font setup satex does not run |
| `\month` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\moveleft` | [`Mode(Shift { vertical: false })`][builtins-2907] | exact | [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own mode-transition rules |
| `\moveright` | [`Mode(Shift { vertical: false })`][builtins-2907] | exact | [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own mode-transition rules |
| `\mskip` | [`Typeset(Glue)`][builtins-2980] | approximated | `\mskip` appends an item of unknown size: its glue is in math units, which need math-font setup satex does not run |
| `\muexpr` | [`Expr(MuSkip)`][builtins-2709] | exact | [eTeX manual § 3.5](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf) |
| `\multiply` | [`Arith(Multiply)`][builtins-2671] | approximated | exact for known operands. Interval arithmetic and [tex.web § 222](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s zero default otherwise |
| `\muskip` | [`Register(MuSkip)`][builtins-2662] | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\muskipdef` | [`RegisterDef(MuSkip)`][builtins-2662] | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\mutoglue` | [`LastItem(MuToGlue)`][builtins-2804] | exact | computed from a known glue spec |
| `\newlinechar` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\noalign` | [`Mode(NoAlign)`][builtins-2924] | approximated | cells run in the alignment's own list, not distinguished from the preamble |
| `\noboundary` | [`Mode(Horizontal(NoBoundary))`][builtins-2941] | exact | a real item, appended to the list satex tracks |
| `\noexpand` | [`NoExpand`][builtins-2635] | exact | [tex.web §§ 367, 358](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) |
| `\noindent` | [`Mode(StartPar { indent: false })`][builtins-2907] | exact | [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own mode-transition rules |
| `\nolimits` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\nonscript` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\nonstopmode` | [`Command(Interaction(1))`][builtins-2845] | exact | sets `\interactionmode` |
| `\nulldelimiterspace` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\nullfont` | [`FontIdent(0)`][builtins-2695] | exact | [tex.web § 1217](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web): selecting a font is a local assignment |
| `\number` | [`Text(Number)`][builtins-2736] | approximated | exact for a known operand. Widens when the token's meaning or a register's digits are unresolved |
| `\numexpr` | [`Expr(Count)`][builtins-2709] | exact | [eTeX manual § 3.5](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf) |
| `\omit` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\openin` | [`OpenIn`][builtins-2667] | approximated | [tex.web § 1351](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web). Exact for a known stream number; one satex cannot pin down leaves every stream's state unknown |
| `\openout` | [`Command(OpenOut)`][builtins-2849] | approximated | tracks the stream state a later `\input` of the written file reads back; a stream number satex cannot pin down widens every stream |
| `\or` | [`Or`][builtins-2650] | exact | [tex.web § 510](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web), including `insert_relax` for an else or fi met mid-test |
| `\outer` | [`Prefix(Outer)`][builtins-2633] | exact | [tex.web § 1211](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) |
| `\output` | [`TokensParameter`][builtins-2678] | approximated | the stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\outputpenalty` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\over` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\overfullrule` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\overline` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\overwithdelims` | [`Command(Delimiters { dimen: false })`][builtins-2869] | approximated | scanned, not built |
| `\pagedepth` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pagediscards` | [`Mode(Vertical(Discards))`][builtins-2948] | approximated | widens to Unknown. A rarely-taken [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) path this pass does not distinguish further |
| `\pagefilllstretch` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pagefillstretch` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pagefilstretch` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pagegoal` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pageshrink` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pagestretch` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pagetotal` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\par` | [`Mode(Par)`][builtins-2907] | exact | [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own mode-transition rules |
| `\parfillskip` | [`GlueParameter { mu: false }`][builtins-2676] | exact | the stored parameter table |
| `\parindent` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\parshape` | [`Special(ParShape)`][builtins-2833] | approximated | a table satex stores and reads back exactly for a known entry; one it cannot pin down widens every entry the write may have reached |
| `\parshapedimen` | [`LastItem(ParShapeDimen)`][builtins-2811] | exact | read from the stored `\parshape` |
| `\parshapeindent` | [`LastItem(ParShapeIndent)`][builtins-2811] | exact | read from the stored `\parshape` |
| `\parshapelength` | [`LastItem(ParShapeLength)`][builtins-2811] | exact | read from the stored `\parshape` |
| `\parskip` | [`GlueParameter { mu: false }`][builtins-2676] | exact | the stored parameter table |
| `\patterns` | [`Command(GeneralText)`][builtins-2869] | approximated | scanned, not built |
| `\pausing` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\penalty` | [`Typeset(Number)`][builtins-2974] | exact | the penalty is appended as scanned |
| `\postdisplaypenalty` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\predisplaydirection` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\predisplaypenalty` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\predisplaysize` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pretolerance` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\prevdepth` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\prevgraf` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\protected` | [`Prefix(Protected)`][builtins-2633] | exact | [tex.web § 1211](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) |
| `\radical` | [`Command(Number)`][builtins-2869] | approximated | scanned, not built |
| `\raise` | [`Mode(Shift { vertical: true })`][builtins-2907] | exact | [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own mode-transition rules |
| `\read` | [`Read`][builtins-2667] | approximated | [tex.web § 1351](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web). Exact for a known stream number; one satex cannot pin down leaves every stream's state unknown |
| `\readline` | [`Command(ReadLine)`][builtins-2857] | approximated | [eTeX manual § 3.2](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf), including the terminal-fatal case; a stream number satex cannot pin down gives an unknown line and does not end the job |
| `\relax` | [`Relax`][builtins-2710] | exact | [tex.web § 1211](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) |
| `\relpenalty` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\right` | [`Typeset(Delimiter)`][builtins-2979] | exact | [tex.web § 1161](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web): the delimiter is the one token the primitive itself consumes |
| `\righthyphenmin` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\rightskip` | [`GlueParameter { mu: false }`][builtins-2676] | exact | the stored parameter table |
| `\romannumeral` | [`Text(Roman)`][builtins-2736] | approximated | exact for a known operand. Widens when the token's meaning or a register's digits are unresolved |
| `\savinghyphcodes` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\savingvdiscards` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\scantokens` | [`ScanTokens { text: false }`][builtins-2646] | exact | [eTeX manual § 3.9](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf). A capacity overflow (`MAX_IN_OPEN`) is diagnosed, not assumed away |
| `\scriptfont` | [`Command(FamilyFont)`][builtins-2853] | approximated | stored and consumed by satex's own font and math-code tables; an operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\scriptscriptfont` | [`Command(FamilyFont)`][builtins-2853] | approximated | stored and consumed by satex's own font and math-code tables; an operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\scriptscriptstyle` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\scriptspace` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\scriptstyle` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\scrollmode` | [`Command(Interaction(2))`][builtins-2845] | exact | sets `\interactionmode` |
| `\setbox` | [`Command(SetBox)`][builtins-2848] | exact | forwarded to the real box-scanning path |
| `\setlanguage` | [`Command(Number)`][builtins-2869] | approximated | scanned, not built |
| `\sfcode` | [`CharCode(Space)`][builtins-2686] | unsupported + diagnostic | exact for one known character. Diagnosed (`unknown-char-code`) and the whole table widened when the candidates can't be enumerated at all |
| `\shipout` | [`Mode(ShipOut)`][builtins-2919] | exact | removes the box from the list, [tex.web § 1073](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own effect on it |
| `\show` | [`Command(Token)`][builtins-2846] | exact | `\show` |
| `\showbox` | [`Command(Number)`][builtins-2869] | approximated | scanned, not built |
| `\showboxbreadth` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\showboxdepth` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\showgroups` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\showifs` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\showlists` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\showthe` | [`Command(Internal)`][builtins-2847] | exact | `\showthe` |
| `\showtokens` | [`Command(GeneralText)`][builtins-2869] | approximated | scanned, not built |
| `\skewchar` | [`FontParam(SkewChar)`][builtins-2691] | approximated | reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\skip` | [`Register(Skip)`][builtins-2662] | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\skipdef` | [`RegisterDef(Skip)`][builtins-2662] | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\spacefactor` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\spaceskip` | [`GlueParameter { mu: false }`][builtins-2676] | exact | the stored parameter table |
| `\span` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\special` | [`Pdf(Special)`][builtins-2718] | approximated | the manual's keywords are scanned exactly. No PDF is built, so the payload is recorded as an occurrence, not replayed |
| `\splitbotmark` | [`Convert(Mark)`][builtins-2898] | approximated | scanned, answer unknown |
| `\splitbotmarks` | [`Convert(Marks)`][builtins-2898] | approximated | scanned, answer unknown |
| `\splitdiscards` | [`Mode(Vertical(Discards))`][builtins-2948] | approximated | widens to Unknown. A rarely-taken [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) path this pass does not distinguish further |
| `\splitfirstmark` | [`Convert(Mark)`][builtins-2898] | approximated | scanned, answer unknown |
| `\splitfirstmarks` | [`Convert(Marks)`][builtins-2898] | approximated | scanned, answer unknown |
| `\splitmaxdepth` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\splittopskip` | [`GlueParameter { mu: false }`][builtins-2676] | exact | the stored parameter table |
| `\string` | [`Text(String)`][builtins-2736] | approximated | exact for a known operand. Widens when the token's meaning or a register's digits are unresolved |
| `\tabskip` | [`GlueParameter { mu: false }`][builtins-2676] | exact | the stored parameter table |
| `\textfont` | [`Command(FamilyFont)`][builtins-2853] | approximated | stored and consumed by satex's own font and math-code tables; an operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\textstyle` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\the` | [`Text(The)`][builtins-2736] | approximated | exact for a known operand. Widens when the token's meaning or a register's digits are unresolved |
| `\thickmuskip` | [`GlueParameter { mu: true }`][builtins-2676] | exact | the stored parameter table |
| `\thinmuskip` | [`GlueParameter { mu: true }`][builtins-2676] | exact | the stored parameter table |
| `\time` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\toks` | [`Register(Toks)`][builtins-2662] | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\toksdef` | [`RegisterDef(Toks)`][builtins-2662] | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\tolerance` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\topmark` | [`Convert(Mark)`][builtins-2898] | approximated | scanned, answer unknown |
| `\topmarks` | [`Convert(Marks)`][builtins-2898] | approximated | scanned, answer unknown |
| `\topskip` | [`GlueParameter { mu: false }`][builtins-2676] | exact | the stored parameter table |
| `\tracingassigns` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\tracingcommands` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\tracinggroups` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\tracingifs` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\tracinglostchars` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\tracingmacros` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\tracingnesting` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\tracingonline` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\tracingoutput` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\tracingpages` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\tracingparagraphs` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\tracingrestores` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\tracingscantokens` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\tracingstacklevels` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\tracingstats` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\uccode` | [`CharCode(Upper)`][builtins-2686] | unsupported + diagnostic | exact for one known character. Diagnosed (`unknown-char-code`) and the whole table widened when the candidates can't be enumerated at all |
| `\uchyph` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\underline` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\unexpanded` | [`Reinject`][builtins-2644] | exact | [eTeX manual § 3.4](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf) |
| `\unhbox` | [`Mode(Horizontal(Unpackage { take: true }))`][builtins-2943] | approximated | a void register adds nothing. A real one becomes Unknown, since its content is not tracked once boxed |
| `\unhcopy` | [`Mode(Horizontal(Unpackage { take: false }))`][builtins-2943] | approximated | a void register adds nothing. A real one becomes Unknown, since its content is not tracked once boxed |
| `\unkern` | [`Typeset(Plain)`][builtins-2973] | exact | `\unskip`, `\unkern` and `\unpenalty` remove the real tail item |
| `\unless` | [`Unless`][builtins-2652] | approximated | etex.ch. Exact before a conditional; before a name of unknown meaning the conditional it may open is left undecided |
| `\unpenalty` | [`Typeset(Plain)`][builtins-2973] | exact | `\unskip`, `\unkern` and `\unpenalty` remove the real tail item |
| `\unskip` | [`Typeset(Plain)`][builtins-2973] | exact | `\unskip`, `\unkern` and `\unpenalty` remove the real tail item |
| `\unvbox` | [`Mode(Vertical(Unpackage { take: true }))`][builtins-2943] | approximated | a void register adds nothing. A real one becomes Unknown, since its content is not tracked once boxed |
| `\unvcopy` | [`Mode(Vertical(Unpackage { take: false }))`][builtins-2943] | approximated | a void register adds nothing. A real one becomes Unknown, since its content is not tracked once boxed |
| `\uppercase` | [`CaseShift(Upper)`][builtins-2690] | exact | [tex.web § 1288](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web). Widens a code an unknown token was assigned |
| `\vadjust` | [`Mode(Insert { vadjust: true })`][builtins-2921] | approximated | opens the insert's own list. Not told apart from the vertical list around it |
| `\valign` | [`Mode(Horizontal(Align))`][builtins-2953] | approximated | cells run in the alignment's own list, not distinguished from the preamble |
| `\vbadness` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\vbox` | [`Mode(Box(VBox))`][builtins-2961] | exact | the real box machinery |
| `\vcenter` | [`Mode(Box(VCenter))`][builtins-2961] | exact | the real box machinery |
| `\vfil` | [`Mode(Vertical(Fil))`][builtins-2941] | exact | a real item, appended to the list satex tracks |
| `\vfill` | [`Mode(Vertical(Fil))`][builtins-2941] | exact | a real item, appended to the list satex tracks |
| `\vfilneg` | [`Mode(Vertical(Fil))`][builtins-2941] | exact | a real item, appended to the list satex tracks |
| `\vfuzz` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\voffset` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\vrule` | [`Mode(Horizontal(Rule))`][builtins-2909] | approximated | a real rule item in a horizontal list, but a `\vrule` met in a vertical list starts a paragraph and widens the enclosing box's dimensions rather than answering them exactly |
| `\vsize` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\vskip` | [`Mode(Vertical(Glue))`][builtins-2941] | exact | a real item, appended to the list satex tracks |
| `\vsplit` | [`Mode(Box(VSplit))`][builtins-2962] | approximated | the result always widens to Unknown, even from a void register. A real split's remainder is not tracked |
| `\vss` | [`Mode(Vertical(Fil))`][builtins-2941] | exact | a real item, appended to the list satex tracks |
| `\vtop` | [`Mode(Box(VTop))`][builtins-2961] | exact | the real box machinery |
| `\wd` | [`Special(BoxDimen)`][builtins-2831] | approximated | round-trips through the register consistently. The box behind it is never built |
| `\widowpenalties` | [`Special(Penalties)`][builtins-2833] | approximated | a table satex stores and reads back exactly for a known entry; one it cannot pin down widens every entry the write may have reached |
| `\widowpenalty` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\write` | [`Write`][builtins-2705] | approximated | `\immediate\write` is exact. A deferred `\write`'s text is recorded unexpanded, since satex never reaches shipout to expand it |
| `\xdef` | [`Def { global: true, expand: true }`][builtins-2630] | exact | [tex.web § 476](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web). An unterminated definition is diagnosed, not read past the file's end |
| `\xleaders` | [`Mode(Leaders)`][builtins-2915] | approximated | the box or rule is scanned exactly. [tex.web § 1078](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s repeated copies are not laid out, since satex tracks natural size, not a page |
| `\xspaceskip` | [`GlueParameter { mu: false }`][builtins-2676] | exact | the stored parameter table |
| `\year` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |

### pdfTeX extensions

Source: the [pdfTeX manual, "Primitives"](https://ctan.org/pkg/pdftex).

| primitive | modeled as | verdict | how |
| --- | --- | --- | --- |
| `\efcode` | [`FontParam(CharCode(1000))`][builtins-2691] | approximated | reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\expanded` | [`Expanded`][builtins-2648] | exact | [interface3, "Selecting tokens"](https://ctan.org/pkg/l3kernel); [pdfTeX manual, "\expanded"](https://ctan.org/pkg/pdftex) |
| `\ifincsname` | [`If(InCsname)`][builtins-2779] | exact | computed from state satex already tracks (`eval_extra_condition`) |
| `\ifpdfabsdim` | [`If(AbsDim)`][builtins-2779] | exact | computed from state satex already tracks (`eval_extra_condition`) |
| `\ifpdfabsnum` | [`If(AbsNum)`][builtins-2779] | exact | computed from state satex already tracks (`eval_extra_condition`) |
| `\ifpdfprimitive` | [`If(Primitive)`][builtins-2779] | exact | computed from state satex already tracks (`eval_extra_condition`) |
| `\ignoreprimitiveerror` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\knaccode` | [`FontParam(CharCode(0))`][builtins-2691] | approximated | reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\knbccode` | [`FontParam(CharCode(0))`][builtins-2691] | approximated | reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\knbscode` | [`FontParam(CharCode(0))`][builtins-2691] | approximated | reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\leftmarginkern` | [`Convert(MarginKern)`][builtins-2898] | approximated | scanned, answer unknown |
| `\letterspacefont` | [`Command(CopyFont { amount: true })`][builtins-2853] | approximated | stored and consumed by satex's own font and math-code tables; an operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\lpcode` | [`FontParam(CharCode(0))`][builtins-2691] | approximated | reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\partokencontext` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\partokenname` | [`Command(Token)`][builtins-2846] | exact | `\show` |
| `\pdfadjustspacing` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfannot` | [`Command(PdfAnnot)`][builtins-2869] | approximated | scanned, not built |
| `\pdfappendkern` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfcatalog` | [`Command(PdfCatalog)`][builtins-2869] | approximated | scanned, not built |
| `\pdfcolorstack` | [`Command(PdfColorStack)`][builtins-2862] | approximated | the operation and stack id are tracked. The colour text itself is a recorded occurrence, not replayed |
| `\pdfcolorstackinit` | [`Convert(ColorStackInit)`][builtins-2888] | approximated | `\pdfcolorstackinit` hands out the next colour stack number, widened rather than counted exactly. The colour text is a recorded occurrence, not replayed |
| `\pdfcompresslevel` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfcopyfont` | [`Command(CopyFont { amount: false })`][builtins-2853] | approximated | stored and consumed by satex's own font and math-code tables; an operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\pdfcreationdate` | [`Convert(CreationDate)`][builtins-2898] | approximated | scanned, answer unknown |
| `\pdfdecimaldigits` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfdest` | [`Command(PdfDest)`][builtins-2869] | approximated | scanned, not built |
| `\pdfdestmargin` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfdraftmode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfeachlinedepth` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfeachlineheight` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfelapsedtime` | [`LastItem(ElapsedTime)`][builtins-2813] | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdfendlink` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\pdfendthread` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\pdfescapehex` | [`Convert(EscapeHex)`][builtins-2881] | exact | the manual's own algorithm, run for real |
| `\pdfescapename` | [`Convert(EscapeName)`][builtins-2881] | exact | the manual's own algorithm, run for real |
| `\pdfescapestring` | [`Convert(EscapeString)`][builtins-2881] | exact | the manual's own algorithm, run for real |
| `\pdffakespace` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\pdffiledump` | [`Convert(FileDump)`][builtins-2883] | exact | reads the real file |
| `\pdffilemoddate` | [`Convert(FileModDate)`][builtins-2884] | approximated | always Unknown. satex does not read file modification times |
| `\pdffilesize` | [`Convert(FileSize)`][builtins-2883] | exact | reads the real file |
| `\pdffirstlineheight` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdffontattr` | [`Command(FontText)`][builtins-2869] | approximated | scanned, not built |
| `\pdffontexpand` | [`Command(FontExpand)`][builtins-2869] | approximated | scanned, not built |
| `\pdffontname` | [`Convert(PdfFontName)`][builtins-2887] | approximated | widens to Unknown. No PDF font object is built |
| `\pdffontobjnum` | [`Convert(PdfFontObjNum)`][builtins-2887] | approximated | widens to Unknown. No PDF font object is built |
| `\pdffontsize` | [`Convert(PdfFontSize)`][builtins-2886] | exact | the font's real design size |
| `\pdfgamma` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfgentounicode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfglyphtounicode` | [`Command(TwoTexts)`][builtins-2869] | approximated | scanned, not built |
| `\pdfhorigin` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfignoreddimen` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfimageapplygamma` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfimagegamma` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfimagehicolor` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfimageresolution` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfincludechars` | [`Command(FontText)`][builtins-2869] | approximated | scanned, not built |
| `\pdfinclusioncopyfonts` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfinfo` | [`Command(GeneralText)`][builtins-2869] | approximated | scanned, not built |
| `\pdfinfoomitdate` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfinsertht` | [`Convert(InsertHt)`][builtins-2898] | approximated | scanned, answer unknown |
| `\pdfinterwordspaceoff` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\pdfinterwordspaceon` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\pdflastannot` | [`LastItem(LastXPos)`][builtins-2813] | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflastlinedepth` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdflastlink` | [`LastItem(LastXPos)`][builtins-2813] | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflastmatch` | [`Convert(LastMatch)`][builtins-2898] | approximated | scanned, answer unknown |
| `\pdflastobj` | [`LastItem(LastXPos)`][builtins-2813] | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflastxform` | [`LastItem(LastXPos)`][builtins-2813] | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflastximage` | [`LastItem(LastXPos)`][builtins-2813] | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflastximagepages` | [`LastItem(LastXPos)`][builtins-2813] | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflastxpos` | [`LastItem(LastXPos)`][builtins-2813] | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflastypos` | [`LastItem(LastYPos)`][builtins-2813] | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflinkmargin` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfliteral` | [`Pdf(Literal)`][builtins-2718] | approximated | the manual's keywords are scanned exactly. No PDF is built, so the payload is recorded as an occurrence, not replayed |
| `\pdfmajorversion` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfmapfile` | [`Command(GeneralText)`][builtins-2869] | approximated | scanned, not built |
| `\pdfmapline` | [`Command(GeneralText)`][builtins-2869] | approximated | scanned, not built |
| `\pdfmatch` | [`Convert(Match)`][builtins-2898] | approximated | scanned, answer unknown |
| `\pdfmdfivesum` | [`Convert(MdFiveSum)`][builtins-2883] | exact | reads the real file |
| `\pdfminorversion` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfnames` | [`Command(GeneralText)`][builtins-2869] | approximated | scanned, not built |
| `\pdfnobuiltintounicode` | [`Command(Font)`][builtins-2869] | approximated | scanned, not built |
| `\pdfnoligatures` | [`Command(Font)`][builtins-2869] | approximated | scanned, not built |
| `\pdfnormaldeviate` | [`Convert(NormalDeviate)`][builtins-2898] | approximated | scanned, answer unknown |
| `\pdfobj` | [`Pdf(Object)`][builtins-2718] | approximated | the manual's keywords are scanned exactly. No PDF is built, so the payload is recorded as an occurrence, not replayed |
| `\pdfobjcompresslevel` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfomitcharset` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfoutline` | [`Command(PdfOutline)`][builtins-2869] | approximated | scanned, not built |
| `\pdfoutput` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfpageattr` | [`TokensParameter`][builtins-2678] | approximated | the stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\pdfpagebox` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfpageheight` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfpageref` | [`Convert(PageRef)`][builtins-2898] | approximated | scanned, answer unknown |
| `\pdfpageresources` | [`Pdf(Resources)`][builtins-2718] | approximated | the manual's keywords are scanned exactly. No PDF is built, so the payload is recorded as an occurrence, not replayed |
| `\pdfpagesattr` | [`TokensParameter`][builtins-2678] | approximated | the stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\pdfpagewidth` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfpkmode` | [`TokensParameter`][builtins-2678] | approximated | the stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\pdfpkresolution` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfprependkern` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfprimitive` | [`Convert(Primitive)`][builtins-2893] | exact | re-injects the frozen meaning for normal expansion to re-process, as `\pdfprimitive` does |
| `\pdfprotrudechars` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfptexuseunderscore` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfpxdimen` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfrandomseed` | [`LastItem(RandomSeed)`][builtins-2813] | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdfrefobj` | [`Command(Number)`][builtins-2869] | approximated | scanned, not built |
| `\pdfrefxform` | [`Command(Number)`][builtins-2869] | approximated | scanned, not built |
| `\pdfrefximage` | [`Command(Number)`][builtins-2869] | approximated | scanned, not built |
| `\pdfresettimer` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\pdfrestore` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\pdfretval` | [`LastItem(LastXPos)`][builtins-2813] | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdfrunninglinkoff` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\pdfrunninglinkon` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\pdfsave` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\pdfsavepos` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\pdfsetmatrix` | [`Command(GeneralText)`][builtins-2869] | approximated | scanned, not built |
| `\pdfsetrandomseed` | [`Command(Number)`][builtins-2869] | approximated | scanned, not built |
| `\pdfshellescape` | [`LastItem(ShellEscape)`][builtins-2812] | exact | the configuration's own setting |
| `\pdfstartlink` | [`Command(PdfStartLink)`][builtins-2869] | approximated | scanned, not built |
| `\pdfstartthread` | [`Command(PdfThread)`][builtins-2869] | approximated | scanned, not built |
| `\pdfstrcmp` | [`Convert(StrCmp)`][builtins-2881] | exact | the manual's own algorithm, run for real |
| `\pdfsuppressptexinfo` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdftexbanner` | [`Convert(PdftexBanner)`][builtins-2898] | approximated | scanned, answer unknown |
| `\pdftexrevision` | [`Convert(PdftexRevision)`][builtins-2878] | exact | a real constant or stored name |
| `\pdftexversion` | [`LastItem(PdftexVersion)`][builtins-2798] | exact | read from state satex already tracks |
| `\pdfthread` | [`Command(PdfThread)`][builtins-2869] | approximated | scanned, not built |
| `\pdfthreadmargin` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdftracingfonts` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdftrailer` | [`Command(GeneralText)`][builtins-2869] | approximated | scanned, not built |
| `\pdftrailerid` | [`Command(GeneralText)`][builtins-2869] | approximated | scanned, not built |
| `\pdfunescapehex` | [`Convert(UnescapeHex)`][builtins-2881] | exact | the manual's own algorithm, run for real |
| `\pdfuniformdeviate` | [`Convert(UniformDeviate)`][builtins-2898] | approximated | scanned, answer unknown |
| `\pdfuniqueresname` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfvorigin` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pdfxform` | [`Command(PdfXForm)`][builtins-2869] | approximated | scanned, not built |
| `\pdfxformname` | [`Convert(XFormName)`][builtins-2898] | approximated | scanned, answer unknown |
| `\pdfximage` | [`Command(PdfXImage)`][builtins-2861] | exact | the file name reaches `observe::image` |
| `\pdfximagebbox` | [`Convert(ImageBBox)`][builtins-2898] | approximated | scanned, answer unknown |
| `\quitvmode` | [`Mode(QuitVMode)`][builtins-2907] | exact | [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own mode-transition rules |
| `\rightmarginkern` | [`Convert(MarginKern)`][builtins-2898] | approximated | scanned, answer unknown |
| `\rpcode` | [`FontParam(CharCode(0))`][builtins-2691] | approximated | reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\shbscode` | [`FontParam(CharCode(0))`][builtins-2691] | approximated | reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\showstream` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\stbscode` | [`FontParam(CharCode(0))`][builtins-2691] | approximated | reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\synctex` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\tagcode` | [`FontParam(TagCode { lig: true })`][builtins-2691] | approximated | reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own recovery. An entry satex cannot pin down widens every one it may name |

### XeTeX extensions

Source: the [XeTeX reference](https://tug.org/xetex/).

| primitive | modeled as | verdict | how |
| --- | --- | --- | --- |
| `\Uchar` | [`Convert(Uchar { catcode: false })`][builtins-2885] | exact | [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web), eTeX and XeTeX's character and category-code construction rules |
| `\Ucharcat` | [`Convert(Uchar { catcode: true })`][builtins-2885] | exact | [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web), eTeX and XeTeX's character and category-code construction rules |
| `\Udelcode` | [`Special(UCode { math: false, num: false })`][builtins-2833] | approximated | a table satex stores and reads back exactly for a known entry; one it cannot pin down widens every entry the write may have reached |
| `\Udelcodenum` | [`Special(UCode { math: false, num: true })`][builtins-2833] | approximated | a table satex stores and reads back exactly for a known entry; one it cannot pin down widens every entry the write may have reached |
| `\Udelimiter` | [`Command(Numbers(3))`][builtins-2869] | approximated | scanned, not built |
| `\Umathaccent` | [`Command(MathAccent)`][builtins-2869] | approximated | scanned, not built |
| `\Umathchar` | [`Command(Numbers(3))`][builtins-2869] | approximated | scanned, not built |
| `\Umathchardef` | [`Command(UMathCharDef { num: false })`][builtins-2853] | approximated | stored and consumed by satex's own font and math-code tables; an operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\Umathcharnum` | [`Command(Numbers(1))`][builtins-2869] | approximated | scanned, not built |
| `\Umathcharnumdef` | [`Command(UMathCharDef { num: true })`][builtins-2853] | approximated | stored and consumed by satex's own font and math-code tables; an operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\Umathcode` | [`Special(UCode { math: true, num: false })`][builtins-2833] | approximated | a table satex stores and reads back exactly for a known entry; one it cannot pin down widens every entry the write may have reached |
| `\Umathcodenum` | [`Special(UCode { math: true, num: true })`][builtins-2833] | approximated | a table satex stores and reads back exactly for a known entry; one it cannot pin down widens every entry the write may have reached |
| `\Uradical` | [`Command(Numbers(2))`][builtins-2869] | approximated | scanned, not built |
| `\XeTeXOTcountfeatures` | [`LastItem(Xetex(OtCountFeatures))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXOTcountlanguages` | [`LastItem(Xetex(OtCountLanguages))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXOTcountscripts` | [`LastItem(Xetex(OtCountScripts))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXOTfeaturetag` | [`LastItem(Xetex(OtFeatureTag))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXOTlanguagetag` | [`LastItem(Xetex(OtLanguageTag))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXOTscripttag` | [`LastItem(Xetex(OtScriptTag))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXcharclass` | [`Special(CharClass)`][builtins-2833] | approximated | a table satex stores and reads back exactly for a known entry; one it cannot pin down widens every entry the write may have reached |
| `\XeTeXcharglyph` | [`LastItem(Xetex(CharGlyph))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXcountfeatures` | [`LastItem(Xetex(CountFeatures))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXcountglyphs` | [`LastItem(Xetex(CountGlyphs))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXcountselectors` | [`LastItem(Xetex(CountSelectors))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXcountvariations` | [`LastItem(Xetex(CountVariations))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXdashbreakstate` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\XeTeXdefaultencoding` | [`Command(FileName)`][builtins-2869] | approximated | scanned, not built |
| `\XeTeXfeaturecode` | [`LastItem(Xetex(FeatureCode))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXfeaturename` | [`Convert(Xetex(FeatureName))`][builtins-2896] | approximated | exact from a loaded font's real tables. Unknown once they aren't resolved |
| `\XeTeXfindfeaturebyname` | [`LastItem(Xetex(FindFeatureByName))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXfindselectorbyname` | [`LastItem(Xetex(FindSelectorByName))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXfindvariationbyname` | [`LastItem(Xetex(FindVariationByName))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXfirstfontchar` | [`LastItem(Xetex(FirstFontChar))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXfonttype` | [`LastItem(Xetex(FontType))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXgenerateactualtext` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\XeTeXglyph` | [`Mode(Horizontal(Char))`][builtins-2941] | exact | a real item, appended to the list satex tracks |
| `\XeTeXglyphbounds` | [`LastItem(Xetex(GlyphBounds))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXglyphindex` | [`LastItem(Xetex(GlyphIndex))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXglyphname` | [`Convert(Xetex(GlyphName))`][builtins-2896] | approximated | exact from a loaded font's real tables. Unknown once they aren't resolved |
| `\XeTeXhyphenatablelength` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\XeTeXinputencoding` | [`Command(FileName)`][builtins-2869] | approximated | scanned, not built |
| `\XeTeXinputnormalization` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\XeTeXinterchartokenstate` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\XeTeXinterchartoks` | [`Special(InterCharToks)`][builtins-2833] | approximated | a table satex stores and reads back exactly for a known entry; one it cannot pin down widens every entry the write may have reached |
| `\XeTeXinterwordspaceshaping` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\XeTeXisdefaultselector` | [`LastItem(Xetex(IsDefaultSelector))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXisexclusivefeature` | [`LastItem(Xetex(IsExclusiveFeature))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXlastfontchar` | [`LastItem(Xetex(LastFontChar))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXlinebreaklocale` | [`Command(FileName)`][builtins-2869] | approximated | scanned, not built |
| `\XeTeXlinebreakpenalty` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\XeTeXlinebreakskip` | [`GlueParameter { mu: false }`][builtins-2676] | exact | the stored parameter table |
| `\XeTeXpdffile` | [`Command(PicFile)`][builtins-2869] | approximated | scanned, not built |
| `\XeTeXpdfpagecount` | [`LastItem(PdfPageCount)`][builtins-2820] | approximated | widens to Unknown. The file's page count needs parsing a PDF satex does not read |
| `\XeTeXpicfile` | [`Command(PicFile)`][builtins-2869] | approximated | scanned, not built |
| `\XeTeXprotrudechars` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\XeTeXrevision` | [`Convert(XetexRevision)`][builtins-2878] | exact | a real constant or stored name |
| `\XeTeXselectorcode` | [`LastItem(Xetex(SelectorCode))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXselectorname` | [`Convert(Xetex(SelectorName))`][builtins-2896] | approximated | exact from a loaded font's real tables. Unknown once they aren't resolved |
| `\XeTeXtracingfonts` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\XeTeXupwardsmode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\XeTeXuseglyphmetrics` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\XeTeXvariation` | [`LastItem(Xetex(Variation))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXvariationdefault` | [`LastItem(Xetex(VariationDefault))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXvariationmax` | [`LastItem(Xetex(VariationMax))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXvariationmin` | [`LastItem(Xetex(VariationMin))`][builtins-2822] | approximated | the font's real tables |
| `\XeTeXvariationname` | [`Convert(Xetex(VariationName))`][builtins-2896] | approximated | exact from a loaded font's real tables. Unknown once they aren't resolved |
| `\XeTeXversion` | [`LastItem(XetexVersion)`][builtins-2817] | exact | a constant |
| `\creationdate` | [`Convert(CreationDate)`][builtins-2898] | approximated | scanned, answer unknown |
| `\elapsedtime` | [`LastItem(ElapsedTime)`][builtins-2813] | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\filedump` | [`Convert(FileDump)`][builtins-2883] | exact | reads the real file |
| `\filemoddate` | [`Convert(FileModDate)`][builtins-2884] | approximated | always Unknown. satex does not read file modification times |
| `\filesize` | [`Convert(FileSize)`][builtins-2883] | exact | reads the real file |
| `\ifprimitive` | [`If(Primitive)`][builtins-2779] | exact | computed from state satex already tracks (`eval_extra_condition`) |
| `\mdfivesum` | [`Convert(MdFiveSum)`][builtins-2883] | exact | reads the real file |
| `\normaldeviate` | [`Convert(NormalDeviate)`][builtins-2898] | approximated | scanned, answer unknown |
| `\primitive` | [`Convert(Primitive)`][builtins-2893] | exact | re-injects the frozen meaning for normal expansion to re-process, as `\pdfprimitive` does |
| `\randomseed` | [`LastItem(RandomSeed)`][builtins-2813] | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\resettimer` | [`Command(Nothing)`][builtins-2844] | exact | no operand |
| `\setrandomseed` | [`Command(Number)`][builtins-2869] | approximated | scanned, not built |
| `\shellescape` | [`LastItem(ShellEscape)`][builtins-2812] | exact | the configuration's own setting |
| `\strcmp` | [`Convert(StrCmp)`][builtins-2881] | exact | the manual's own algorithm, run for real |
| `\suppressfontnotfounderror` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\uniformdeviate` | [`Convert(UniformDeviate)`][builtins-2898] | approximated | scanned, answer unknown |

### LuaTeX extensions

Source: the [LuaTeX manual](https://ctan.org/pkg/luatex).

| primitive | modeled as | verdict | how |
| --- | --- | --- | --- |
| `\Udelimiterover` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Udelimiterunder` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Uhextensible` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Uleft` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathaxis` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathbinbinspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathbinclosespacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathbininnerspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathbinopenspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathbinopspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathbinordspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathbinpunctspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathbinrelspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathcharclass` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathcharfam` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathcharslot` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathclosebinspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathcloseclosespacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathcloseinnerspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathcloseopenspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathcloseopspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathcloseordspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathclosepunctspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathcloserelspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathconnectoroverlapmin` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathfractiondelsize` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathfractiondenomdown` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathfractiondenomvgap` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathfractionnumup` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathfractionnumvgap` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathfractionrule` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathinnerbinspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathinnerclosespacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathinnerinnerspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathinneropenspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathinneropspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathinnerordspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathinnerpunctspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathinnerrelspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathlimitabovebgap` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathlimitabovekern` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathlimitabovevgap` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathlimitbelowbgap` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathlimitbelowkern` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathlimitbelowvgap` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathnolimitsubfactor` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathnolimitsupfactor` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathopbinspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathopclosespacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathopenbinspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathopenclosespacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathopeninnerspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathopenopenspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathopenopspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathopenordspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathopenpunctspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathopenrelspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathoperatorsize` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathopinnerspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathopopenspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathopopspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathopordspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathoppunctspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathoprelspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathordbinspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathordclosespacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathordinnerspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathordopenspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathordopspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathordordspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathordpunctspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathordrelspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathoverbarkern` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathoverbarrule` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathoverbarvgap` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathoverdelimiterbgap` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathoverdelimitervgap` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathpunctbinspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathpunctclosespacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathpunctinnerspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathpunctopenspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathpunctopspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathpunctordspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathpunctpunctspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathpunctrelspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathquad` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathradicaldegreeafter` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathradicaldegreebefore` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathradicaldegreeraise` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathradicalkern` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathradicalrule` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathradicalvgap` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathrelbinspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathrelclosespacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathrelinnerspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathrelopenspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathrelopspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathrelordspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathrelpunctspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathrelrelspacing` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathskewedfractionhgap` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathskewedfractionvgap` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathspaceafterscript` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathstackdenomdown` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathstacknumup` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathstackvgap` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathsubshiftdown` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathsubshiftdrop` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathsubsupshiftdown` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathsubsupvgap` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathsubtopmax` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathsupbottommin` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathsupshiftdrop` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathsupshiftup` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathsupsubbottommax` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathunderbarkern` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathunderbarrule` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathunderbarvgap` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathunderdelimiterbgap` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umathunderdelimitervgap` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Umiddle` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Unosubscript` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Unosuperscript` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Uoverdelimiter` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Uright` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Uroot` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Uskewed` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Uskewedwithdelims` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Ustack` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Ustartdisplaymath` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Ustartmath` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Ustopdisplaymath` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Ustopmath` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Usubscript` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Usuperscript` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Uunderdelimiter` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\Uvextensible` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\adjustspacing` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\alignmark` | [`AlignMark`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\aligntab` | [`AlignTab`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\attribute` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\attributedef` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\automaticdiscretionary` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\automatichyphenmode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\automatichyphenpenalty` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\begincsname` | [`BeginCsname`][builtins-2643] | exact | [LuaTeX manual, "Macros"](https://ctan.org/pkg/luatex) |
| `\bodydir` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\bodydirection` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\boundary` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\boxdir` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\boxdirection` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\breakafterdirmode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\catcodetable` | [`Lua(SelectCatcodes)`][builtins-2996] | exact | a real string escape or catcode-table operation |
| `\clearmarks` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\compoundhyphenmode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\copyfont` | [`Command(CopyFont { amount: false })`][builtins-2853] | approximated | stored and consumed by satex's own font and math-code tables; an operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\crampeddisplaystyle` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\crampedscriptscriptstyle` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\crampedscriptstyle` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\crampedtextstyle` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\csstring` | [`Text(String)`][builtins-2736] | approximated | exact for a known operand. Widens when the token's meaning or a register's digits are unresolved |
| `\deferred` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\directlua` | [`Lua(Direct)`][builtins-2991] | approximated | the chunk runs for real where a Lua host is available. A failed or unavailable run is diagnosed and widened |
| `\discretionaryligaturemode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\draftmode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\dviextension` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\dvifeedback` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\dvivariable` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\eTeXVersion` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\eTeXglueshrinkorder` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\eTeXgluestretchorder` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\eTeXminorversion` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\endlocalcontrol` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\etoksapp` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\etokspre` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\exceptionpenalty` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\exhyphenchar` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\expandglyphsinfont` | [`Command(FontExpand)`][builtins-2869] | approximated | scanned, not built |
| `\explicitdiscretionary` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\explicithyphenpenalty` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\firstvalidlanguage` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\fixupboxesmode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\fontid` | [`Convert(FontId)`][builtins-2878] | exact | a real constant or stored name |
| `\formatname` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\gleaders` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\glyphdimensionsmode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\gtoksapp` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\gtokspre` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\hjcode` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\hpack` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\hyphenationbounds` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\hyphenationmin` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\hyphenpenaltymode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\ifabsdim` | [`If(AbsDim)`][builtins-2779] | exact | computed from state satex already tracks (`eval_extra_condition`) |
| `\ifabsnum` | [`If(AbsNum)`][builtins-2779] | exact | computed from state satex already tracks (`eval_extra_condition`) |
| `\ifcondition` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\ignoreligaturesinfont` | [`Command(Font)`][builtins-2869] | approximated | scanned, not built |
| `\immediateassigned` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\immediateassignment` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\initcatcodetable` | [`Lua(InitCatcodes)`][builtins-2996] | exact | a real string escape or catcode-table operation |
| `\insertht` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\lastnamedcs` | [`LastNamedCs`][builtins-2643] | exact | [LuaTeX manual, "Macros"](https://ctan.org/pkg/luatex) |
| `\lastsavedboxresourceindex` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\lastsavedimageresourceindex` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\lastsavedimageresourcepages` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\lastxpos` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\lastypos` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\latelua` | [`Lua(Late)`][builtins-2991] | approximated | the chunk runs for real where a Lua host is available. A failed or unavailable run is diagnosed and widened |
| `\lateluafunction` | [`Command(Number)`][builtins-2869] | approximated | scanned, not built |
| `\leftghost` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\letcharcode` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\linedir` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\linedirection` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\localbrokenpenalty` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\localinterlinepenalty` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\localleftbox` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\localrightbox` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\luabytecode` | [`Command(Number)`][builtins-2869] | approximated | scanned, not built |
| `\luabytecodecall` | [`Command(Number)`][builtins-2869] | approximated | scanned, not built |
| `\luacopyinputnodes` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\luadef` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\luaescapestring` | [`Lua(Escape)`][builtins-2996] | exact | a real string escape or catcode-table operation |
| `\luafunction` | [`Command(Number)`][builtins-2869] | approximated | scanned, not built |
| `\luafunctioncall` | [`Command(Number)`][builtins-2869] | approximated | scanned, not built |
| `\luatexbanner` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\luatexrevision` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\luatexversion` | [`LastItem(LuatexVersion)`][builtins-2818] | exact | probed from the real `luatex --version` for this run |
| `\mathdefaultsmode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\mathdelimitersmode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\mathdir` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\mathdirection` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\mathdisplayskipmode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\mathemptydisplaymode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\matheqdirmode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\matheqnogapstep` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\mathflattenmode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\mathitalicsmode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\mathnolimitsmode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\mathoption` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\mathpenaltiesmode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\mathrulesfam` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\mathrulesmode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\mathrulethicknessmode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\mathscriptboxmode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\mathscriptcharmode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\mathscriptsmode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\mathstyle` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\mathsurroundmode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\mathsurroundskip` | [`GlueParameter { mu: false }`][builtins-2676] | exact | the stored parameter table |
| `\nohrule` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\nokerns` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\noligs` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\nospaces` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\novrule` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\outputbox` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\outputmode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\pagebottomoffset` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pagedir` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\pagedirection` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\pageheight` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pageleftoffset` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pagerightoffset` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pagetopoffset` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pagewidth` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\pardir` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\pardirection` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\pdfextension` | [`Pdf(Extension)`][builtins-2718] | approximated | the manual's keywords are scanned exactly. No PDF is built, so the payload is recorded as an occurrence, not replayed |
| `\pdffeedback` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\pdfvariable` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\postexhyphenchar` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\posthyphenchar` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\prebinoppenalty` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\predisplaygapfactor` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\preexhyphenchar` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\prehyphenchar` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\prerelpenalty` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\protrudechars` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\protrusionboundary` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\pxdimen` | [`DimenParameter`][builtins-2676] | exact | the stored parameter table |
| `\rightghost` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\saveboxresource` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\savecatcodetable` | [`Lua(SaveCatcodes)`][builtins-2996] | exact | a real string escape or catcode-table operation |
| `\saveimageresource` | [`Command(PdfXImage)`][builtins-2861] | exact | the file name reaches `observe::image` |
| `\savepos` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\scantextokens` | [`ScanTokens { text: true }`][builtins-2646] | exact | [eTeX manual § 3.9](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf). A capacity overflow (`MAX_IN_OPEN`) is diagnosed, not assumed away |
| `\setfontid` | [`Command(SetFontId)`][builtins-2853] | approximated | stored and consumed by satex's own font and math-code tables; an operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\shapemode` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\suppressifcsnameerror` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\suppresslongerror` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\suppressmathparerror` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\suppressoutererror` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\suppressprimitiveerror` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\textdir` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\textdirection` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\toksapp` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\tokspre` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\tpack` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\tracingfonts` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\useboxresource` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\useimageresource` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\variablefam` | [`IntegerParameter`][builtins-2676] | exact | the stored parameter table |
| `\vpack` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\wordboundary` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\xtoksapp` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |
| `\xtokspre` | [`Unmodeled`][builtins-2725] | unsupported + diagnostic | widens the mode |

**925 of 925 primitives modeled soundly.**

[builtins-2630]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2630
[builtins-2632]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2632
[builtins-2633]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2633
[builtins-2634]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2634
[builtins-2635]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2635
[builtins-2639]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2639
[builtins-2643]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2643
[builtins-2644]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2644
[builtins-2646]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2646
[builtins-2648]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2648
[builtins-2650]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2650
[builtins-2652]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2652
[builtins-2657]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2657
[builtins-2662]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2662
[builtins-2666]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2666
[builtins-2667]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2667
[builtins-2671]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2671
[builtins-2676]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2676
[builtins-2678]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2678
[builtins-2682]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2682
[builtins-2686]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2686
[builtins-2690]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2690
[builtins-2691]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2691
[builtins-2695]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2695
[builtins-2698]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2698
[builtins-2699]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2699
[builtins-2703]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2703
[builtins-2704]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2704
[builtins-2705]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2705
[builtins-2709]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2709
[builtins-2710]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2710
[builtins-2711]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2711
[builtins-2716]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2716
[builtins-2718]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2718
[builtins-2725]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2725
[builtins-2735]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2735
[builtins-2736]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2736
[builtins-2747]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2747
[builtins-2748]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2748
[builtins-2753]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2753
[builtins-2755]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2755
[builtins-2757]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2757
[builtins-2760]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2760
[builtins-2763]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2763
[builtins-2765]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2765
[builtins-2770]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2770
[builtins-2774]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2774
[builtins-2779]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2779
[builtins-2781]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2781
[builtins-2790]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2790
[builtins-2792]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2792
[builtins-2796]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2796
[builtins-2798]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2798
[builtins-2801]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2801
[builtins-2804]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2804
[builtins-2806]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2806
[builtins-2807]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2807
[builtins-2811]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2811
[builtins-2812]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2812
[builtins-2813]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2813
[builtins-2817]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2817
[builtins-2818]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2818
[builtins-2820]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2820
[builtins-2822]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2822
[builtins-2831]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2831
[builtins-2833]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2833
[builtins-2844]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2844
[builtins-2845]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2845
[builtins-2846]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2846
[builtins-2847]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2847
[builtins-2848]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2848
[builtins-2849]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2849
[builtins-2853]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2853
[builtins-2857]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2857
[builtins-2861]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2861
[builtins-2862]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2862
[builtins-2869]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2869
[builtins-2878]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2878
[builtins-2881]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2881
[builtins-2883]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2883
[builtins-2884]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2884
[builtins-2885]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2885
[builtins-2886]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2886
[builtins-2887]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2887
[builtins-2888]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2888
[builtins-2893]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2893
[builtins-2896]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2896
[builtins-2898]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2898
[builtins-2907]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2907
[builtins-2909]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2909
[builtins-2915]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2915
[builtins-2919]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2919
[builtins-2921]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2921
[builtins-2924]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2924
[builtins-2926]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2926
[builtins-2928]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2928
[builtins-2941]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2941
[builtins-2943]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2943
[builtins-2948]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2948
[builtins-2953]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2953
[builtins-2961]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2961
[builtins-2962]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2962
[builtins-2973]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2973
[builtins-2974]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2974
[builtins-2975]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2975
[builtins-2979]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2979
[builtins-2980]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2980
[builtins-2991]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2991
[builtins-2996]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/builtins.rs#L2996
