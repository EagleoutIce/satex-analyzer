# Abstract semantics: Lean model to Rust

> Written with AI assistance from Claude Code (Claude Opus 5.5; build fixes with Claude Sonnet 5.5), directed and reviewed by the author.

Each row maps a definition of `lean/Satex/Abstract.lean` to the Rust code
that implements it. Status:

- **same**: the Rust computes what the model says.
- **finer**: the Rust keeps more than the model. Sound if the extra
  precision is justified, which the model does not check.
- **coarser**: the Rust keeps less. Sound, less precise.
- **deviates**: the Rust differs in a way that can lose a concrete
  outcome. Each is a potential soundness bug, listed again below.

Line numbers and statuses are of 26ef7ce (2026-11-04, branch `getting-sound`). The branch has moved on since (at 5082e4a, for example, `read_delimited` is at machine.rs:10947 and `charge` at 7813), so look names up rather than lines. Rows that changed after 26ef7ce say so. New findings against this code: REVIEW-2.md (R2-1 to R2-7).

## Domains

| Lean | Rust | Status |
| --- | --- | --- |
| `Limits` | `Limits` defaults, src/project/config.rs:240-280 | same |
| `Itv`, `Itv.γ`, `Itv.exact` | `Num`, `Num::exact`, src/engine/value.rs:177-186 (`exact` 183) | same at 26ef7ce. Later the Rust `Num` carries an `nz` bit (non-zero, value.rs:185-260 at 5082e4a), finer; the model has none |
| `Itv.join` | `Num::join`, value.rs:201 | same |
| `Itv.widen`, `wlo`, `whi` | `Num::widen`, value.rs:208 | same. Sound only for values in the 32-bit word (`Itv.le_widen` needs `inWord`) |
| `Itv.full` | `Num::full`, value.rs:193 | for counts the word in model and Rust (D10: Rust fixed 2026-11-04, model 2026-11-07). **deviates** for dimensions in model and Rust: `±maxdimen`, but `\advance` and box sums reach the word (REVIEW-2 R2-5; also `Dim::ANY`, mode.rs:232, `register_top`, exec.rs:5037) |
| `Itv.compare` | `Num::compare`, value.rs:322 | same |
| `Itv.narrow` | `Num::narrow`, value.rs:342 | same (Rust saturates at `i64` ends) |
| `GlueHull`, `.γ`, `.join` | `GlueHull`, value.rs:364-445 | same |
| `AVal.ints` | `Value::Int`, `Dimen`, `Num`, `OneOf` of numbers, value.rs:127-158 | same. Quantity (count vs dimen) is not modelled: Rust joins mixed quantities to `Unknown` (value.rs:726), coarser |
| `AVal.glues` | `Value::Glue`, `MuGlue`, `GlueNum` | same |
| `AVal.toks` | `Value::Toks`, `OneOf` of lists, `typed_toks_join`, env.rs:2847 | Rust finer past the bound: common head and tail around one typed token (`typed_join`, env.rs:2777); the model takes one hole |
| `AVal.box` | box registers' `Dim`s, src/engine/mode.rs | same in shape; box material, recorded lists (`Value::List`) and the forced breaks of `\vadjust` material (`Natural::forced`, kept in order) not modelled |
| `AVal.meanings`, `AMeaning` | `Binding.meaning` + `Binding.may`, env.rs:24-35 | same. `may = None` with `Meaning::Unknown` is `top`; `(m, {Undefined, m})` (`maybe_undefined`) is `meanings [m, undefined]` |
| `AMeaning.text` | a macro whose body holds typed tokens, `macro_hull`, env.rs:2195 | same |
| `AVal.join` | `Value::join`, value.rs:777; `env::join`, env.rs:2675 | numbers and glue same. Rust merges touching intervals (`Value::union`, value.rs:750), finer and sound. Meanings: Rust past the bound takes `typed_join` (finer). A join with the top is the top on either side (fixed 2026-11-04, D1) |
| `AVal.widen` | `widen_slot`, env.rs:2122 | numbers same (`p.widen(p ∪ q)`). Glue and box: the model takes the bounded hull (`AVal.join`), no widening; their chains end only by the round count. Rust finer for texts at `Reach::Joined` (`shaped`, `numbers`, `printed_hull`, env.rs:2234-2422); justified only because a loop head reruns from the join. `Reach::Havoc` keeps unmoved bounds and equal texts: see D4, D5 |
| `TClass`, `TClass.union`, `.le`, `.effHex`, `.norm` | `TextClass`, `union`, `contains`, `hex`, `new`, src/engine/tex.rs:289-370 | same |
| `kindOf`, `TClass.admits` | `TextClass::of`, `holds`, tex.rs:372-395 | `kindOf` same. `holds` tests with `meets`, which masks `HEX`, so the Rust ignores the hex restriction `TClass.admits` checks: coarser, sound. Rust's placeholder characters (`UNKNOWN_CHAR`, `UNKNOWN_DIGIT`, ...) are holes in the model |
| `Hole` | a typed-unknown control sequence (`Machine::unknown_text`, machine.rs:13185) with `LEN_MARK` bound (env.rs:2455) | same |
| `AItem.digits`, `printInt` | typed digits, `TYPED_DIGITS`, src/engine/scan.rs:16 | same. Rust also splits a read of digits by sign and length (domains.md "Joins at `\fi`"), not modelled |
| `AItem.oneOf` | one-of tokens, `split_one_of_groups`, machine.rs:11883 | Rust finer: narrows every copy on a path to the texts it took. The model forgets that copies are one choice (coarser, sound) |
| `AText.γ` | — | the model's meaning of an abstract token list: the concatenation of its items' texts |
| `AText.hull` | `run_hull`, env.rs:2286 | Rust finer (keeps one typed token when all texts are one, and printed integers) |
| `Ty`, `Ty.parent`, `Ty.join`, `Ty.le` | `Ty`, `parent`, `join`, `leq`, src/analysis/roles.rs:210-414 | same, case for case, at 26ef7ce. The Rust has since added `Ty::Overlay`, `KeyKind::Language`, `FileKind::Theme` and `RegKind::{Char, MathChar, UMathChar}` (roles.rs:216-418 at 5082e4a), which the model lacks. `γ` is a parameter (`Ty.Admissible`): types add no narrowing to the run |
| `ASave`, `SaveRel` | the journal `Entry`, `Mark` with `many`, env.rs:46-170 | same for sure entries. Levels: Rust's 0 is the model's 1 (`level_one`). `touched` has no Rust entry: where the model cannot decide a save, the Rust decides it by its frame-relative level and widens the saved slots at `unfold`. Model coarser: a touched location is unknown at its frame's end |
| `ACond`, `CondRel` | `CondFrame` with `many`, explore.rs `Key.conds` | same |
| `AState`, `AState.γ` | `Env` + `PathState`, env.rs:173; machine.rs | levels `Option Nat` (`none`: any). `pinned`: the group level is TeX's. The Rust counts a `many` mark as one level and keeps comparing levels relative to frames, finer than the model, which stops trusting levels once a `many` frame closed. Also: | catcode sets, files written (`Outputs`), streams (`Streams` of `At` hulls, machine.rs:1158-1261, joined by `join_streams`, pinned by `pin_stream`, machine.rs:4168), `Wild` name families, `Corr` arm pairing and marks are not modelled |
| `APaths`, `APaths.γ` | `Paths` (parked/arrived configs), explore.rs | same |

## Transfers

| Lean | Rust | Status |
| --- | --- | --- |
| `AState.assign` | `Env::set` / `set_tracked`, env.rs:839-880 | same where the level is known (tex.web §§ 277, 279). Unknown level or unpinned: `touched` (P1, P5) |
| `AState.weakAssign`, `weakAt` | `weaken_registers`, src/engine/exec.rs:3726 (`weaken_register_range` 3738) | same values up to `limits.value_set` locations. Levels: the model joins old and new (`lvJoin`) and leaves `touched`. Not checked against the Rust's levels |
| `AState.havocRegs` | `weaken_registers` past the bound, `register_top`, exec.rs:5037 (dimension top too small: R2-5) | same |
| `AState.beginGroup`, `restoreOne`, `closeFrom`, `endGroup` | `push_group`, `pop_group`, `unwind_into`, env.rs:957, 1045, 1108 | same for plain frames (§§ 282-283), `\aftergroup` tokens as one list (`pop_group`). `many` frames: `unfold`, env.rs:752 (one copy closes and its saved slots widen, or none is left and the closer acts below), `many_from`, env.rs:1968 |
| `branch` | `Machine::branch`, explore.rs:304 | same while choices remain. Past `limits.choices`/`RUNS`/hot site, `branch` gives `None`; each caller must then over-approximate. D8's callers do so since 2026-11-04 (`unforked_end`) |
| `ifNumTest`, `ifNumNarrow` | `\ifnum` over `member_nums`, exec.rs:5228; `Num::narrow` | same |
| `AState.joinable`, `AState.join` | `join_configs`, explore.rs:2755; `Env::join_paths`, env.rs:1580 | Rust finer: joins paths in different groups ("in the groups of the path that goes on") and keeps arms paired (`Corr`). The model keeps such paths apart. Open: not checked |
| `scanDelim`, `AText.cuts`, `dropAbs`, `stripBraces` | `read_delimited`, machine.rs:10343; `find_delimiter`, machine.rs:10224; `strip_braces`, machine.rs:12511; holes: `hole`, `hides_delimiter`, `tail_split`, machine.rs:10753-10890 | **deviates**: a split-off rest of unknown text is taken to hold no delimiter (D7); a scan refused a fork assumes the argument does not end in unknown text (D8); a scan reading past a one-of token into a `\fi` ends there (`joined_at`, machine.rs:10383), an assumption. R2-1 and R2-6 fixed (REVIEW-2 "Status"). The model lists every cut of the input and keeps those whose right side may begin with the delimiter. The Rust prunes cuts further by brace depth and `hides_delimiter`, finer. Give-back rewinds the file (`give_back`, machine.rs:4792). `stripBraces`: the model keeps the argument or its tokens as one hole, the Rust strips known frames exactly (finer) |
| `scanUndelim`, `groupBody` | `read_undelimited`, machine.rs:9887 | same in shape |
| `scanArgs`, `scanParams`, `splitParams`, `substitute` | `read_arguments`, `read_call_arguments`, machine.rs:9722-9750 | same in shape; `\par` in a non-`\long` argument (§ 396) and `#{` are not modelled. Leading tokens: `dropAbs` |
| `expandWith` | macro call with meaning-set split (`limits.meaning_splits`) | a set left unsplit runs `may_run` over its macros (2026-11-04). **deviates** for `top`: Rust havocs nothing, reported as Assumption `unknown-text-run` (D11) |
| `loopHead`, `loopRounds`, `loopRounds_le` | loop heads, explore.rs:946-1124 (`widen_after`, `paths.loops`) | same in shape. The Rust summarizes a diverging head only when its loop is registered (`paths.loops`, explore.rs:1069-1124); otherwise it goes on, and only the line budget ends it (`Machine::charge`, machine.rs:7315) |
| `summaryHavoc` | `Machine::summarize`, explore.rs:1452, via `Env::widen` with `Reach::Havoc`, env.rs:1226 | **deviates** (D2, D3, D5). Levels and save entries: the model makes written locations' levels unknown and leaves `touched` (P5). The Rust keeps the level per slot and restores to the value TeX gives or unknown digits (`semantics::model_deviations::group_ends_restore_after_open_groups_and_summaries`) |
| `callSummaryRust` vs `callSummarySound` | `summarize_call`, `apply_summary`, src/analysis/machine/calls.rs:161, 197 | **deviates** (D4) |
| `mayRunSound` | `Machine::may_run`, machine.rs (also via `widen_conditional`, exec.rs:2370) | coarser since 2026-11-04 for registers, parameters, codes, `\gdef`/`\xdef`, built names. **deviates** past `limits.may_run_depth` (reported) and for `\let` targets (D6 e) |
| `WriteCmd.writeLater`, `WriteCmd.openLater`, `WriteCmd.closeLater`, `BoxEnd.shipout` | `write_stream`, exec.rs; `open_or_close`, commands.rs; `defer`, `ship_pending`, `join_pending`, machine.rs | Rust finer: a whatsit keeps its stream and unexpanded text in `Pending` (path state, a set of lists), and `\shipout`, an output-less page (§ 1023) and `\end` run them in the environment then (§§ 1370, 1374). The model's shipout adds any output (sound, coarser). **deviates**: whatsits wait in one queue per path, not in the box they were appended to: one in a box never shipped is written at the next shipout, one in a box shipped later (a float held back) early |
| `dropErrors`, `DropSound` | `certain_error`, machine.rs:6883; `retract`, machine.rs:5591 | same: drops only paths whose error comes from library code (`own_file` keeps document errors). A refuted run's diagnostics are retracted (explore.rs:812), also for a cycle ended by `no_arm`, which is a run (R2-2) |

## Deviations: potential soundness bugs

Each lets the Rust lose a concrete outcome the model keeps.

- **D1 (fixed 2026-11-04). A join forgets that one side is any meaning.** env.rs:2716.
  `join(x, y)` with `x = (Unknown, may: Some(S))` and `y = (Unknown, may:
  None)` (the top): `members(y)` is `None`, so `set` is `None`, and `same`
  is true because both meanings are `Unknown`. The result is `x`, the set
  `S`. Swapped, the result is the top. A loop head can then see no growth
  and stop early.
- **D2 (fixed 2026-11-04 where a side is joined; open without one). Havoc skips a name undefined on this path.** env.rs:1236.
  `Env::widen` does `let Some(Some(mine)) = ... else { continue }`: a name
  undefined here but written on the other side keeps "undefined" only.
  Possible: sound if every use of the widened path is joined with the
  other side afterwards; not checked.
- **D3. Havoc keeps non-macro meanings.** env.rs:2660 (`widened`). `widened`
  replaces only macros and sets. A name a pass `\let` to a primitive, a
  `\chardef` or a register keeps that one meaning; domains.md says
  "anything else is an unknown meaning".
- **D4. Call summaries cover only the runs seen.** calls.rs:161-252. After
  `call_summary_after` (2) runs with the same written names, a third call
  with unknown operands assigns the join of those runs, widened with
  `Reach::Havoc`. Bounds no run moved stay, equal texts stay, and names
  only a third run would write are not written. `relate` (calls.rs:226)
  goes further and maps the runs' text to this call's numbers. Sound only
  if two runs determine every run.
- **D5. Loop summaries keep what no pass moved.** explore.rs:1426,
  env.rs:2117-2170 (`widen_slot` with `Reach::Havoc`). Same pattern as D4:
  documented ("bounds no pass moved stay") and backed by one more run of
  the body, but that run is not iterated to a fixpoint.
- **D6 (a, b, d fixed 2026-11-04; c configurable as `limits.may_run_depth` and reported past it; e open). `may_run` misses writes.** machine.rs:13581-13824.
  (a) (line numbers in (a)-(b) of 2026-11-03) Only count and dimen registers were set to any value (13159, 13293):
  skip, muskip, toks registers, code tables and catcodes the code writes
  are not. (b) `\gdef`, `\xdef`, `\global\let` inside a group the code
  opens: only the `\global` prefix is tracked (13229), so
  `Primitive::Def { global: true }` is dropped as local. (c) Macros are
  followed `limits.may_run_depth` (default 3) deep and up to
  `limits.expansion_tokens` tokens; past that only a diagnostic is given.
  (d) Names built by `\csname` are not seen. (e) A `\let` target takes its
  meaning now, not when the code would run.
- **D7. A split-off rest of unknown text holds no delimiter.**
  machine.rs:10120 (`is_unknown_text`). Stated as a trade-off for
  termination; it can miss the argument ending in that rest.
- **D8 (fixed 2026-11-04: `unforked_end`, `unforked-argument-end`; the other end's code runs before the body, not after). A refused fork picks one outcome.** machine.rs:10130
  (`unknown_ends_argument`, now `unforked_end`, 10139): when `branch` returns `None` the argument is
  taken not to end in the unknown text. Callers: `read_before_group` (machine.rs:10930) and the parameter-text
  read (machine.rs:10997).
- **D9 (as D6). Past `limits.choices` a conditional's arms are covered by D6.**
  exec.rs:2370 (`widen_conditional`) skips both arms and runs `may_run`
  over them, so D6's gaps apply. Reported as Assumption
  `undecided-condition`.
- **D10 (fixed 2026-11-04; no caller used it, `may_run` now gives dimensions the word too). `Num::full(false)` is not the top of a count.** value.rs:193 (then)
  gave `[-(2^31-1), 2^31-1]`; a count reaches `-2^31` by `\advance`
  (tex.web § 1238 checks no integer overflow). Where it stands for any
  count, `-2^31` is missed. `may_run` uses the word range (correct).
- **D11 (fixed 2026-11-04 for joined sets; top reported, not havocked). Executing a name of unknown meaning widens nothing.**
  domains.md "Known limits", first row. The model's `expandWith` havocs.

PROOF-STATUS P1, P5, P6 (2026-11-04). P1 and P5 are not seen in the Rust.
Groups a loop left open and a summarized loop inside a group give the value
TeX gives or unknown digits at the group's end
(`semantics::model_deviations::group_ends_restore_after_open_groups_and_summaries`).
The model now covers both with `touched` save entries (`assign_local_sound`,
`summaryHavoc_sound`, `endGroup_sound`). P6: the model's cut covers
everything. The Rust cut (`resync`) joins each category the skipped region
changed with its old one (`region-catcodes`), and makes the groups the
region opened one `many` frame (2026-11-04, test
`a_cut_keeps_the_groups_the_region_opened_open`; groups of different kinds
are closed and reported, `region-groups`). Lines the skipped rest would
write to open files are assumed not written, reported as `region-writes`.
That assumption is where the Rust's cut is not the model's.

Primitives the Rust registers as `Unmodeled` (builtins.rs) now have
`Concrete` rules (`primitives.tsv`): exact ones for LuaTeX's math parameters
(`mathParam`), `\toksapp` and relatives (`toksApp`), `\textfont` and
relatives (`famFont`), `\letcharcode`; over-approximating ones otherwise
(`Exec.lua`, `Exec.operands`, `fontCopy`, `Test.condition`). The Rust still
reports each as unsupported when it runs.

Not soundness: `Ty::parent` puts `Reg(k)` under `Top`, not under
`Register` (roles.rs:371-401), though `register` is documented as "a
register of a kind the text does not fix". Types are an editor aid only.

## Not modelled (open)

Catcode sets and catcode splits, files written (`Outputs`, `join_written`, deferred `Pending`),
`\openin` streams, `Wild` name families, `Corr` arm pairing, `Modes` tests,
box material, recorded lists and line breaking, marks, the output routine,
`\csname` joins, the digit-length split of typed digits, `input_bound`,
cuts and the three-stage line budget (job 3 models the budget generically:
`Soundness.wf_of_budget`).
