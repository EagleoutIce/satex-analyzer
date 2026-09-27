<!-- generated from doc/src/wiki/primitive-coverage.md.in by satex-doc; do not edit -->

# Primitive coverage

[tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) parts 24-49 define TeX82's primitives. The [eTeX manual](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf), the [pdfTeX manual](https://mirror.ctan.org/systems/doc/pdftex/manual/pdftex-a.pdf), the [XeTeX reference](https://tug.org/xetex/) and the [LuaTeX manual](https://mirror.ctan.org/systems/doc/luatex/base/luatex.pdf) add the rest.

`src/builtins.rs` maps every one of these names to a `Primitive`, and `builtins::verdict` judges each `Primitive` from what `src/exec.rs`, `src/mode.rs` and `src/commands.rs` do with it. This page is generated from both, so it cannot drift from the interpreter. See [Domains](Domains) for the values a primitive's operands and results are tracked as, and [Lints](Lints) and [Chktex-Coverage](Chktex-Coverage) for what a lint rule builds on top of a primitive's real effect.

Every primitive gets one of three verdicts:

- **exact** — satex reproduces the engine's behaviour.
- **approximated** — a sound over-approximation. The table says how.
- **unsupported + diagnostic** — not modelled, but everything it could affect is widened and the run says so.

A primitive that scans its operands and drops their effect with neither of the last two is a bug, not a fourth verdict. satex never rests at "unsupported" silently ([Domains § Soundness invariant](Domains#soundness-invariant)). Any found this way are fixed, not documented as sound, and listed separately below until they are.

### TeX82 & eTeX

Source: [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) parts 24-49. eTeX is in every format satex analyzes (`Engine::has_etex`), so its primitives are listed with TeX82's, not apart.

| primitive | modeled as | verdict | how |
| --- | --- | --- | --- |
| `\ ` (control space) | `Mode(Horizontal(Space))` | exact | a real item, appended to the list satex tracks |
| `\-` | `Mode(Horizontal(Hyphen))` | approximated | widens to Unknown. A rarely-taken [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) path this pass does not distinguish further |
| `\/` | `Command(Nothing)` | exact | no operand |
| `\TeXXeTstate` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\above` | `Command(Dimen)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\abovedisplayshortskip` | `GlueParameter { mu: false }` | exact | read and written from the stored parameter table |
| `\abovedisplayskip` | `GlueParameter { mu: false }` | exact | read and written from the stored parameter table |
| `\abovewithdelims` | `Command(Delimiters { dimen: true })` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\accent` | `Accent` | approximated | widens to Unknown when the accented character does not resolve in horizontal mode |
| `\adjdemerits` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\advance` | `Arith(Advance)` | approximated | exact for known operands. Interval arithmetic and [tex.web § 222](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s zero default otherwise |
| `\afterassignment` | `Defer` | exact | [tex.web §§ 1211, 1](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) (semi-simple groups, `\aftergroup`/`\afterassignment`) |
| `\aftergroup` | `Defer` | exact | [tex.web §§ 1211, 1](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) (semi-simple groups, `\aftergroup`/`\afterassignment`) |
| `\atop` | `Command(Nothing)` | exact | no operand |
| `\atopwithdelims` | `Command(Delimiters { dimen: false })` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\badness` | `LastItem(Badness)` | approximated | always Unknown. Badness needs the line-breaking satex does not run |
| `\baselineskip` | `GlueParameter { mu: false }` | exact | read and written from the stored parameter table |
| `\batchmode` | `Command(Interaction(0))` | exact | sets `\interactionmode` |
| `\beginL` | `Command(Nothing)` | exact | no operand |
| `\beginR` | `Command(Nothing)` | exact | no operand |
| `\begingroup` | `BeginGroup` | exact | [tex.web §§ 1211, 1](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) (semi-simple groups, `\aftergroup`/`\afterassignment`) |
| `\belowdisplayshortskip` | `GlueParameter { mu: false }` | exact | read and written from the stored parameter table |
| `\belowdisplayskip` | `GlueParameter { mu: false }` | exact | read and written from the stored parameter table |
| `\binoppenalty` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\botmark` | `Convert(Mark)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |
| `\botmarks` | `Convert(Marks)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |
| `\box` | `Register(Box)` | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\boxmaxdepth` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\brokenpenalty` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\catcode` | `CatcodeAssign` | unsupported + diagnostic | exact for one known character and category. Diagnosed (`unknown-catcode`) and widened when either can't be pinned down |
| `\char` | `Mode(Horizontal(Char))` | exact | a real item, appended to the list satex tracks |
| `\chardef` | `RegisterDef(Char)` | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\cleaders` | `Mode(Leaders)` | approximated | the box or rule is scanned exactly. [tex.web § 1078](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s repeated copies are not laid out, since satex tracks natural size, not a page |
| `\closein` | `CloseIn` | approximated | [tex.web § 1351](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web). Exact for a known stream number; one satex cannot pin down leaves every stream's state unknown |
| `\closeout` | `Command(CloseOut)` | approximated | tracks the stream state a later `\input` of the written file reads back; a stream number satex cannot pin down widens every stream |
| `\clubpenalties` | `Special(Penalties)` | approximated | a table satex stores and reads back exactly for a known entry; one it cannot pin down widens every entry the write may have reached |
| `\clubpenalty` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\copy` | `Mode(Box(Copy))` | exact | the real box machinery |
| `\count` | `Register(Count)` | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\countdef` | `RegisterDef(Count)` | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\cr` | `Command(Nothing)` | exact | no operand |
| `\crcr` | `Command(Nothing)` | exact | no operand |
| `\csname` | `Csname` | approximated | exact once the name is resolved. A bounded join of candidate meanings when it is built from unknown digits (`one_of_names`) |
| `\currentgrouplevel` | `LastItem(GroupLevel)` | exact | read from state satex already tracks |
| `\currentgrouptype` | `LastItem(GroupType)` | exact | read from state satex already tracks |
| `\currentifbranch` | `LastItem(IfBranch)` | approximated | exact once the conditional stack has settled. Unknown while a branch is undecided |
| `\currentiflevel` | `LastItem(IfLevel)` | approximated | exact once the conditional stack has settled. Unknown while a branch is undecided |
| `\currentiftype` | `LastItem(IfType)` | approximated | exact once the conditional stack has settled. Unknown while a branch is undecided |
| `\day` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\deadcycles` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\def` | `Def { global: false, expand: false }` | exact | [tex.web § 476](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web). An unterminated definition is diagnosed, not read past the file's end |
| `\defaulthyphenchar` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\defaultskewchar` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\delcode` | `CharCode(Delimiter)` | unsupported + diagnostic | exact for one known character. Diagnosed (`unknown-char-code`) and the whole table widened when the candidates can't be enumerated at all |
| `\delimiter` | `Command(Number)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\delimiterfactor` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\delimitershortfall` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\detokenize` | `Text(Detokenize)` | approximated | exact for a known operand. Widens when the token's meaning or a register's digits are unresolved |
| `\dimen` | `Register(Dimen)` | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\dimendef` | `RegisterDef(Dimen)` | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\dimexpr` | `Expr(Dimen)` | exact | [eTeX manual § 3.5](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf) |
| `\discretionary` | `Mode(Horizontal(Discretionary))` | exact | a real item, appended to the list satex tracks |
| `\displayindent` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\displaylimits` | `Command(Nothing)` | exact | no operand |
| `\displaystyle` | `Command(Nothing)` | exact | no operand |
| `\displaywidowpenalties` | `Special(Penalties)` | approximated | a table satex stores and reads back exactly for a known entry; one it cannot pin down widens every entry the write may have reached |
| `\displaywidowpenalty` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\displaywidth` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\divide` | `Arith(Divide)` | approximated | exact for known operands. Interval arithmetic and [tex.web § 222](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s zero default otherwise |
| `\doublehyphendemerits` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\dp` | `Special(BoxDimen)` | approximated | round-trips through the register consistently. The box behind it is never built |
| `\dump` | `Command(Nothing)` | exact | no operand |
| `\eTeXrevision` | `Convert(EtexRevision)` | exact | a real constant or stored name |
| `\eTeXversion` | `LastItem(EtexVersion)` | exact | read from state satex already tracks |
| `\edef` | `Def { global: false, expand: true }` | exact | [tex.web § 476](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web). An unterminated definition is diagnosed, not read past the file's end |
| `\else` | `Else` | exact | [tex.web § 510](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web), including `insert_relax` for an else or fi met mid-test |
| `\emergencystretch` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\end` | `End` | exact | [tex.web § 1054](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) (`end_job`) |
| `\endL` | `Command(Nothing)` | exact | no operand |
| `\endR` | `Command(Nothing)` | exact | no operand |
| `\endcsname` | `Endcsname` | exact | [LuaTeX manual, "Macros"](https://ctan.org/pkg/luatex) |
| `\endgroup` | `EndGroup` | exact | [tex.web §§ 1211, 1](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) (semi-simple groups, `\aftergroup`/`\afterassignment`) |
| `\endinput` | `Endinput` | unsupported + diagnostic | exact when unconditional. Diagnosed (`conditional-endinput`) and the file read on when reached under an undecided conditional |
| `\endlinechar` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\eqno` | `Mode(EqNo)` | exact | [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own mode-transition rules |
| `\errhelp` | `TokensParameter` | approximated | read and written from the stored parameter table exactly, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\errmessage` | `Message { error: true }` | exact | [tex.web § 1279](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) |
| `\errorcontextlines` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\errorstopmode` | `Command(Interaction(3))` | exact | sets `\interactionmode` |
| `\escapechar` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\everycr` | `TokensParameter` | approximated | read and written from the stored parameter table exactly, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\everydisplay` | `TokensParameter` | approximated | read and written from the stored parameter table exactly, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\everyeof` | `TokensParameter` | approximated | read and written from the stored parameter table exactly, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\everyhbox` | `TokensParameter` | approximated | read and written from the stored parameter table exactly, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\everyjob` | `TokensParameter` | approximated | read and written from the stored parameter table exactly, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\everymath` | `TokensParameter` | approximated | read and written from the stored parameter table exactly, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\everypar` | `TokensParameter` | approximated | read and written from the stored parameter table exactly, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\everyvbox` | `TokensParameter` | approximated | read and written from the stored parameter table exactly, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\exhyphenpenalty` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\expandafter` | `ExpandAfter` | exact | [tex.web § 367](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) |
| `\fam` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\fi` | `Fi` | exact | [tex.web § 510](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web), including `insert_relax` for an else or fi met mid-test |
| `\finalhyphendemerits` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\firstmark` | `Convert(Mark)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |
| `\firstmarks` | `Convert(Marks)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |
| `\floatingpenalty` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\font` | `FontDef` | exact | [tex.web § 1257](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) (`\font`) |
| `\fontchardp` | `LastItem(FontCharDp)` | approximated | exact for a TFM font ([etex_man § 3.10](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf)). A native font's height, depth and italic correction are not read, so it widens to Unknown |
| `\fontcharht` | `LastItem(FontCharHt)` | approximated | exact for a TFM font ([etex_man § 3.10](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf)). A native font's height, depth and italic correction are not read, so it widens to Unknown |
| `\fontcharic` | `LastItem(FontCharIc)` | approximated | exact for a TFM font ([etex_man § 3.10](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf)). A native font's height, depth and italic correction are not read, so it widens to Unknown |
| `\fontcharwd` | `LastItem(FontCharWd)` | exact | a TFM's or a native font's real glyph advance |
| `\fontdimen` | `FontParam(Dimen)` | approximated | reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\fontname` | `Convert(FontName)` | exact | a real constant or stored name |
| `\futurelet` | `Let { global: false, future: true }` | exact | [tex.web § 1221](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web), including the two-token `\futurelet` lookahead |
| `\gdef` | `Def { global: true, expand: false }` | exact | [tex.web § 476](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web). An unterminated definition is diagnosed, not read past the file's end |
| `\global` | `Prefix(Global)` | exact | [tex.web § 1211](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) |
| `\globaldefs` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\glueexpr` | `Expr(Skip)` | exact | [eTeX manual § 3.5](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf) |
| `\glueshrink` | `LastItem(GlueShrink)` | exact | computed from a known glue spec |
| `\glueshrinkorder` | `LastItem(GlueShrinkOrder)` | exact | computed from a known glue spec |
| `\gluestretch` | `LastItem(GlueStretch)` | exact | computed from a known glue spec |
| `\gluestretchorder` | `LastItem(GlueStretchOrder)` | exact | computed from a known glue spec |
| `\gluetomu` | `LastItem(GlueToMu)` | exact | computed from a known glue spec |
| `\halign` | `Mode(Vertical(Align))` | approximated | cells run in the alignment's own list, not distinguished from the preamble |
| `\hangafter` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\hangindent` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\hbadness` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\hbox` | `Mode(Box(HBox))` | exact | the real box machinery |
| `\hfil` | `Mode(Horizontal(Fil))` | exact | a real item, appended to the list satex tracks |
| `\hfill` | `Mode(Horizontal(Fil))` | exact | a real item, appended to the list satex tracks |
| `\hfilneg` | `Mode(Horizontal(Fil))` | exact | a real item, appended to the list satex tracks |
| `\hfuzz` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\hoffset` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\holdinginserts` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\hrule` | `Mode(Vertical(Rule))` | exact | a real item, appended to the list satex tracks |
| `\hsize` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\hskip` | `Mode(Horizontal(Glue))` | exact | a real item, appended to the list satex tracks |
| `\hss` | `Mode(Horizontal(Fil))` | exact | a real item, appended to the list satex tracks |
| `\ht` | `Special(BoxDimen)` | approximated | round-trips through the register consistently. The box behind it is never built |
| `\hyphenation` | `Command(GeneralText)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\hyphenchar` | `FontParam(HyphenChar)` | approximated | reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\hyphenpenalty` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\if` | `If(Chars)` | approximated | exact for known characters. Undecided when either operand is an unresolved character |
| `\ifcase` | `If(Case)` | approximated | [tex.web § 509](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web). Every arm is analyzed as its own path when the selector is unknown |
| `\ifcat` | `If(Catcodes)` | approximated | exact for known characters. Undecided when either operand is an unresolved character |
| `\ifcsname` | `If(Csname)` | exact | builds the name and looks it up |
| `\ifdefined` | `If(Defined)` | approximated | exact for a known name. Narrowed by which names could still be undefined otherwise |
| `\ifdim` | `If(Dim)` | approximated | exact for known operands. Narrows the interval on each branch otherwise |
| `\ifeof` | `If(Eof)` | approximated | satex resolves the real file, so end-of-file is known for a known stream number; one it cannot pin down leaves the test undecided |
| `\iffalse` | `If(False)` | exact | a constant |
| `\iffontchar` | `If(FontChar)` | exact | [eTeX manual § 3.10](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf): read from the font's real character table |
| `\ifhbox` | `If(Box)` | approximated | exact for a known box state (void, `\hbox`, `\vbox`). Undecided once the register is Unknown |
| `\ifhmode` | `If(HMode)` | approximated | decided from the mode satex tracks through the run. Open groups the run couldn't follow leave it undecided |
| `\ifinner` | `If(InnerMode)` | approximated | decided from the mode satex tracks through the run. Open groups the run couldn't follow leave it undecided |
| `\ifmmode` | `If(MathMode)` | approximated | decided from the mode satex tracks through the run. Open groups the run couldn't follow leave it undecided |
| `\ifnum` | `If(Num)` | approximated | exact for known operands. Narrows the interval on each branch otherwise |
| `\ifodd` | `If(Odd)` | approximated | exact for known operands. Narrows the interval on each branch otherwise |
| `\iftrue` | `If(True)` | exact | a constant |
| `\ifvbox` | `If(Box)` | approximated | exact for a known box state (void, `\hbox`, `\vbox`). Undecided once the register is Unknown |
| `\ifvmode` | `If(VMode)` | approximated | decided from the mode satex tracks through the run. Open groups the run couldn't follow leave it undecided |
| `\ifvoid` | `If(Box)` | approximated | exact for a known box state (void, `\hbox`, `\vbox`). Undecided once the register is Unknown |
| `\ifx` | `If(IfX)` | approximated | exact for known meanings ([tex.web § 507](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s `original` handles a redefined primitive). Member-by-member across a set of possible meanings otherwise, decided only when they agree |
| `\ignorespaces` | `IgnoreSpaces` | exact | [tex.web § 1045](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) |
| `\immediate` | `Prefix(Immediate)` | exact | [tex.web § 1211](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) |
| `\indent` | `Mode(StartPar { indent: true })` | exact | [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own mode-transition rules |
| `\input` | `Load(Input)` | exact | [tex.web § 537](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) (`\input`) |
| `\inputlineno` | `LastItem(InputLineNo)` | exact | read from state satex already tracks |
| `\insert` | `Mode(Insert { vadjust: false })` | approximated | opens the insert's own list. Not told apart from the vertical list around it |
| `\insertpenalties` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\interactionmode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\interlinepenalties` | `Special(Penalties)` | approximated | a table satex stores and reads back exactly for a known entry; one it cannot pin down widens every entry the write may have reached |
| `\interlinepenalty` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\jobname` | `Text(JobName)` | exact | the run's real main file name |
| `\kern` | `Typeset(Dimen)` | approximated | `\kern` is exact. `\mkern`'s math-unit dimension always widens to Unknown, since it needs math-font setup satex does not run |
| `\language` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\lastbox` | `Mode(Box(LastBox))` | exact | the real box machinery |
| `\lastkern` | `LastItem(Kern)` | approximated | reads the tail of the list satex tracks. Unknown once the tail itself is |
| `\lastlinefit` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\lastnodetype` | `LastItem(NodeType)` | approximated | -1 for a genuinely empty list (etex.ch). Unknown otherwise |
| `\lastpenalty` | `LastItem(Penalty)` | approximated | reads the tail of the list satex tracks. Unknown once the tail itself is |
| `\lastskip` | `LastItem(Skip)` | approximated | reads the tail of the list satex tracks. Unknown once the tail itself is |
| `\lccode` | `CharCode(Lower)` | unsupported + diagnostic | exact for one known character. Diagnosed (`unknown-char-code`) and the whole table widened when the candidates can't be enumerated at all |
| `\leaders` | `Mode(Leaders)` | approximated | the box or rule is scanned exactly. [tex.web § 1078](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s repeated copies are not laid out, since satex tracks natural size, not a page |
| `\left` | `Mode(Left)` | approximated | documented: satex keeps no group for it, so the display may read as inner from here on |
| `\lefthyphenmin` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\leftskip` | `GlueParameter { mu: false }` | exact | read and written from the stored parameter table |
| `\leqno` | `Mode(EqNo)` | exact | [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own mode-transition rules |
| `\let` | `Let { global: false, future: false }` | exact | [tex.web § 1221](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web), including the two-token `\futurelet` lookahead |
| `\limits` | `Command(Nothing)` | exact | no operand |
| `\linepenalty` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\lineskip` | `GlueParameter { mu: false }` | exact | read and written from the stored parameter table |
| `\lineskiplimit` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\long` | `Prefix(Long)` | exact | [tex.web § 1211](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) |
| `\looseness` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\lower` | `Mode(Shift { vertical: true })` | exact | [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own mode-transition rules |
| `\lowercase` | `CaseShift(Lower)` | exact | [tex.web § 1288](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web). Widens a code an unknown token was assigned |
| `\mag` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\mark` | `Command(GeneralText)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\marks` | `Command(NumberText)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\mathaccent` | `Command(Number)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\mathbin` | `Command(Nothing)` | exact | no operand |
| `\mathchar` | `Command(Number)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\mathchardef` | `RegisterDef(MathChar)` | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\mathchoice` | `Mode(MathChoice)` | exact | [tex.web § 1172](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web): opens all four styles in sequence |
| `\mathclose` | `Command(Nothing)` | exact | no operand |
| `\mathcode` | `CharCode(Math)` | unsupported + diagnostic | exact for one known character. Diagnosed (`unknown-char-code`) and the whole table widened when the candidates can't be enumerated at all |
| `\mathinner` | `Command(Nothing)` | exact | no operand |
| `\mathop` | `Command(Nothing)` | exact | no operand |
| `\mathopen` | `Command(Nothing)` | exact | no operand |
| `\mathord` | `Command(Nothing)` | exact | no operand |
| `\mathpunct` | `Command(Nothing)` | exact | no operand |
| `\mathrel` | `Command(Nothing)` | exact | no operand |
| `\mathsurround` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\maxdeadcycles` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\maxdepth` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\meaning` | `Text(Meaning)` | approximated | exact for a known operand. Widens when the token's meaning or a register's digits are unresolved |
| `\medmuskip` | `GlueParameter { mu: true }` | exact | read and written from the stored parameter table |
| `\message` | `Message { error: false }` | exact | [tex.web § 1279](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) |
| `\middle` | `Typeset(Delimiter)` | exact | [tex.web § 1161](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web): the delimiter is the one token the primitive itself consumes |
| `\mkern` | `Typeset(Dimen)` | approximated | `\kern` is exact. `\mkern`'s math-unit dimension always widens to Unknown, since it needs math-font setup satex does not run |
| `\month` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\moveleft` | `Mode(Shift { vertical: false })` | exact | [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own mode-transition rules |
| `\moveright` | `Mode(Shift { vertical: false })` | exact | [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own mode-transition rules |
| `\mskip` | `Typeset(Glue)` | approximated | `\mskip` appends an item of unknown size: its glue is in math units, which need math-font setup satex does not run |
| `\muexpr` | `Expr(MuSkip)` | exact | [eTeX manual § 3.5](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf) |
| `\multiply` | `Arith(Multiply)` | approximated | exact for known operands. Interval arithmetic and [tex.web § 222](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s zero default otherwise |
| `\muskip` | `Register(MuSkip)` | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\muskipdef` | `RegisterDef(MuSkip)` | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\mutoglue` | `LastItem(MuToGlue)` | exact | computed from a known glue spec |
| `\newlinechar` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\noalign` | `Mode(NoAlign)` | approximated | cells run in the alignment's own list, not distinguished from the preamble |
| `\noboundary` | `Mode(Horizontal(NoBoundary))` | exact | a real item, appended to the list satex tracks |
| `\noexpand` | `NoExpand` | exact | [tex.web §§ 367, 358](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) |
| `\noindent` | `Mode(StartPar { indent: false })` | exact | [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own mode-transition rules |
| `\nolimits` | `Command(Nothing)` | exact | no operand |
| `\nonscript` | `Command(Nothing)` | exact | no operand |
| `\nonstopmode` | `Command(Interaction(1))` | exact | sets `\interactionmode` |
| `\nulldelimiterspace` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\nullfont` | `FontIdent(0)` | exact | [tex.web § 1217](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web): selecting a font is a local assignment |
| `\number` | `Text(Number)` | approximated | exact for a known operand. Widens when the token's meaning or a register's digits are unresolved |
| `\numexpr` | `Expr(Count)` | exact | [eTeX manual § 3.5](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf) |
| `\omit` | `Command(Nothing)` | exact | no operand |
| `\openin` | `OpenIn` | approximated | [tex.web § 1351](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web). Exact for a known stream number; one satex cannot pin down leaves every stream's state unknown |
| `\openout` | `Command(OpenOut)` | approximated | tracks the stream state a later `\input` of the written file reads back; a stream number satex cannot pin down widens every stream |
| `\or` | `Or` | exact | [tex.web § 510](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web), including `insert_relax` for an else or fi met mid-test |
| `\outer` | `Prefix(Outer)` | exact | [tex.web § 1211](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) |
| `\output` | `TokensParameter` | approximated | read and written from the stored parameter table exactly, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\outputpenalty` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\over` | `Command(Nothing)` | exact | no operand |
| `\overfullrule` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\overline` | `Command(Nothing)` | exact | no operand |
| `\overwithdelims` | `Command(Delimiters { dimen: false })` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pagedepth` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pagediscards` | `Mode(Vertical(Discards))` | approximated | widens to Unknown. A rarely-taken [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) path this pass does not distinguish further |
| `\pagefilllstretch` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pagefillstretch` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pagefilstretch` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pagegoal` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pageshrink` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pagestretch` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pagetotal` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\par` | `Mode(Par)` | exact | [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own mode-transition rules |
| `\parfillskip` | `GlueParameter { mu: false }` | exact | read and written from the stored parameter table |
| `\parindent` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\parshape` | `Special(ParShape)` | approximated | a table satex stores and reads back exactly for a known entry; one it cannot pin down widens every entry the write may have reached |
| `\parshapedimen` | `LastItem(ParShapeDimen)` | exact | read from the stored `\parshape` |
| `\parshapeindent` | `LastItem(ParShapeIndent)` | exact | read from the stored `\parshape` |
| `\parshapelength` | `LastItem(ParShapeLength)` | exact | read from the stored `\parshape` |
| `\parskip` | `GlueParameter { mu: false }` | exact | read and written from the stored parameter table |
| `\patterns` | `Command(GeneralText)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pausing` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\penalty` | `Typeset(Number)` | exact | the penalty is appended as scanned |
| `\postdisplaypenalty` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\predisplaydirection` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\predisplaypenalty` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\predisplaysize` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pretolerance` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\prevdepth` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\prevgraf` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\protected` | `Prefix(Protected)` | exact | [tex.web § 1211](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) |
| `\radical` | `Command(Number)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\raise` | `Mode(Shift { vertical: true })` | exact | [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own mode-transition rules |
| `\read` | `Read` | approximated | [tex.web § 1351](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web). Exact for a known stream number; one satex cannot pin down leaves every stream's state unknown |
| `\readline` | `Command(ReadLine)` | approximated | [eTeX manual § 3.2](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf), including the terminal-fatal case; a stream number satex cannot pin down gives an unknown line and does not end the job |
| `\relax` | `Relax` | exact | [tex.web § 1211](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) |
| `\relpenalty` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\right` | `Typeset(Delimiter)` | exact | [tex.web § 1161](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web): the delimiter is the one token the primitive itself consumes |
| `\righthyphenmin` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\rightskip` | `GlueParameter { mu: false }` | exact | read and written from the stored parameter table |
| `\romannumeral` | `Text(Roman)` | approximated | exact for a known operand. Widens when the token's meaning or a register's digits are unresolved |
| `\savinghyphcodes` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\savingvdiscards` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\scantokens` | `ScanTokens { text: false }` | exact | [eTeX manual § 3.9](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf). A capacity overflow (`MAX_IN_OPEN`) is diagnosed, not assumed away |
| `\scriptfont` | `Command(FamilyFont)` | approximated | stored and consumed by satex's own font and math-code tables; an operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\scriptscriptfont` | `Command(FamilyFont)` | approximated | stored and consumed by satex's own font and math-code tables; an operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\scriptscriptstyle` | `Command(Nothing)` | exact | no operand |
| `\scriptspace` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\scriptstyle` | `Command(Nothing)` | exact | no operand |
| `\scrollmode` | `Command(Interaction(2))` | exact | sets `\interactionmode` |
| `\setbox` | `Command(SetBox)` | exact | forwarded to the real box-scanning path |
| `\setlanguage` | `Command(Number)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\sfcode` | `CharCode(Space)` | unsupported + diagnostic | exact for one known character. Diagnosed (`unknown-char-code`) and the whole table widened when the candidates can't be enumerated at all |
| `\shipout` | `Mode(ShipOut)` | exact | removes the box from the list, [tex.web § 1073](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own effect on it |
| `\show` | `Command(Token)` | exact | `\show` |
| `\showbox` | `Command(Number)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\showboxbreadth` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\showboxdepth` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\showgroups` | `Command(Nothing)` | exact | no operand |
| `\showifs` | `Command(Nothing)` | exact | no operand |
| `\showlists` | `Command(Nothing)` | exact | no operand |
| `\showthe` | `Command(Internal)` | exact | `\showthe` |
| `\showtokens` | `Command(GeneralText)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\skewchar` | `FontParam(SkewChar)` | approximated | reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\skip` | `Register(Skip)` | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\skipdef` | `RegisterDef(Skip)` | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\spacefactor` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\spaceskip` | `GlueParameter { mu: false }` | exact | read and written from the stored parameter table |
| `\span` | `Command(Nothing)` | exact | no operand |
| `\special` | `Pdf(Special)` | approximated | the manual's keywords are scanned exactly. No PDF is built, so the payload is recorded as an occurrence, not replayed |
| `\splitbotmark` | `Convert(Mark)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |
| `\splitbotmarks` | `Convert(Marks)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |
| `\splitdiscards` | `Mode(Vertical(Discards))` | approximated | widens to Unknown. A rarely-taken [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) path this pass does not distinguish further |
| `\splitfirstmark` | `Convert(Mark)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |
| `\splitfirstmarks` | `Convert(Marks)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |
| `\splitmaxdepth` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\splittopskip` | `GlueParameter { mu: false }` | exact | read and written from the stored parameter table |
| `\string` | `Text(String)` | approximated | exact for a known operand. Widens when the token's meaning or a register's digits are unresolved |
| `\tabskip` | `GlueParameter { mu: false }` | exact | read and written from the stored parameter table |
| `\textfont` | `Command(FamilyFont)` | approximated | stored and consumed by satex's own font and math-code tables; an operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\textstyle` | `Command(Nothing)` | exact | no operand |
| `\the` | `Text(The)` | approximated | exact for a known operand. Widens when the token's meaning or a register's digits are unresolved |
| `\thickmuskip` | `GlueParameter { mu: true }` | exact | read and written from the stored parameter table |
| `\thinmuskip` | `GlueParameter { mu: true }` | exact | read and written from the stored parameter table |
| `\time` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\toks` | `Register(Toks)` | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\toksdef` | `RegisterDef(Toks)` | approximated | exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\tolerance` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\topmark` | `Convert(Mark)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |
| `\topmarks` | `Convert(Marks)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |
| `\topskip` | `GlueParameter { mu: false }` | exact | read and written from the stored parameter table |
| `\tracingassigns` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\tracingcommands` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\tracinggroups` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\tracingifs` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\tracinglostchars` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\tracingmacros` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\tracingnesting` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\tracingonline` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\tracingoutput` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\tracingpages` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\tracingparagraphs` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\tracingrestores` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\tracingscantokens` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\tracingstacklevels` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\tracingstats` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\uccode` | `CharCode(Upper)` | unsupported + diagnostic | exact for one known character. Diagnosed (`unknown-char-code`) and the whole table widened when the candidates can't be enumerated at all |
| `\uchyph` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\underline` | `Command(Nothing)` | exact | no operand |
| `\unexpanded` | `Reinject` | exact | [eTeX manual § 3.4](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf) |
| `\unhbox` | `Mode(Horizontal(Unpackage { take: true }))` | approximated | a void register adds nothing. A real one becomes Unknown, since its content is not tracked once boxed |
| `\unhcopy` | `Mode(Horizontal(Unpackage { take: false }))` | approximated | a void register adds nothing. A real one becomes Unknown, since its content is not tracked once boxed |
| `\unkern` | `Typeset(Plain)` | exact | `\unskip`, `\unkern` and `\unpenalty` remove the real tail item |
| `\unless` | `Unless` | approximated | etex.ch. Exact before a conditional; before a name of unknown meaning the conditional it may open is left undecided |
| `\unpenalty` | `Typeset(Plain)` | exact | `\unskip`, `\unkern` and `\unpenalty` remove the real tail item |
| `\unskip` | `Typeset(Plain)` | exact | `\unskip`, `\unkern` and `\unpenalty` remove the real tail item |
| `\unvbox` | `Mode(Vertical(Unpackage { take: true }))` | approximated | a void register adds nothing. A real one becomes Unknown, since its content is not tracked once boxed |
| `\unvcopy` | `Mode(Vertical(Unpackage { take: false }))` | approximated | a void register adds nothing. A real one becomes Unknown, since its content is not tracked once boxed |
| `\uppercase` | `CaseShift(Upper)` | exact | [tex.web § 1288](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web). Widens a code an unknown token was assigned |
| `\vadjust` | `Mode(Insert { vadjust: true })` | approximated | opens the insert's own list. Not told apart from the vertical list around it |
| `\valign` | `Mode(Horizontal(Align))` | approximated | cells run in the alignment's own list, not distinguished from the preamble |
| `\vbadness` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\vbox` | `Mode(Box(VBox))` | exact | the real box machinery |
| `\vcenter` | `Mode(Box(VCenter))` | exact | the real box machinery |
| `\vfil` | `Mode(Vertical(Fil))` | exact | a real item, appended to the list satex tracks |
| `\vfill` | `Mode(Vertical(Fil))` | exact | a real item, appended to the list satex tracks |
| `\vfilneg` | `Mode(Vertical(Fil))` | exact | a real item, appended to the list satex tracks |
| `\vfuzz` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\voffset` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\vrule` | `Mode(Horizontal(Rule))` | approximated | a real rule item in a horizontal list, but a `\vrule` met in a vertical list starts a paragraph and widens the enclosing box's dimensions rather than answering them exactly |
| `\vsize` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\vskip` | `Mode(Vertical(Glue))` | exact | a real item, appended to the list satex tracks |
| `\vsplit` | `Mode(Box(VSplit))` | approximated | the result always widens to Unknown, even from a void register. A real split's remainder is not tracked |
| `\vss` | `Mode(Vertical(Fil))` | exact | a real item, appended to the list satex tracks |
| `\vtop` | `Mode(Box(VTop))` | exact | the real box machinery |
| `\wd` | `Special(BoxDimen)` | approximated | round-trips through the register consistently. The box behind it is never built |
| `\widowpenalties` | `Special(Penalties)` | approximated | a table satex stores and reads back exactly for a known entry; one it cannot pin down widens every entry the write may have reached |
| `\widowpenalty` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\write` | `Write` | approximated | `\immediate\write` is exact. A deferred `\write`'s text is recorded unexpanded, since satex never reaches shipout to expand it |
| `\xdef` | `Def { global: true, expand: true }` | exact | [tex.web § 476](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web). An unterminated definition is diagnosed, not read past the file's end |
| `\xleaders` | `Mode(Leaders)` | approximated | the box or rule is scanned exactly. [tex.web § 1078](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s repeated copies are not laid out, since satex tracks natural size, not a page |
| `\xspaceskip` | `GlueParameter { mu: false }` | exact | read and written from the stored parameter table |
| `\year` | `IntegerParameter` | exact | read and written from the stored parameter table |

### pdfTeX extensions

Source: the [pdfTeX manual, "Primitives"](https://ctan.org/pkg/pdftex).

| primitive | modeled as | verdict | how |
| --- | --- | --- | --- |
| `\efcode` | `FontParam(CharCode(1000))` | approximated | reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\expanded` | `Expanded` | exact | [interface3, "Selecting tokens"](https://ctan.org/pkg/l3kernel); [pdfTeX manual, "\expanded"](https://ctan.org/pkg/pdftex) |
| `\ifincsname` | `If(InCsname)` | exact | computed from state satex already tracks (`eval_extra_condition`) |
| `\ifpdfabsdim` | `If(AbsDim)` | exact | computed from state satex already tracks (`eval_extra_condition`) |
| `\ifpdfabsnum` | `If(AbsNum)` | exact | computed from state satex already tracks (`eval_extra_condition`) |
| `\ifpdfprimitive` | `If(Primitive)` | exact | computed from state satex already tracks (`eval_extra_condition`) |
| `\ignoreprimitiveerror` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\knaccode` | `FontParam(CharCode(0))` | approximated | reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\knbccode` | `FontParam(CharCode(0))` | approximated | reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\knbscode` | `FontParam(CharCode(0))` | approximated | reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\leftmarginkern` | `Convert(MarginKern)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |
| `\letterspacefont` | `Command(CopyFont { amount: true })` | approximated | stored and consumed by satex's own font and math-code tables; an operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\lpcode` | `FontParam(CharCode(0))` | approximated | reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\partokencontext` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\partokenname` | `Command(Token)` | exact | `\show` |
| `\pdfadjustspacing` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdfannot` | `Command(PdfAnnot)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdfappendkern` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdfcatalog` | `Command(PdfCatalog)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdfcolorstack` | `Command(PdfColorStack)` | approximated | the operation and stack id are tracked. The colour text itself is a recorded occurrence, not replayed |
| `\pdfcolorstackinit` | `Convert(ColorStackInit)` | approximated | `\pdfcolorstackinit` hands out the next colour stack number, widened rather than counted exactly. The colour text is a recorded occurrence, not replayed |
| `\pdfcompresslevel` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdfcopyfont` | `Command(CopyFont { amount: false })` | approximated | stored and consumed by satex's own font and math-code tables; an operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\pdfcreationdate` | `Convert(CreationDate)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |
| `\pdfdecimaldigits` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdfdest` | `Command(PdfDest)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdfdestmargin` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pdfdraftmode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdfeachlinedepth` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pdfeachlineheight` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pdfelapsedtime` | `LastItem(ElapsedTime)` | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdfendlink` | `Command(Nothing)` | exact | no operand |
| `\pdfendthread` | `Command(Nothing)` | exact | no operand |
| `\pdfescapehex` | `Convert(EscapeHex)` | exact | the manual's own algorithm, run for real |
| `\pdfescapename` | `Convert(EscapeName)` | exact | the manual's own algorithm, run for real |
| `\pdfescapestring` | `Convert(EscapeString)` | exact | the manual's own algorithm, run for real |
| `\pdffakespace` | `Command(Nothing)` | exact | no operand |
| `\pdffiledump` | `Convert(FileDump)` | exact | reads the real file |
| `\pdffilemoddate` | `Convert(FileModDate)` | approximated | always Unknown. satex does not read file modification times |
| `\pdffilesize` | `Convert(FileSize)` | exact | reads the real file |
| `\pdffirstlineheight` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pdffontattr` | `Command(FontText)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdffontexpand` | `Command(FontExpand)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdffontname` | `Convert(PdfFontName)` | approximated | widens to Unknown. No PDF font object is built |
| `\pdffontobjnum` | `Convert(PdfFontObjNum)` | approximated | widens to Unknown. No PDF font object is built |
| `\pdffontsize` | `Convert(PdfFontSize)` | exact | the font's real design size |
| `\pdfgamma` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdfgentounicode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdfglyphtounicode` | `Command(TwoTexts)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdfhorigin` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pdfignoreddimen` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pdfimageapplygamma` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdfimagegamma` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdfimagehicolor` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdfimageresolution` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdfincludechars` | `Command(FontText)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdfinclusioncopyfonts` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdfinfo` | `Command(GeneralText)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdfinfoomitdate` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdfinsertht` | `Convert(InsertHt)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |
| `\pdfinterwordspaceoff` | `Command(Nothing)` | exact | no operand |
| `\pdfinterwordspaceon` | `Command(Nothing)` | exact | no operand |
| `\pdflastannot` | `LastItem(LastXPos)` | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflastlinedepth` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pdflastlink` | `LastItem(LastXPos)` | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflastmatch` | `Convert(LastMatch)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |
| `\pdflastobj` | `LastItem(LastXPos)` | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflastxform` | `LastItem(LastXPos)` | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflastximage` | `LastItem(LastXPos)` | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflastximagepages` | `LastItem(LastXPos)` | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflastxpos` | `LastItem(LastXPos)` | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflastypos` | `LastItem(LastYPos)` | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflinkmargin` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pdfliteral` | `Pdf(Literal)` | approximated | the manual's keywords are scanned exactly. No PDF is built, so the payload is recorded as an occurrence, not replayed |
| `\pdfmajorversion` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdfmapfile` | `Command(GeneralText)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdfmapline` | `Command(GeneralText)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdfmatch` | `Convert(Match)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |
| `\pdfmdfivesum` | `Convert(MdFiveSum)` | exact | reads the real file |
| `\pdfminorversion` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdfnames` | `Command(GeneralText)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdfnobuiltintounicode` | `Command(Font)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdfnoligatures` | `Command(Font)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdfnormaldeviate` | `Convert(NormalDeviate)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |
| `\pdfobj` | `Pdf(Object)` | approximated | the manual's keywords are scanned exactly. No PDF is built, so the payload is recorded as an occurrence, not replayed |
| `\pdfobjcompresslevel` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdfomitcharset` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdfoutline` | `Command(PdfOutline)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdfoutput` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdfpageattr` | `TokensParameter` | approximated | read and written from the stored parameter table exactly, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\pdfpagebox` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdfpageheight` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pdfpageref` | `Convert(PageRef)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |
| `\pdfpageresources` | `Pdf(Resources)` | approximated | the manual's keywords are scanned exactly. No PDF is built, so the payload is recorded as an occurrence, not replayed |
| `\pdfpagesattr` | `TokensParameter` | approximated | read and written from the stored parameter table exactly, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\pdfpagewidth` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pdfpkmode` | `TokensParameter` | approximated | read and written from the stored parameter table exactly, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\pdfpkresolution` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdfprependkern` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdfprimitive` | `Convert(Primitive)` | exact | re-injects the frozen meaning for normal expansion to re-process, as `\pdfprimitive` does |
| `\pdfprotrudechars` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdfptexuseunderscore` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdfpxdimen` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pdfrandomseed` | `LastItem(RandomSeed)` | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdfrefobj` | `Command(Number)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdfrefxform` | `Command(Number)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdfrefximage` | `Command(Number)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdfresettimer` | `Command(Nothing)` | exact | no operand |
| `\pdfrestore` | `Command(Nothing)` | exact | no operand |
| `\pdfretval` | `LastItem(LastXPos)` | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdfrunninglinkoff` | `Command(Nothing)` | exact | no operand |
| `\pdfrunninglinkon` | `Command(Nothing)` | exact | no operand |
| `\pdfsave` | `Command(Nothing)` | exact | no operand |
| `\pdfsavepos` | `Command(Nothing)` | exact | no operand |
| `\pdfsetmatrix` | `Command(GeneralText)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdfsetrandomseed` | `Command(Number)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdfshellescape` | `LastItem(ShellEscape)` | exact | the configuration's own setting |
| `\pdfstartlink` | `Command(PdfStartLink)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdfstartthread` | `Command(PdfThread)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdfstrcmp` | `Convert(StrCmp)` | exact | the manual's own algorithm, run for real |
| `\pdfsuppressptexinfo` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdftexbanner` | `Convert(PdftexBanner)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |
| `\pdftexrevision` | `Convert(PdftexRevision)` | exact | a real constant or stored name |
| `\pdftexversion` | `LastItem(PdftexVersion)` | exact | read from state satex already tracks |
| `\pdfthread` | `Command(PdfThread)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdfthreadmargin` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pdftracingfonts` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdftrailer` | `Command(GeneralText)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdftrailerid` | `Command(GeneralText)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdfunescapehex` | `Convert(UnescapeHex)` | exact | the manual's own algorithm, run for real |
| `\pdfuniformdeviate` | `Convert(UniformDeviate)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |
| `\pdfuniqueresname` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pdfvorigin` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pdfxform` | `Command(PdfXForm)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\pdfxformname` | `Convert(XFormName)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |
| `\pdfximage` | `Command(PdfXImage)` | exact | the file name reaches `observe::image` |
| `\pdfximagebbox` | `Convert(ImageBBox)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |
| `\quitvmode` | `Mode(QuitVMode)` | exact | [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own mode-transition rules |
| `\rightmarginkern` | `Convert(MarginKern)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |
| `\rpcode` | `FontParam(CharCode(0))` | approximated | reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\shbscode` | `FontParam(CharCode(0))` | approximated | reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\showstream` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\stbscode` | `FontParam(CharCode(0))` | approximated | reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\synctex` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\tagcode` | `FontParam(CharCode(1))` | approximated | reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)'s own recovery. An entry satex cannot pin down widens every one it may name |

### XeTeX extensions

Source: the [XeTeX reference](https://tug.org/xetex/).

| primitive | modeled as | verdict | how |
| --- | --- | --- | --- |
| `\Uchar` | `Convert(Uchar { catcode: false })` | exact | [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web), eTeX and XeTeX's character and category-code construction rules |
| `\Ucharcat` | `Convert(Uchar { catcode: true })` | exact | [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web), eTeX and XeTeX's character and category-code construction rules |
| `\Udelcode` | `Special(UCode { math: false, num: false })` | approximated | a table satex stores and reads back exactly for a known entry; one it cannot pin down widens every entry the write may have reached |
| `\Udelcodenum` | `Special(UCode { math: false, num: true })` | approximated | a table satex stores and reads back exactly for a known entry; one it cannot pin down widens every entry the write may have reached |
| `\Udelimiter` | `Command(Numbers(3))` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\Umathaccent` | `Command(MathAccent)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\Umathchar` | `Command(Numbers(3))` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\Umathchardef` | `Command(UMathCharDef { num: false })` | approximated | stored and consumed by satex's own font and math-code tables; an operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\Umathcharnum` | `Command(Numbers(1))` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\Umathcharnumdef` | `Command(UMathCharDef { num: true })` | approximated | stored and consumed by satex's own font and math-code tables; an operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\Umathcode` | `Special(UCode { math: true, num: false })` | approximated | a table satex stores and reads back exactly for a known entry; one it cannot pin down widens every entry the write may have reached |
| `\Umathcodenum` | `Special(UCode { math: true, num: true })` | approximated | a table satex stores and reads back exactly for a known entry; one it cannot pin down widens every entry the write may have reached |
| `\Uradical` | `Command(Numbers(2))` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\XeTeXOTcountfeatures` | `LastItem(Xetex(OtCountFeatures))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXOTcountlanguages` | `LastItem(Xetex(OtCountLanguages))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXOTcountscripts` | `LastItem(Xetex(OtCountScripts))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXOTfeaturetag` | `LastItem(Xetex(OtFeatureTag))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXOTlanguagetag` | `LastItem(Xetex(OtLanguageTag))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXOTscripttag` | `LastItem(Xetex(OtScriptTag))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXcharclass` | `Special(CharClass)` | approximated | a table satex stores and reads back exactly for a known entry; one it cannot pin down widens every entry the write may have reached |
| `\XeTeXcharglyph` | `LastItem(Xetex(CharGlyph))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXcountfeatures` | `LastItem(Xetex(CountFeatures))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXcountglyphs` | `LastItem(Xetex(CountGlyphs))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXcountselectors` | `LastItem(Xetex(CountSelectors))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXcountvariations` | `LastItem(Xetex(CountVariations))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXdashbreakstate` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\XeTeXdefaultencoding` | `Command(FileName)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\XeTeXfeaturecode` | `LastItem(Xetex(FeatureCode))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXfeaturename` | `Convert(Xetex(FeatureName))` | approximated | exact from a loaded font's real tables. Unknown once they aren't resolved |
| `\XeTeXfindfeaturebyname` | `LastItem(Xetex(FindFeatureByName))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXfindselectorbyname` | `LastItem(Xetex(FindSelectorByName))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXfindvariationbyname` | `LastItem(Xetex(FindVariationByName))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXfirstfontchar` | `LastItem(Xetex(FirstFontChar))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXfonttype` | `LastItem(Xetex(FontType))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXgenerateactualtext` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\XeTeXglyph` | `Mode(Horizontal(Char))` | exact | a real item, appended to the list satex tracks |
| `\XeTeXglyphbounds` | `LastItem(Xetex(GlyphBounds))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXglyphindex` | `LastItem(Xetex(GlyphIndex))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXglyphname` | `Convert(Xetex(GlyphName))` | approximated | exact from a loaded font's real tables. Unknown once they aren't resolved |
| `\XeTeXhyphenatablelength` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\XeTeXinputencoding` | `Command(FileName)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\XeTeXinputnormalization` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\XeTeXinterchartokenstate` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\XeTeXinterchartoks` | `Special(InterCharToks)` | approximated | a table satex stores and reads back exactly for a known entry; one it cannot pin down widens every entry the write may have reached |
| `\XeTeXinterwordspaceshaping` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\XeTeXisdefaultselector` | `LastItem(Xetex(IsDefaultSelector))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXisexclusivefeature` | `LastItem(Xetex(IsExclusiveFeature))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXlastfontchar` | `LastItem(Xetex(LastFontChar))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXlinebreaklocale` | `Command(FileName)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\XeTeXlinebreakpenalty` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\XeTeXlinebreakskip` | `GlueParameter { mu: false }` | exact | read and written from the stored parameter table |
| `\XeTeXpdffile` | `Command(PicFile)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\XeTeXpdfpagecount` | `LastItem(PdfPageCount)` | approximated | widens to Unknown. The file's page count needs parsing a PDF satex does not read |
| `\XeTeXpicfile` | `Command(PicFile)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\XeTeXprotrudechars` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\XeTeXrevision` | `Convert(XetexRevision)` | exact | a real constant or stored name |
| `\XeTeXselectorcode` | `LastItem(Xetex(SelectorCode))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXselectorname` | `Convert(Xetex(SelectorName))` | approximated | exact from a loaded font's real tables. Unknown once they aren't resolved |
| `\XeTeXtracingfonts` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\XeTeXupwardsmode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\XeTeXuseglyphmetrics` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\XeTeXvariation` | `LastItem(Xetex(Variation))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXvariationdefault` | `LastItem(Xetex(VariationDefault))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXvariationmax` | `LastItem(Xetex(VariationMax))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXvariationmin` | `LastItem(Xetex(VariationMin))` | approximated | exact from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved |
| `\XeTeXvariationname` | `Convert(Xetex(VariationName))` | approximated | exact from a loaded font's real tables. Unknown once they aren't resolved |
| `\XeTeXversion` | `LastItem(XetexVersion)` | exact | a constant |
| `\creationdate` | `Convert(CreationDate)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |
| `\elapsedtime` | `LastItem(ElapsedTime)` | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\filedump` | `Convert(FileDump)` | exact | reads the real file |
| `\filemoddate` | `Convert(FileModDate)` | approximated | always Unknown. satex does not read file modification times |
| `\filesize` | `Convert(FileSize)` | exact | reads the real file |
| `\ifprimitive` | `If(Primitive)` | exact | computed from state satex already tracks (`eval_extra_condition`) |
| `\mdfivesum` | `Convert(MdFiveSum)` | exact | reads the real file |
| `\normaldeviate` | `Convert(NormalDeviate)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |
| `\primitive` | `Convert(Primitive)` | exact | re-injects the frozen meaning for normal expansion to re-process, as `\pdfprimitive` does |
| `\randomseed` | `LastItem(RandomSeed)` | approximated | genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\resettimer` | `Command(Nothing)` | exact | no operand |
| `\setrandomseed` | `Command(Number)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\shellescape` | `LastItem(ShellEscape)` | exact | the configuration's own setting |
| `\strcmp` | `Convert(StrCmp)` | exact | the manual's own algorithm, run for real |
| `\suppressfontnotfounderror` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\uniformdeviate` | `Convert(UniformDeviate)` | approximated | operands are scanned per the manual. The answer needs state (regex captures, PDF pages, the page builder's marks, a clock, an RNG) satex does not build, so it widens to Unknown |

### LuaTeX extensions

Source: the [LuaTeX manual](https://ctan.org/pkg/luatex).

| primitive | modeled as | verdict | how |
| --- | --- | --- | --- |
| `\Udelimiterover` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Udelimiterunder` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Uhextensible` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Uleft` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathaxis` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathbinbinspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathbinclosespacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathbininnerspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathbinopenspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathbinopspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathbinordspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathbinpunctspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathbinrelspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathcharclass` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathcharfam` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathcharslot` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathclosebinspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathcloseclosespacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathcloseinnerspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathcloseopenspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathcloseopspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathcloseordspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathclosepunctspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathcloserelspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathconnectoroverlapmin` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathfractiondelsize` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathfractiondenomdown` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathfractiondenomvgap` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathfractionnumup` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathfractionnumvgap` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathfractionrule` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathinnerbinspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathinnerclosespacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathinnerinnerspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathinneropenspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathinneropspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathinnerordspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathinnerpunctspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathinnerrelspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathlimitabovebgap` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathlimitabovekern` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathlimitabovevgap` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathlimitbelowbgap` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathlimitbelowkern` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathlimitbelowvgap` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathnolimitsubfactor` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathnolimitsupfactor` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathopbinspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathopclosespacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathopenbinspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathopenclosespacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathopeninnerspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathopenopenspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathopenopspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathopenordspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathopenpunctspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathopenrelspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathoperatorsize` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathopinnerspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathopopenspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathopopspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathopordspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathoppunctspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathoprelspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathordbinspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathordclosespacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathordinnerspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathordopenspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathordopspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathordordspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathordpunctspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathordrelspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathoverbarkern` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathoverbarrule` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathoverbarvgap` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathoverdelimiterbgap` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathoverdelimitervgap` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathpunctbinspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathpunctclosespacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathpunctinnerspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathpunctopenspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathpunctopspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathpunctordspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathpunctpunctspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathpunctrelspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathquad` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathradicaldegreeafter` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathradicaldegreebefore` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathradicaldegreeraise` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathradicalkern` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathradicalrule` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathradicalvgap` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathrelbinspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathrelclosespacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathrelinnerspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathrelopenspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathrelopspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathrelordspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathrelpunctspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathrelrelspacing` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathskewedfractionhgap` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathskewedfractionvgap` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathspaceafterscript` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathstackdenomdown` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathstacknumup` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathstackvgap` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathsubshiftdown` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathsubshiftdrop` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathsubsupshiftdown` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathsubsupvgap` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathsubtopmax` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathsupbottommin` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathsupshiftdrop` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathsupshiftup` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathsupsubbottommax` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathunderbarkern` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathunderbarrule` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathunderbarvgap` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathunderdelimiterbgap` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umathunderdelimitervgap` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Umiddle` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Unosubscript` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Unosuperscript` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Uoverdelimiter` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Uright` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Uroot` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Uskewed` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Uskewedwithdelims` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Ustack` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Ustartdisplaymath` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Ustartmath` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Ustopdisplaymath` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Ustopmath` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Usubscript` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Usuperscript` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Uunderdelimiter` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\Uvextensible` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\adjustspacing` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\alignmark` | `AlignMark` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\aligntab` | `AlignTab` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\attribute` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\attributedef` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\automaticdiscretionary` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\automatichyphenmode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\automatichyphenpenalty` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\begincsname` | `BeginCsname` | exact | [LuaTeX manual, "Macros"](https://ctan.org/pkg/luatex) |
| `\bodydir` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\bodydirection` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\boundary` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\boxdir` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\boxdirection` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\breakafterdirmode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\catcodetable` | `Lua(SelectCatcodes)` | exact | a real string escape or catcode-table operation |
| `\clearmarks` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\compoundhyphenmode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\copyfont` | `Command(CopyFont { amount: false })` | approximated | stored and consumed by satex's own font and math-code tables; an operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\crampeddisplaystyle` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\crampedscriptscriptstyle` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\crampedscriptstyle` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\crampedtextstyle` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\csstring` | `Text(String)` | approximated | exact for a known operand. Widens when the token's meaning or a register's digits are unresolved |
| `\deferred` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\directlua` | `Lua(Direct)` | approximated | the chunk runs for real where a Lua host is available. A failed or unavailable run is diagnosed and widened |
| `\discretionaryligaturemode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\draftmode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\dviextension` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\dvifeedback` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\dvivariable` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\eTeXVersion` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\eTeXglueshrinkorder` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\eTeXgluestretchorder` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\eTeXminorversion` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\endlocalcontrol` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\etoksapp` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\etokspre` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\exceptionpenalty` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\exhyphenchar` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\expandglyphsinfont` | `Command(FontExpand)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\explicitdiscretionary` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\explicithyphenpenalty` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\firstvalidlanguage` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\fixupboxesmode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\fontid` | `Convert(FontId)` | exact | a real constant or stored name |
| `\formatname` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\gleaders` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\glyphdimensionsmode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\gtoksapp` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\gtokspre` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\hjcode` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\hpack` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\hyphenationbounds` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\hyphenationmin` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\hyphenpenaltymode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\ifabsdim` | `If(AbsDim)` | exact | computed from state satex already tracks (`eval_extra_condition`) |
| `\ifabsnum` | `If(AbsNum)` | exact | computed from state satex already tracks (`eval_extra_condition`) |
| `\ifcondition` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\ignoreligaturesinfont` | `Command(Font)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\immediateassigned` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\immediateassignment` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\initcatcodetable` | `Lua(InitCatcodes)` | exact | a real string escape or catcode-table operation |
| `\insertht` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\lastnamedcs` | `LastNamedCs` | exact | [LuaTeX manual, "Macros"](https://ctan.org/pkg/luatex) |
| `\lastsavedboxresourceindex` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\lastsavedimageresourceindex` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\lastsavedimageresourcepages` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\lastxpos` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\lastypos` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\latelua` | `Lua(Late)` | approximated | the chunk runs for real where a Lua host is available. A failed or unavailable run is diagnosed and widened |
| `\lateluafunction` | `Command(Number)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\leftghost` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\letcharcode` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\linedir` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\linedirection` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\localbrokenpenalty` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\localinterlinepenalty` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\localleftbox` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\localrightbox` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\luabytecode` | `Command(Number)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\luabytecodecall` | `Command(Number)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\luacopyinputnodes` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\luadef` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\luaescapestring` | `Lua(Escape)` | exact | a real string escape or catcode-table operation |
| `\luafunction` | `Command(Number)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\luafunctioncall` | `Command(Number)` | approximated | the operand is scanned exactly. The primitive's effect is typesetting or PDF output satex does not build, so nothing reads the result back |
| `\luatexbanner` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\luatexrevision` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\luatexversion` | `LastItem(LuatexVersion)` | exact | probed from the real `luatex --version` for this run |
| `\mathdefaultsmode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\mathdelimitersmode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\mathdir` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\mathdirection` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\mathdisplayskipmode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\mathemptydisplaymode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\matheqdirmode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\matheqnogapstep` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\mathflattenmode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\mathitalicsmode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\mathnolimitsmode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\mathoption` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\mathpenaltiesmode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\mathrulesfam` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\mathrulesmode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\mathrulethicknessmode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\mathscriptboxmode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\mathscriptcharmode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\mathscriptsmode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\mathstyle` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\mathsurroundmode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\mathsurroundskip` | `GlueParameter { mu: false }` | exact | read and written from the stored parameter table |
| `\nohrule` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\nokerns` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\noligs` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\nospaces` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\novrule` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\outputbox` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\outputmode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\pagebottomoffset` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pagedir` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\pagedirection` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\pageheight` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pageleftoffset` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pagerightoffset` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pagetopoffset` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pagewidth` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\pardir` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\pardirection` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\pdfextension` | `Pdf(Extension)` | approximated | the manual's keywords are scanned exactly. No PDF is built, so the payload is recorded as an occurrence, not replayed |
| `\pdffeedback` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\pdfvariable` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\postexhyphenchar` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\posthyphenchar` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\prebinoppenalty` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\predisplaygapfactor` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\preexhyphenchar` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\prehyphenchar` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\prerelpenalty` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\protrudechars` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\protrusionboundary` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\pxdimen` | `DimenParameter` | exact | read and written from the stored parameter table |
| `\rightghost` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\saveboxresource` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\savecatcodetable` | `Lua(SaveCatcodes)` | exact | a real string escape or catcode-table operation |
| `\saveimageresource` | `Command(PdfXImage)` | exact | the file name reaches `observe::image` |
| `\savepos` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\scantextokens` | `ScanTokens { text: true }` | exact | [eTeX manual § 3.9](https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf). A capacity overflow (`MAX_IN_OPEN`) is diagnosed, not assumed away |
| `\setfontid` | `Command(SetFontId)` | approximated | stored and consumed by satex's own font and math-code tables; an operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\shapemode` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\suppressifcsnameerror` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\suppresslongerror` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\suppressmathparerror` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\suppressoutererror` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\suppressprimitiveerror` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\textdir` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\textdirection` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\toksapp` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\tokspre` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\tpack` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\tracingfonts` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\useboxresource` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\useimageresource` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\variablefam` | `IntegerParameter` | exact | read and written from the stored parameter table |
| `\vpack` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\wordboundary` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\xtoksapp` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |
| `\xtokspre` | `Unmodeled` | unsupported + diagnostic | `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else |

**925 of 925 primitives modeled soundly.**
