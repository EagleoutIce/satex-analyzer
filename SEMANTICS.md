# The semantics SaTeX assumes

This document states what `satex` believes TeX, ε-TeX and LaTeX2e do. Every
claim here is a claim about the engines, not about `satex`; where `satex`
deliberately departs from them, the departure is marked **[abstraction]** and
justified.

Sources: [*TeXbook*][texbook] (Knuth, *The TeXbook*, cited by chapter — it has no
numbered sections), [*tex.web*][tex.web] (Knuth, *TeX: The Program*, cited by module
number), [*TeX by Topic*][tex-by-topic] (Eijkhout), [*etex_man*][etex_man] (the ε-TeX manual), [*clsguide*][clsguide]
and [*usrguide*][usrguide] (LaTeX Project), [*expl3*][expl3] (the interface documentation),
[*texdimens*][texdimens] (Burnol), and [`latex.ltx`][latex.ltx] itself.

---

## 1. Processing model

TeX reads in three stages. The **mouth** turns characters into tokens, the
**gullet** expands expandable tokens, and the **stomach** executes everything
else. ([*TeXbook*][texbook] ch. 7; [*TeX by Topic*][tex-by-topic] ch. 1–2.)

`satex` implements the same three stages. It does not implement the fourth,
typesetting: no boxes, glue setting or page builder. Everything that only
affects typeset output is a no-operation.

A run starts in the interaction mode `interaction:` in `satex.yaml` names,
as the engine's `-interaction` would set it (`nonstopmode` by default, as
unattended builds run). In `\batchmode` and `\nonstopmode` a `\read` from
the terminal is a fatal error that ends the job ([*tex.web*][tex.web] § 484); LaTeX's
prompt for a missing file (`\@missingfileerror`) is such a read.

## 2. Category codes

Every character carries one of 16 category codes, and the code is fixed at the
moment the character is tokenized, not when the token is used. ([*TeXbook*][texbook] ch. 7;
[*tex.web*][tex.web] § 232.)

* A run without a format starts from INITEX's table: letters 11, `\` 0,
  `%` 14, `^^@` 9, `^^M` 5, space 10, `^^?` 15, every other character 12
  (tab too). A LaTeX format hands the document [`latex.ltx`][latex.ltx]'s table.
  ([*tex.web*][tex.web] § 232.)
* `\catcode` is a **local** assignment: it is undone by the closing brace of
  the group it was made in. ([*TeXbook*][texbook] ch. 20.)
* `^^X`, where `X` has a code below 128, denotes the character whose code
  differs from `X`'s in bit 6; `^^XY` with two lower-case hex digits denotes
  that code directly. Above 127 no substitution happens. ([*tex.web*][tex.web] § 352.)
* `\makeatletter` sets `@` to category 11, `\makeatother` to 12.
  ([*clsguide*][clsguide] § 2.)
* `\ExplSyntaxOn` sets tab and space to ignored (9), `"` and `|` to other
  (12), `:` and `_` to letter (11), `^` to superscript (7), `~` to space (10),
  and `\endlinechar` to 32. Because 32 is ignored, an end of line contributes
  nothing while expl3 syntax is in force — a blank line makes no `\par`.
  ([`expl3-code.tex`][expl3-code].)

## 3. The line reader

TeX strips trailing spaces from each input line and appends the character
`\endlinechar` (13 by default, category 5); if `\endlinechar` is outside
0–255 nothing is appended and the line yields no end-of-line token at all.
The last line of a file is a line even without a final newline, and gets the
`\endlinechar` like any other ([tex.web][tex.web] § 31).

Reading then runs a three-state machine — new line `N`, skipping blanks `S`,
middle of line `M`. With the appended character in category 5, state `N`
yields `\par`, `M` a space, `S` nothing. In any other category it is read
like any character of that category: in category 10 (after
`\catcode13=10`, as `\ProvidesFile` does) it is a space token of character
code 32 in state `M` and nothing in `N` or `S`. A comment character discards the rest
of the line and moves to state `N`. State `S` is entered after any control
word — including a one-letter one — and after the control space `\ `, but
after no other control symbol. ([*TeXbook*][texbook] ch. 8; [*tex.web*][tex.web] § 303–306, § 240.)

A CRLF pair is one line ending, not two. **[abstraction]** TeX proper reads a
file line by line and never sees the line terminator; `satex` reads a
character stream and collapses CRLF so that files written on Windows tokenize
as they would on any other platform.

## 4. Tokens and their equality

A token is either a control sequence or a (character, category code) pair.
Two character tokens are equal when both components agree; two control
sequence tokens when their names agree. Source position is not part of a
token. ([*TeXbook*][texbook] ch. 7.)

A category-10 character read from the input becomes a space token of character
code 32 whatever the source character was. ([*tex.web*][tex.web] § 347.)

## 5. Meanings

Every control sequence has a *meaning*: a primitive, a macro, a character
(`\chardef`, or `\let` to a character token), a register, or undefined.
`\let\a\b` copies the *current* meaning of `\b` into `\a`: the two are
`\ifx`-equal until either is redefined. ([*TeXbook*][texbook] ch. 20.) A copy of a
primitive is that primitive: `\let\e\escapechar` makes `\e=-1` set
`\escapechar`, `\meaning\e` prints `\escapechar`, and `\ifx` tells copies of
two different primitives apart. ([*tex.web*][tex.web] § 1221, § 507.)

`\futurelet\a\b\c` assigns the meaning of `\c` to `\a` and leaves both `\b`
and `\c` in the input. ([*TeXbook*][texbook] ch. 20.)

## 6. Macro definition

`\def⟨control sequence⟩⟨parameter text⟩{⟨replacement text⟩}`.

* In the parameter text, `#1`…`#9` are parameters, numbered consecutively
  from 1, and any other token is a delimiter the argument list must match
  literally. `#0` is not a parameter.
* A `#` immediately before the `{` that opens the replacement text makes the
  last argument delimited by a begin-group character, and leaves that
  character in the input: `\def\every#1#{\csname every#1\endcsname}`.
  The `{` is a delimiter like any other, so with no parameter before it
  (`\def\m#{\detokenize}`) a call must be followed by `{`, which the call
  consumes and the replacement text puts back. ([*TeXbook*][texbook] ch. 20;
  [*tex.web*][tex.web] § 476.)
* In the replacement text, `#n` is replaced by the *n*th argument and `##`
  stands for a single `#`. ([*TeXbook*][texbook] ch. 20.)
* `\gdef` and `\xdef` are global; `\edef` and `\xdef` expand the replacement
  text at definition time. ([*TeXbook*][texbook] ch. 20.) What `\the` and `\unexpanded`
  contribute to it is stored as it stands: a `#` among those tokens is a
  character, not a parameter. ([*tex.web*][tex.web] § 478.) The replacement text is
  scanned while it is expanded and ends at the `}` that balances the braces
  expansion leaves, so `\edef\a{\iffalse}\fi b}` is `b`. pdfTeX's
  `\expanded` reads its text the same way. ([*tex.web*][tex.web] § 473, § 477.)
* `\long`, `\outer` and `\protected` are prefixes that apply to the next
  definition. ([*TeXbook*][texbook] ch. 20; [*etex_man*][etex_man] for `\protected`.)

## 7. Argument matching

* An **undelimited** argument is the next token, or, if that token begins a
  group, the whole group with one level of braces removed. Leading spaces are
  skipped. ([*TeXbook*][texbook] ch. 20.)
* A **delimited** argument does *not* skip leading spaces.
* Without `\long`, a `\par` token in any argument is a runaway-argument
  error; `\long` permits it. ([*TeXbook*][texbook] ch. 20.)
* A **delimited** argument runs to the next occurrence of its delimiter at
  brace level 0; if the collected text is wholly enclosed in one pair of
  braces, that pair is removed. ([*TeXbook*][texbook] ch. 20.)

## 8. The gullet

Expandable: macros, conditionals, `\csname`, `\expandafter`, `\noexpand`,
`\the`, `\number`, `\romannumeral`, `\string`, `\meaning`, `\jobname`,
`\fontname`, `\input`, `\endinput`, and the mark primitives `\topmark`,
`\firstmark`, `\botmark`, `\splitfirstmark`, `\splitbotmark`; ε-TeX adds
`\detokenize`, `\unexpanded`, `\scantokens`, `\unless` and `\eTeXrevision`,
and pdfTeX adds `\expanded`. Everything else is executed by the stomach.
([*TeX by Topic*][tex-by-topic], "Ordinary expansion"; [*etex_man*][etex_man] § 3.)

* `\expandafter⟨t1⟩⟨t2⟩` expands `⟨t2⟩` once and then puts `⟨t1⟩` back in
  front of the result.
* `\noexpand⟨t⟩`, when expanded, puts `⟨t⟩` back behind a marker; the next
  read of it treats an expandable `⟨t⟩` as `\relax` (the stomach does nothing
  with it) and `\ifx` gives it a meaning equal to no other token's, so
  `\expandafter\ifx\noexpand\a\a` is false for any macro `\a`, protected or
  not. ([*tex.web*][tex.web] § 358, § 367.)
* `\read⟨n⟩to⟨cs⟩` appends the current `\endlinechar` to each line it
  reads and reads further lines until the braces balance; a read at the end
  of the file gives `\par` and closes the stream, and only then is `\ifeof`
  true. ([*tex.web*][tex.web] § 482–486.)
* `\detokenize` and `\meaning` write a control sequence as the escape
  character and its name, followed by a space when the name has more than one
  character or its one character has category letter *now* — `\_ ` under
  expl3's catcodes, `\_` outside them. ([*tex.web*][tex.web] § 262.)
* `\csname⟨tokens⟩\endcsname` builds a control sequence from the expansion of
  `⟨tokens⟩`, which must be character tokens; their category codes are
  irrelevant, only the character codes count. If the name was undefined it
  becomes `\relax`, **locally**, and is thereafter defined — which changes
  later `\ifdefined` and `\@ifundefined` results. ([*TeXbook*][texbook] ch. 7;
  [*tex.web*][tex.web] § 372.)
* LuaTeX's `\begincsname` builds a name the same way but leaves an undefined
  one undefined and expands to nothing then; `\lastnamedcs` is the name the
  last `\csname`, `\begincsname` or `\ifcsname` built. (*LuaTeX manual*.)
* `\scantokens` writes its text to a pseudo file, a line wherever
  `\newlinechar` stands, and reads it back as a file is read: character by
  character under the category codes in force then, so a catcode change
  inside the text governs the rest of it, even on the same line. ([*etex.ch*][etex.ch]
  `pseudo_input`.)
* LuaTeX's `\scantextokens` reads its text as `\scantokens` does, but its last
  line gets no `\endlinechar` and no `\everyeof`. Its end is not the end of
  a file, so a definition or argument goes on past it. (*LuaTeX manual*.)
* A line gets the `\endlinechar` in force when TeX starts reading it, which is
  once the previous line has been used up. ([*tex.web*][tex.web] § 362.)
* Lua runs in one real Lua 5.3 state a run (LuaTeX's Lua): a `\directlua`
  chunk, the modules it `require`s or `dofile`s (found by SaTeX's resolver and
  recorded as loads), and the function a `token.set_lua` command calls
  (function `id` of `lua.get_functions_table()`, passed `id`). What the chunk
  or function prints with `tex.print`/`sprint`/`tprint`/`cprint`/`write` is
  read after it returns; `token.put_next` puts tokens back at once.
  (*LuaTeX manual*, "Lua general", "The token library", "The tex library".)
  * `string`, `table`, `math`, `utf8` and `coroutine` are Lua's own. The
    engine's libraries are Rust effects on the machine: the token library's
    scanners, `get_next`, `put_next`, `new`, `create`, `is_defined`,
    `get_macro`, `set_macro`, `set_char`, `set_lua` (local unless
    `"global"`, unexpandable if `"protected"`) and token fields; `tex.count`
    and `tex.dimen` (get, set, `getcount`, `setcount`, …), catcodes,
    `inputlineno`, `luatexversion`, `enableprimitives`, `runtoks(f)`;
    `kpse.find_file`, `lfs.attributes`; `texio.write` is a no-op;
    `callback.register` of a callback whose effect SaTeX does not read;
    `os.date("%z")`; `node.id`/`node.subtype` numbers; `sio`'s readers;
    `lua.newtable`; LPeg itself (vendored C, `vendor/lpeg`); a read-only
    `io.open`/`io.lines` of files the resolver finds (an absolute path only
    when the resolver finds its file name there). There is no writing, no
    other `os` (the clock is unknown), no `debug`, no binary chunk. Command
    numbers are LuaTeX's (`token.commands()`); the `mode` of a register
    command is its number.
  * Every global and library field LuaTeX has (`src/plugin/luatex_fields.txt`,
    listed by LuaHBTeX; `luaharfbuzz` only when the engine is known to be
    LuaHBTeX) exists: one SaTeX does not implement is, if a function, a
    function that stops the run when called, and otherwise the unknown
    value. A field LuaTeX does not have is nil. A value SaTeX does not know (a token field, the clock, a
    `status` field, …) is the unknown value. Arithmetic, concatenation,
    length and indexing of it give it again. Lua is patched
    (`vendor/lua/SATEX.patch`) so that a truth test (`if`, `while`,
    `and`, `or`, `not`), `==`, `rawequal`, `<`, use as a table key, a `for`
    bound, `type`, `tostring`, `table.concat` or a library argument check
    of it stops the run. Passing it to the engine stops the run, and so do
    a module SaTeX cannot find and a callback that changes what it reads.
    `pcall` does not catch any of these, so no branch is taken on an unknown
    value. `status.ini_version` is true while the kernel is read (or with
    no format), and `status.luatex_engine` is the engine `fmtutil.cnf`
    builds the `lualatex` format with. A run also stops past the step limit (1000 Lua instructions a
    step) or the memory limit. **[abstraction]**
  * A chunk that stops leaves an unknown token and a `lua-output` gap. So
    does a command whose function stops, after reading what it read so far,
    with a `lua-function` gap. The names the text that did not run (from
    the line where the chunk, function or module loading stopped) defines
    through `token.set_*` get an unknown meaning. That includes direct
    calls, calls through a local and calls through a forwarding function
    defined anywhere in the file. The modules the chunk names are loads.
  * After a run that stopped, or one inside undecided conditionals, a
    global the state lacks is unknown rather than nil when the text that
    may not have run (the chunk, the modules whose loading stopped, the
    function's own lines) assigns it, and every global when that text uses
    `load`, `_G`, `_ENV` or `rawset`. After a package cache restores Lua
    this state did not run, every missing global is unknown.
    **[abstraction]**

## 9. Conditionals

`\if…⟨test⟩⟨true text⟩\else⟨false text⟩\fi`. Conditionals are expanded in the
gullet; the branch that is not taken is skipped without expansion, so a `\def`
inside it does not happen. `\ifcase⟨number⟩` selects among `\or`-separated
cases and falls back to `\else`. ([*TeXbook*][texbook] ch. 20.)

While skipping, TeX recognizes conditionals, `\or`, `\else` and `\fi` by
their **meaning**, not their name, so `\let\myif\iftrue` affects the nesting
count and a `\csname if…\endcsname` does not. Braces are not tracked while
skipping: grouping and conditional nesting are independent.
([*TeX by Topic*][tex-by-topic], "Incorrect matching".) A file that ends while TeX is
skipping ends the skip with an inserted `\fi` ("Incomplete \if…").
([*tex.web*][tex.web] § 336.)

`\if` and `\ifcat` expand until two unexpandable tokens remain; an
unexpandable control sequence counts as character code 256 for `\if` and
category 16 for `\ifcat`, so all control sequences compare equal to each
other. ([*TeX by Topic*][tex-by-topic].) A token behind `\noexpand` is not expanded, and
an active character there counts as itself; any other active character is
taken by its meaning, as a control sequence is ([*tex.web*][tex.web] § 506).

A conditional is on the condition stack while its test is read. A conditional
that the test itself starts and leaves open (`\if e\ifx AB x\else y\fi …`)
sits above it: the next `\else` or `\fi` belongs to the inner one, and when
the outer one skips its false text, the inner `\fi` it passes ends the inner
conditional and an inner `\else` is passed over. ([*tex.web*][tex.web] § 498, § 500.)

Decidable when the operands are known: `\iftrue`, `\iffalse`, `\ifx`,
`\ifdefined`, `\ifcsname`, `\if`, `\ifcat`, `\ifnum`, `\ifdim`, `\ifodd`.
Mode-dependent: `\ifvmode`, `\ifhmode`, `\ifmmode`, `\ifinner` follow
the mode ([*tex.web*][tex.web] §§ 211, 1045–1200). A run starts in vertical mode;
horizontal material (a letter or other character, `\char`, a `\chardef`
character, `\indent`, `\noindent`, `\unhbox`, `\vrule`, `\hskip` and the
`\hfil` family, `\accent`, `\discretionary`, `\-`, control space,
`\valign`, `$`) starts a paragraph in a vertical mode, inserting `\everypar`
and reading the token again; `\par` ends it; vertical material (`\vskip`,
`\hrule`, `\unvbox`, `\halign`) in horizontal mode first reads a `\par`
token. `\hbox`, `\vbox`, `\vtop`, `\vcenter`, `\insert`, `\vadjust`,
`\noalign`, the parts of `\discretionary` and `\mathchoice`, `$`, `$$`,
`\eqno` and a `{` in math begin a list of their own in the mode that list
has (with `\everyhbox`, `\everyvbox`, `\everymath`, `\everydisplay`), and
its end restores the modes around it; `\currentgrouptype` reports the group
code. **[abstraction]** The run keeps the *set* of modes it may be in: paths
that disagree join to their union, a command of unknown meaning may start or
end a paragraph, and a test is decided only when every mode in the set
answers alike. Where the set matters to what a token does (a paragraph start
with a non-empty `\everypar`, vertical material under a redefined `\par`),
the run splits into one path per mode. Alignment cells and rows share one
mode set (restricted horizontal or internal vertical), `\left` may make a
display inner, and the output routine never runs. Box registers are tracked by kind ([*tex.web*][tex.web] §§ 1077–1110): a register is
void until `\setbox` fills it when its box ends (so the box's own text still
sees the old contents), locally or with `\global` globally; `\box` and
`\unhbox`/`\unvbox` empty it in place without a save-stack entry, `\copy`
and the `copy` forms do not; `\lastbox` and `\vsplit` give an unknown box.
`\ifvoid`, `\ifhbox`, `\ifvbox` are decided when the kind is known.
`\wd`, `\ht`, `\dp` are 0 for a void box, the `to` size (or 0 across it)
for a box that received nothing, what `\wd⟨n⟩=` set on a non-void box, and
unknown otherwise (a box with material is typesetting). `\ifeof` follows the
streams.
`\currentiftype` and `\currentifbranch` follow the condition stack;
`\lastnodetype` is -1 until anything may have been put on a list.
([*etex_man*][etex_man] for `\ifdefined`, `\ifcsname`, `\unless`.)

**[abstraction]** When the test is not decidable, `satex` analyzes every arm
and merges the resulting environments: a name defined in one arm becomes
*uncertain* and carries a control dependency on the conditional. This is what
makes `satex controls` able to say what a package option governs.

## 10. Grouping and the save stack

A group is opened by a begin-group *character* (`{`, catcode 1) and closed by
an end-group character (`}`, catcode 2) — including a control sequence whose
meaning is such a character, as `\bgroup` and `\egroup` are in plain TeX
(`\let\bgroup={`) — by `\begingroup`…`\endgroup`, by a math shift (`$`…`$`
and `$$`…`$$`), by `\left`…`\right`, by a box constructor (`\hbox`, `\vbox`,
`\vtop`, `\vcenter`), by an alignment entry, and by the output routine and
inserts. Each records its kind on the save stack, and a closer of the wrong
kind is an error: `}` ends only a simple group, `\endgroup` only a semi-simple
one. ([*tex.web*][tex.web] § 269 (`saved_group_code`), § 1063, § 1145; *The TeXbook*,
ch. 24.)

Every non-global assignment is undone when the group closes.
`\aftergroup⟨token⟩` keeps the token back and inserts it just after the group
ends, in the order the `\aftergroup`s were given. ([*tex.web*][tex.web] § 326.) `\global` sets the
value at the outermost level; the save-stack entries already pushed for that
name are not removed, they are discarded instead of restored when the group
ends — which is why alternating local and global assignments to one name make
the save stack grow. ([*tex.web*][tex.web] § 279, § 283; [*TeX by Topic*][tex-by-topic], "Save size".)

Some assignments ignore grouping entirely: `\globaldefs` forces every
assignment global when positive and makes `\global` a no-op when negative, and
font assignments, `\hyphenation`, `\patterns` and the box dimensions
`\ht`/`\dp`/`\wd` are always global. ([*TeX by Topic*][tex-by-topic], "Local and global
assignments".)

`\usepackage` and `\input` do **not** open a group: a package would be unable
to define anything if they did. `\makeatletter` around a package file is a
category code change, which LaTeX undoes explicitly. ([*clsguide*][clsguide] § 2.)

## 11. Base types

TeX's internal quantities are ⟨number⟩, ⟨dimen⟩, ⟨glue⟩, ⟨muglue⟩ and
⟨token list⟩. ([*TeXbook*][texbook] ch. 10, § 449.)

* A dimension is a signed integer of scaled points, `1pt = 65536sp`, with
  magnitude at most 2³⁰ − 1. ([*TeXbook*][texbook] ch. 10.)
* Unit ratios, as exact fractions of a point: `pc` 12⁄1, `in` 7227⁄100,
  `bp` 7227⁄7200, `cm` 7227⁄254, `mm` 7227⁄2540, `dd` 1238⁄1157,
  `cc` 14856⁄1157, `nd` 685⁄642, `nc` 1370⁄107, `sp` 1⁄65536.
* `⟨decimal⟩⟨unit⟩` is `trunc_toward_zero(round_ties_away(65536·decimal) ·
  ratio)`: the decimal is rounded to scaled points first, then the unit
  conversion is truncated. So `0.1pt` is 6554sp and `1in` is 4736286sp.
  ([*texdimens*][texdimens].)
* Glue is a width plus stretch and shrink, each with an order of infinity:
  finite, `fil`, `fill`, `filll`. ([*TeXbook*][texbook] ch. 12.)
* `\numexpr` and `\dimexpr` evaluate `+ - * /` with `*` and `/` binding
  tighter than `+` and `-`, left to right within one precedence level, and
  allow parenthesised subexpressions. Division rounds to nearest, ties away
  from zero. A multiplication immediately followed by a division is one
  scaling operation, not two rounded steps. Operators, parentheses and the
  terminator are looked for in the next non-blank token after expansion, so
  `\def\a{(4-1)}\numexpr 2*\a\relax` is 6. Registers nothing has assigned
  hold zero. One `\relax` may terminate the
  expression and is absorbed; otherwise it ends at the first token that cannot
  continue it, and that token is not absorbed. An overflow anywhere
  (beyond 2^31-1, or `\maxdimen` for dimensions and glue) makes the whole
  expression 0 with "Arithmetic overflow". ([*etex_man*][etex_man] § 3.5, [*etex.ch*][etex.ch]
  `scan_expr`.)
  Precisely ([*etex.ch*][etex.ch]): an operand beyond its level's bound (2³¹−1 for
  numbers and for the factor of `*` or `/`, `\maxdimen` for dimensions
  and each glue component) is an error; `+`/`-` is `add_or_sub`, `*`
  `mult_integers` or `nx_plus_y`, `/` `quotient` and `*`…`/` `fract`
  (`x·n/d` rounded, ties away from zero, checked only against the final
  bound, so `\numexpr 65536*32768/2` is 2³⁰). Adding glues of different
  orders keeps the higher one *unnegated*, even for `-`
  (`\glueexpr 1pt plus 2fil - 3pt plus 1fill` is `-2pt plus 1fill`); an
  expression of one glue is not normalized, so `\gluestretchorder` of
  `\glueexpr 0pt plus 0fil` is 1.
* Register arithmetic ([*tex.web*][tex.web] §§ 105-106, 1236-1240) is on 32-bit
  integers. `\advance` of a count or a dimension has **no** overflow
  check and wraps (`2147483647+1` is −2³¹; a dimension may pass
  `\maxdimen`, up to 2³¹−1sp); glue widths and amounts wrap alike, and
  of different orders the higher replaces the lower. `\multiply` is
  `mult_integers` (bound 2³¹−1) for a count and `nx_plus_y` (bound
  `\maxdimen`) for a dimension and each glue amount, orders kept;
  `\divide` is `x_over_n`, truncating toward zero (−2³¹ ÷ −1 is −2³¹).
  "Arithmetic overflow" (a bound passed, a zero divisor) leaves the
  register unchanged. A glue assigned with all three amounts zero becomes
  the zero glue of order normal (`trap_zero_glue`, § 1229).
* `scan_dimen` (§§ 448-461): the fraction is `round_decimals` of at most 17
  digits; an integer part of 2¹⁴pt or more, or a result of 2³⁰sp or more,
  is "Dimension too large" and `\maxdimen` (with its sign). `true` first
  divides by `\mag` (`xn_over_d`; a `\mag` outside 1..32768 counts as
  1000). An internal unit, `em`, `ex` or `px` scales by
  `nx_plus_y(n, v, xn_over_d(v, f, 2¹⁶))`, whose overflow is `\maxdimen`
  *before* the sign: `20000` times a unit of −1pt is `+\maxdimen`. `sp`
  ignores the fraction. `fil`, `fill`, `filll` (more `l`s stay `filll`)
  scale no unit.
* `em` and `ex` are `\fontdimen6` and `\fontdimen5` of the current font
  ([*tex.web*][tex.web] § 455), known when its metrics are.
* A number with no digits is 0 ("Missing number, treated as zero",
  [*tex.web*][tex.web] § 446); a token of unknown meaning where a number is expected
  makes it unknown.

### Fonts

* `\font\cs=⟨file name⟩` with `at ⟨dimen⟩` or `scaled ⟨number⟩` reads the
  name like `\input` does, and the space that ends it is consumed. `at`
  outside (0, 2048pt) becomes 10pt, and `scaled` outside 1..32768 becomes
  1000. A font already loaded with the same name and size (or the same
  request) is shared, so `\ifx` sees one font. ([*tex.web*][tex.web] §§ 526,
  1257–1260.)
* Metrics come from the TFM file, found beside the document or under the
  trees' `fonts/tfm` (through `ls-R`). `\fontdimen`, `\fontcharwd…ic`,
  `\iffontchar` and `\fontname` answer with the numbers the engine
  computes (`store_scaled`, § 571). A missing character measures 0.
  `\fontdimen` past the end grows the table only for the font loaded last,
  and the new entries start at zero (§ 580). Font assignments are global.
* A font the engine cannot load is `\nullfont`, with the warning
  `font-not-loadable` unless `\suppressfontnotfounderror` is positive.
  When whether it loads is unknown, the font's metrics and parameter count
  are unknown, and an `\ifx` against `\nullfont` is undecided. That happens
  when pdfTeX has only a METAFONT source (mktextfm may build the TFM), and
  when LuaTeX has no TFM (luaotfload's Lua callback may find the font).
* XeTeX (`load_native_font`): a `"quoted"` name is tried first as an
  installed font, then as a TFM file; an unquoted name is tried as a TFM
  file first. An installed font is `[file]` under the trees' `fonts/opentype`
  or `fonts/truetype`, or a family, full name or PostScript name that
  `fc-list` lists, which asks fontconfig as XeTeX does. Of a single `[file]`
  font satex reads the OpenType tables (`src/otf.rs`): the character map
  (`\iffontchar`, `\XeTeXcharglyph`, `\XeTeX{first,last}fontchar`,
  `\XeTeXcountglyphs`), advances (`\fontcharwd`, `.notdef`'s for a missing
  character), `\XeTeXfonttype` 2 unless it has Graphite tables, and the
  eight parameters: slant −tan(italic angle), space, space/2, space/3,
  x-height, the size, space/3, cap height, font units scaled in single
  precision and rounded as XeTeX's `D2Fix`. Its design size (so its size
  without `at`, and the `at` clause of `\fontname` and `\meaning`), glyph
  heights, depths, names and layout tables are not read: unknown, as is
  everything about a font fontconfig finds by name; their operands are
  still read. For a TFM font those queries answer at
  once and read nothing past the font (`not_native_font_error`).
* LuaTeX's `\fontid` is the font's number, with `\nullfont` as 0 and the
  others numbered in the order they were loaded. `\setfontid` selects a
  font by that number.
* `\Umathcode` and `\Umathchardef` pack family·2²⁴ + class·2²¹ + slot.
  `\Udelcode` packs 2³⁰ + family·2²¹ + slot. `\the\Umathcode` reads 0
  ("try `\Umathcodenum`"). An extended code read through `\mathcode` or
  `\delcode` is 0, as XeTeX 0.999998 reads it.
* The pdfTeX per-character codes start at 1000 for `\efcode`, 1 for
  `\tagcode` and 0 for the rest (`\lpcode`, `\rpcode`, `\knbscode`, …).

## 12. The LaTeX2e layer

`satex` reads the installation's own [`latex.ltx`][latex.ltx], class and package files, so
the kernel is not assumed and nothing about it is stubbed: `\newcommand`,
`\newenvironment`, `\NewDocumentCommand`, `\newcounter`, `\newlength`,
`\newif`, `\DeclareOption`, `\ProcessOptions`, `\define@key` and the rest run
for real, the way the engine would run them. Every one of them ends in an
ordinary primitive — `\def`, `\let`, a register allocation — which `satex`
records as one kind of `Definition` fact (`by` names whichever command the
file actually called, a user's own wrapper included) rather than a separate
record type per LaTeX concept.

A definition's semantic tag is read off what running it left behind, never
off the defining command's spelling:

* **counter** / **length**: a count or skip register was allocated; the
  counter's storage prefix (`\c@` in a stock kernel) is learned by probing
  `\value` with a made-up name once, rather than assumed.
* **switch**: the name starts `if` and now means `\iftrue` or `\iffalse`,
  `\newif`'s own effect ([*TeXbook*][texbook] App. B, [`plain.tex`][plain]).
* **environment**: `\⟨x⟩` and `\end⟨x⟩` were defined at the same source
  position, `\newenvironment`'s own effect.
* **option**: a macro was stored as `\ds@⟨option⟩`, the kernel's own
  bookkeeping for a declared option ([*clsguide*][clsguide] § 4.4–4.5); `\ProcessOptions`
  runs the declared options in declaration order against what the package
  and the class were given, `\ProcessOptions*` in the given order instead.
* **key**: a macro was stored under a key-store convention `\define@key`,
  keyval's `\setkeys`, or pgfkeys/l3keys use — `\KV@⟨family⟩@⟨key⟩` (or a
  package's own renamed copy of the same prefix, as enumitem's `enitkv@`
  is) or `\pgfk@⟨path⟩` and its `.@cmd`/`.@def` companions — recognized by
  the convention, not by package name.
* Otherwise, the primitive that made it: `\def` is `macro`, `\let` is
  `alias`, a register allocation is `register`.

Argument signatures are found the same structural way, including for
`\NewDocumentCommand`: every command's signature is *probed* (§ 13) rather
than read off a spec table, because `\def`, ltcmd/xparse's own expansion, and
a package's own `\@ifnextchar`-style hand code all look the same from
outside.

Labels, citations and the other keys a document declares through its
auxiliary files are observed the same way. Every line a `\write` hands an
opened file — expanded as `\immediate` expands it, or as shipout does with
`\protect` acting as `\noexpand` — is run in a copy of the kernel state at
`\begin{document}`, `@` a letter, where the next run reads the `.aux` back.
A key is text the document gave the writing call: characters whose tokens
stand in the document's own file, never text found by searching the source.
A name the line defines that is made of such a key is a key the writing
call declares (`\newlabel{k}` defines `\r@k`, `\bibcite{k}` `\b@k`); a key
the same call declares as another's extension (cleveref's `k@cref`) is that
key with the extension as suffix. A name of that shape a body call reads is
a use of the key, declared or not. A line that defines nothing but whose
argument its own call reads in such a name (`\cite{k}` writes `\citation{k}`
and tests `\b@k`) names the key for the program that reads the file: a
citation, and the declarations of that shape are bibliography entries. A
line written in another line's argument (`\@writefile{toc}{\contentsline…}`)
goes onward to a list file; an entry whose level has a running head
(`\⟨level⟩mark`) is a sectioning unit. What else is written names files; a
name that resolves to a `.bib` database, or ends in `.bib` (a `.bcf`
datasource), is a bibliography. Keys that never pass through a read-back file are declared the same way
during the run: a name a call in the document's own files defines whose
spelling embeds the content of one of the call's braced arguments
(`\newglossaryentry{k}` defines `\glo@k@name`, `\DeclareAcronym{k}` its
properties) declares key `k`, unless the call loaded a file of that name or
read the key itself as a name. The first call to define names of a key
declares it; a later one, and a read of a name of its shape by a call given
that key, uses it. The keys a line carries for another program
(`\indexentry`, `\glossaryentry`) are entries. No table of `.aux` commands
is consulted.

Copied-not-run text keeps the expansion behaviour of the definition it comes
from: one whose real definition is `\protected` (the expl3 definition
functions `\cs_new:Npn`, `\cs_set_eq:NN`, `\cs_generate_variant:Nn`,
`\prg_new_conditional:Npnn`, `\keys_define:nn`, the ltcmd declarations, the
lthooks setters) is copied, not run, by an `\edef` body, `\expanded`,
`\message` and `\write` ([*etex_man*][etex_man] § 3.4).

## 13. expl3

An expl3 *function* is `\⟨module⟩_⟨description⟩:⟨signature⟩`, or
`\__⟨module⟩_…` when private; a *variable* is
`\⟨scope⟩_⟨module⟩_⟨description⟩_⟨type⟩` and has no colon. The signature gives
the number of arguments: one per letter of `N n c V v o x e f T F`, and an
empty signature means none. A signature containing `p` (a parameter text),
`w` (unspecified) or `D` gives no reliable arity. ([*expl3*][expl3] § 3.2.6, § 4.)

`satex` uses this rule to step over any expl3 function correctly, including
ones it does not model.

### What a command takes

The signature `explain` reports is not read off the names of the kernel's
dispatchers or off a spec table (§ 12): every command, `\NewDocumentCommand`
included, is run instead. A copy of the machine at the
end of the run, or at `\begin{document}` for the preamble, reads the command
followed by probe input and stops when the main loop takes a probe character
itself, which the command then did not consume. Nothing it does is recorded.
The probe input is `⟨p⟩`, what is known to be taken so far, followed by
`{X}{X}…`; `n` is how much of that the command consumes. The run also notes:

- where a parameter text was not matched ([tex.web][tex.web] § 398, "doesn't match its
  definition"): required text such as the `x` of `\def\a x#1`, or a delimited
  argument that ran out of input before its delimiter (§ 392), or the `{` of
  `#{`. This is learned wherever it happens — in the command's own parameter
  text, in a macro it calls in any position, behind `\@ifnextchar` or
  `\futurelet`. Consumption stops there: the argument was not read.
- which characters a probe character, taken by `\let` or `\futurelet`, is
  compared with by `\ifx` (what the command looks for there);
- which keywords and quantities TeX's scanners read at a probe character
  ([The TeXbook][texbook], chapter 24): `to`, `plus`, `=`; ⟨number⟩, ⟨dimen⟩, ⟨glue⟩.

At `⟨p⟩`, in order: a star is taken when `⟨p⟩*…` consumes exactly `n + 1`
and is wanted to the same effect after it (or, where nothing was wanted, the
starred form then wants a delimited argument: `\@ifstar\a\b`); a character
`c` looked for is an optional argument (`[P]`, `n + 3`), a flag (`c`, as ltcmd's
`t`), an embellishment (`c{X}`, `e`) or the opening of a delimited argument
(`cP` followed by the closing the run then wants, `d`/`r`: required when the
call without it raises an error). None of these is taken when it is the very
text the plain call was refused for (`\def\a[#1]`), or when a macro took it as
text of a delimited argument (`\def\a#1#{…}`). Then a keyword tried before
any quantity began there, with the quantity it takes, keywords taking the
same being alternatives (`to|spread`); then a quantity; then required text or
a delimited argument wanted right here; and a mandatory argument when `n > 0`.
The default of an optional argument is shown when the macro argument that
held `P` holds text of its own when the bracket is left out, and text no call
can write (ltcmd's `-NoValue-`) is none. The probe is bounded in tokens and
time; past what it could decide the signature is unknown (`…`); where it
cannot say at all, a delimited parameter text is what the command reads.

The signature is written in one notation, xparse's where it has a letter:
`s`, `m`, `o`, `O{…}`, `t…`, `d`/`D`/`r`/`R`, `e{…}`, `u{…}` for text up to a
delimiter, required text as itself, `[kw ⟨quantity⟩]` for a keyword and
`⟨quantity⟩` for a quantity. `effective` writes a call. An expl3 name's
signature is only compared with this (`expl3-signature-mismatch`).

### Meanings and their context

A name can mean different things at different places in a run beyond the
preamble/document split (§ "Several definitions" in the wiki): `\item`
outside any list is the kernel's own error, inside `enumerate` or `itemize`
(both built on `\trivlist`) it typesets one. Every recorded definition
carries the environment stack in force where it was made — `\@currenvir`
generalized to however deeply the run had nested, tracked the same way as
`\begin{document}`'s own boundary (§ 9's `document_depth`) — read from the
run's own `\begin`/`\end` occurrences, never from a table of environment
names. `explain` groups a name's definitions by that context: two made in
different places with the same replacement text are one meaning, shown
under every context that holds it; two with different text are two. A name
never redefined can still differ by context when what it calls does: the
error a real use in the run raised there (found the same way `raised-error`
finds one, § lint) stands in for the meaning; without a real use in that
context, the meaning falls back to what probing the command turns up
instead.

## 14. Where `satex` is not exact

The semantics above are followed exactly. Where a real run's answer cannot be
known, `satex` **over-approximates**: it takes every outcome that the engine
could take, so that nothing real is missed. It never picks one outcome to move
on, and it never invents a meaning.

* **Undecided conditionals** are analyzed arm by arm and the environments are
  merged (§ 9), so a name defined in any arm exists afterwards and carries the
  control dependency that made it.
* **Undecided loops** are analyzed to a fixpoint. **[abstraction]** Every
  split — an undecided conditional, a name of several meanings, a mode
  test — is a *loop head* while its paths run. A path that meets the head
  again with nothing read from the source since, in the same place in the
  input and with the same conditional and group stacks and category codes,
  is a pass of a loop whose exit is undecided (`\loop … \ifhbox …
  \repeat`, pgfmath's `\let\next\pgfmathdivide@@ … \next`). Its state
  is joined into the head's (the join of the bindings below; the modes
  united) and the path stops. When that grew the joined state, the head
  analyzes all its paths again from it, which covers every earlier round;
  when nothing grew it, that is the fixpoint and the last round's paths
  are the loop's result, for every number of passes. After `widen_after`
  rounds (3) every binding that still grows is **widened**: a meaning set
  to an unknown meaning (a switch to an undecided switch), a moving bound
  of an interval to the end of TeX's range, other values to unknown; so
  the chain is finite and every loop ends within `widen_after + 2`
  rounds. Should the state still grow then, the run goes on with the last
  round and says so (`undecided-loop`). A path that meets the head
  elsewhere — a recursion that is not a tail call, a loop reading on in
  its input — is not a pass of the same state; it takes the other arm, so
  it leaves after one more pass (`undecided-loop`).
* **Numbers are intervals.** **[abstraction]** A count or dimension whose
  value is not known is still a number of its kind, in an interval
  `[lo, hi]` within TeX's 32-bit range and, when there are at most
  `value_set` (5) of them, one of a set of values. A box dimension or a
  node size starts from ±`\maxdimen`; a register of unknown value, and
  widening, from the whole 32-bit range, which `\advance` can reach.
  `\advance`, `\multiply`, `\divide` and every step of `\numexpr` and
  `\dimexpr` are TeX's exact operation (§ 11) on every combination of
  members when there are few (at most 64), else the operation on
  unbounded integers at the corners of the intervals with each cut at
  zero (each operation is monotone in each argument where the sign is
  constant, truncation and rounding included). A corner past the bound
  is an error: the in-range results lie in the clipped hull, joined with
  the unchanged register (`\multiply`, `\divide`) or with 0 (an
  expression, whose error makes the whole result 0); a zero divisor in
  the interval is likewise an error. A wrapping `\advance` (or −2³¹ ÷ −1)
  whose corner may leave 32 bits is any 32-bit number. An expression
  every member of which errs is 0. `\ifnum`, `\ifdim` and `\ifodd` are
  decided whenever the intervals (or every pair of members) decide them;
  `\ifcase` is decided when the selector's interval lies in one case (all
  negative is the `\else` text), and otherwise splits only into the arms
  from its lowest to its highest value, and the `\else` text when it may
  be negative or past the last arm. An undecided `\ifnum`/`\ifdim` of a register
  against a known number narrows the register on each arm to the values
  that take it (`\ifdim\wd0>5pt` leaves more than 5pt on the true arm).
  A `\glueexpr` with an unknown operand is unknown. Text of the number
  (`\the`) is unknown digits.
* **Joins by program point.** Paths of one split that stand at the same
  place in the input, with state a join can combine, are joined there even
  while other paths of the split have not come (equal states merge into
  one), so a dispatch that passes its continuation along, as pgfmath's
  parser does, does not multiply the paths. A path that stopped at a loop
  head, or reached `\end`, drops out of the join.
* **Typesetting is not modeled.** Box dimensions, the last item of a list
  and page breaks are unknown, so a conditional on them is undecided and goes
  through the rule above. The mode is tracked (§ 9).
* **Unknown text.** What the engine yields and `satex` cannot know — a
  random number, an unknown register or parameter under `\the`, `\number`
  or `\string` with an unknown `\escapechar`, a line read from the terminal
  in `\scrollmode` or `\errorstopmode`
  (a stream that is not open), the text of an undecided conditional inside
  an `\edef` — is a token of unknown meaning. A number scanned from it is
  unknown, text built from it (`\detokenize`, `\pdfmdfivesum`) is unknown,
  a replacement text holding it is not certain, and `\ifx` of such a macro
  with another macro is undecided. `\csname` with it inside builds one
  control sequence of unknown meaning known by the characters before and
  after the unknown part; defining it may define any name with that prefix
  and suffix, so from then on such a name is not known to be undefined
  (`\MT@inh@…` never makes `\@nil` so). Defining unknown text as a name
  leaves no name known to be undefined. Unknown text may hold a delimiter: a delimited argument that
  meets it at brace level 0 ends in it, and required text before an
  argument is taken to begin it; the rest of that text is read next as
  unknown text that holds no delimiter, so it is split once and a loop
  taking it apart ends. Unknown digits taken as one token (an
  undelimited argument, the token `\futurelet` looks at) are one digit,
  0 to 9, followed by unknown digits that may be none; where `\futurelet`
  looks at those, the run splits into a path on which it sees one more
  digit and one on which it sees what follows. `\csname` of a name with
  one unknown digit in it is one of the ten names, holding one of their
  meanings (an undefined one `\relax`), and `\ifx` of names that hold one
  of several meanings is decided when every pair of meanings decides it
  alike; `\let` copies such a set with the meaning. `\expandafter` of a
  name of unknown meaning, and `\edef` expanding it, give unknown text;
  of one a join left one of several macros without parameters, a token
  that stands for one of their texts. Signs that come from such a name
  whose every text is signs make the number unknown and are read past.
  A box's size is its list's natural size ([tex.web][tex.web] §§ 649, 668) while
  every item on the list is known: characters by their TFM metrics with
  kerns and `=:` ligatures (§§ 1034-1040), interword glue by the space
  factor (§§ 1041-1044), kerns, glue, rules, boxes, and whatsits, marks and
  penalties, which add nothing. `\lastskip`, `\lastkern`, `\lastpenalty`,
  `\unskip` and relatives read and change the last items (§§ 424, 1105).
  Anything else, such as math, alignments, paragraphs, other ligatures and
  native fonts, leaves the size unknown.
  `\ht`/`\dp` of an `\hbox` and `\wd` of a `\vbox` of unknown size
  are at least zero ([tex.web][tex.web] §§ 649, 668), and a known factor times a
  dimension in an interval keeps the interval.
  The digits of an unknown number (`\the`,
  `\number`, `\romannumeral` of an unknown quantity) are unknown text to a
  scan and to `\csname`, but a character of category 12 to `\ifx`, `\if`,
  `\ifcat`, `\let` and `\futurelet`: `\ifx` against a macro or a
  primitive is false, against a character of another category false, and
  against one of category 12 undecided; `\if` is undecided unless the other
  token is a control sequence.
* **A skip that leaves its file.** When skipping for a conditional whose
  `\if…` came from one file (a package's macro, say) reaches another file's
  text, the run splits (`conditional-crosses-file`): one path skips on as
  TeX does, the other ends the conditional there, as TeX would at that
  file's end (§ 9), and the paths join like the arms of an undecided
  conditional. A test or `\fi` that `satex` got wrong in package code thus
  never swallows the document. Inside an `\edef` or a scan only the second
  path is taken.
* **Joins.** Paths that meet with different category codes, a different
  waiting `\afterassignment` token or different `\aftergroup` tokens run on
  apart until those agree (or the join budget runs out, `paths-diverged`);
  what they define meanwhile is not certain. A register or code assigned on
  one path only is unknown after the join. **[abstraction]** A name the
  paths gave different meanings has *one of* those meanings after the join
  (up to `meaning_set`, 5; more widen it to an unknown meaning), and
  everything that does not look at that set sees an unknown meaning;
  `\ifdefined` and `\if` are decided when every member decides them
  alike. A register the paths left with different numbers holds their
  join (above). Within `meaning_splits` nested paths, the main loop
  executing such a name, or `\ifx` comparing it, splits the run into a path per meaning on which the name has
  that meaning, so `\ifx … \let\next\a \else \let\next\b \fi \next` runs
  `\a` and `\b` each on its own path and `\ifx\next\relax` is false on both;
  the paths join like the arms of an undecided conditional,, and a dispatch
  that comes round again is a loop head as above. Nested deeper than
  `meaning_splits` paths, or inside an `\edef` or a scan, the name is one
  of unknown meaning. The limit is 1 by default: on
  TikZ (`doc/architecture.tex`) deeper splits run pgfmath's parser over
  more unknown text and report more that a real run does not. A name that is a switch on every
  path (`\iftrue`, `\iffalse`, or a switch already joined) is an undecided
  switch after the join, still a conditional, so its `\else` and `\fi` match
  it. `\endinput` on a path ends the file for that path.
* **File end inside a scan** ([*tex.web*][tex.web] §§ 336–339): a file or `\scantokens`
  pseudo file that ends while a definition or text is scanned ends it (the
  inserted `}`), while conditional text is skipped ends the skip (`\fi`),
  and while macro arguments are matched abandons the call; each is a
  `file-ended` warning. `\everyeof` is read first, when an `\input` file
  or pseudo file ends, so a scan can find its delimiter there (expl3's
  `\file_get:nnN`). ([*etex.ch*][etex.ch] § 362.) The token `\noexpand` reads is read with the scanner
  normal, so `\everyeof{\noexpand}` still lets a scan run past the end.

Two limits are incompleteness rather than over-approximation, because a sound
analysis of them would not terminate. Neither is ever silent: each reports an
imprecision diagnostic naming the construct and the limit it hit, and any
finding that depends on such a point says so.

* **Recursion** is what the run observes: a macro is recursive when its
  expansion reaches it again. That is a call written in the replacement text
  of a macro whose expansion is under way, or whose expansion just ended
  with that call (a tail call). A name handed in as an argument does not
  count. A redefinition forgets what earlier definitions reached. Only for a
  document macro nothing expanded are the names in its replacement text
  taken as its calls, and findings then say so. `unguarded-recursion` reports
  an observed recursion only when the run could not see it end.
* **Recursion** is widened: a macro that re-enters itself more than
  `max_expansions` times, or is expanded more than `max_site_expansions` times
  at one call site — the shape of every TeX loop — stops being unfolded, and
  its result becomes unknown.
* **Budgets** bound tokens, graph size, fact counts, held tokens, the save
  stack, and the material one conditional may span. Reaching one stops the
  analysis with a `budget-exhausted` diagnostic. As in TeX, running out of
  capacity — expansion nested 300 deep inside scans, a token list longer than
  the held-token bound, more than 15 open (pseudo) files — stops the run.

`satex query diagnostics` lists every such point in a run, and
`satex lint --filter 'code=analysis-imprecision'` groups them by reason.

## 15. Caches

A cache never changes an answer: a run that starts from one reports exactly
what the run that wrote it did.

* The **kernel cache** holds the meanings and catcodes [`latex.ltx`][latex.ltx] leaves. It
  is keyed on the installation, the engine, the preloaded format and the
  interpreter's source, and rejected when [`latex.ltx`][latex.ltx] changed.
* A **package cache** holds what one `\usepackage` or `\documentclass` of
  the document did, and the *read set* it did it from: every meaning and
  register value the package consulted before assigning it. Hooks, options
  and the packages it asks about are ordinary kernel state (`lthooks`,
  `\opt@⟨file⟩`, `\ver@⟨file⟩`) and enter the read set like any other name.
  The class options are one of them: the kernel's `\ProcessOptions` looks
  for each declared option in `\@classoptionslist` (`\in@` over an `\edef`
  copy), so a package that declares options — or maps the list itself, as
  xcolor does — is read again when they change, and only one that declares
  none stays served.
  A read records only the part it looked at: whether a name is defined, what
  skipping a conditional sees of it, and for `\ifx` of a macro with a
  non-macro only that it is a macro (the comparison is false whatever the
  text). A text *copied through* is not read either: `\xdef\l{\l,x}`, where
  `\l` holds characters only and no parameters, appends its old text
  unexamined (the kernel's `\@filelist`). The segment records the new text
  with the old one's place in it, and a replay puts whatever text `\l`
  holds there, so a different list before the load still hits and ends as
  reading the file would. Any other use of the old text — a conditional,
  `\meaning`, a match against a delimited parameter, a copy under another
  name, a second copy, a local definition, the text growing past a limit
  it met — is an ordinary read of it.
  The file is cut into *segments* at line starts where the interpreter is at
  rest (no open group, argument or expansion beyond the load's own); each
  segment stores its read set and its effect, and the segments form a tree.
  A run replays the longest prefix of segments whose read sets it agrees on
  and reads the rest of the file, storing that tail as a new branch. A
  segment is rejected when a file read by then changed or a name looked up
  then now resolves elsewhere. The effect is relative to the load: its call
  site and the call it is read on behalf of (`\begin{document}` for a load
  from a hook) stand for whichever are current where it is replayed, and a
  redefinition's reset of what the name calls is replayed with the calls
  that follow it. Register allocation is replayed as allocation: what the
  package took from `\count10`–`\count19` it takes again from the replaying
  run's counters, and its registers are renumbered to match. **[abstraction]**
  A register number the package keeps as plain text, other than as a printed
  register such as `\count24`, is not renumbered.

[texbook]: https://ctan.org/pkg/texbook
[tex.web]: https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web
[tex-by-topic]: https://ctan.org/pkg/texbytopic
[etex_man]: https://mirrors.ctan.org/systems/e-tex/v2.6/doc/etex_man.pdf
[clsguide]: https://ctan.org/pkg/clsguide
[usrguide]: https://ctan.org/pkg/usrguide
[expl3]: https://ctan.org/pkg/expl3
[texdimens]: https://ctan.org/pkg/texdimens
[expl3-code]: https://github.com/latex3/latex3/blob/main/l3kernel/expl3-code.tex
[plain]: https://ctan.org/pkg/plain
[etex.ch]: https://mirrors.ctan.org/systems/e-tex/v2.6/etex.ch
[latex.ltx]: https://github.com/latex3/latex2e/blob/develop/base/ltkernel.dtx
