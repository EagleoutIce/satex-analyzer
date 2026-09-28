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

## `Value`: what a control sequence or register holds

[`src/value.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/value.rs) defines:

| Variant | Abstracts | Top element |
| --- | --- | --- |
| `Int(i64)` | a `\count`, `\numexpr` result, or other exact integer | n/a (always exact) |
| `Dimen(Scaled)` | a `\dimen`, exact, in scaled points | n/a |
| `Glue(Glue)` | a `\skip`: width plus stretch/shrink with order | n/a |
| `MuGlue(Glue)` | a `\muskip`, same shape as `Glue`, math units | n/a |
| `Toks(Rc<[Token]>)` | a `\toks` register's exact token list | n/a |
| `Range { dimen, num }` | a count or dimension known only within an interval, or as one of a few values | `Num::full(dimen)`: the whole `[-MAX_INT, MAX_INT]` or `[-MAX_DIMEN, MAX_DIMEN]` |
| `Color(Colors)` | an xcolor colour (`\color@⟨name⟩`) | hull spanning the model's full range, `set: None` |
| `Unknown` | anything else: unresolved typesetting output, text with unknown digits, a read through an unpinned index | the domain's top. `Env::value` also reads a fully-covering `Range` as `Unknown` |

A register read always goes through `Env::value` ([`src/env.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/env.rs)).
A `Range` is only visible to code that specifically inspects the interval (a
comparison in `\ifnum`/`\ifdim`), never to a plain `\the`.

## `Num`: interval plus bounded may-set

```rust
pub struct Num {
    pub lo: i64,
    pub hi: i64,
    pub set: Option<Vec<i64>>,  // sorted, deduped, within [lo, hi], holds both ends
}
```

Two abstractions are carried together, not one:

- **The interval** `[lo, hi]` decides monotone comparisons (`\ifnum`,
  `\ifdim`) cheaply even when the exact members are unknown or too many to
  list.
- **The may-set** `set` (bounded by [`MAY_VALUES`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/value.rs),
  or `limits.value_set` below where a join is in play) keeps the *exact*
  values when there are few, so
  `\ifodd`, equality tests and further arithmetic stay exact through a small
  join instead of falling back to interval reasoning immediately.

When a join's members would exceed the cap, `set` becomes `None` and the
value falls back to interval-only (`Num::join`, `Num::of_set_capped`):
nothing is lost, the domain just widens to a coarser member of the same
lattice.

**Worked example** (`Num::join`): a run has `\count0=1` on one path and
`\count0=2` on the other (say, from two arms of an undecided `\ifodd`).
Joining gives `Num { lo: 1, hi: 2, set: Some([1, 2]) }`, a two-element
may-set, not the interval `[1,2]` alone, so `\ifodd\count0` downstream is
still undecided (both members disagree) but `\ifnum\count0=3` is decided
false (neither member is 3), which a bare interval could not tell you.

Arithmetic (`Num::lift`) mirrors this: exact per point when every input is a
point, exact over every combination when the sets are small (≤ 64 combined
points), and otherwise the corners of the interval boxes (cut at zero, since
TeX's arithmetic is monotone on each side of zero) bound the ideal result.
An overflow of the exact range is TeX's arithmetic error, kept in-range by
clipping. A 32-bit wraparound (`\advance` past the register's own width,
[tex.web § 1238](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)) instead widens to the *entire* 32-bit range
(`WORD_MIN..=WORD_MAX`), not `\maxdimen`, since `\advance` can pass that
too.

## Colours: `Color` and `Colors`

xcolor colours are recorded the same way as `Num`, one abstraction level up:

- `Color { model, components: Rc<[Num]> }` is one colour: an xcolor model
  (`Model`, xcolor table 3) and one `Num` per
  component, in scaled points (the units xcolor's own arithmetic uses).
- `Colors { hull: Color, set: Option<Rc<[Rc<[Num]>]>> }` is what a binding
  actually holds: a component-wise interval hull (each component's `Num`
  already an interval) plus a bounded may-set of whole colours, the same
  "interval plus small may-set" shape as `Num`, lifted from one number to a
  tuple of them.

`Colors::join` takes the hull component-wise (`Num::join` per component) and
unions the may-sets, capped the same way `value_set` caps `Num`. Two
colours of different models have no common bound. Converting one into the
other's model would invent a value xcolor never actually stored, so joining
across models gives `None`, and the caller widens to `Value::Unknown`
instead of fabricating a converted spec.

This domain lives entirely in `src/value.rs` already. The colour lints and
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

**Worked example**: `\ifx\a\b \let\next\foo \else \let\next\bar \fi`. After
the join, `\next`'s binding has `may = Some([foo, bar])`. `\ifx\next\relax`
is undecided (the members disagree). Executing `\next` splits into a path
where it is `\foo` and one where it is `\bar`, which then join again like
the arms of an undecided conditional.

## Modes and box dimensions

`Modes` ([`src/mode.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/mode.rs)) is a bitset over TeX's six
modes ([tex.web § 211](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web): vertical, internal vertical, horizontal, restricted
horizontal, display math, math). The run keeps the *set* of modes it may be
in. `Modes::test` decides an `\ifvmode`/`\ifhmode`/`\ifmmode`/`\ifinner`
only when every mode in the set answers alike, and disagreeing paths join
by `union`. `INNER` picks out the three "-mode" (inner) variants [tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)
distinguishes from their outer counterparts.

`Dim { lo, hi }` is the analogous domain for a single length: box
dimensions and node sizes, added up the way `hpack` ([tex.web § 649](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)) and
`vpack` ([§ 668](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)) add a list's items. `Dim::ANY` (`±MAX_DIMEN`) is the top of
the lattice. What an item `satex` cannot follow widens the running size to,
rather than leaving the item out, which would under-approximate. `Natural`
bundles the running `along`/`height`/`depth`/`wide` dimensions plus the
list's tail, mirroring the fields `hpack`/`vpack` compute.

## `CatcodeTable::unknown`: unknown category as a lattice element

`CatcodeTable` ([`src/tex.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/tex.rs)) ([tex.web § 232](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)) tracks, per character, a
set `unknown` of characters whose `\catcode` was set to a number satex
cannot interpret. This is not a missing table entry: `CatcodeTable::get`
still returns a best-guess category (the last one known), but any caller
that matters must first check `CatcodeTable::is_unknown`. The *unknown*
state is itself a real element of the per-character lattice, one level
above every concrete category code, not an absence of information that
falls back to a default silently.

## The join, in general

Across every domain the join is the same shape: **hull plus (bounded)
union**. Take the smallest concrete summary that covers both sides
(interval hull for `Num`/`Dim`/`Color` components, set union for modes and
meaning-sets), and once that summary would need more members than the
configured cap, drop to the coarser hull-only element instead of failing.

Execution around an undecided split (conditional, meaning-set dispatch, an
undecided mode test) runs **each arm to completion separately**, then joins
the resulting environments, globals included, rather than letting one arm's
effects overwrite the other's. A name only one arm defined exists
afterwards holding that definition. A register the arms leave with
different values holds their `Num`/`Value` join. Category codes or a
pending `\afterassignment`/`\aftergroup` token that disagree keep the paths
running apart until they agree again or the join's token budget
(`limits.join_tokens` below) runs out.

A loop whose exit is undecided (`\loop … \ifhbox … \repeat`, or pgfmath's
tail-recursive dispatch) is analyzed to a **fixpoint**: each round's result
is joined into the loop head's running state. If a round grew that state,
the head reruns from the join. Once a round changes nothing, that is the
fixpoint. Only after `limits.widen_after` rounds below of continued growth
does the state get widened: a meaning set to unknown meaning, a growing
interval bound to the end of TeX's 32-bit range, other values to `Unknown`.
So every loop terminates within `widen_after + 2` rounds, win or lose, and a
run that still grows past that says so (`undecided-loop`).

## `limits`: where the domains stop being precise

From [`src/config.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/config.rs), the knobs that turn precision
into a bounded search. Full description of each on [Configuration](Configuration#what-a-run-may-use).

| setting | default | what it bounds |
| --- | --- | --- |
| [`limits.steps`](Configuration#what-a-run-may-use) | 1000000000 | Tokens the document may digest, and the kernel on top of that. |
| [`limits.format_steps`](Configuration#what-a-run-may-use) | 9000000000 |  |
| [`limits.file_depth`](Configuration#what-a-run-may-use) | 64 | How deep `\input` may nest. |
| [`limits.expansion_depth`](Configuration#what-a-run-may-use) | 32 | How often a macro may re-enter itself, and how often one call site may expand without the source moving on — TeX loops are tail recursive. |
| [`limits.site_expansions`](Configuration#what-a-run-may-use) | 16384 |  |
| [`limits.stall_tokens`](Configuration#what-a-run-may-use) | 50000000 | Tokens a run may digest without reading from a file before the expansion it is in is given up as a loop that does not end. |
| [`limits.repeat_failures`](Configuration#what-a-run-may-use) | 8 | How often one construct satex cannot follow may be met at one place before the expansion it is in is given up the same way.  A path that fails at a site once fails there again on every round, so counting the failure into the millions says nothing the first one did not. |
| [`limits.seconds`](Configuration#what-a-run-may-use) | 300 | Wall-clock seconds a run may take before it stops and says so; 0 for no limit.  A safety net: no document should come near it. |
| [`limits.branch_depth`](Configuration#what-a-run-may-use) | 8 | How deep undecided conditions may nest before only one arm is read. |
| [`limits.join_tokens`](Configuration#what-a-run-may-use) | 65536 | Tokens the arms of one undecided conditional may read, together, before they have to meet again in the input. |
| [`limits.meaning_splits`](Configuration#what-a-run-may-use) | 4 | How deep in nested paths a name that a join left with one of several meanings still splits the run into a path per meaning when it is executed or compared by `\ifx`; deeper, and with 0 everywhere, it is read as a name of unknown meaning. |
| [`limits.widen_after`](Configuration#what-a-run-may-use) | 3 | How many rounds a loop head analyzes its paths again from a grown joined state before what still grows is widened (SEMANTICS [§ 14](Configuration#what-a-run-may-use)). |
| [`limits.meaning_set`](Configuration#what-a-run-may-use) | 8 | How many meanings a join leaves a name with as a set (one of them) before it has an unknown meaning. |
| [`limits.value_set`](Configuration#what-a-run-may-use) | 8 | How many values a join leaves a count or dimension register with as a set before only their interval is kept. |
| [`limits.expansion_tokens`](Configuration#what-a-run-may-use) | 1048576 | Tokens one expansion, argument or conditional may span.  TeX itself has no such limit, so these only stop a single pathological read from taking all of `memory`; what makes a run terminate is `steps`, `held_tokens` and the recursion widening.  Only tokens an expansion produces count: source read straight from a file is bounded by the file, so a document body is never cut short by this. |
| [`limits.conditional_tokens`](Configuration#what-a-run-may-use) | 1048576 |  |
| [`limits.held_tokens`](Configuration#what-a-run-may-use) | 4000000 | Tokens the input stack may hold at once. |
| [`limits.expansion_stack`](Configuration#what-a-run-may-use) | 4000 | How many token lists may be open at once: one per macro being expanded, argument being read or file being input. |
| [`limits.vertices`](Configuration#what-a-run-may-use) | 400000 | Ceilings on what the run records. |
| [`limits.facts`](Configuration#what-a-run-may-use) | 400000 |  |
| [`limits.trace`](Configuration#what-a-run-may-use) | 200000 |  |
| [`limits.save_stack`](Configuration#what-a-run-may-use) | 2000000 | Entries on the save stack: one per assignment a group may have to undo ([tex.web § 268](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)). |
| [`limits.memory`](Configuration#what-a-run-may-use) | 4.0 GiB | What this run may hold at once, written the way `ulimit` takes it (`512M`, `4GiB`); zero lifts the limit.  Only the `satex` binary counts allocations, so a library user gets no limit. |
| [`limits.threads`](Configuration#what-a-run-may-use) | 0 | Threads any parallel work may use; 0 lets satex choose.  The interpreter runs on one thread because TeX's state is one machine, so this bounds the file walks around it. |
| [`limits.cache_size`](Configuration#what-a-run-may-use) | 1.0 GiB | How much the package caches in the cache directory may take together; the least recently used go first.  Zero lifts the bound. |

Each is a place `satex` chooses breadth of exploration over runtime, never a
place it silently drops soundness. Past the limit the value goes up the
lattice, coarser, never wrong, and a diagnostic records that the limit was
hit.

## Registers behind an unpinned index

A write through a register index `satex` cannot pin to one number
(`\count#1=...` for an unknown `#1`) cannot be attributed to a single slot,
so it must be sound for *every* register that index could have named
(`Machine::weaken_registers`, [`src/exec.rs`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/exec.rs)):

- When the index's `Num` still lists few enough candidates
  (`index_targets`, capped by `limits.value_set`), each candidate register
  is joined with the written value.
- Otherwise every register of that kind is weakened: registers already seen
  are joined with the value, and any register not yet seen can no longer be
  assumed to hold its [tex.web § 232](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) zero default. A further unseen-register
  read (`unseen_register`) then answers with the whole range of the kind
  (`register_top`) instead of zero.

This is the same over-approximation principle as everywhere else: an
indeterminate write must widen *everything it could have reached*, never
just the registers `satex` happens to already have slots for.

## Known limits

The invariant above is what `satex` aims at, and these are the places where
it does not yet reach it. They are stated here rather than left implicit.

### Text `satex` could not follow, once it is executed

A character whose category `satex` cannot pin down runs a meaning of its own
only when its category is 13, and [tex.web §§ 232 and 344](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) say that meaning is
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
the call reads as text that is not known ([tex.web § 366](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)), and what those
rounds would have assigned is widened with it. Category codes, groups,
files, streams and `\write` are not widened there. The sound answer for
category codes is the top of that table, which leaves nothing tokenizable,
so the run would stop rather than go on imprecisely.

### Loops that span call sites

The per-site bound counts re-entries of one site, so it cannot see a loop
whose rounds land on different sites. Only the stall limit stops such a
loop, and it does so after far more work. Catching it needs a loop head for
expansion, of the kind the conditional paths already have.

## Soundness invariant

Every domain above answers one of two ways: **exact**, or **an
over-approximation that provably contains every real outcome**.
"Unsupported" is never a third answer left implicit. A construct `satex`
cannot follow moves its value up the lattice (`Unknown`, the full interval,
the full mode set, an unknown meaning) and records why, rather than being
silently skipped or guessed at. This is what makes every finding `satex`
reports trustworthy: it may miss nothing it should have flagged, though it
may (and, at a stated limit, does) flag something a real run would not.

Sources cited above: [tex.web § 211](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) (modes), [§ 232](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) (category codes), [§ 433](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)
(register codes), [§§ 448–460](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) (`scan_dimen`), [§ 649](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) (`hpack`), [§ 668](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web)
(`vpack`), [§ 1238](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) (`\advance` wraps without an overflow check).
