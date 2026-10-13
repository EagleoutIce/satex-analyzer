<!-- generated from doc/src/wiki/domains.md.in by satex-doc; do not edit -->

<p align="center"><a href="https://eagleoutice.github.io/satex-analyzer/"><img src="https://raw.githubusercontent.com/EagleoutIce/satex-analyzer/main/doc/wiki/img/satex-wide.svg" alt="SaTeX: Static Analysis for (La)TeX and Friends" width="600"></a></p>

# Abstract domains

`satex` is an abstract interpreter: it runs TeX's own machine, but over
values that stand for *sets* of what a real run could hold. This page lists
those domains, what each one abstracts, and where it joins, narrows and
widens. For the rules that govern *when* a join, split or widen happens, see
[SEMANTICS § 14](https://eagleoutice.github.io/satex-analyzer/SEMANTICS#14-where-satex-is-not-exact).
See [Primitive-coverage](Primitive-coverage) for how each primitive uses these
domains, and [Lints](Lints) and [Chktex-Coverage](Chktex-Coverage) for what a
rule builds on top of them.

Every domain here obeys one invariant: a construct is followed **exactly**,
or its result is **over-approximated**. `satex` never guesses a single
outcome and never treats "I can't follow this" as a resting state.
Anything it cannot pin down goes up the lattice to a value that covers every
real outcome, and it says so in a diagnostic.

The domains on this page:

- [`Value`](#value-what-a-control-sequence-or-register-holds) and [the set lift `OneOf`](#the-set-lift-oneof)
- [Character codes as sets](#character-codes-as-sets)
- [Colors](#colors-color-and-colors)
- [Meaning sets](#meaning-sets-what-a-control-sequence-may-mean), with [names from unknown text](#names-from-unknown-text)
- [Modes and box dimensions](#modes-and-box-dimensions), with [paragraphs](#paragraphs-and-line-breaking)
- [Unknown text](#unknown-text-the-textclass-lattice), with [unknown characters and digits](#unknown-characters-and-digits) and [length bounds](#length-bounds-on-typed-text)
- [Unknown category codes](#catcodetableunknown-unknown-category-as-a-lattice-element)
- [Files written in the run](#files-written-in-the-run)
- [Call summaries](#call-summaries-what-a-call-did-twice), with [the line guard](#the-line-guard-termination)
- [The join](#the-join-in-general), with [joins at `\fi`](#joins-at-fi) and [joins at the limits](#joins-at-the-limits)
- [`limits`](#limits-where-the-domains-stop-being-precise) and [registers behind an unpinned index](#registers-behind-an-unpinned-index), with [code tables behind an unknown character](#code-tables-behind-an-unknown-character)
- [Known limits](#known-limits) and the [soundness invariant](#soundness-invariant)

## `Value`: what a control sequence or register holds

[`src/value.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/value.rs) defines:

Every kind of quantity has one lattice: an exact value, a bounded set of
exact values that a join of different ones leaves, and a top. The set is
the same mechanism for every kind ([the set lift](#the-set-lift-oneof),
below), so the table has no gaps.

| Kind | Exact value | Interval / hull | Bounded set (≤ `limits.value_set` exact, ≤ `limits.interval_set` with intervals) | Top |
| --- | --- | --- | --- | --- |
| count (`\count`, `\numexpr`, integer parameters) | `Int(i64)` | `Num { lo, hi }` | `OneOf([Int or Num, …])` | `Num::full(false)`, `[-MAX_INT, MAX_INT]`, read as `Unknown` |
| dimension (`\dimen`, `\dimexpr`) | `Dimen(Scaled)` | `Num { lo, hi }`, scaled points | `OneOf([Dimen or Num, …])` | `Num::full(true)`, `[-MAX_DIMEN, MAX_DIMEN]`, read as `Unknown` |
| glue (`\skip`) | `Glue(Glue)` | `GlueHull`: one `Num` per component ([tex.web § 150](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L3147)) | `OneOf([Glue, …])` | `GlueHull::full()`, read as `Unknown` |
| muglue (`\muskip`) | `MuGlue(Glue)` | `GlueHull` | `OneOf([MuGlue, …])` | `GlueHull::full()`, read as `Unknown` |
| token list (`\toks`) | `Toks(Rc<[Token]>)` | past the set: the common beginning and end around one typed unknown token, when the middles are balanced and hold no control sequence | `OneOf([Toks, …])` | `Unknown` |
| color (`\color@⟨name⟩`) | `Color(Colors)`, every component a point | `Colors { hull: Color }`, one `Num` per component | `OneOf([Color, …])` | the hull over the model's full range |

A token list join past `limits.value_set` keeps its frame, as a macro join does: pgfmath's operand stacks with different numbers on top join to one stack whose top is typed digits, not to `Unknown`, so a pop by delimited parameters still finds its braces ([tex.web § 1227](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L22968)).
`Unknown` is the top of every kind: unresolved typesetting output, text with unknown digits, a read through an unpinned index.
A bounded number that becomes text (`\the`, `\number`, `\pdfuniformdeviate`) is one token of unknown digits typed with its `Num`, which a later scan reads back as that `Num`, so a bound survives `\numexpr` through text and a recursion over it ends.
A register read goes through `Env::value` ([`src/env.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/env.rs)), which reads a `Num`, `GlueNum` or `OneOf` as `Unknown`.
Only code that inspects the abstract value sees it. A comparison in `\ifnum`/`\ifdim` decides by the interval; a join or loop head compares the hulls.

A count and dimension share one payload shape, `Num`, tagged by `Quantity` (`Int`, `Dimen`, `Glue`, `MuGlue`).
Glue and muglue use a `GlueHull` instead: one `Num` per component, because glue has components ([tex.web § 150](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L3147)) that a single interval cannot carry.
`Value::Num` and `Value::GlueNum` form one family, split only where the shape differs.

## The set lift: `OneOf`

`Value::OneOf(Rc<[Value]>)` is TeX's *one* set lift, `Set_k(D)`: a few
values of one kind, each exact or an element of `D` (a `Num` interval, a
`GlueHull`, a `Colors` hull, or `Unknown` for token lists, which have none).
There is one implementation (`Value::join`, `Value::members`, `Value::hull`)
that every kind goes through. A number left in two disjoint ranges is the
set `{[0pt,10pt], [100pt,110pt]}`, not their hull `[0pt,110pt]`.

- **Concretization**: a `OneOf` stands for the union of its members: each exact member is itself, each `D` member every value it admits.
- **Join**: the union of both sides' members. A number inside an interval member is dropped, and intervals that overlap or touch become one.
  The set stays while it has at most `k` members: `limits.value_set` (8) when all are exact, `limits.interval_set` (4) once an interval is among them.
  Past `k`, or once a side is `Unknown`, the join widens: every member's own `D` (`Value::hull`), joined together (`Value::join_hulls`).
- **Widening** (`α(members)`): the same operation a join-past-`k` does, applied once — the interval hull of every member, componentwise for a `GlueHull`.
  Only the gaps between members are lost. The hull still contains every value the set did.
- **Tests**: `\ifnum` and `\ifdim` on a register that holds a set decide it member by member (`Value::member_nums`). The test is decided when every member gives the same answer.
  Otherwise each arm keeps each member's part that passes, narrowed (`Num::narrow`), and drops the members with none. The arms rejoin at their `\fi` ([joins at `\fi`](#joins-at-fi)).
- **Other kinds**: macro bodies are [meaning sets](#meaning-sets-what-a-control-sequence-may-mean) (`limits.meaning_set`), token lists are sets of texts in the same lift. Past their limit both keep their common frame with typed unknown text inside.

Arithmetic (`Num::lift`) and conditionals (`Num::compare`, `Num::odd`,
`Num::narrow`) work on the widened `D` directly: exact at a point, otherwise
the corners of the interval box (cut at zero, since TeX's arithmetic is
monotone on each side of zero) bound the ideal result. An overflow of the
exact range is TeX's arithmetic error, kept in-range by clipping. A 32-bit
wraparound (`\advance` past the register's own width, [tex.web § 1238](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L23134))
instead widens to the *entire* 32-bit range (`WORD_MIN..=WORD_MAX`), not
`\maxdimen`, since `\advance` can pass that too.

## Character codes as sets

A code table entry (`\lccode`, `\uccode`, `\sfcode`, `\mathcode`, `\delcode`, [tex.web § 1232](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L23040)) holds a `Value` like a register.
A join keeps a few different codes as a `OneOf`, up to `limits.value_set`.
The scan keeps them too. Where a ⟨number⟩ is a character that is one of several (`` `\x `` for a `\x` of several meanings), it reads as the hull of their codes plus the exact set (`abs_exact` in [`src/scan.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/scan.rs)).
`assign_char_code` stores a set or an interval as read while every member is a valid code.
A set with an invalid member, or any set for `\mathcode`, becomes `Unknown`.

`\lowercase` and `\uppercase` (tex.web § 1288, `case_shift`) read the entry of each character.
Where it is one of a few codes, the character becomes one token that stands for one of the shifted characters, each with its category kept.
Where the code is unknown, the character becomes unknown text.

## Colors: `Color` and `Colors`

xcolor colors are recorded the same way as a count or dimension, one abstraction level up:

- `Color { model, components: Rc<[Num]> }` is one color: an xcolor model (`Model`, xcolor table 3) and one `Num` per component, in scaled points (xcolor's arithmetic units).
- `Colors { hull: Color }` is what a binding holds: a component-wise interval hull.
  An exact color has every component as a point (`Color::exact`); a few exact colors a binding may be are, like every other kind, `Value::OneOf` of them, not a set `Colors` keeps.

`Colors::join` takes the hull component-wise (`Num::join` per component).
Two colors of different models have no common bound. Converting one model to another would invent a value xcolor never stored, so joining across models gives `None`.
The caller then widens to `Value::Unknown` instead of fabricating a converted spec.

This domain lives entirely in `src/value.rs` already. The color lints and
producers that populate it are being finished separately from this page.

## Meaning sets: what a control sequence may mean

`Binding` ([`src/env.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/env.rs)) carries one `Meaning`, plus, after a join of
paths that disagreed, `may: Option<Rc<[Meaning]>>`. That set holds the
meanings the name may have, bounded by `limits.meaning_set` below. While
`may` is set, `meaning` itself reads as unknown to anything that does not
look at the set.

- `\ifx` and `\ifdefined` are decided when every member of the set(s)
  agrees.
- Executing a name with several meanings, or comparing it with `\ifx`,
  *splits* the run into one path per meaning, but only `limits.meaning_splits`
  levels deep (below). Beyond that, or inside an `\edef`/scan, the
  name is simply of unknown meaning. The default is deliberately shallow.
  Measured on a TikZ document, splitting further runs
  pgfmath's parser over more unknown text for one more error caught at
  seven times the run time.
- `\let` copies the whole set, not just one meaning.
- A loop head widens a meaning that keeps growing, and a write `satex` cannot follow widens a name in place.
  Macros without parameters whose texts differ only in their digit runs first become one macro, with each run made typed digits.
  Otherwise a set of macros without parameters that share their `\long`, `\outer` and `\protected` prefixes widens to one such macro whose text is unknown text (`macro_hull` in [`src/env.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/env.rs)).
  The text holds at least one token when every member's text does.
  So `\ifx` can still tell the widened name from a primitive, a character or a macro with parameters.
  Any other set widens to an unknown meaning.

**Worked example**: `\ifx\a\b \let\next\foo \else \let\next\bar \fi`. After
the join, `\next`'s binding has `may = Some([foo, bar])`. `\ifx\next\relax`
is undecided (the members disagree). Executing `\next` splits into a path
where it is `\foo` and one where it is `\bar`, which then join again like
the arms of an undecided conditional.

## Modes and box dimensions

`Modes` ([`src/mode.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/mode.rs)) is a bitset over TeX's six
modes ([tex.web § 211](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L4220): vertical, internal vertical, horizontal, restricted
horizontal, display math, math). The run keeps the *set* of modes it may be
in. `Modes::test` decides an `\ifvmode`/`\ifhmode`/`\ifmmode`/`\ifinner`
only when every mode in the set answers alike, and disagreeing paths join
by `union`. `INNER` picks out the three "-mode" (inner) variants [tex.web](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web)
distinguishes from their outer counterparts.

`Dim { lo, hi }` is the analogous domain for a single length: box
dimensions and node sizes, added up the way `hpack` ([tex.web § 649](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L12910)) and
`vpack` ([§ 668](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L13152)) add a list's items. `Dim::ANY` (`±MAX_DIMEN`) is the top of
the lattice. What an item `satex` cannot follow widens the running size to,
rather than leaving the item out, which would under-approximate. `Natural`
bundles the running `along`/`height`/`depth`/`wide` dimensions plus the
list's tail, mirroring the fields `hpack`/`vpack` compute.  The tail's items
(`Last`: a character, glue, a kern, a penalty, anything else) have
`Last::Unknown` as their top, and whether a character node is a ligature is
a flag whose top is "not known".  `\lastnodetype` reads both (etex.ch), so it
is exact where the font's ligature/kerning program was followed ([tex.web §§ 1034](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L20114)–[1040](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L20271), boundary characters included). Where the flag is not known it
is the interval 0 to 7, never glue, a kern or a penalty. Reading the tail
inside a word does not end the word ([§ 1036](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L20193)). A `\write` is a whatsit, or
nothing after `\immediate` ([§ 1375](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L24937)), as are `\pdfstartlink`, `\pdfendlink`,
`\pdfsave` and `\pdfrestore`.

### Boxes

A box register holds its width, height and depth, one `Dim` each.
`\wd`, `\ht` and `\dp` read them, and `\setbox` writes them when the box is packed.
The sizes come from the list, as `hpack` and `vpack` would compute them:

- A character is its box from the font's TFM metrics: width, height, depth, plus the kerns and ligatures of its ligature/kerning program.
  A font without metrics widens the sizes.
- `hpack` starts height and depth at zero ([tex.web § 649](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L12910)), and `vpack` starts the width at zero ([§ 668](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L13152)).
  So these are clamped to at least zero. The other sizes may be negative, as in TeX.
- `vpack` moves the depth past `\boxmaxdepth` into the height ([§ 668](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L13152)).
  A `\vtop` takes its height from its first box ([§ 1087](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L21078)), and a `\vcenter` is centred on the math axis ([§ 736](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L14454)).
- Inline math made of plain characters is followed ([§§ 752](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L14748)–[755](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L14862), [1151](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L21874)–[1155](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L21946)).
  An ordinary character adds its box, its italic correction and its kern with the character before.
  The formula adds two `\mathsurround` kerns ([§§ 1194](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L22404)–[1196](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L22455)).
  A `\mathchardef` symbol is one atom, as a character is ([§ 1155](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L21946)).
  Relations, openings, closings and punctuation add the space their classes call for ([§§ 764](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L15038)–[766](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L15073)): `\thinmuskip`, `\medmuskip` or `\thickmuskip` at its natural width, in units of the math quad over 18 ([§§ 716](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L14093), [732](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L14391)).
  A binary atom may turn ordinary ([§§ 728](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L14323)–[729](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L14348)), so its width is widened. An operator is shifted, so its whole size is.
- `\unhbox` of a naturally packed hbox of boxes and rules alone adds their natural width ([§ 1110](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L21332)).
  The register also keeps how many there are, so the list's tail is known after it: `\unskip` then takes nothing.
- `\unhbox` of any other hbox adds the natural width recorded when it was packed, whatever it was packed to: [§ 1110](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L21332) moves the list with its glue unset.
  Its last item is kept, so an `\unskip` after it takes its glue away. tikz's `align` lines in an `\halign` stay exact.
- Glue and kerns in inline math are no atoms ([§ 761](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L14984)). `\mskip` and `\mkern` are in math units, a math quad over 18 in text style ([§§ 716](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L14093)–[717](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L14117), [732](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L14391)).
  The character before takes its italic correction ([§ 755](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L14862)). amsmath's `\dots` ends in `\,`, so its width stays known.
- `\immediate\write`, `\openout` and `\closeout` leave the list as it was ([§ 1375](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L24937)).
  Deferred, each is a whatsit at the list's end ([§ 1352](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L24702)).
- A `\discretionary` adds its text without a break to the list. Its break texts do not count ([§§ 651](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L12946), [1119](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L21419)–[1120](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L21440)).
  In inline math, `\lowercase`, `\openin`, `\closein` and `\pdffontexpand` add no atom, so loading a font in a formula keeps its size.
- A box of a kind `satex` does not know has `Dim::ANY` for all three sizes.

### Paragraphs and line breaking

A paragraph in a vertical box is line-broken as `line_break` does it ([tex.web §§ 813](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L15997)–[890](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L17438)).
`Para` in [`src/mode.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/mode.rs) holds the range of lines made so far and the vertical list they go onto.

- Each line is a box of the line width ([§ 889](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/mode.rs)).
  That is `\hsize` without `\parshape` and `\hangindent` ([§§ 847-848](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/mode.rs)). With either, the width is unknown.
- A penalty of -10000 or less forces a break ([§ 851](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/mode.rs)). The glue, kerns and penalties after it are discarded ([§ 879](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/mode.rs)).
  Forced breaks are exact.
- Between forced breaks, a text whose items are all known is broken by Knuth-Plass ([§§ 829-875](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/mode.rs), src/linebreak.rs).
  Each item is recorded as it is appended, and the record must add up to the summary's width, height and depth.
  The passes are those of [§ 863](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/mode.rs): without hyphenation within `\pretolerance`, with it within `\tolerance`, then with `\emergencystretch`.
  The line count, each line's height and depth, and `\prevgraf` ([§ 890](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/mode.rs)) are then exact.
  It needs `\looseness`=0, `\lastlinefit`≤0, no `\parshape` or `\hangindent`, no infinite shrink, and known parameters.
  A ligature, math, `\discretionary`, insert or mark in the text, or a kern or boundary character at a hyphen, falls back to the rules below.
- Otherwise, a line that fits the width with infinite stretch has badness 0 ([§§ 851-859](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/mode.rs)).
  It is then one line, provided no break could cost less: penalties, `\adjdemerits` and hyphen demerits are not negative, the line before is `decent_fit` ([§ 817](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/mode.rs)) and `\looseness` is 0.
  With finite stretch only, the line's badness has an upper bound `b` ([§ 108](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/mode.rs)): the most it falls short of the width over the least it stretches.
  Its glue stretches no less than nothing unless some glue has negative stretch.
  It is one line when (`\linepenalty` + `b`)² < 2·`\linepenalty`² and `b` ≤ 99, `\tolerance` and `\pretolerance`: two lines cost more ([§§ 817, 859](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/mode.rs)).
  A line without a legal breakpoint ([§ 831](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/mode.rs)) is one line however wide.
- Otherwise the text is from one line up to one line per breakpoint.
  Each line's height and depth lie between zero and the text's height and depth.
- At `\par` the final glue is dropped and `\penalty10000\parfillskip` is appended ([§ 816](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/mode.rs)).
  The breakpoint that glue was is gone, as is one `\unskip` or `\unpenalty` takes away ([§ 1105](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/mode.rs)).
- Between lines comes interline glue ([§ 679](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/mode.rs)): `\baselineskip` less the depth above and the height below, or `\lineskip` where that would fall below `\lineskiplimit`.
  Where the sizes allow both, the distance is the join of both.
- `\prevgraf` is 0 at a paragraph's start and the line range at its end ([§§ 1091, 890](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/mode.rs)).
- `\prevdepth` is tracked per vertical list ([§ 219](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/mode.rs)). A box starts with `ignore_depth` ([§ 1083](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/mode.rs)). The lines set it to the last line's depth.
- `\parskip` goes onto the list before the paragraph when the list is not empty ([§ 1091](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/mode.rs)).

Hyphenation follows [tex.web §§ 919](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L18072)–[965](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L18820) on the format's own data.
`\patterns` and `\hyphenation` store their text per `\language`, letters lowercased by `\lccode` (src/hyph.rs).
At a paragraph's end each word gets the gaps its patterns or exception mark, within `\lefthyphenmin` and `\righthyphenmin`.
Whether the line breaker takes a gap is not followed, so each gap is a may-hyphen (`·` in the typeset view).
A paragraph known to be one line gets none: a hyphen is set only at a line's end ([§ 902](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L17669)).
An unknown language, pattern text or `\lccode` marks every gap of the word.

The line parameters are read at `\par`. When they changed since a line was set, the lines are not followed and their sizes widen.

## Unknown text: the `TextClass` lattice

Unknown text is not one element. `TextClass` ([`src/tex.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/tex.rs)) is a set of seven token kinds, and unknown text of a class holds only tokens of those kinds.
The order is set inclusion.

| Kind | Letter | Tokens |
| --- | --- | --- |
| `DIGIT` | `d` | `0` to `9` of category 12 |
| `SIGN` | `s` | `+`, `-`, `.` and `,` of category 12 |
| `LETTER` | `l` | category 11 |
| `OTHER` | `o` | any other character that does not open a group |
| `SPACE` | `_` | category 10 |
| `BRACE` | `b` | balanced braces |
| `CS` | `c` | control sequences and active characters, so anything |

Named elements, from the bottom:

```text
unknown text          any class (all seven kinds, `TextClass::ANY`)
  |
cs-free text          d s l o _ b        no control sequence
  |
dimension text        d s l              a number and unit letters
  |
number text           d s                signed, with a point or comma
  |
typed digits          d                  [lo, hi], at least one digit
```

Each step down rules out more.
Text of a class holds a token only if the class holds its kind (`TextClass::holds`).
So a scan delimited by `}` finds none in text without `b`, a scan delimited by a space finds none without `_`, a `\futurelet` look-ahead or an `\ifx` against a letter cannot match without `l`, and nothing without `c` can be a control sequence or an active character.
All unknown text comes from one constructor, `Machine::unknown_text`. It takes a class with a lower and upper length bound, or the texts the unknown text stands for, and yields typed text.
The rest of typed text after a scan split it is typed text of the same class, perhaps empty, shorter by the delimiter. It may hold the delimiter again ([tex.web § 392](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L8022)).
Text of which nothing is known is the widest typed text: characters that are neither active nor braces.
`\detokenize` and `\string` print typed text in its printed class (`TextClass::printed`, [tex.web § 464](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L9142)): letters and braces become category 12, a control sequence its characters. `\string` prints only the first token, the rest keeps its class.
The untyped marker is left for texts that name more control sequences than typed text keeps. `--stats` counts it per call site.

Typed digits do more. A later `\ifnum`, `\numexpr` or `\csname` reads their `Num` back, so the test is decided by the interval and the name is one of a bounded set.
Every class also says whether it has at least one token.

The join of two classes is the union of their kinds, and the text is non-empty only if both are.
A run of unknown text collapses into one token of the class the run spans, and a class that holds `c` is plain unknown text, the top.
Widening therefore moves up this lattice and stops at the top.

Stubs (`stubs:` in [Configuration](Configuration#stubs)) use these elements for what a macro leaves:

| `result` | Element |
| --- | --- |
| `nothing` | the empty text |
| `number` | typed digits over the range of a TeX integer |
| `{ number: [lo, hi] }` | typed digits `[lo, hi]` |
| `dimen` | dimension text |
| `{ text: digits }` | typed digits with no range |
| `{ text: number }` | number text |
| `{ text: dimension }` | dimension text |
| `{ text: cs-free }` | cs-free text, possibly empty |
| `text` or `{ text: any }` | unknown text |
| `box` | a box of unknown size |
| `{ box: { width, height, depth } }` | a box within the given `Dim` intervals |

### Unknown characters and digits

One unknown character is a character token with a code no document spells ([`src/tex.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/tex.rs)).
Each stands for a fixed set of characters of category 12:

| Character | May be |
| --- | --- |
| `UNKNOWN_CHAR` | a digit or `-`, the first character `\the` prints for a number |
| `UNKNOWN_DIGIT` | a digit `0` to `9` |
| `UNKNOWN_NONZERO` | a digit `1` to `9` |

`\the` and `\number` never print a leading zero ([tex.web §§ 103](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L2199), [104](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L2225)).
`satex` keeps this in the integer digits it makes for an unknown number, marked with `LEAD_MARK` ([`src/machine.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/machine.rs)).
A number known to be at least 1 starts with `UNKNOWN_NONZERO`. One that may be negative starts with `UNKNOWN_CHAR`.
Reading the first token of such digits splits the run into one path per case:

- `-`, with every digit still to come,
- `0`, with nothing after it,
- `1` to `9`, with at most one digit fewer after it.

So a loop that strips digits one by one sees the integer part end after a `0`.

`\romannumeral` of an unknown number gives typed text of class `o`, possibly empty: its letters are category 12, as [tex.web § 69](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L1663) prints them.
A bounded number under `\number` gives typed digits with that bound.

A dimension in an interval of one sign prints under `\the` as its sign, typed digits with the interval of its integer part, a point, at most five unknown digits and `pt` ([§ 103](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L2199)).
A scan reads it back as an interval ([§§ 448](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L8829)–[458](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L9009)): each unit is monotone in the integer part and the fraction, so the value lies between the values at their ends.
So `\the` keeps a bound through `\pgf@sys@tonumber` and pgfmath's `round`, `floor` and `int`.
An interval that holds both signs still prints as unknown digits.

### Length bounds on typed text

Typed digits and number text may carry an upper bound on the number of tokens they hold (`LEN_MARK`, `text_len` in [`src/env.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/env.rs)).
The bound is at most 20, a sign and 19 digits.

- `\the` or `\number` of an unknown count gives at most 11 tokens: a sign and 10 digits ([tex.web § 65](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L1613)).
- `\the` of an unknown dimension gives at most 14: a sign, 5 digits, a point, 5 places and `pt` ([§ 103](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L2199)).
- A count or dimension in an interval prints with the digits of its larger bound.
- Reading one token of bounded text leaves a rest with a bound one smaller. A rest with bound 0 is no text at all.
- A join of texts keeps the largest bound of their differing parts. A run of unknown tokens that collapses into one token sums their bounds.

A loop head counts the passes of a loop against `limits.widen_after` ([Configuration](Configuration#what-a-run-may-use)).
A pass whose pending input holds strictly fewer tokens than the pass before is not counted (`input_bound`, `loop_bounds`).
Such a loop ends by construction, so each of its passes stays exact. A loop that consumes `\the\count0` digit by digit is followed to its end, not widened.

## `CatcodeTable::unknown`: unknown category as a lattice element

`CatcodeTable` ([`src/tex.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/tex.rs)) ([tex.web § 232](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L4811)) tracks, per character, a
set `unknown` of characters whose `\catcode` was set to a number satex
cannot interpret. This is not a missing table entry: `CatcodeTable::get`
still returns a best-guess category (the last one known), but any caller
that matters must first check `CatcodeTable::is_unknown`. The *unknown*
state is itself a real element of the per-character lattice, one level
above every concrete category code, not an absence of information that
falls back to a default silently.

## Files written in the run

`satex` writes nothing to disk. It keeps what the run writes per file name, in `Outputs` ([`src/machine.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/machine.rs)), as part of each path's state.
A name without an extension gets `.tex` ([tex.web § 1374](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L24914)).
A file the run wrote is in one of two states:

- known text: `\immediate\openout` made it empty, and each `\immediate\write` to an open stream appended a line,
- unknown text: a deferred `\write`, which acts only at shipout, or a stream number `satex` could not pin down. Such a `\write` makes every written file unknown.

A file the run did not write is what is on disk, or absent.

Reads look at the run's own files first ([tex.web § 537](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L10365)):

- `\input` and `\openin` of a file with known text read that text, even where no such file is on disk.
- A file with unknown text that is not on disk is there to read, with text that is not known.
- Otherwise the file on disk is read. This includes a file with unknown text that is also on disk, which is a [known limit](#known-limits). A file that is neither written nor on disk is absent, and `\openin` leaves the stream closed ([§ 1275](#known-limits)).

The `\openin` streams are a table of the lines each stream has left.
A join of paths that differ in any stream leaves no stream known. `\ifeof` is then undecided.
A stream number `satex` cannot pin down does the same.

## Names from unknown text

[`Wild`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/machine.rs)
holds the families of names that definitions of partly unknown names may
have made ([tex.web § 372](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L7745)): each is the known text before and after the
unknown part, as in `\csname MT@inh@\x\endcsname`. An undefined name of
such a family may be defined. The join is the union. Past 64 patterns,
they widen to what they share.

A name of wholly unknown text adds no family. The standing assumption
(`unknown-name-defines-fresh`, an imprecision) is that it is a new name:
it redefines no name that has a meaning and no name spelled out elsewhere.
A lookup of unknown text stays unknown, so it may still find it. Without
this, one such definition would make every name maybe-defined, and every
`\ifcsname` after it undecided.

## Call summaries: what a call did twice

[`src/machine/calls.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/machine/calls.rs)
summarizes a macro call by its effect. Parsers that walk unknown text a
token at a time (pgfmath, l3fp, calc, listings) then converge after two
runs instead of running on.

- **Key.** The macro's definition, the call site, and the control sequences
  in its arguments. A call writes the names it is handed, so other names
  make another key.
- **What a run records.** The first `limits.call_summary_after` runs (2 by default) of a key over unknown operands run the real body
  ([tex.web § 389](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L7938): the body replaces the call). Each run is recorded from
  the call until the main loop reads below the call's input level. A run
  records the names it wrote, with what they hold then, and the names it
  read. A run over known operands is not recorded: what it wrote holds for
  those values alone, so joining it into a call on unknown ones would be
  silently wrong.
- **When it applies.** On the third call and later, and only if an operand
  is unknown: an argument holds unknown text, or a name the runs read holds
  unknown text or several meanings. A number not known exactly does not
  count: it is computed with, not walked a token at a time. The
  recorded writes are then joined and widened (`widen_slot`, so a digit
  text stays digits) and assigned, and the body is not run. Each key
  reports one `call-summarized` imprecision.
- **What it never joins.** A run that read on in the file, changed a
  category code, left a group, a conditional or the mode changed, added to
  the list being built, or met a cut. Runs that wrote different names. An accumulator: a text the second
  run left as the first run's text and more. An update: a name a run read
  and the runs left different, such as a stack push. What it wrote came
  from what it read, which the next call holds anew. Anything but data:
  a summary joins numbers, and texts of letters, others and spaces under
  one parameter text. Runs that left a name with other code, braces, a
  parameter, another meaning or a meaning set are never joined. Such a
  verdict holds for the key from then on. Each of these runs the real body
  again.

### The line guard: termination

One check bounds every run, whatever the precision. Every step passes
`Machine::check_budget`: each token read, each pass of the path driver, and
each replay or re-entry of a command. The guard there charges the steps to
the source line read now (the innermost file's `file:line`), summed over
all paths. A file the line reads is its own lines. A package replayed from
its cache is charged to no line: its steps were its own lines' when it was
read, and the line that loads it gets only its own work. At the end of a
cacheable load, read or replayed, the counters of the lines it read are
dropped, so both go on with the same counters.

- **Abstracts:** the work spent on one line, a counter per line.
- **First overrun** of `limits.line_steps`: the expansion above the line
  unwinds to the main loop, what it wrote is widened to typed unknowns,
  an unknown token stands for what it would have made, and the rest of the
  line is read. Reported as `line-budget-cut`.
- **Second overrun:** the rest of the line is skipped, with the warning
  `stalled-line-skipped`.
- **Third overrun,** or no line left to skip: the run stops with
  `budget-exhausted`.
- **Paths:** a path parked on the line before a cut takes the same cut
  when it runs again, so forks cannot outrun the guard.
- **Bound:** the run spends at most `3 × limits.line_steps` steps per line it
  reads (a source line, paired with the data line `\read` took last), plus a
  constant.
- **Budget-free:** the loop summaries and the conditional fold make loops
  converge, and the guard cuts what they miss. Its bound, like every other
  cut's, is a fixed count, never a share of `limits.steps`. So a run cuts
  the same places whatever its total budget, until it reaches that budget.
- **Loop entries:** a loop summary unwinds to where its loop was entered.
  A later meet of the same head counts as a pass only while every list
  below the entry's own is unread since. Once one was read on, the entry's
  call ended ([tex.web § 325](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L7015)), and the meet is a new entry, however deep. So a summary never
  closes groups or drops frames that its caller owns.
- **Endless loops:** a call that meets a state it was in before
  (`limits.cycle_after`) ends its path only while another path waits to
  read on. A path no other path covers is summarized and goes on, since
  the repeat may come from satex's own imprecision.
- **Unwinding:** every cut (a loop summary, a recursion past
  `limits.expansion_depth`, the line guard) drops token lists and closes
  what their text opened, by identity. Each list records the conditionals,
  the innermost group and the assignment count at its push. The cut closes
  the conditionals and groups opened since the lowest dropped list, as
  `\endgroup` would ([tex.web §§ 281](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L6035)–[283](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L6070)). A value a group puts back that
  was written since that push is widened, since the dropped text would
  have run on. Closers are never counted in the dropped text: a macro such
  as calc's `\calc@close` closes groups too.
- **Covered passes:** a pass of a summarized loop ends only while a waiting
  path went on past that loop's summary. Other waiting arms are not its
  continuation: without such a path the pass is cut and goes on.

## The join, in general

Across every domain the join is the same shape: **hull plus (bounded)
union**. Take the smallest concrete summary that covers both sides
(interval hull for `Num`/`Dim`/`Color` components, set union for modes and
meaning-sets), and once that summary would need more members than the
configured cap, drop to the coarser hull-only element instead of failing.

Execution around an undecided split (conditional, meaning-set dispatch, an
undecided mode test) runs **each arm to completion separately**, then joins
the resulting environments, globals included, rather than letting one arm's
effects overwrite the other's. With `arm_joins` on (the default), inside a body read while expanding (an `\edef`) the
arms part at the split and join where they meet again in the body. What the body holds so far is part of the path: the join keeps the
text both arms share and puts one token for either arm's text between. So
twelve independent tests in one body are twelve splits, not 4096 runs. A name only one arm defined exists
afterwards holding that definition. A register the arms leave with
different values holds their `Num`/`Value` join. Category codes or a
pending `\afterassignment`/`\aftergroup` token that disagree keep the paths
apart, up to two per place. After the `\fi` of a conditional both arms closed
they join at once ([joins at `\fi`](#joins-at-fi)). Past two per place they are joined too: category codes by
their join, the token lists into one token that stands for either list. Paths
are compared by what is left to read, not by which copy of a token list holds
it. The path furthest behind runs next, so paths that can meet do. Paths that
never meet run on to the end separately. No path is dropped. Where they stand
in different groups, their save stacks are joined entry by entry.

A loop whose exit is undecided (`\loop … \ifhbox … \repeat`, or pgfmath's
tail-recursive dispatch) is analyzed to a **fixpoint**: each round's result
is joined into the loop head's running state. If a round grew that state,
the head reruns from the join. Once a round changes nothing, that is the
fixpoint. Only after `limits.widen_after` rounds below of continued growth
does the state get widened: a meaning set to unknown meaning, a growing
interval bound to the end of TeX's 32-bit range, other values to `Unknown`.
So every loop terminates within `widen_after + 2` rounds, win or lose, and a
run that still grows past that says so (`undecided-loop`).
A head whose widened state still grows (its input keeps changing, as a
recursion's frames do) is summarized after `2 × widen_after` widened rounds.
A path's place leaves out the lines it read with `\read`: the exits of a loop that reads one line per pass stand at one place, and their streams join as ranges.

### Joins at `\fi`

Both arms of an undecided conditional that reach the same `\fi` go on as one path from there.
The conditional is known by its `\if…` and the depth of the conditional stack below it ([tex.web §§ 489](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L9601), [498](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L9711)), so nested and recursive conditionals stay apart.
Each path keeps the conditional it closed last (`fi_mark` in [`src/machine.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/machine.rs)).
Two paths at one place that closed the same one join whatever else tells them apart ([`src/machine/explore.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/machine/explore.rs), `partner`).
The join is the general one above. Data becomes an interval or a set. A macro the arms define differently is a meaning set of both bodies. A macro only one arm defines may be undefined or that body, so a later `\ifdefined` takes both arms. Registers, category codes, `\let` and `\chardef` join the same way.
Every arm's definitions stay recorded as facts, so where a name may have been defined is over-approximated. No side is picked.
Paths that wait to run different code (an `\aftergroup` or `\afterassignment` token) do not join here.
Arms that leave the conditional another way (`\expandafter\@firstoftwo`, a scan that reads on past the `\fi`) do not meet at it. They join at the next common place by the rule above.
There is no widening here: the join is the one a loop head widens.

A test whose interval straddles its constant splits it (`\ifdim` of `[0pt,345pt]` against `100pt`).
Each arm holds its part of the interval, `[100.00002pt,345pt]` and `[0pt,100pt]`, so a test inside an arm that its part decides is decided.
At the `\fi` the parts join back into their hull.

### Joins at the limits

One command may make at most `limits.choices` undecided choices on one path ([Configuration](Configuration#what-a-run-may-use)).
Past that, an undecidable conditional is widened, not cut ([`src/exec.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/exec.rs), `widen_conditional`).
Both arms are skipped to the matching `\fi`, and the conditional gives one token of unknown text in their place.
The run records an `undecided-condition` assumption: the arms are assumed to touch nothing.
No path is dropped.

Paths join at one point of the run: between two commands, where the running path stands as a waiting one does (the same input, conditionals, groups and scans).
That covers a group's end, a macro's end, a `\fi` and a loop head alike ([`src/machine/explore.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/machine/explore.rs), `absorb`).
A path's place leaves out the lines it read with `\read`: the exits of a loop that reads one line per pass stand at one place, and their streams join as ranges.
Past `limits.alike` waiting paths, those at one source position in the same conditionals, groups and scans are joined although their pending input differs, but only where it differs in text, not in code.
Paths whose pending input differs in code wait on until `limits.paths`: joined to unknown text, code would lose what it does.
Past `limits.paths`, the two furthest back at one source position are cut to the input levels they share and joined.
Measured on a TikZ node with an unknown height, `limits.alike = 8` takes 2.8k forks where 16 took 5.1k, and 4 takes 114.

#### Code a join makes unknown

A join that turns pending code into unknown text keeps what that code may do (`may_run` in [`src/machine.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/machine.rs)).
It reads the dropped code and every macro it may call.
A name that has no meaning yet and stands after `\def`, `\let` or a register definition there may now be defined: it gets the unknown meaning and a definition fact.
This is the rule for a `\def` of an unknown name: it defines a fresh name, and names with a meaning keep theirs.
A count or dimension register the code names may now hold any value of its kind, as a typed interval.
A forced join at `limits.paths` does this at the join, where the code's order is known.
Typed unknown text that names code does it when it runs, from the names it may hold.
Both report `joined-code-may-define`.
Untyped unknown text names nothing, so it cannot do this.

Where two paths differ in their pending input, the join keeps the text they share and puts one token for either text between.
That token stands for a whole text, so the differing parts must be whole too (`whole` in [`src/machine/explore.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/machine/explore.rs)).
Their braces must balance, and every conditional opened in them must close in them.
A `\fi`, `\else` or `\or` without its `\if` in the part would leave the conditional around it without an end.
Where a part is not whole, the shared end is shortened until it is, or the texts are not folded.
Only data differs this way between two paths: digits, characters, typed text.
The token is one of the texts, exact until widened: a `0` and typed digits stay apart.
Data that pending code still reads is part of that code: `\b0.5\end` and `\b1\end` differ in code.
Only data read first, a value being scanned, joins.
Runs of digits become typed digits only between two passes of one loop.
Two paths whose pending code differs stand at different program points, as do paths that wait to run different `\aftergroup` or `\afterassignment` tokens.
They never join: each runs on until both reach a common point, the end of the group, macro or conditional they are in.
Code differs only between two passes of one loop, where the widening makes it unknown text that may hold either.

A path that raises an error with an exact cause, a name certainly
undefined expanded or run, is dropped even when its state is
approximated (assumption: the document compiles). Its texts never reach
a join, so a walk past an undefined delimiter adds nothing.

## `limits`: where the domains stop being precise

From [`src/config.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/config.rs), the knobs that turn precision
into a bounded search. The other limits (time, memory, threads, record sizes)
never make a result coarser: they only stop a run, and the run says so. Full description of each on [Configuration](Configuration#what-a-run-may-use).

| setting | default | what it bounds |
| --- | --- | --- |
| [`limits.choices`](Configuration#what-a-run-may-use) | 8 | Undecided choices one command may make on one path.  Past them a conditional follows only its first arm, reported as an `unsupported` gap. |
| [`limits.site_forks`](Configuration#what-a-run-may-use) | 1024 | Undecided choices one site (a conditional, a name of several meanings) may make, over all paths and replays, while the run reads one source line. Past them the expansion above that line is cut as `line_steps` cuts it (`site-forks-cut`): what it assigned is unknown, and the line reads on after an unknown token. Many paths through one place say little the join of a few did not. 0 turns it off. |
| [`limits.paths`](Configuration#what-a-run-may-use) | 16 | Paths one drive keeps waiting before the two furthest back at one source position are cut to the input levels they share and joined. |
| [`limits.meaning_splits`](Configuration#what-a-run-may-use) | 4 | Choices one command may have made on one path for a name that a join left with one of several meanings to still split the run into a path per meaning; past them, and with 0 everywhere, it is read as a name of unknown meaning. |
| [`limits.widen_after`](Configuration#what-a-run-may-use) | 3 | How many rounds a loop head analyzes its paths again from a grown joined state before what still grows is widened (SEMANTICS [§ 14](Configuration#what-a-run-may-use)). |
| [`limits.recursion_depth`](Configuration#what-a-run-may-use) | 100 | How many levels deep a recursion whose continuation is undecided is followed before it is assumed to end within them. |
| [`limits.loop_heads`](Configuration#what-a-run-may-use) | 512 | How many loop heads and loop entries one drive keeps with their joined states; past them the oldest is forgotten. Its count stays in the load's loop summaries, so it is summarized when met again. |
| [`limits.call_summary_after`](Configuration#what-a-run-may-use) | 2 | How many runs of one call, at one site with the same names handed to it, are recorded before a later call with unknown operands applies their join instead of running; 0 never summarizes a call. |
| [`limits.meaning_set`](Configuration#what-a-run-may-use) | 8 | How many meanings a join leaves a name with as a set (one of them) before it has an unknown meaning. |
| [`limits.value_set`](Configuration#what-a-run-may-use) | 8 | How many values a join leaves a count or dimension register with as a set before only their interval is kept. |

Each is a place `satex` chooses breadth of exploration over runtime, never a
place it silently drops soundness. Past the limit the value goes up the
lattice, coarser, never wrong, and a diagnostic records that the limit was
hit.

## Registers behind an unpinned index

A write through a register index `satex` cannot pin to one number
(`\count#1=...` for an unknown `#1`) cannot be attributed to a single slot,
so it must be sound for *every* register that index could have named
(`Machine::weaken_registers`, [`src/exec.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/exec.rs)):

- When the index's `Num` interval is still narrow enough
  (`index_targets`, capped by `limits.value_set`), each register in it
  is joined with the written value.
- Otherwise every register of that kind is weakened: registers already seen
  are joined with the value, and any register not yet seen can no longer be
  assumed to hold its [tex.web § 232](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L4811) zero default. A further unseen-register
  read (`unseen_register`) then answers with the whole range of the kind
  (`register_top`) instead of zero.

This is the same over-approximation principle as everywhere else: an
indeterminate write must widen *everything it could have reached*, never
just the registers `satex` happens to already have slots for.

## Code tables behind an unknown character

A `\catcode`, `\lccode`, `\uccode`, `\sfcode` or `\mathcode` write
through a character number `satex` cannot pin down ([tex.web § 1232](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L23040)) leaves
each entry it may name at {old, new}.

- Categories: `CatcodeTable` keeps the hull of the codes (`reach`) and a bit
  set of the categories written (`reach_cats`). A character in reach has its
  own category or one of those. The join takes the hull and the union, the
  widening moves a bound that grew to the end of the codes.
- Codes: the written values are joined in one slot per table
  (`reach_family`, `family_value`). An entry reads its own code joined with
  that slot.
- A character that may be a letter or an other character is read as a
  one-of of the two tokens, not as unknown text. Other category sets still
  read as unknown text.

**Named assumption `unknown-char-catcode-assumed`.** A write through an
*unknown* character never changes the structural characters `\ { } % # $
& ^ _`, space and end-of-line. Unknown means no interval, or one that runs
past the last character code (a widened loop counter). A write through a
bounded interval that covers them does widen them. Writes of the two kinds
are kept apart (`reach_structural`, the `anyu` code slot), so a join keeps
the assumption only where every write had it. It is reported once per site.

## Known limits

The invariant above is what `satex` aims at, and these are the places where
it does not yet reach it. They are stated here rather than left implicit.

### Text `satex` could not follow, once it is executed

A character whose category `satex` cannot pin down runs a meaning of its own
only when its category is 13, and [tex.web §§ 232](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L4811), [344](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L7286) say that meaning is
the one bound for that character. None of the other categories runs
anything. So executing such a character joins in what that one binding could
assign, and a character nothing binds leaves the run exactly as precise as it
was. This bound is what keeps the widening free: on the two documents it was
measured against it costs nothing at all.

What that does not cover:

- A marker that stands for a *name* `satex` could not build, which
  `\csname` over unknown text leaves, has no such bound. It could be any
  control sequence, so nothing is widened when one is executed, and a
  register or a meaning that name would have assigned keeps the value it
  had.
- Even where the bound applies, only what the arm merge covers is joined
  in, which is the bindings. A group the meaning would have opened or
  closed, a mode it would have changed, a file or a stream it would have
  opened, and a `\write` it would have made are not, so a lint that reads
  those facts can miss one.

### Expansion that is widened away

A recursion that is stopped at its bound has its remaining rounds widened:
the call reads as text that is not known ([tex.web § 366](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L7636)), and what those
rounds would have assigned is widened with it. Category codes, groups,
files, streams and `\write` are not widened there. The sound answer for
category codes is the top of that table, which leaves nothing tokenizable,
so the run would stop rather than go on imprecisely.

### Files the run wrote with unknown text

A file the run wrote with unknown text, which also exists on disk, is read as it is on disk.
The run may have changed it, so that read can be wrong.

### Loops that span call sites

The per-site bound counts re-entries of one site, so it cannot see a loop
whose rounds land on different sites.  Such a loop meets its loop head
again when it is decided by an undecided conditional, since paths meet by
what is left to read, and is then analyzed to a fixpoint.  Only a loop that
never meets a split is left to the stall limit.

## Soundness invariant

Every domain above answers one of two ways: **exact**, or **an
over-approximation that provably contains every real outcome**.
"Unsupported" is never a third answer left implicit. A construct `satex`
cannot follow moves its value up the lattice (`Unknown`, the full interval,
the full mode set, an unknown meaning) and records why, rather than being
silently skipped or guessed at. This is what makes every finding `satex`
reports trustworthy: it may miss nothing it should have flagged, though it
may (and, at a stated limit, does) flag something a real run would not.

Sources cited above: [tex.web § 211](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L4220) (modes), [§ 232](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L4811) (category codes), [§ 433](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L8617)
(register codes), [§§ 448](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L8829)–[460](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L9056) (`scan_dimen`), [§ 649](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L12910) (`hpack`), [§ 668](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L13152)
(`vpack`), [§ 1238](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L23134) (`\advance` wraps without an overflow check).
