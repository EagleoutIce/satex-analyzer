# Soundness proof status (job 3)

File: `lean/Satex/Soundness.lean`, imported from `lean/Satex.lean`.
It is **not built**. No Lean toolchain is installed. It was written for core Lean 4 without mathlib and has no `sorry`.
Where a lemma is false for the model, the theorem takes the failing property as a named hypothesis, and the counterexample is below.
Check the build first: the proofs rely on `omega`, `split`, `simp only [f]` with equation lemmas, and defeq through `ite`/`match`.

## Proven (modulo the build)

| Area | Theorems |
| --- | --- |
| generic domain | `AbsDom`, `γ_join`, `join_mono`, `Sound` closed under fork/join, decided arm, sequencing, weakening, `top` |
| generic loop | `loopAbs_post`, `loopAbs_sound` (any number of passes), `loopPasses_le` (≤ k + 1 rounds) |
| intervals | `itv_le_refl/trans`, `itv_join_lub`, `itv_γ_join`, `itv_γ_widen` (inside the word), `widen_chain_changes_le_two` (a widening chain changes at most twice) |
| glue hulls | `ghLe_*`, `gh_join_l/r`, `gh_γ_mono`, `hullItv_cover`, `hullGlue_cover` |
| values | `aval_join_sound` (CHANGES item 1, given `HullSound`), `avalDom : AbsDom Val AVal` |
| assignment | `assign_global_sound` (§ 279), `assign_local_sound` (§ 277, level pinned), `define_sound` (`\globaldefs`), `pinned_of_plain` |
| conditionals | `branch_sound` (item 6), `compare_sound` (`\ifnum`/`\ifdim` decide soundly), `astate_join_sound` (item 10, join at `\fi`) |
| expansion | `atext_γ_append`, `substitute_sound` (§ 390), `dropToks_sound` (known delimiters, § 397), `text_meaning_body` |
| loop head | `loopHead_post`, `loopHead_sound` (k passes, widening, summary), `loopHead_rounds` |
| global | `run_covered` (induction on `Concrete.Steps`), `paths_covered`, `sound_and_terminating` (budgeted run: covers every concrete trace, well-founded, length ≤ budget) |

## Open (no `sorry`, simply not stated as proven)

- `endGroup_sound` (item 5) is false as modelled: see P2. It also needs P1's pin.
- `scanDelim_sound`, `scanUndelim_sound` and `scanParams_sound` (item 7) need a lemma relating `Concrete.firstDelim` to `scanDelim` over holes. Only the known-token part, `dropToks_sound`, is done.
- `expand_sound` (item 8) needs item 7 and the `Expand` rule for macro calls.
- `weakAssign_sound` (item 4) needs a fold over `assign_*`. It only applies when levels are pinned.
- `AVal.widen_sound` (item 2) needs `HullSound` and `inWord` on every interval.
- The `TClass` order (`le`/`union` upper bound) is not proven. It needs a 16-boolean case split, and core Lean has no `bv_decide`.
- The `Ty` lattice is not proven. Types add no narrowing, so soundness does not depend on them.
- A local-soundness lemma for each `Exec` rule. The global theorems take it as the hypothesis `hloc`.

## Counterexamples: where the model (and maybe the Rust) is not sound

- **P1. Local assignment under a `many` group frame.** `AState.assign` gives a new value the level `a.curLevel`, and decides whether to save by that level. With a `many` frame, `a.curLevel` is not pinned to the concrete level.
  - Example: abstract save `[group c true]` at `curLevel` 2, and a concrete state with zero copies (`curLevel` 1). Then `\count0=1` locally makes the abstract level 2 and the concrete level 1, which breaks `γ`. The abstract run also pushes a restore that the concrete run lacks, so `SaveRel` fails.
  - The Rust ("levels per slot", "stacks as intervals") should be checked for the same effect: a later `}` restores, or fails to restore, by a wrong level.
- **P2. `\aftergroup` tokens.** Concrete `unsave` pushes all inserted tokens as one backed-up list. `AState.unsave` pushes one level per token, and `InputRel` compares level by level. So `{\aftergroup\a\aftergroup\b}` is not covered. This is a representation mismatch in the model only: make `InputRel` flatten adjacent backed-up levels, or push one list.
- **P3. `TClass.admits` applies the hex restriction to signs.** Take `number ∪ {l, hex}`: the union has `hex = true` and does not admit `-` (`.char 45 .other` is not a hex code). So `AText.hull` of `[digits [-5,-5]]` and a hex-letter hole misses `-5`. The Rust `TextClass` keeps SIGN apart from OTHER (tex.rs:421), so this is likely a model bug. Restrict `admits` by `hex` only for the letter and other kinds.
- **P4. `AText.hull` requires balanced text.** `Hole.γ` demands `balanced p`, but `AVal.toks` admits unbalanced members (`[tok '{']`). Concrete token registers are always balanced, so either add `balanced` to `AVal.γ` for toks, or drop it from the hull. Because of P3 and P4, `HullSound` is false today.
- **P5. `summaryHavoc` keeps levels and the save stack.** A body that runs `\count0=1` at group level 2, while `\count0` was set at level 1, pushes a restore entry on its first pass and changes the slot level. The summary keeps `a.save` and `a.level`, so `γ` fails. CHANGES item 9 is false as stated.
  - Possibly also in the Rust: if a loop or call summary moves a slot's level without the save entry, a later exact local write inside the same group is not restored at `}` (for example `\count0=7 {…summarized loop… \count0=5} \the\count0` gives 5 instead of 7). Check `widen_slot` in env.rs.
- **P6. The budget cut needs a real top.** `sound_and_terminating` holds only if a cut moves to a state that covers everything. `AState` has no such state (input, save and cond are exact). The Rust cut widens assignments only, not catcodes, groups, files, streams or `\write` (domains.md "Known limits", row 2). So cut runs are not covered.

## How D1–D11 meet the proof

- D1 breaks `loopHead_sound`'s hypothesis `hg` (`grows = false → join ≤ head`): a loop can stop before its fixpoint.
- D4 and D5 break `hs` (the summary must be a post-fixpoint above the entry).
- D6 and D9 break the `may_run` havoc, which would need every write.
- D7 and D8 break completeness of `scanParams` (item 7): a concrete argument split has no abstract pair.
- D10 is outside `itv_γ_widen` (`inWord`), because `Num::full(false)` is used as a top that it is not.
- D11: the model havocs here, so `expandWith` stays sound, but the Rust does not.
- D2 and D3 weaken the havoc used by `loopHead`'s `widen` (`hw`) and `summary` (`hs`).
