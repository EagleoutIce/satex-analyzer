<!-- generated from doc/src/wiki/primitive-coverage.md.in by satex-doc; do not edit -->

<p align="center"><a href="https://eagleoutice.github.io/satex-analyzer/"><img src="https://raw.githubusercontent.com/EagleoutIce/satex-analyzer/main/doc/wiki/img/satex-wide.svg" alt="SaTeX: Static Analysis for (La)TeX and Friends" width="600"></a></p>

# Primitive coverage

The [formal model](Formal-semantics) gives each primitive a rule. This chapter says what the interpreter does with it.

[tex.web](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web) parts 24-49 define TeX82's primitives. The [eTeX manual](https://mirrors.ctan.org/systems/doc/etex/etex_man.pdf), the [pdfTeX manual](https://mirrors.ctan.org/systems/doc/pdftex/manual/pdftex-a.pdf), the [XeTeX reference](https://tug.org/xetex/) and the [LuaTeX manual](https://mirrors.ctan.org/systems/doc/luatex/luatex.pdf) add the rest.

`src/engine/builtins.rs` maps every one of these names to a `Primitive`, and `builtins::verdict` judges each `Primitive` from what `src/engine/exec.rs`, `src/engine/mode.rs` and `src/engine/commands.rs` do with it. This page is generated from both, so it cannot drift from the interpreter. Every **modeled as** cell links to the arm of `builtins::verdict` that judges the primitive, built from `file!()` and `line!()` at that arm rather than from a table typed by hand. See [Domains](Domains) for the values a primitive's operands and results are tracked as, and [Lints](Lints) and [Chktex-Coverage](Chktex-Coverage) for what a lint rule builds on top of a primitive's real effect.

Each primitive is modeled with one of three fidelities:

- **exact**: satex reproduces the engine's behavior.
- **approximated**: a sound over-approximation. The table says how.
- **unsupported + diagnostic**: not modeled, but everything it could affect is widened and the run says so.

Five answers in the **how** column repeat over many rows. They are written out once here and abbreviated in the table:

- **widens the mode**: `Machine::note_gap` plus widening the mode. The primitive is a meaning of its own for `\ifx`, nothing else.
- **the stored parameter table**: the value is read and written from the parameter table satex stores.
- **scanned, not built**: the operand is scanned exactly. The effect is typesetting or PDF output satex does not build, so nothing reads the result back.
- **scanned, answer unknown**: operands are scanned per the manual. The answer needs state satex does not build (regex captures, PDF pages, the page builder's marks, a clock, an RNG), so it widens to Unknown.
- **the font's real tables**: read from a loaded font's real TFM or OpenType tables. Unknown once the font or its tables aren't resolved.

A primitive that scans its operands and drops their effect with neither of the last two is a bug, not a fourth kind. satex never rests at "unsupported" silently ([Domains § Soundness invariant](Domains#soundness-invariant)). Any found this way are fixed, not documented as sound, and listed separately below until they are.

## TeX82 & eTeX ✓

**Fully modeled**: all 392 primitives, 225 of them exactly. Source: [tex.web](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web) parts 24-49. eTeX is in every engine but Knuth's `tex` (`Engine::has_etex`), so its primitives are listed with TeX82's, not apart.

| primitive | modeled as | fidelity | how |
| --- | --- | --- | --- |
| `\ ` (control space) | [`Mode(Horizontal(Space))`][builtins-2858] | exact | Real item, appended to the list satex tracks |
| `\-` | [`Mode(Horizontal(Hyphen))`][builtins-2865] | approximated | Widens to Unknown. A rarely-taken [tex.web](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web) path this pass does not distinguish further |
| `\/` | [`Command(Nothing)`][builtins-2734] | approximated | No operand. The size of what `\span`, a fraction or `\overline`/`\underline` builds is widened, not computed |
| `\TeXXeTstate` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\above` | [`Command(Fraction { delims: false, dimen: true })`][builtins-2751] | approximated | List so far becomes the numerator and the list starts afresh ([tex.web § 1181](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22252)). The fraction's size widens |
| `\abovedisplayshortskip` | [`GlueParameter { mu: false }`][builtins-2567] | exact | Stored parameter table |
| `\abovedisplayskip` | [`GlueParameter { mu: false }`][builtins-2567] | exact | Stored parameter table |
| `\abovewithdelims` | [`Command(Fraction { delims: true, dimen: true })`][builtins-2751] | approximated | List so far becomes the numerator and the list starts afresh ([tex.web § 1181](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22252)). The fraction's size widens |
| `\accent` | [`Accent`][builtins-2603] | exact | Tex.web [§§ 1123](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L21488)–[1125](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L21526) for TFM fonts: the accent box, its shift by the x-height, the kerns and the character after the assignments. Widens where the font, either character or what follows is not known |
| `\adjdemerits` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\advance` | [`Arith(Advance)`][builtins-2562] | approximated | Exact for known operands. Interval arithmetic and [tex.web § 222](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L4513)'s zero default otherwise |
| `\afterassignment` | [`Defer(Assignment)`][builtins-2548] | exact | Tex.web [§§ 1211](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22720), [1](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L95) (semi-simple groups, `\aftergroup`/`\afterassignment`) |
| `\aftergroup` | [`Defer(Group)`][builtins-2548] | exact | Tex.web [§§ 1211](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22720), [1](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L95) (semi-simple groups, `\aftergroup`/`\afterassignment`) |
| `\atop` | [`Command(Fraction { delims: false, dimen: false })`][builtins-2751] | approximated | List so far becomes the numerator and the list starts afresh ([tex.web § 1181](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22252)). The fraction's size widens |
| `\atopwithdelims` | [`Command(Fraction { delims: true, dimen: false })`][builtins-2751] | approximated | List so far becomes the numerator and the list starts afresh ([tex.web § 1181](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22252)). The fraction's size widens |
| `\badness` | [`LastItem(Badness)`][builtins-2686] | approximated | Always Unknown. Badness needs the line-breaking satex does not run |
| `\baselineskip` | [`GlueParameter { mu: false }`][builtins-2567] | exact | Stored parameter table |
| `\batchmode` | [`Command(Interaction(0))`][builtins-2755] | exact | Sets `\interactionmode` |
| `\beginL` | [`Mode(Horizontal(LR))`][builtins-2858] | exact | Real item, appended to the list satex tracks |
| `\beginR` | [`Mode(Horizontal(LR))`][builtins-2858] | exact | Real item, appended to the list satex tracks |
| `\begingroup` | [`BeginGroup`][builtins-2548] | exact | Tex.web [§§ 1211](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22720), [1](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L95) (semi-simple groups, `\aftergroup`/`\afterassignment`) |
| `\belowdisplayshortskip` | [`GlueParameter { mu: false }`][builtins-2567] | exact | Stored parameter table |
| `\belowdisplayskip` | [`GlueParameter { mu: false }`][builtins-2567] | exact | Stored parameter table |
| `\binoppenalty` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\botmark` | [`Convert(Mark(2))`][builtins-2810] | approximated | Scanned, answer unknown |
| `\botmarks` | [`Convert(Marks(2))`][builtins-2810] | approximated | Scanned, answer unknown |
| `\box` | [`Register(Box)`][builtins-2553] | approximated | Exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\boxmaxdepth` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\brokenpenalty` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\catcode` | [`CatcodeAssign`][builtins-2573] | approximated | Exact for one known character and category. Through an unknown character or value (`unknown-catcode`) each character it may name keeps its category or takes one it may be given, structural ones included. The reader splits the path on such a character |
| `\char` | [`Mode(Horizontal(Char))`][builtins-2858] | exact | Real item, appended to the list satex tracks |
| `\chardef` | [`RegisterDef(Char)`][builtins-2553] | approximated | Exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\cleaders` | [`Mode(Leaders)`][builtins-2830] | approximated | Box or rule is scanned exactly. [tex.web § 1078](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L20920)'s repeated copies are not laid out, since satex tracks natural size, not a page |
| `\closein` | [`CloseIn`][builtins-2558] | approximated | Tex.web [§ 1351](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L24696). Exact for a known stream number. One satex cannot pin down leaves every stream's state unknown |
| `\closeout` | [`Command(CloseOut)`][builtins-2759] | approximated | Tracks the stream state a later `\input` of the written file reads back. A stream number satex cannot pin down widens every stream |
| `\clubpenalties` | [`Special(Penalties)`][builtins-2723] | approximated | Table satex stores and reads back exactly for a known entry. One it cannot pin down widens every entry the write may have reached |
| `\clubpenalty` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\copy` | [`Mode(Box(Copy))`][builtins-2878] | exact | Real box machinery |
| `\count` | [`Register(Count)`][builtins-2553] | approximated | Exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\countdef` | [`RegisterDef(Count)`][builtins-2553] | approximated | Exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\cr` | [`Command(Nothing)`][builtins-2734] | approximated | No operand. The size of what `\span`, a fraction or `\overline`/`\underline` builds is widened, not computed |
| `\crcr` | [`Command(Nothing)`][builtins-2734] | approximated | No operand. The size of what `\span`, a fraction or `\overline`/`\underline` builds is widened, not computed |
| `\csname` | [`Csname`][builtins-2530] | approximated | Exact once the name is resolved. A bounded join of candidate meanings when it is built from unknown digits (`one_of_names`) |
| `\currentgrouplevel` | [`LastItem(GroupLevel)`][builtins-2688] | exact | Read from state satex already tracks |
| `\currentgrouptype` | [`LastItem(GroupType)`][builtins-2688] | exact | Read from state satex already tracks |
| `\currentifbranch` | [`LastItem(IfBranch)`][builtins-2691] | approximated | Exact once the conditional stack has settled. Unknown while a branch is undecided |
| `\currentiflevel` | [`LastItem(IfLevel)`][builtins-2691] | approximated | Exact once the conditional stack has settled. Unknown while a branch is undecided |
| `\currentiftype` | [`LastItem(IfType)`][builtins-2691] | approximated | Exact once the conditional stack has settled. Unknown while a branch is undecided |
| `\day` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\deadcycles` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\def` | [`Def { global: false, expand: false }`][builtins-2521] | exact | Tex.web [§ 476](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L9357). An unterminated definition is diagnosed, not read past the file's end |
| `\defaulthyphenchar` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\defaultskewchar` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\delcode` | [`CharCode(Delimiter)`][builtins-2577] | approximated | Exact for one known character and code. Through an unknown character or value (`unknown-char-code`) each entry it may name holds its old code or the new, read back as that set |
| `\delimiter` | [`Command(Number)`][builtins-2779] | approximated | Scanned, not built |
| `\delimiterfactor` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\delimitershortfall` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\detokenize` | [`Text(Detokenize)`][builtins-2626] | approximated | Exact for a known operand. Widens when the token's meaning or a register's digits are unresolved |
| `\dimen` | [`Register(Dimen)`][builtins-2553] | approximated | Exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\dimendef` | [`RegisterDef(Dimen)`][builtins-2553] | approximated | Exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\dimexpr` | [`Expr(Dimen)`][builtins-2597] | exact | ETeX manual [§§ 3](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L195), [5](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L268) |
| `\discretionary` | [`Mode(Horizontal(Discretionary))`][builtins-2858] | exact | Real item, appended to the list satex tracks |
| `\displayindent` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\displaylimits` | [`Command(Limits)`][builtins-2743] | approximated | No operand. Where the scripts go is not followed, so the formula's size widens |
| `\displaystyle` | [`Mode(MathStyle(0))`][builtins-2842] | approximated | Tex.web [§ 1171](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22139): the style of the atoms after it in inline math. Scripts in display style widen |
| `\displaywidowpenalties` | [`Special(Penalties)`][builtins-2723] | approximated | Table satex stores and reads back exactly for a known entry. One it cannot pin down widens every entry the write may have reached |
| `\displaywidowpenalty` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\displaywidth` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\divide` | [`Arith(Divide)`][builtins-2562] | approximated | Exact for known operands. Interval arithmetic and [tex.web § 222](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L4513)'s zero default otherwise |
| `\doublehyphendemerits` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\dp` | [`Special(BoxDimen)`][builtins-2721] | approximated | Round-trips through the register consistently. The box behind it is never built |
| `\dump` | [`End`][builtins-2591] | exact | Tex.web [§ 1054](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L20477) (`end_job`) |
| `\eTeXrevision` | [`Convert(EtexRevision)`][builtins-2788] | exact | Real constant or stored name |
| `\eTeXversion` | [`LastItem(EtexVersion)`][builtins-2688] | exact | Read from state satex already tracks |
| `\edef` | [`Def { global: false, expand: true }`][builtins-2521] | exact | Tex.web [§ 476](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L9357). An unterminated definition is diagnosed, not read past the file's end |
| `\else` | [`Else`][builtins-2541] | exact | Tex.web [§ 510](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L9892), including `insert_relax` for an else or fi met mid-test |
| `\emergencystretch` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\end` | [`End`][builtins-2591] | exact | Tex.web [§ 1054](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L20477) (`end_job`) |
| `\endL` | [`Mode(Horizontal(LR))`][builtins-2858] | exact | Real item, appended to the list satex tracks |
| `\endR` | [`Mode(Horizontal(LR))`][builtins-2858] | exact | Real item, appended to the list satex tracks |
| `\endcsname` | [`Endcsname`][builtins-2534] | exact | [LuaTeX manual, "Macros"](https://ctan.org/pkg/luatex) |
| `\endgroup` | [`EndGroup`][builtins-2548] | exact | Tex.web [§§ 1211, 1](https://ctan.org/pkg/luatex) (semi-simple groups, `\aftergroup`/`\afterassignment`) |
| `\endinput` | [`Endinput`][builtins-2590] | exact | Tex.web [§ 362](https://ctan.org/pkg/luatex): under an undecided conditional each path ends its own file |
| `\endlinechar` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\eqno` | [`Mode(EqNo)`][builtins-2819] | approximated | Opens the equation number. The display's placement ([tex.web § 1199](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22500)) is widened |
| `\errhelp` | [`TokensParameter`][builtins-2569] | approximated | Stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\errmessage` | [`Message { error: true }`][builtins-2592] | exact | Tex.web [§ 1279](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L23541) |
| `\errorcontextlines` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\errorstopmode` | [`Command(Interaction(3))`][builtins-2755] | exact | Sets `\interactionmode` |
| `\escapechar` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\everycr` | [`TokensParameter`][builtins-2569] | approximated | Stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\everydisplay` | [`TokensParameter`][builtins-2569] | approximated | Stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\everyeof` | [`TokensParameter`][builtins-2569] | approximated | Stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\everyhbox` | [`TokensParameter`][builtins-2569] | approximated | Stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\everyjob` | [`TokensParameter`][builtins-2569] | approximated | Stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\everymath` | [`TokensParameter`][builtins-2569] | approximated | Stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\everypar` | [`TokensParameter`][builtins-2569] | approximated | Stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\everyvbox` | [`TokensParameter`][builtins-2569] | approximated | Stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\exhyphenpenalty` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\expandafter` | [`ExpandAfter`][builtins-2525] | exact | Tex.web [§ 367](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L7681) |
| `\fam` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\fi` | [`Fi`][builtins-2541] | exact | Tex.web [§ 510](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L9892), including `insert_relax` for an else or fi met mid-test |
| `\finalhyphendemerits` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\firstmark` | [`Convert(Mark(1))`][builtins-2810] | approximated | Scanned, answer unknown |
| `\firstmarks` | [`Convert(Marks(1))`][builtins-2810] | approximated | Scanned, answer unknown |
| `\floatingpenalty` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\font` | [`FontDef`][builtins-2557] | exact | Tex.web [§ 1257](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L23346) (`\font`) |
| `\fontchardp` | [`LastItem(FontCharDp)`][builtins-2697] | approximated | Exact for a TFM font ([etex_man § 3.10](https://mirrors.ctan.org/systems/doc/etex/etex_man.pdf)). A native font's height, depth and italic correction are not read, so it widens to Unknown |
| `\fontcharht` | [`LastItem(FontCharHt)`][builtins-2697] | approximated | Exact for a TFM font ([etex_man § 3.10](https://mirrors.ctan.org/systems/doc/etex/etex_man.pdf)). A native font's height, depth and italic correction are not read, so it widens to Unknown |
| `\fontcharic` | [`LastItem(FontCharIc)`][builtins-2697] | approximated | Exact for a TFM font ([etex_man § 3.10](https://mirrors.ctan.org/systems/doc/etex/etex_man.pdf)). A native font's height, depth and italic correction are not read, so it widens to Unknown |
| `\fontcharwd` | [`LastItem(FontCharWd)`][builtins-2696] | exact | TFM's or a native font's real glyph advance |
| `\fontdimen` | [`FontParam(Dimen)`][builtins-2582] | approximated | Reads the stored font table. A read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L11284)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\fontname` | [`Convert(FontName)`][builtins-2788] | exact | Real constant or stored name |
| `\futurelet` | [`Let { global: false, future: true }`][builtins-2523] | exact | Tex.web [§ 1221](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22838), including the two-token `\futurelet` lookahead |
| `\gdef` | [`Def { global: true, expand: false }`][builtins-2521] | exact | Tex.web [§ 476](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L9357). An unterminated definition is diagnosed, not read past the file's end |
| `\global` | [`Prefix(Global)`][builtins-2524] | exact | Tex.web [§ 1211](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22720) |
| `\globaldefs` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\glueexpr` | [`Expr(Skip)`][builtins-2597] | exact | ETeX manual [§§ 3](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L195), [5](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L268) |
| `\glueshrink` | [`LastItem(GlueShrink)`][builtins-2694] | exact | Computed from a known glue spec |
| `\glueshrinkorder` | [`LastItem(GlueShrinkOrder)`][builtins-2694] | exact | Computed from a known glue spec |
| `\gluestretch` | [`LastItem(GlueStretch)`][builtins-2694] | exact | Computed from a known glue spec |
| `\gluestretchorder` | [`LastItem(GlueStretchOrder)`][builtins-2694] | exact | Computed from a known glue spec |
| `\gluetomu` | [`LastItem(GlueToMu)`][builtins-2694] | exact | Computed from a known glue spec |
| `\halign` | [`Mode(Vertical(Align))`][builtins-2870] | approximated | Cells run in the alignment's own list, not distinguished from the preamble |
| `\hangafter` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\hangindent` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\hbadness` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\hbox` | [`Mode(Box(HBox))`][builtins-2878] | exact | Real box machinery |
| `\hfil` | [`Mode(Horizontal(Fil(0)))`][builtins-2858] | exact | Real item, appended to the list satex tracks |
| `\hfill` | [`Mode(Horizontal(Fil(1)))`][builtins-2858] | exact | Real item, appended to the list satex tracks |
| `\hfilneg` | [`Mode(Horizontal(Fil(3)))`][builtins-2858] | exact | Real item, appended to the list satex tracks |
| `\hfuzz` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\hoffset` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\holdinginserts` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\hrule` | [`Mode(Vertical(Rule))`][builtins-2858] | exact | Real item, appended to the list satex tracks |
| `\hsize` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\hskip` | [`Mode(Horizontal(Glue))`][builtins-2858] | exact | Real item, appended to the list satex tracks |
| `\hss` | [`Mode(Horizontal(Fil(2)))`][builtins-2858] | exact | Real item, appended to the list satex tracks |
| `\ht` | [`Special(BoxDimen)`][builtins-2721] | approximated | Round-trips through the register consistently. The box behind it is never built |
| `\hyphenation` | [`Command(GeneralText)`][builtins-2779] | approximated | Scanned, not built |
| `\hyphenchar` | [`FontParam(HyphenChar)`][builtins-2582] | approximated | Reads the stored font table. A read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L11284)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\hyphenpenalty` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\if` | [`If(Chars)`][builtins-2650] | approximated | Exact for known characters. Undecided when either operand is an unresolved character |
| `\ifcase` | [`If(Case)`][builtins-2653] | approximated | Tex.web [§ 509](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L9875). Every arm is analyzed as its own path when the selector is unknown |
| `\ifcat` | [`If(Catcodes)`][builtins-2650] | approximated | Exact for known characters. Undecided when either operand is an unresolved character |
| `\ifcsname` | [`If(Csname)`][builtins-2645] | exact | Builds the name and looks it up |
| `\ifdefined` | [`If(Defined)`][builtins-2643] | approximated | Exact for a known name. Narrowed by which names could still be undefined otherwise |
| `\ifdim` | [`If(Dim)`][builtins-2647] | approximated | Exact for known operands. Narrows the interval on each branch otherwise |
| `\ifeof` | [`If(Eof)`][builtins-2655] | approximated | Satex resolves the real file, so end-of-file is known for a known stream number. One it cannot pin down leaves the test undecided |
| `\iffalse` | [`If(False)`][builtins-2637] | exact | Constant |
| `\iffontchar` | [`If(FontChar)`][builtins-2671] | exact | ETeX manual [§§ 3](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L195), [10](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L348): read from the font's real character table |
| `\ifhbox` | [`If(Box)`][builtins-2664] | approximated | Exact for a known box state (void, `\hbox`, `\vbox`). Undecided once the register is Unknown |
| `\ifhmode` | [`If(HMode)`][builtins-2660] | approximated | Decided from the mode satex tracks through the run. Open groups the run couldn't follow leave it undecided |
| `\ifinner` | [`If(InnerMode)`][builtins-2660] | approximated | Decided from the mode satex tracks through the run. Open groups the run couldn't follow leave it undecided |
| `\ifmmode` | [`If(MathMode)`][builtins-2660] | approximated | Decided from the mode satex tracks through the run. Open groups the run couldn't follow leave it undecided |
| `\ifnum` | [`If(Num)`][builtins-2647] | approximated | Exact for known operands. Narrows the interval on each branch otherwise |
| `\ifodd` | [`If(Odd)`][builtins-2647] | approximated | Exact for known operands. Narrows the interval on each branch otherwise |
| `\iftrue` | [`If(True)`][builtins-2637] | exact | Constant |
| `\ifvbox` | [`If(Box)`][builtins-2664] | approximated | Exact for a known box state (void, `\hbox`, `\vbox`). Undecided once the register is Unknown |
| `\ifvmode` | [`If(VMode)`][builtins-2660] | approximated | Decided from the mode satex tracks through the run. Open groups the run couldn't follow leave it undecided |
| `\ifvoid` | [`If(Box)`][builtins-2664] | approximated | Exact for a known box state (void, `\hbox`, `\vbox`). Undecided once the register is Unknown |
| `\ifx` | [`If(IfX)`][builtins-2638] | approximated | Exact for known meanings ([tex.web § 507](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L9840)'s `original` handles a redefined primitive). Member-by-member across a set of possible meanings otherwise, decided only when they agree |
| `\ignorespaces` | [`IgnoreSpaces`][builtins-2599] | exact | Tex.web [§ 1045](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L20377) |
| `\immediate` | [`Prefix(Immediate)`][builtins-2524] | exact | Tex.web [§ 1211](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22720) |
| `\indent` | [`Mode(StartPar { indent: true })`][builtins-2822] | exact | Tex.web's own mode-transition rules |
| `\input` | [`Load(Input)`][builtins-2589] | exact | Tex.web [§ 537](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L10365) (`\input`) |
| `\inputlineno` | [`LastItem(InputLineNo)`][builtins-2688] | exact | Read from state satex already tracks |
| `\insert` | [`Mode(Insert { vadjust: false })`][builtins-2836] | approximated | Opens the insert's own list. Not told apart from the vertical list around it |
| `\insertpenalties` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\interactionmode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\interlinepenalties` | [`Special(Penalties)`][builtins-2723] | approximated | Table satex stores and reads back exactly for a known entry. One it cannot pin down widens every entry the write may have reached |
| `\interlinepenalty` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\jobname` | [`Text(JobName)`][builtins-2625] | exact | Run's real main file name |
| `\kern` | [`Typeset(Dimen)`][builtins-2896] | approximated | `\kern` is exact. `\mkern`'s math-unit dimension always widens to Unknown, since it needs math-font setup satex does not run |
| `\language` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\lastbox` | [`Mode(Box(LastBox))`][builtins-2879] | approximated | Real box machinery for the one box of a vertical list. A box taken from a horizontal list widens |
| `\lastkern` | [`LastItem(Kern)`][builtins-2680] | approximated | Reads the tail of the list satex tracks. Unknown once the tail itself is |
| `\lastlinefit` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\lastnodetype` | [`LastItem(NodeType)`][builtins-2682] | approximated | Reads the tail of the list: a character, a ligature, glue, a kern or a penalty, and -1 for the empty main list (etex.ch). Unknown otherwise |
| `\lastpenalty` | [`LastItem(Penalty)`][builtins-2680] | approximated | Reads the tail of the list satex tracks. Unknown once the tail itself is |
| `\lastskip` | [`LastItem(Skip)`][builtins-2680] | approximated | Reads the tail of the list satex tracks. Unknown once the tail itself is |
| `\lccode` | [`CharCode(Lower)`][builtins-2577] | approximated | Exact for one known character and code. Through an unknown character or value (`unknown-char-code`) each entry it may name holds its old code or the new, read back as that set |
| `\leaders` | [`Mode(Leaders)`][builtins-2830] | approximated | Box or rule is scanned exactly. [tex.web § 1078](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L20920)'s repeated copies are not laid out, since satex tracks natural size, not a page |
| `\left` | [`Mode(LeftRight { opens: true, closes: false })`][builtins-2846] | approximated | Math left group is kept ([tex.web § 1191](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22367)). The size of the list it delimits widens |
| `\lefthyphenmin` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\leftskip` | [`GlueParameter { mu: false }`][builtins-2567] | exact | Stored parameter table |
| `\leqno` | [`Mode(EqNo)`][builtins-2819] | approximated | Opens the equation number. The display's placement ([tex.web § 1199](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22500)) is widened |
| `\let` | [`Let { global: false, future: false }`][builtins-2523] | exact | Tex.web [§ 1221](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22838), including the two-token `\futurelet` lookahead |
| `\limits` | [`Command(Limits)`][builtins-2743] | approximated | No operand. Where the scripts go is not followed, so the formula's size widens |
| `\linepenalty` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\lineskip` | [`GlueParameter { mu: false }`][builtins-2567] | exact | Stored parameter table |
| `\lineskiplimit` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\long` | [`Prefix(Long)`][builtins-2524] | exact | Tex.web [§ 1211](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22720) |
| `\looseness` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\lower` | [`Mode(Shift { vertical: true })`][builtins-2822] | exact | Tex.web's own mode-transition rules |
| `\lowercase` | [`CaseShift(Lower)`][builtins-2581] | exact | Tex.web [§ 1288](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L23608). Widens a code an unknown token was assigned |
| `\mag` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\mark` | [`Command(GeneralText)`][builtins-2779] | approximated | Scanned, not built |
| `\marks` | [`Command(NumberText)`][builtins-2779] | approximated | Scanned, not built |
| `\mathaccent` | [`Command(Number)`][builtins-2779] | approximated | Scanned, not built |
| `\mathbin` | [`Command(MathComp(2))`][builtins-2738] | approximated | Sets the class of the next atom, whose size and spacing are followed. An operator whose nucleus is a list widens ([tex.web § 749](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L14663) centres a one-character list) |
| `\mathchar` | [`Command(Number)`][builtins-2779] | approximated | Scanned, not built |
| `\mathchardef` | [`RegisterDef(MathChar)`][builtins-2553] | approximated | Exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\mathchoice` | [`Mode(MathChoice)`][builtins-2841] | exact | Tex.web [§ 1172](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22146): opens all four styles in sequence |
| `\mathclose` | [`Command(MathComp(5))`][builtins-2738] | approximated | Sets the class of the next atom, whose size and spacing are followed. An operator whose nucleus is a list widens ([tex.web § 749](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L14663) centres a one-character list) |
| `\mathcode` | [`CharCode(Math)`][builtins-2577] | approximated | Exact for one known character and code. Through an unknown character or value (`unknown-char-code`) each entry it may name holds its old code or the new, read back as that set |
| `\mathinner` | [`Command(MathComp(7))`][builtins-2738] | approximated | Sets the class of the next atom, whose size and spacing are followed. An operator whose nucleus is a list widens ([tex.web § 749](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L14663) centres a one-character list) |
| `\mathop` | [`Command(MathComp(1))`][builtins-2738] | approximated | Sets the class of the next atom, whose size and spacing are followed. An operator whose nucleus is a list widens ([tex.web § 749](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L14663) centres a one-character list) |
| `\mathopen` | [`Command(MathComp(4))`][builtins-2738] | approximated | Sets the class of the next atom, whose size and spacing are followed. An operator whose nucleus is a list widens ([tex.web § 749](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L14663) centres a one-character list) |
| `\mathord` | [`Command(MathComp(0))`][builtins-2738] | approximated | Sets the class of the next atom, whose size and spacing are followed. An operator whose nucleus is a list widens ([tex.web § 749](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L14663) centres a one-character list) |
| `\mathpunct` | [`Command(MathComp(6))`][builtins-2738] | approximated | Sets the class of the next atom, whose size and spacing are followed. An operator whose nucleus is a list widens ([tex.web § 749](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L14663) centres a one-character list) |
| `\mathrel` | [`Command(MathComp(3))`][builtins-2738] | approximated | Sets the class of the next atom, whose size and spacing are followed. An operator whose nucleus is a list widens ([tex.web § 749](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L14663) centres a one-character list) |
| `\mathsurround` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\maxdeadcycles` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\maxdepth` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\meaning` | [`Text(Meaning)`][builtins-2626] | approximated | Exact for a known operand. Widens when the token's meaning or a register's digits are unresolved |
| `\medmuskip` | [`GlueParameter { mu: true }`][builtins-2567] | exact | Stored parameter table |
| `\message` | [`Message { error: false }`][builtins-2592] | exact | Tex.web [§ 1279](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L23541) |
| `\middle` | [`Mode(LeftRight { opens: true, closes: true })`][builtins-2846] | approximated | Math left group is kept ([tex.web § 1191](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22367)). The size of the list it delimits widens |
| `\mkern` | [`Typeset(Dimen)`][builtins-2896] | approximated | `\kern` is exact. `\mkern`'s math-unit dimension always widens to Unknown, since it needs math-font setup satex does not run |
| `\month` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\moveleft` | [`Mode(Shift { vertical: false })`][builtins-2822] | exact | Tex.web's own mode-transition rules |
| `\moveright` | [`Mode(Shift { vertical: false })`][builtins-2822] | exact | Tex.web's own mode-transition rules |
| `\mskip` | [`Typeset(Glue)`][builtins-2904] | approximated | `\mskip` appends an item of unknown size: its glue is in math units, which need math-font setup satex does not run |
| `\muexpr` | [`Expr(MuSkip)`][builtins-2597] | exact | ETeX manual [§§ 3](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L195), [5](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L268) |
| `\multiply` | [`Arith(Multiply)`][builtins-2562] | approximated | Exact for known operands. Interval arithmetic and [tex.web § 222](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L4513)'s zero default otherwise |
| `\muskip` | [`Register(MuSkip)`][builtins-2553] | approximated | Exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\muskipdef` | [`RegisterDef(MuSkip)`][builtins-2553] | approximated | Exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\mutoglue` | [`LastItem(MuToGlue)`][builtins-2694] | exact | Computed from a known glue spec |
| `\newlinechar` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\noalign` | [`Mode(NoAlign)`][builtins-2839] | approximated | Cells run in the alignment's own list, not distinguished from the preamble |
| `\noboundary` | [`Mode(Horizontal(NoBoundary))`][builtins-2858] | exact | Real item, appended to the list satex tracks |
| `\noexpand` | [`NoExpand`][builtins-2526] | exact | Tex.web [§§ 367](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L7681), [358](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L7503) |
| `\noindent` | [`Mode(StartPar { indent: false })`][builtins-2822] | exact | Tex.web's own mode-transition rules |
| `\nolimits` | [`Command(Limits)`][builtins-2743] | approximated | No operand. Where the scripts go is not followed, so the formula's size widens |
| `\nonscript` | [`Command(NonScript)`][builtins-2746] | approximated | Glue of no size in math ([tex.web § 1171](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22139)). Script styles, which cancel the next glue, widen |
| `\nonstopmode` | [`Command(Interaction(1))`][builtins-2755] | exact | Sets `\interactionmode` |
| `\nulldelimiterspace` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\nullfont` | [`FontIdent(0)`][builtins-2586] | exact | Tex.web [§ 1217](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22811): selecting a font is a local assignment |
| `\number` | [`Text(Number)`][builtins-2626] | approximated | Exact for a known operand. Widens when the token's meaning or a register's digits are unresolved |
| `\numexpr` | [`Expr(Count)`][builtins-2597] | exact | ETeX manual [§§ 3](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L195), [5](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L268) |
| `\omit` | [`Command(Nothing)`][builtins-2734] | approximated | No operand. The size of what `\span`, a fraction or `\overline`/`\underline` builds is widened, not computed |
| `\openin` | [`OpenIn`][builtins-2558] | approximated | Tex.web [§ 1351](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L24696). Exact for a known stream number. One satex cannot pin down leaves every stream's state unknown |
| `\openout` | [`Command(OpenOut)`][builtins-2759] | approximated | Tracks the stream state a later `\input` of the written file reads back. A stream number satex cannot pin down widens every stream |
| `\or` | [`Or`][builtins-2541] | exact | Tex.web [§ 510](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L9892), including `insert_relax` for an else or fi met mid-test |
| `\outer` | [`Prefix(Outer)`][builtins-2524] | exact | Tex.web [§ 1211](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22720) |
| `\output` | [`TokensParameter`][builtins-2569] | approximated | Stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\outputpenalty` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\over` | [`Command(Fraction { delims: false, dimen: false })`][builtins-2751] | approximated | List so far becomes the numerator and the list starts afresh ([tex.web § 1181](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22252)). The fraction's size widens |
| `\overfullrule` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\overline` | [`Command(Nothing)`][builtins-2734] | approximated | No operand. The size of what `\span`, a fraction or `\overline`/`\underline` builds is widened, not computed |
| `\overwithdelims` | [`Command(Fraction { delims: true, dimen: false })`][builtins-2751] | approximated | List so far becomes the numerator and the list starts afresh ([tex.web § 1181](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22252)). The fraction's size widens |
| `\pagedepth` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pagediscards` | [`Mode(Vertical(Discards(false)))`][builtins-2865] | approximated | Widens to Unknown. A rarely-taken [tex.web](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web) path this pass does not distinguish further |
| `\pagefilllstretch` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pagefillstretch` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pagefilstretch` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pagegoal` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pageshrink` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pagestretch` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pagetotal` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\par` | [`Mode(Par)`][builtins-2822] | exact | Tex.web's own mode-transition rules |
| `\parfillskip` | [`GlueParameter { mu: false }`][builtins-2567] | exact | Stored parameter table |
| `\parindent` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\parshape` | [`Special(ParShape)`][builtins-2723] | approximated | Table satex stores and reads back exactly for a known entry. One it cannot pin down widens every entry the write may have reached |
| `\parshapedimen` | [`LastItem(ParShapeDimen)`][builtins-2701] | exact | Read from the stored `\parshape` |
| `\parshapeindent` | [`LastItem(ParShapeIndent)`][builtins-2701] | exact | Read from the stored `\parshape` |
| `\parshapelength` | [`LastItem(ParShapeLength)`][builtins-2701] | exact | Read from the stored `\parshape` |
| `\parskip` | [`GlueParameter { mu: false }`][builtins-2567] | exact | Stored parameter table |
| `\patterns` | [`Command(GeneralText)`][builtins-2779] | approximated | Scanned, not built |
| `\pausing` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\penalty` | [`Typeset(Number)`][builtins-2895] | exact | Penalty is appended as scanned |
| `\postdisplaypenalty` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\predisplaydirection` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\predisplaypenalty` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\predisplaysize` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pretolerance` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\prevdepth` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\prevgraf` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\protected` | [`Prefix(Protected)`][builtins-2524] | exact | Tex.web [§ 1211](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22720) |
| `\radical` | [`Command(Number)`][builtins-2779] | approximated | Scanned, not built |
| `\raise` | [`Mode(Shift { vertical: true })`][builtins-2822] | exact | Tex.web's own mode-transition rules |
| `\read` | [`Read`][builtins-2558] | approximated | Tex.web [§ 1351](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L24696). Exact for a known stream number. One satex cannot pin down leaves every stream's state unknown |
| `\readline` | [`Command(ReadLine)`][builtins-2767] | approximated | ETeX manual [§§ 3](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L195), [2](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L131), including the terminal-fatal case. A stream number satex cannot pin down gives an unknown line and does not end the job |
| `\relax` | [`Relax`][builtins-2598] | exact | Tex.web [§ 1211](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22720) |
| `\relpenalty` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\right` | [`Mode(LeftRight { opens: false, closes: true })`][builtins-2846] | approximated | Math left group is kept ([tex.web § 1191](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22367)). The size of the list it delimits widens |
| `\righthyphenmin` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\rightskip` | [`GlueParameter { mu: false }`][builtins-2567] | exact | Stored parameter table |
| `\romannumeral` | [`Text(Roman)`][builtins-2626] | approximated | Exact for a known operand. Widens when the token's meaning or a register's digits are unresolved |
| `\savinghyphcodes` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\savingvdiscards` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\scantokens` | [`ScanTokens { text: false }`][builtins-2537] | exact | ETeX manual [§§ 3](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L195), [9](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L333). A capacity overflow (`MAX_IN_OPEN`) is diagnosed, not assumed away |
| `\scriptfont` | [`Command(FamilyFont)`][builtins-2763] | approximated | Stored and consumed by satex's own font and math-code tables. An operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\scriptscriptfont` | [`Command(FamilyFont)`][builtins-2763] | approximated | Stored and consumed by satex's own font and math-code tables. An operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\scriptscriptstyle` | [`Mode(MathStyle(6))`][builtins-2842] | approximated | Tex.web [§ 1171](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22139): the style of the atoms after it in inline math. Scripts in display style widen |
| `\scriptspace` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\scriptstyle` | [`Mode(MathStyle(4))`][builtins-2842] | approximated | Tex.web [§ 1171](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22139): the style of the atoms after it in inline math. Scripts in display style widen |
| `\scrollmode` | [`Command(Interaction(2))`][builtins-2755] | exact | Sets `\interactionmode` |
| `\setbox` | [`Command(SetBox)`][builtins-2758] | exact | Forwarded to the real box-scanning path |
| `\setlanguage` | [`Command(SetLanguage)`][builtins-2750] | exact | Whatsit in horizontal mode, an illegal case elsewhere ([tex.web § 1377](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L24970)) |
| `\sfcode` | [`CharCode(Space)`][builtins-2577] | approximated | Exact for one known character and code. Through an unknown character or value (`unknown-char-code`) each entry it may name holds its old code or the new, read back as that set |
| `\shipout` | [`Mode(ShipOut)`][builtins-2834] | exact | Removes the box from the list, [tex.web § 1073](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L20853)'s own effect on it |
| `\show` | [`Command(Token)`][builtins-2756] | exact | `\show` |
| `\showbox` | [`Command(Number)`][builtins-2779] | approximated | Scanned, not built |
| `\showboxbreadth` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\showboxdepth` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\showgroups` | [`Command(Nothing)`][builtins-2734] | approximated | No operand. The size of what `\span`, a fraction or `\overline`/`\underline` builds is widened, not computed |
| `\showifs` | [`Command(Nothing)`][builtins-2734] | approximated | No operand. The size of what `\span`, a fraction or `\overline`/`\underline` builds is widened, not computed |
| `\showlists` | [`Command(Nothing)`][builtins-2734] | approximated | No operand. The size of what `\span`, a fraction or `\overline`/`\underline` builds is widened, not computed |
| `\showthe` | [`Command(Internal)`][builtins-2757] | exact | `\showthe` |
| `\showtokens` | [`Command(GeneralText)`][builtins-2779] | approximated | Scanned, not built |
| `\skewchar` | [`FontParam(SkewChar)`][builtins-2582] | approximated | Reads the stored font table. A read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L11284)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\skip` | [`Register(Skip)`][builtins-2553] | approximated | Exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\skipdef` | [`RegisterDef(Skip)`][builtins-2553] | approximated | Exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\spacefactor` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\spaceskip` | [`GlueParameter { mu: false }`][builtins-2567] | exact | Stored parameter table |
| `\span` | [`Command(Nothing)`][builtins-2734] | approximated | No operand. The size of what `\span`, a fraction or `\overline`/`\underline` builds is widened, not computed |
| `\special` | [`Pdf(Special)`][builtins-2607] | approximated | Manual's keywords are scanned exactly. No PDF is built, so the payload is recorded as an occurrence, not replayed |
| `\splitbotmark` | [`Convert(Mark(4))`][builtins-2810] | approximated | Scanned, answer unknown |
| `\splitbotmarks` | [`Convert(Marks(4))`][builtins-2810] | approximated | Scanned, answer unknown |
| `\splitdiscards` | [`Mode(Vertical(Discards(true)))`][builtins-2865] | approximated | Widens to Unknown. A rarely-taken [tex.web](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web) path this pass does not distinguish further |
| `\splitfirstmark` | [`Convert(Mark(3))`][builtins-2810] | approximated | Scanned, answer unknown |
| `\splitfirstmarks` | [`Convert(Marks(3))`][builtins-2810] | approximated | Scanned, answer unknown |
| `\splitmaxdepth` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\splittopskip` | [`GlueParameter { mu: false }`][builtins-2567] | exact | Stored parameter table |
| `\string` | [`Text(String)`][builtins-2626] | approximated | Exact for a known operand. Widens when the token's meaning or a register's digits are unresolved |
| `\tabskip` | [`GlueParameter { mu: false }`][builtins-2567] | exact | Stored parameter table |
| `\textfont` | [`Command(FamilyFont)`][builtins-2763] | approximated | Stored and consumed by satex's own font and math-code tables. An operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\textstyle` | [`Mode(MathStyle(2))`][builtins-2842] | approximated | Tex.web [§ 1171](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L22139): the style of the atoms after it in inline math. Scripts in display style widen |
| `\the` | [`Text(The)`][builtins-2626] | approximated | Exact for a known operand. Widens when the token's meaning or a register's digits are unresolved |
| `\thickmuskip` | [`GlueParameter { mu: true }`][builtins-2567] | exact | Stored parameter table |
| `\thinmuskip` | [`GlueParameter { mu: true }`][builtins-2567] | exact | Stored parameter table |
| `\time` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\toks` | [`Register(Toks)`][builtins-2553] | approximated | Exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\toksdef` | [`RegisterDef(Toks)`][builtins-2553] | approximated | Exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`) |
| `\tolerance` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\topmark` | [`Convert(Mark(0))`][builtins-2810] | approximated | Scanned, answer unknown |
| `\topmarks` | [`Convert(Marks(0))`][builtins-2810] | approximated | Scanned, answer unknown |
| `\topskip` | [`GlueParameter { mu: false }`][builtins-2567] | exact | Stored parameter table |
| `\tracingassigns` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\tracingcommands` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\tracinggroups` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\tracingifs` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\tracinglostchars` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\tracingmacros` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\tracingnesting` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\tracingonline` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\tracingoutput` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\tracingpages` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\tracingparagraphs` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\tracingrestores` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\tracingscantokens` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\tracingstacklevels` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\tracingstats` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\uccode` | [`CharCode(Upper)`][builtins-2577] | approximated | Exact for one known character and code. Through an unknown character or value (`unknown-char-code`) each entry it may name holds its old code or the new, read back as that set |
| `\uchyph` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\underline` | [`Command(Nothing)`][builtins-2734] | approximated | No operand. The size of what `\span`, a fraction or `\overline`/`\underline` builds is widened, not computed |
| `\unexpanded` | [`Reinject`][builtins-2535] | exact | ETeX manual [§§ 3](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L195), [4](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L231) |
| `\unhbox` | [`Mode(Horizontal(Unpackage { take: true }))`][builtins-2860] | approximated | Void register adds nothing. A real one becomes Unknown, since its content is not tracked once boxed |
| `\unhcopy` | [`Mode(Horizontal(Unpackage { take: false }))`][builtins-2860] | approximated | Void register adds nothing. A real one becomes Unknown, since its content is not tracked once boxed |
| `\unkern` | [`Typeset(Plain)`][builtins-2894] | exact | `\unskip`, `\unkern` and `\unpenalty` remove the real tail item |
| `\unless` | [`Unless`][builtins-2543] | approximated | Etex.ch. Exact before a conditional. Before a name of unknown meaning the conditional it may open is left undecided |
| `\unpenalty` | [`Typeset(Plain)`][builtins-2894] | exact | `\unskip`, `\unkern` and `\unpenalty` remove the real tail item |
| `\unskip` | [`Typeset(Plain)`][builtins-2894] | exact | `\unskip`, `\unkern` and `\unpenalty` remove the real tail item |
| `\unvbox` | [`Mode(Vertical(Unpackage { take: true }))`][builtins-2860] | approximated | Void register adds nothing. A real one becomes Unknown, since its content is not tracked once boxed |
| `\unvcopy` | [`Mode(Vertical(Unpackage { take: false }))`][builtins-2860] | approximated | Void register adds nothing. A real one becomes Unknown, since its content is not tracked once boxed |
| `\uppercase` | [`CaseShift(Upper)`][builtins-2581] | exact | Tex.web [§ 1288](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L23608). Widens a code an unknown token was assigned |
| `\vadjust` | [`Mode(Insert { vadjust: true })`][builtins-2836] | approximated | Opens the insert's own list. Not told apart from the vertical list around it |
| `\valign` | [`Mode(Horizontal(Align))`][builtins-2870] | approximated | Cells run in the alignment's own list, not distinguished from the preamble |
| `\vbadness` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\vbox` | [`Mode(Box(VBox))`][builtins-2878] | exact | Real box machinery |
| `\vcenter` | [`Mode(Box(VCenter))`][builtins-2878] | exact | Real box machinery |
| `\vfil` | [`Mode(Vertical(Fil(0)))`][builtins-2858] | exact | Real item, appended to the list satex tracks |
| `\vfill` | [`Mode(Vertical(Fil(1)))`][builtins-2858] | exact | Real item, appended to the list satex tracks |
| `\vfilneg` | [`Mode(Vertical(Fil(3)))`][builtins-2858] | exact | Real item, appended to the list satex tracks |
| `\vfuzz` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\voffset` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\vrule` | [`Mode(Horizontal(Rule))`][builtins-2824] | approximated | Real rule item in a horizontal list, but a `\vrule` met in a vertical list starts a paragraph and widens the enclosing box's dimensions rather than answering them exactly |
| `\vsize` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\vskip` | [`Mode(Vertical(Glue))`][builtins-2858] | exact | Real item, appended to the list satex tracks |
| `\vsplit` | [`Mode(Box(VSplit))`][builtins-2883] | approximated | Result always widens to Unknown, even from a void register. A real split's remainder is not tracked |
| `\vss` | [`Mode(Vertical(Fil(2)))`][builtins-2858] | exact | Real item, appended to the list satex tracks |
| `\vtop` | [`Mode(Box(VTop))`][builtins-2878] | exact | Real box machinery |
| `\wd` | [`Special(BoxDimen)`][builtins-2721] | approximated | Round-trips through the register consistently. The box behind it is never built |
| `\widowpenalties` | [`Special(Penalties)`][builtins-2723] | approximated | Table satex stores and reads back exactly for a known entry. One it cannot pin down widens every entry the write may have reached |
| `\widowpenalty` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\write` | [`Write`][builtins-2593] | approximated | `\immediate\write` is exact. A deferred `\write`'s text is recorded unexpanded, since satex never reaches shipout to expand it |
| `\xdef` | [`Def { global: true, expand: true }`][builtins-2521] | exact | Tex.web [§ 476](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L9357). An unterminated definition is diagnosed, not read past the file's end |
| `\xleaders` | [`Mode(Leaders)`][builtins-2830] | approximated | Box or rule is scanned exactly. [tex.web § 1078](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L20920)'s repeated copies are not laid out, since satex tracks natural size, not a page |
| `\xspaceskip` | [`GlueParameter { mu: false }`][builtins-2567] | exact | Stored parameter table |
| `\year` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |

## pdfTeX extensions ✓

**Fully modeled**: all 158 primitives, 81 of them exactly. Source: the [pdfTeX manual, "Primitives"](https://ctan.org/pkg/pdftex).

| primitive | modeled as | fidelity | how |
| --- | --- | --- | --- |
| `\efcode` | [`FontParam(CharCode(1000))`][builtins-2582] | approximated | Reads the stored font table. A read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L11284)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\expanded` | [`Expanded`][builtins-2539] | exact | Interface3, "Selecting tokens", and [pdfTeX manual, "\expanded"](https://ctan.org/pkg/pdftex) |
| `\ifincsname` | [`If(InCsname)`][builtins-2669] | exact | Computed from state satex already tracks (`eval_extra_condition`) |
| `\ifpdfabsdim` | [`If(AbsDim)`][builtins-2669] | exact | Computed from state satex already tracks (`eval_extra_condition`) |
| `\ifpdfabsnum` | [`If(AbsNum)`][builtins-2669] | exact | Computed from state satex already tracks (`eval_extra_condition`) |
| `\ifpdfprimitive` | [`If(Primitive)`][builtins-2669] | exact | Computed from state satex already tracks (`eval_extra_condition`) |
| `\ignoreprimitiveerror` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\knaccode` | [`FontParam(CharCode(0))`][builtins-2582] | approximated | Reads the stored font table. A read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L11284)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\knbccode` | [`FontParam(CharCode(0))`][builtins-2582] | approximated | Reads the stored font table. A read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L11284)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\knbscode` | [`FontParam(CharCode(0))`][builtins-2582] | approximated | Reads the stored font table. A read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L11284)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\leftmarginkern` | [`Convert(LeftMarginKern)`][builtins-2810] | approximated | Scanned, answer unknown |
| `\letterspacefont` | [`Command(CopyFont { amount: true })`][builtins-2763] | approximated | Stored and consumed by satex's own font and math-code tables. An operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\lpcode` | [`FontParam(CharCode(0))`][builtins-2582] | approximated | Reads the stored font table. A read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L11284)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\partokencontext` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\partokenname` | [`Command(Token)`][builtins-2756] | exact | `\show` |
| `\pdfadjustinterwordglue` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfadjustspacing` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfannot` | [`Command(PdfAnnot)`][builtins-2779] | approximated | Scanned, not built |
| `\pdfappendkern` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfcatalog` | [`Command(PdfCatalog)`][builtins-2779] | approximated | Scanned, not built |
| `\pdfcolorstack` | [`Command(PdfColorStack)`][builtins-2772] | approximated | Operation and stack id are tracked. The color text itself is a recorded occurrence, not replayed |
| `\pdfcolorstackinit` | [`Convert(ColorStackInit)`][builtins-2800] | approximated | `\pdfcolorstackinit` hands out the next color stack number, widened rather than counted exactly. The color text is a recorded occurrence, not replayed |
| `\pdfcompresslevel` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfcopyfont` | [`Command(CopyFont { amount: false })`][builtins-2763] | approximated | Stored and consumed by satex's own font and math-code tables. An operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\pdfcreationdate` | [`Convert(CreationDate)`][builtins-2810] | approximated | Scanned, answer unknown |
| `\pdfdecimaldigits` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfdest` | [`Command(PdfDest)`][builtins-2779] | approximated | Scanned, not built |
| `\pdfdestmargin` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfdraftmode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfeachlinedepth` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfeachlineheight` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfelapsedtime` | [`LastItem(ElapsedTime)`][builtins-2703] | approximated | Genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdfendlink` | [`Command(Whatsit)`][builtins-2745] | exact | No operand, a whatsit on the current list |
| `\pdfendthread` | [`Command(Whatsit)`][builtins-2745] | exact | No operand, a whatsit on the current list |
| `\pdfescapehex` | [`Convert(EscapeHex)`][builtins-2791] | exact | Manual's own algorithm, run for real |
| `\pdfescapename` | [`Convert(EscapeName)`][builtins-2791] | exact | Manual's own algorithm, run for real |
| `\pdfescapestring` | [`Convert(EscapeString)`][builtins-2791] | exact | Manual's own algorithm, run for real |
| `\pdffakespace` | [`Command(Nothing)`][builtins-2734] | approximated | No operand. The size of what `\span`, a fraction or `\overline`/`\underline` builds is widened, not computed |
| `\pdffiledump` | [`Convert(FileDump)`][builtins-2793] | exact | Reads the real file |
| `\pdffilemoddate` | [`Convert(FileModDate)`][builtins-2795] | approximated | Form of a date, its digits unknown: satex does not read file modification times |
| `\pdffilesize` | [`Convert(FileSize)`][builtins-2793] | exact | Reads the real file |
| `\pdffirstlineheight` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdffontattr` | [`Command(FontText)`][builtins-2779] | approximated | Scanned, not built |
| `\pdffontexpand` | [`Command(FontExpand)`][builtins-2779] | approximated | Scanned, not built |
| `\pdffontname` | [`Convert(PdfFontName)`][builtins-2799] | approximated | Widens to Unknown. No PDF font object is built |
| `\pdffontobjnum` | [`Convert(PdfFontObjNum)`][builtins-2799] | approximated | Widens to Unknown. No PDF font object is built |
| `\pdffontsize` | [`Convert(PdfFontSize)`][builtins-2798] | exact | Font's real design size |
| `\pdfforcepagebox` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfgamma` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfgentounicode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfglyphtounicode` | [`Command(TwoTexts)`][builtins-2779] | approximated | Scanned, not built |
| `\pdfhorigin` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfignoreddimen` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfimageapplygamma` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfimagegamma` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfimagehicolor` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfimageresolution` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfincludechars` | [`Command(FontText)`][builtins-2779] | approximated | Scanned, not built |
| `\pdfinclusioncopyfonts` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfinclusionerrorlevel` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfinfo` | [`Command(GeneralText)`][builtins-2779] | approximated | Scanned, not built |
| `\pdfinfoomitdate` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfinsertht` | [`Convert(InsertHt)`][builtins-2810] | approximated | Scanned, answer unknown |
| `\pdfinterwordspaceoff` | [`Command(Nothing)`][builtins-2734] | approximated | No operand. The size of what `\span`, a fraction or `\overline`/`\underline` builds is widened, not computed |
| `\pdfinterwordspaceon` | [`Command(Nothing)`][builtins-2734] | approximated | No operand. The size of what `\span`, a fraction or `\overline`/`\underline` builds is widened, not computed |
| `\pdflastannot` | [`LastItem(LastXPos)`][builtins-2703] | approximated | Genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflastlinedepth` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdflastlink` | [`LastItem(LastXPos)`][builtins-2703] | approximated | Genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflastmatch` | [`Convert(LastMatch)`][builtins-2810] | approximated | Scanned, answer unknown |
| `\pdflastobj` | [`LastItem(LastXPos)`][builtins-2703] | approximated | Genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflastxform` | [`LastItem(LastXPos)`][builtins-2703] | approximated | Genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflastximage` | [`LastItem(LastXPos)`][builtins-2703] | approximated | Genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflastximagecolordepth` | [`LastItem(LastXPos)`][builtins-2703] | approximated | Genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflastximagepages` | [`LastItem(LastXPos)`][builtins-2703] | approximated | Genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflastxpos` | [`LastItem(LastXPos)`][builtins-2703] | approximated | Genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflastypos` | [`LastItem(LastYPos)`][builtins-2703] | approximated | Genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdflinkmargin` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfliteral` | [`Pdf(Literal)`][builtins-2607] | approximated | Manual's keywords are scanned exactly. No PDF is built, so the payload is recorded as an occurrence, not replayed |
| `\pdfmajorversion` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfmapfile` | [`Command(GeneralText)`][builtins-2779] | approximated | Scanned, not built |
| `\pdfmapline` | [`Command(GeneralText)`][builtins-2779] | approximated | Scanned, not built |
| `\pdfmatch` | [`Convert(Match)`][builtins-2810] | approximated | Scanned, answer unknown |
| `\pdfmdfivesum` | [`Convert(MdFiveSum)`][builtins-2793] | exact | Reads the real file |
| `\pdfminorversion` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfmovechars` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfnames` | [`Command(GeneralText)`][builtins-2779] | approximated | Scanned, not built |
| `\pdfnobuiltintounicode` | [`Command(Font)`][builtins-2779] | approximated | Scanned, not built |
| `\pdfnoligatures` | [`Command(Font)`][builtins-2779] | approximated | Scanned, not built |
| `\pdfnormaldeviate` | [`Convert(NormalDeviate)`][builtins-2810] | approximated | Scanned, answer unknown |
| `\pdfobj` | [`Pdf(Object)`][builtins-2607] | approximated | Manual's keywords are scanned exactly. No PDF is built, so the payload is recorded as an occurrence, not replayed |
| `\pdfobjcompresslevel` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfomitcharset` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfomitinfodict` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfomitprocset` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfoptionalwaysusepdfpagebox` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfoptionpdfinclusionerrorlevel` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfoptionpdfminorversion` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfoutline` | [`Command(PdfOutline)`][builtins-2779] | approximated | Scanned, not built |
| `\pdfoutput` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfpageattr` | [`TokensParameter`][builtins-2569] | approximated | Stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\pdfpagebox` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfpageheight` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfpageref` | [`Convert(PageRef)`][builtins-2810] | approximated | Scanned, answer unknown |
| `\pdfpageresources` | [`Pdf(Resources)`][builtins-2607] | approximated | Manual's keywords are scanned exactly. No PDF is built, so the payload is recorded as an occurrence, not replayed |
| `\pdfpagesattr` | [`TokensParameter`][builtins-2569] | approximated | Stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\pdfpagewidth` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfpkmode` | [`TokensParameter`][builtins-2569] | approximated | Stored parameter table, but setting any `\every…` list widens the dimensions of boxes that were already packed before it was read |
| `\pdfpkresolution` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfprependkern` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfprimitive` | [`Convert(Primitive)`][builtins-2805] | exact | Re-injects the frozen meaning for normal expansion to re-process, as `\pdfprimitive` does |
| `\pdfprotrudechars` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfptexuseunderscore` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfpxdimen` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfrandomseed` | [`LastItem(RandomSeed)`][builtins-2703] | approximated | Genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdfrefobj` | [`Command(Number)`][builtins-2779] | approximated | Scanned, not built |
| `\pdfrefxform` | [`Command(Number)`][builtins-2779] | approximated | Scanned, not built |
| `\pdfrefximage` | [`Command(Number)`][builtins-2779] | approximated | Scanned, not built |
| `\pdfresettimer` | [`Command(Nothing)`][builtins-2734] | approximated | No operand. The size of what `\span`, a fraction or `\overline`/`\underline` builds is widened, not computed |
| `\pdfrestore` | [`Command(Whatsit)`][builtins-2745] | exact | No operand, a whatsit on the current list |
| `\pdfretval` | [`LastItem(LastXPos)`][builtins-2703] | approximated | Genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\pdfrunninglinkoff` | [`Command(Nothing)`][builtins-2734] | approximated | No operand. The size of what `\span`, a fraction or `\overline`/`\underline` builds is widened, not computed |
| `\pdfrunninglinkon` | [`Command(Nothing)`][builtins-2734] | approximated | No operand. The size of what `\span`, a fraction or `\overline`/`\underline` builds is widened, not computed |
| `\pdfsave` | [`Command(Whatsit)`][builtins-2745] | exact | No operand, a whatsit on the current list |
| `\pdfsavepos` | [`Command(Whatsit)`][builtins-2745] | exact | No operand, a whatsit on the current list |
| `\pdfsetmatrix` | [`Command(GeneralText)`][builtins-2779] | approximated | Scanned, not built |
| `\pdfsetrandomseed` | [`Command(Number)`][builtins-2779] | approximated | Scanned, not built |
| `\pdfshellescape` | [`LastItem(ShellEscape)`][builtins-2702] | exact | Configuration's own setting |
| `\pdfstartlink` | [`Command(PdfStartLink)`][builtins-2779] | approximated | Scanned, not built |
| `\pdfstartthread` | [`Command(PdfThread)`][builtins-2779] | approximated | Scanned, not built |
| `\pdfstrcmp` | [`Convert(StrCmp)`][builtins-2791] | exact | Manual's own algorithm, run for real |
| `\pdfsuppressptexinfo` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfsuppresswarningdupdest` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfsuppresswarningdupmap` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfsuppresswarningpagegroup` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdftexbanner` | [`Convert(PdftexBanner)`][builtins-2810] | approximated | Scanned, answer unknown |
| `\pdftexrevision` | [`Convert(PdftexRevision)`][builtins-2788] | exact | Real constant or stored name |
| `\pdftexversion` | [`LastItem(PdftexVersion)`][builtins-2688] | exact | Read from state satex already tracks |
| `\pdfthread` | [`Command(PdfThread)`][builtins-2779] | approximated | Scanned, not built |
| `\pdfthreadmargin` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdftracingfonts` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdftrailer` | [`Command(GeneralText)`][builtins-2779] | approximated | Scanned, not built |
| `\pdftrailerid` | [`Command(GeneralText)`][builtins-2779] | approximated | Scanned, not built |
| `\pdfunescapehex` | [`Convert(UnescapeHex)`][builtins-2791] | exact | Manual's own algorithm, run for real |
| `\pdfuniformdeviate` | [`Convert(UniformDeviate)`][builtins-2810] | approximated | Scanned, answer unknown |
| `\pdfuniqueresname` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfvorigin` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfxform` | [`Command(PdfXForm)`][builtins-2779] | approximated | Scanned, not built |
| `\pdfxformname` | [`Convert(XFormName)`][builtins-2810] | approximated | Scanned, answer unknown |
| `\pdfximage` | [`Command(PdfXImage)`][builtins-2771] | exact | File name reaches `observe::image` |
| `\pdfximagebbox` | [`Convert(ImageBBox)`][builtins-2810] | approximated | Scanned, answer unknown |
| `\quitvmode` | [`Mode(QuitVMode)`][builtins-2822] | exact | Tex.web's own mode-transition rules |
| `\rightmarginkern` | [`Convert(MarginKern)`][builtins-2810] | approximated | Scanned, answer unknown |
| `\rpcode` | [`FontParam(CharCode(0))`][builtins-2582] | approximated | Reads the stored font table. A read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L11284)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\shbscode` | [`FontParam(CharCode(0))`][builtins-2582] | approximated | Reads the stored font table. A read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L11284)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\showstream` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\stbscode` | [`FontParam(CharCode(0))`][builtins-2582] | approximated | Reads the stored font table. A read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L11284)'s own recovery. An entry satex cannot pin down widens every one it may name |
| `\synctex` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\tagcode` | [`FontParam(TagCode { lig: true })`][builtins-2582] | approximated | Reads the stored font table. A read past the last-loaded font's dimensions is discarded, matching [tex.web § 580](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L11284)'s own recovery. An entry satex cannot pin down widens every one it may name |

## XeTeX extensions ✓

**Fully modeled**: all 94 primitives, 25 of them exactly. Source: the [XeTeX reference](https://tug.org/xetex/).

| primitive | modeled as | fidelity | how |
| --- | --- | --- | --- |
| `\Uchar` | [`Convert(Uchar { catcode: false })`][builtins-2797] | exact | Tex.web, eTeX and XeTeX's character and category-code construction rules |
| `\Ucharcat` | [`Convert(Uchar { catcode: true })`][builtins-2797] | exact | Tex.web, eTeX and XeTeX's character and category-code construction rules |
| `\Udelcode` | [`Special(UCode { math: false, num: false })`][builtins-2723] | approximated | Table satex stores and reads back exactly for a known entry. One it cannot pin down widens every entry the write may have reached |
| `\Udelcodenum` | [`Special(UCode { math: false, num: true })`][builtins-2723] | approximated | Table satex stores and reads back exactly for a known entry. One it cannot pin down widens every entry the write may have reached |
| `\Udelimiter` | [`Command(Numbers(3))`][builtins-2779] | approximated | Scanned, not built |
| `\Umathaccent` | [`Command(MathAccent)`][builtins-2779] | approximated | Scanned, not built |
| `\Umathchar` | [`Command(Numbers(3))`][builtins-2779] | approximated | Scanned, not built |
| `\Umathchardef` | [`Command(UMathCharDef { num: false })`][builtins-2763] | approximated | Stored and consumed by satex's own font and math-code tables. An operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\Umathcharnum` | [`Command(Numbers(1))`][builtins-2779] | approximated | Scanned, not built |
| `\Umathcharnumdef` | [`Command(UMathCharDef { num: true })`][builtins-2763] | approximated | Stored and consumed by satex's own font and math-code tables. An operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\Umathcode` | [`Special(UCode { math: true, num: false })`][builtins-2723] | approximated | Table satex stores and reads back exactly for a known entry. One it cannot pin down widens every entry the write may have reached |
| `\Umathcodenum` | [`Special(UCode { math: true, num: true })`][builtins-2723] | approximated | Table satex stores and reads back exactly for a known entry. One it cannot pin down widens every entry the write may have reached |
| `\Uradical` | [`Command(Numbers(2))`][builtins-2779] | approximated | Scanned, not built |
| `\XeTeXOTcountfeatures` | [`LastItem(Xetex(OtCountFeatures))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXOTcountlanguages` | [`LastItem(Xetex(OtCountLanguages))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXOTcountscripts` | [`LastItem(Xetex(OtCountScripts))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXOTfeaturetag` | [`LastItem(Xetex(OtFeatureTag))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXOTlanguagetag` | [`LastItem(Xetex(OtLanguageTag))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXOTscripttag` | [`LastItem(Xetex(OtScriptTag))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXcharclass` | [`Special(CharClass)`][builtins-2723] | approximated | Table satex stores and reads back exactly for a known entry. One it cannot pin down widens every entry the write may have reached |
| `\XeTeXcharglyph` | [`LastItem(Xetex(CharGlyph))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXcountfeatures` | [`LastItem(Xetex(CountFeatures))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXcountglyphs` | [`LastItem(Xetex(CountGlyphs))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXcountselectors` | [`LastItem(Xetex(CountSelectors))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXcountvariations` | [`LastItem(Xetex(CountVariations))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXdashbreakstate` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\XeTeXdefaultencoding` | [`Command(FileName)`][builtins-2779] | approximated | Scanned, not built |
| `\XeTeXdelcode` | [`Special(UCode { math: false, num: false })`][builtins-2723] | approximated | Table satex stores and reads back exactly for a known entry. One it cannot pin down widens every entry the write may have reached |
| `\XeTeXdelcodenum` | [`Special(UCode { math: false, num: true })`][builtins-2723] | approximated | Table satex stores and reads back exactly for a known entry. One it cannot pin down widens every entry the write may have reached |
| `\XeTeXdelimiter` | [`Command(Numbers(3))`][builtins-2779] | approximated | Scanned, not built |
| `\XeTeXfeaturecode` | [`LastItem(Xetex(FeatureCode))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXfeaturename` | [`Convert(Xetex(FeatureName))`][builtins-2808] | approximated | Exact from a loaded font's real tables. Unknown once they aren't resolved |
| `\XeTeXfindfeaturebyname` | [`LastItem(Xetex(FindFeatureByName))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXfindselectorbyname` | [`LastItem(Xetex(FindSelectorByName))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXfindvariationbyname` | [`LastItem(Xetex(FindVariationByName))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXfirstfontchar` | [`LastItem(Xetex(FirstFontChar))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXfonttype` | [`LastItem(Xetex(FontType))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXgenerateactualtext` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\XeTeXglyph` | [`Mode(Horizontal(Char))`][builtins-2858] | exact | Real item, appended to the list satex tracks |
| `\XeTeXglyphbounds` | [`LastItem(Xetex(GlyphBounds))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXglyphindex` | [`LastItem(Xetex(GlyphIndex))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXglyphname` | [`Convert(Xetex(GlyphName))`][builtins-2808] | approximated | Exact from a loaded font's real tables. Unknown once they aren't resolved |
| `\XeTeXhyphenatablelength` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\XeTeXinputencoding` | [`Command(FileName)`][builtins-2779] | approximated | Scanned, not built |
| `\XeTeXinputnormalization` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\XeTeXinterchartokenstate` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\XeTeXinterchartoks` | [`Special(InterCharToks)`][builtins-2723] | approximated | Table satex stores and reads back exactly for a known entry. One it cannot pin down widens every entry the write may have reached |
| `\XeTeXinterwordspaceshaping` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\XeTeXisdefaultselector` | [`LastItem(Xetex(IsDefaultSelector))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXisexclusivefeature` | [`LastItem(Xetex(IsExclusiveFeature))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXlastfontchar` | [`LastItem(Xetex(LastFontChar))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXlinebreaklocale` | [`Command(FileName)`][builtins-2779] | approximated | Scanned, not built |
| `\XeTeXlinebreakpenalty` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\XeTeXlinebreakskip` | [`GlueParameter { mu: false }`][builtins-2567] | exact | Stored parameter table |
| `\XeTeXmathaccent` | [`Command(MathAccent)`][builtins-2779] | approximated | Scanned, not built |
| `\XeTeXmathchar` | [`Command(Numbers(3))`][builtins-2779] | approximated | Scanned, not built |
| `\XeTeXmathchardef` | [`Command(UMathCharDef { num: false })`][builtins-2763] | approximated | Stored and consumed by satex's own font and math-code tables. An operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\XeTeXmathcharnum` | [`Command(Numbers(1))`][builtins-2779] | approximated | Scanned, not built |
| `\XeTeXmathcharnumdef` | [`Command(UMathCharDef { num: true })`][builtins-2763] | approximated | Stored and consumed by satex's own font and math-code tables. An operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\XeTeXmathcode` | [`Special(UCode { math: true, num: false })`][builtins-2723] | approximated | Table satex stores and reads back exactly for a known entry. One it cannot pin down widens every entry the write may have reached |
| `\XeTeXmathcodenum` | [`Special(UCode { math: true, num: true })`][builtins-2723] | approximated | Table satex stores and reads back exactly for a known entry. One it cannot pin down widens every entry the write may have reached |
| `\XeTeXpdffile` | [`Command(PicFile)`][builtins-2779] | approximated | Scanned, not built |
| `\XeTeXpdfpagecount` | [`LastItem(PdfPageCount)`][builtins-2710] | approximated | Widens to Unknown. The file's page count needs parsing a PDF satex does not read |
| `\XeTeXpicfile` | [`Command(PicFile)`][builtins-2779] | approximated | Scanned, not built |
| `\XeTeXprotrudechars` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\XeTeXradical` | [`Command(Numbers(2))`][builtins-2779] | approximated | Scanned, not built |
| `\XeTeXrevision` | [`Convert(XetexRevision)`][builtins-2788] | exact | Real constant or stored name |
| `\XeTeXselectorcode` | [`LastItem(Xetex(SelectorCode))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXselectorname` | [`Convert(Xetex(SelectorName))`][builtins-2808] | approximated | Exact from a loaded font's real tables. Unknown once they aren't resolved |
| `\XeTeXtracingfonts` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\XeTeXupwardsmode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\XeTeXuseglyphmetrics` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\XeTeXvariation` | [`LastItem(Xetex(Variation))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXvariationdefault` | [`LastItem(Xetex(VariationDefault))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXvariationmax` | [`LastItem(Xetex(VariationMax))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXvariationmin` | [`LastItem(Xetex(VariationMin))`][builtins-2712] | approximated | Font's real tables |
| `\XeTeXvariationname` | [`Convert(Xetex(VariationName))`][builtins-2808] | approximated | Exact from a loaded font's real tables. Unknown once they aren't resolved |
| `\XeTeXversion` | [`LastItem(XetexVersion)`][builtins-2707] | exact | Constant |
| `\creationdate` | [`Convert(CreationDate)`][builtins-2810] | approximated | Scanned, answer unknown |
| `\elapsedtime` | [`LastItem(ElapsedTime)`][builtins-2703] | approximated | Genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\filedump` | [`Convert(FileDump)`][builtins-2793] | exact | Reads the real file |
| `\filemoddate` | [`Convert(FileModDate)`][builtins-2795] | approximated | Form of a date, its digits unknown: satex does not read file modification times |
| `\filesize` | [`Convert(FileSize)`][builtins-2793] | exact | Reads the real file |
| `\ifprimitive` | [`If(Primitive)`][builtins-2669] | exact | Computed from state satex already tracks (`eval_extra_condition`) |
| `\mdfivesum` | [`Convert(MdFiveSum)`][builtins-2793] | exact | Reads the real file |
| `\normaldeviate` | [`Convert(NormalDeviate)`][builtins-2810] | approximated | Scanned, answer unknown |
| `\primitive` | [`Convert(Primitive)`][builtins-2805] | exact | Re-injects the frozen meaning for normal expansion to re-process, as `\pdfprimitive` does |
| `\randomseed` | [`LastItem(RandomSeed)`][builtins-2703] | approximated | Genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown |
| `\resettimer` | [`Command(Nothing)`][builtins-2734] | approximated | No operand. The size of what `\span`, a fraction or `\overline`/`\underline` builds is widened, not computed |
| `\setrandomseed` | [`Command(Number)`][builtins-2779] | approximated | Scanned, not built |
| `\shellescape` | [`LastItem(ShellEscape)`][builtins-2702] | exact | Configuration's own setting |
| `\strcmp` | [`Convert(StrCmp)`][builtins-2791] | exact | Manual's own algorithm, run for real |
| `\suppressfontnotfounderror` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\uniformdeviate` | [`Convert(UniformDeviate)`][builtins-2810] | approximated | Scanned, answer unknown |

## LuaTeX extensions ✓

**Fully modeled**: all 306 primitives, 84 of them exactly. Source: the [LuaTeX manual](https://ctan.org/pkg/luatex).

| primitive | modeled as | fidelity | how |
| --- | --- | --- | --- |
| `\Udelimiterover` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Udelimiterunder` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Uhextensible` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Uleft` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathaxis` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathbinbinspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathbinclosespacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathbininnerspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathbinopenspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathbinopspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathbinordspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathbinpunctspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathbinrelspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathcharclass` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathcharfam` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathcharslot` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathclosebinspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathcloseclosespacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathcloseinnerspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathcloseopenspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathcloseopspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathcloseordspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathclosepunctspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathcloserelspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathconnectoroverlapmin` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathfractiondelsize` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathfractiondenomdown` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathfractiondenomvgap` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathfractionnumup` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathfractionnumvgap` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathfractionrule` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathinnerbinspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathinnerclosespacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathinnerinnerspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathinneropenspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathinneropspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathinnerordspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathinnerpunctspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathinnerrelspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathlimitabovebgap` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathlimitabovekern` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathlimitabovevgap` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathlimitbelowbgap` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathlimitbelowkern` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathlimitbelowvgap` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathnolimitsubfactor` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathnolimitsupfactor` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathopbinspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathopclosespacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathopenbinspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathopenclosespacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathopeninnerspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathopenopenspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathopenopspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathopenordspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathopenpunctspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathopenrelspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathoperatorsize` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathopinnerspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathopopenspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathopopspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathopordspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathoppunctspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathoprelspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathordbinspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathordclosespacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathordinnerspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathordopenspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathordopspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathordordspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathordpunctspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathordrelspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathoverbarkern` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathoverbarrule` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathoverbarvgap` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathoverdelimiterbgap` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathoverdelimitervgap` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathpunctbinspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathpunctclosespacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathpunctinnerspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathpunctopenspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathpunctopspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathpunctordspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathpunctpunctspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathpunctrelspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathquad` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathradicaldegreeafter` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathradicaldegreebefore` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathradicaldegreeraise` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathradicalkern` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathradicalrule` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathradicalvgap` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathrelbinspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathrelclosespacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathrelinnerspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathrelopenspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathrelopspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathrelordspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathrelpunctspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathrelrelspacing` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathskewedfractionhgap` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathskewedfractionvgap` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathspaceafterscript` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathstackdenomdown` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathstacknumup` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathstackvgap` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathsubshiftdown` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathsubshiftdrop` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathsubsupshiftdown` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathsubsupvgap` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathsubtopmax` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathsupbottommin` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathsupshiftdrop` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathsupshiftup` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathsupsubbottommax` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathunderbarkern` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathunderbarrule` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathunderbarvgap` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathunderdelimiterbgap` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umathunderdelimitervgap` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Umiddle` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Unosubscript` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Unosuperscript` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Uoverdelimiter` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Uright` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Uroot` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Uskewed` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Uskewedwithdelims` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Ustack` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Ustartdisplaymath` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Ustartmath` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Ustopdisplaymath` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Ustopmath` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Usubscript` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Usuperscript` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Uunderdelimiter` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\Uvextensible` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\adjustspacing` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\alignmark` | [`AlignMark`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\aligntab` | [`AlignTab`][builtins-2614] | exact | `&`: ends the cell, or begins one between rows ([tex.web § 785](https://github.com/TeX-Live/texlive-source/blob/71911d456eb7d2104062e99fed6010f6054c5be8/texk/web2c/tex.web#L15499)) |
| `\attribute` | [`Command(NumberIsNumber)`][builtins-2779] | approximated | Scanned, not built |
| `\attributedef` | [`Command(NamesNumber)`][builtins-2779] | approximated | Scanned, not built |
| `\automaticdiscretionary` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\automatichyphenmode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\automatichyphenpenalty` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\begincsname` | [`BeginCsname`][builtins-2534] | exact | [LuaTeX manual, "Macros"](https://ctan.org/pkg/luatex) |
| `\bodydir` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\bodydirection` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\boundary` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\boxdir` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\boxdirection` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\breakafterdirmode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\catcodetable` | [`Lua(SelectCatcodes)`][builtins-2920] | exact | Real string escape or catcode-table operation |
| `\clearmarks` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\compoundhyphenmode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\copyfont` | [`Command(CopyFont { amount: false })`][builtins-2763] | approximated | Stored and consumed by satex's own font and math-code tables. An operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\crampeddisplaystyle` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\crampedscriptscriptstyle` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\crampedscriptstyle` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\crampedtextstyle` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\csstring` | [`Text(String)`][builtins-2626] | approximated | Exact for a known operand. Widens when the token's meaning or a register's digits are unresolved |
| `\deferred` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\directlua` | [`Lua(Direct)`][builtins-2915] | approximated | Chunk runs for real where a Lua host is available. A failed or unavailable run is diagnosed and widened |
| `\discretionaryligaturemode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\draftmode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\dviextension` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\dvifeedback` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\dvivariable` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\eTeXVersion` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\eTeXglueshrinkorder` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\eTeXgluestretchorder` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\eTeXminorversion` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\endlocalcontrol` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\etoksapp` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\etokspre` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\exceptionpenalty` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\exhyphenchar` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\expandglyphsinfont` | [`Command(FontExpand)`][builtins-2779] | approximated | Scanned, not built |
| `\explicitdiscretionary` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\explicithyphenpenalty` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\firstvalidlanguage` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\fixupboxesmode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\fontid` | [`Convert(FontId)`][builtins-2788] | exact | Real constant or stored name |
| `\formatname` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\gleaders` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\glet` | [`Let { global: true, future: false }`][builtins-2523] | exact | Tex.web [§ 1221](https://ctan.org/pkg/luatex), including the two-token `\futurelet` lookahead |
| `\glyphdimensionsmode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\gtoksapp` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\gtokspre` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\hjcode` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\hpack` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\hyphenationbounds` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\hyphenationmin` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\hyphenpenaltymode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\ifabsdim` | [`If(AbsDim)`][builtins-2669] | exact | Computed from state satex already tracks (`eval_extra_condition`) |
| `\ifabsnum` | [`If(AbsNum)`][builtins-2669] | exact | Computed from state satex already tracks (`eval_extra_condition`) |
| `\ifcondition` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\ignoreligaturesinfont` | [`Command(Font)`][builtins-2779] | approximated | Scanned, not built |
| `\immediateassigned` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\immediateassignment` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\initcatcodetable` | [`Lua(InitCatcodes)`][builtins-2920] | exact | Real string escape or catcode-table operation |
| `\insertht` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\lastnamedcs` | [`LastNamedCs`][builtins-2534] | exact | [LuaTeX manual, "Macros"](https://ctan.org/pkg/luatex) |
| `\lastsavedboxresourceindex` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\lastsavedimageresourceindex` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\lastsavedimageresourcepages` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\lastxpos` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\lastypos` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\latelua` | [`Lua(Late)`][builtins-2915] | approximated | Chunk runs for real where a Lua host is available. A failed or unavailable run is diagnosed and widened |
| `\lateluafunction` | [`Command(Number)`][builtins-2779] | approximated | Scanned, not built |
| `\leftghost` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\letcharcode` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\linedir` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\linedirection` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\localbrokenpenalty` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\localinterlinepenalty` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\localleftbox` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\localrightbox` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\luabytecode` | [`Command(Number)`][builtins-2779] | approximated | Scanned, not built |
| `\luabytecodecall` | [`Command(Number)`][builtins-2779] | approximated | Scanned, not built |
| `\luacopyinputnodes` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\luadef` | [`Command(NamesNumber)`][builtins-2779] | approximated | Scanned, not built |
| `\luaescapestring` | [`Lua(Escape)`][builtins-2920] | exact | Real string escape or catcode-table operation |
| `\luafunction` | [`Command(Number)`][builtins-2779] | approximated | Scanned, not built |
| `\luafunctioncall` | [`Command(Number)`][builtins-2779] | approximated | Scanned, not built |
| `\luatexbanner` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\luatexrevision` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\luatexversion` | [`LastItem(LuatexVersion)`][builtins-2708] | exact | Probed from the real `luatex --version` for this run |
| `\mathdefaultsmode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\mathdelimitersmode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\mathdir` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\mathdirection` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\mathdisplayskipmode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\mathemptydisplaymode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\matheqdirmode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\matheqnogapstep` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\mathflattenmode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\mathitalicsmode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\mathnolimitsmode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\mathoption` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\mathpenaltiesmode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\mathrulesfam` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\mathrulesmode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\mathrulethicknessmode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\mathscriptboxmode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\mathscriptcharmode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\mathscriptsmode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\mathstyle` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\mathsurroundmode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\mathsurroundskip` | [`GlueParameter { mu: false }`][builtins-2567] | exact | Stored parameter table |
| `\nohrule` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\nokerns` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\noligs` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\nospaces` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\novrule` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\outputbox` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\outputmode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pagebottomoffset` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pagedir` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\pagedirection` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pageheight` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pageleftoffset` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pagerightoffset` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pagetopoffset` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pagewidth` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\pardir` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\pardirection` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\pdfextension` | [`Pdf(Extension)`][builtins-2607] | approximated | Manual's keywords are scanned exactly. No PDF is built, so the payload is recorded as an occurrence, not replayed |
| `\pdffeedback` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\pdfvariable` | [`Pdf(Variable)`][builtins-2607] | approximated | Manual's keywords are scanned exactly. No PDF is built, so the payload is recorded as an occurrence, not replayed |
| `\postexhyphenchar` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\posthyphenchar` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\prebinoppenalty` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\predisplaygapfactor` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\preexhyphenchar` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\prehyphenchar` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\prerelpenalty` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\protrudechars` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\protrusionboundary` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\pxdimen` | [`DimenParameter`][builtins-2567] | exact | Stored parameter table |
| `\rightghost` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\saveboxresource` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\savecatcodetable` | [`Lua(SaveCatcodes)`][builtins-2920] | exact | Real string escape or catcode-table operation |
| `\saveimageresource` | [`Command(PdfXImage)`][builtins-2771] | exact | File name reaches `observe::image` |
| `\savepos` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\scantextokens` | [`ScanTokens { text: true }`][builtins-2537] | exact | ETeX manual [§ 3.9](https://ctan.org/pkg/luatex). A capacity overflow (`MAX_IN_OPEN`) is diagnosed, not assumed away |
| `\setfontid` | [`Command(SetFontId)`][builtins-2763] | approximated | Stored and consumed by satex's own font and math-code tables. An operand satex cannot pin down leaves the entry, or the font the name means, unknown |
| `\shapemode` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\suppressifcsnameerror` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\suppresslongerror` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\suppressmathparerror` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\suppressoutererror` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\suppressprimitiveerror` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\textdir` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\textdirection` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\toksapp` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\tokspre` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\tpack` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\tracingfonts` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\useboxresource` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\useimageresource` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\variablefam` | [`IntegerParameter`][builtins-2567] | exact | Stored parameter table |
| `\vpack` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\wordboundary` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\xtoksapp` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |
| `\xtokspre` | [`Unmodeled`][builtins-2615] | unsupported + diagnostic | Widens the mode |

**950 of 950 primitives modeled soundly.**

[builtins-2521]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2521
[builtins-2523]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2523
[builtins-2524]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2524
[builtins-2525]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2525
[builtins-2526]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2526
[builtins-2530]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2530
[builtins-2534]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2534
[builtins-2535]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2535
[builtins-2537]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2537
[builtins-2539]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2539
[builtins-2541]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2541
[builtins-2543]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2543
[builtins-2548]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2548
[builtins-2553]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2553
[builtins-2557]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2557
[builtins-2558]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2558
[builtins-2562]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2562
[builtins-2567]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2567
[builtins-2569]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2569
[builtins-2573]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2573
[builtins-2577]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2577
[builtins-2581]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2581
[builtins-2582]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2582
[builtins-2586]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2586
[builtins-2589]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2589
[builtins-2590]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2590
[builtins-2591]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2591
[builtins-2592]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2592
[builtins-2593]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2593
[builtins-2597]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2597
[builtins-2598]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2598
[builtins-2599]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2599
[builtins-2603]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2603
[builtins-2607]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2607
[builtins-2614]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2614
[builtins-2615]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2615
[builtins-2625]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2625
[builtins-2626]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2626
[builtins-2637]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2637
[builtins-2638]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2638
[builtins-2643]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2643
[builtins-2645]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2645
[builtins-2647]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2647
[builtins-2650]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2650
[builtins-2653]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2653
[builtins-2655]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2655
[builtins-2660]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2660
[builtins-2664]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2664
[builtins-2669]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2669
[builtins-2671]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2671
[builtins-2680]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2680
[builtins-2682]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2682
[builtins-2686]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2686
[builtins-2688]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2688
[builtins-2691]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2691
[builtins-2694]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2694
[builtins-2696]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2696
[builtins-2697]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2697
[builtins-2701]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2701
[builtins-2702]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2702
[builtins-2703]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2703
[builtins-2707]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2707
[builtins-2708]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2708
[builtins-2710]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2710
[builtins-2712]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2712
[builtins-2721]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2721
[builtins-2723]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2723
[builtins-2734]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2734
[builtins-2738]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2738
[builtins-2743]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2743
[builtins-2745]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2745
[builtins-2746]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2746
[builtins-2750]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2750
[builtins-2751]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2751
[builtins-2755]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2755
[builtins-2756]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2756
[builtins-2757]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2757
[builtins-2758]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2758
[builtins-2759]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2759
[builtins-2763]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2763
[builtins-2767]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2767
[builtins-2771]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2771
[builtins-2772]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2772
[builtins-2779]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2779
[builtins-2788]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2788
[builtins-2791]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2791
[builtins-2793]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2793
[builtins-2795]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2795
[builtins-2797]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2797
[builtins-2798]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2798
[builtins-2799]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2799
[builtins-2800]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2800
[builtins-2805]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2805
[builtins-2808]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2808
[builtins-2810]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2810
[builtins-2819]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2819
[builtins-2822]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2822
[builtins-2824]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2824
[builtins-2830]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2830
[builtins-2834]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2834
[builtins-2836]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2836
[builtins-2839]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2839
[builtins-2841]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2841
[builtins-2842]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2842
[builtins-2846]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2846
[builtins-2858]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2858
[builtins-2860]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2860
[builtins-2865]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2865
[builtins-2870]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2870
[builtins-2878]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2878
[builtins-2879]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2879
[builtins-2883]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2883
[builtins-2894]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2894
[builtins-2895]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2895
[builtins-2896]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2896
[builtins-2904]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2904
[builtins-2915]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2915
[builtins-2920]: https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs#L2920
