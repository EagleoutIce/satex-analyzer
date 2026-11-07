# Soundness proof status

> Written with AI assistance from Claude Code (Claude Opus 5.5; build fixes with Claude Sonnet 5.5), directed and reviewed by the author.

File: `lean/Satex/Soundness.lean`, imported from `lean/Satex.lean`.
`lake build` succeeds (Lean 4.35.0, core only). No `sorry` in any file.
`lean/Satex/Axioms.lean` checks with `#guard_msgs` that the main theorems use only `propext`, `Classical.choice` and `Quot.sound`. The build fails if that changes.

## The main theorem

`satex_sound_and_terminating`: with one budget per run, every concrete run of a document is covered by an abstract run, no abstract run is infinite, and none is longer than its budget.
The budget is one counter in the state that every abstract step draws from (as `limits.steps`). It is not per step and not per line (the Rust's line guard is not modelled).
The budget cut moves to a state that covers everything, so the cut makes the run coarser, never unsound. Past the cut `bγ` is `True`: the theorem says nothing about the rest of the run.

Its local obligation is split by family (`run_covered_families`). A step reads a command and its operands, then runs the command's effect.
Two hypotheses remain, both named:

- `hread : ReadCovers Read read`: the abstract reading (mouth and scanners) covers TeX's reading, state and command.
- `hother : Covered AState.γ (AStep L read) Other`: the families that have no transfer in the model yet (list below).

What this does not say:

- `Read` and `Other` are free, and so is `hsplit`. With `Other := Concrete.Step E`, `hsplit` is trivial and `hother` is the whole soundness obligation. With `Read := ⊥`, `hread` is trivial. So the theorem is the induction and the budget applied to two assumed coverings. No instantiation needs the proven effects.
- `branch_sound`, `ifNum_branch_sound`, `scanDelim_sound`, `loopHead_sound`, `summaryHavoc_sound`, `weakAssign_sound` and `aval_widen_sound` are used by no global theorem. They are guarded in `Axioms.lean` only.
- A step whose change is all in its reading (expansion, `\relax`, a decided test) is covered with the identity effect `ACmd.none`, so it needs no dummy assignment. Its whole obligation is then `hread`.
## Proven

| Area | Theorems |
| --- | --- |
| generic domain | `AbsDom`, `γ_join`, `join_mono`, `Sound` closed under fork, decided arm, sequencing, weakening, `top` |
| generic loop | `loopAbs_post`, `loopAbs_sound`, `loopPasses_le` (≤ k + 1 rounds) |
| intervals | `itv_join_lub`, `itv_γ_join`, `itv_γ_widen`, `widen_chain_changes_le_two` |
| glue hulls | `gh_join_l/r`, `gh_γ_mono`, `hullItv_cover`, `hullGlue_cover` |
| text hull | `hull_sound` (P3 and P4 fixed), `admits_union_l/r`, `printInt_number` |
| values | `aval_join_sound` (no hypothesis), `avalDom`, `aval_widen_sound` (item 2: values in the 32-bit word). `Itv.full false` is the word (D10) |
| assignment | `assign_global_sound` (§ 279), `assign_local_sound` (§ 277, any level, under `many` frames: P1 fixed), `define_sound` (`\globaldefs`), `weakAssign_sound` (item 4) |
| grouping | `beginGroup_sound` (§ 274), `endGroup_sound` (item 5, §§ 281–283: plain and `many` frames, `\aftergroup` as one list: P2 fixed) |
| summaries | `summaryHavoc_sound` (item 9 corrected: levels and save entries of written locations: P5 fixed) |
| conditionals | `branch_sound` (item 6) takes the narrowing's soundness as hypothesis `hnarrow`. `\ifnum` over intervals discharges it: `narrow_sound` (`Itv.narrow`), `ifNumTest_sound`, `ifNumNarrow_sound`, `ifNum_branch_sound`. Other tests have no narrowing lemma. `compare_sound`, `astate_join_sound` (item 10, joinable states only) |
| arguments | `scanDelim_sound` (item 7, over typed holes, digits and one-of tokens, delimiters across items: R2-1, R2-6), `dropAbs_sound` (leading tokens), `stripBraces_sound` (§ 400), `substitute_sound` (§ 390) |
| loop head | `loopHead_post`, `loopHead_sound`, `loopHead_rounds` |
| global | `run_covered`, `run_covered_families`, `fam_covered` (assignment, grouping and identity effects), `budget_star`, `sound_and_terminating`, `satex_sound_and_terminating` |

## Audit (2026-11-04)

Every theorem was checked for hypotheses that never hold or assume the result.

- `HullSound` was false (P3, P4). The model is fixed and the lemma proven. No theorem takes it any more.
- `assign_local_sound` needed the level pinned. It now holds at any level.
- `hloc` assumed local soundness of every step. It is now split: the effects of assignment and grouping are proven, and the reading and the other families are named hypotheses.
- `budget_covers` needed one abstract step per concrete step. It now takes any abstract run per step and cuts it where the budget ends.
- `loopHead_sound` takes `join`, `widen` and `summary` as parameters with hypotheses. None of the model's own transfers meets them. `hj` asks `join` to be an upper bound of any two states, but `AState.join` is sound only for joinable states (`astate_join_sound` needs `a.joinable b`). `hw` asks the same of `widen`, but `AVal.widen` is sound only in the word (`aval_widen_sound` needs `AValInWord`). `hs` asks the summary to be a post-fixpoint of the abstract body. A body that saves entries changes the save stack, so `summaryHavoc` meets `hs` only for bodies without local writes.
- `summaryHavoc_sound` covers runs whose steps keep the concrete footprint (`Footprint`). `Footprint` asks `s'.input = s.input`. The concrete model pushes a macro body without popping exhausted token lists (audit A43, § 390), so each pass of a `\loop`/`\iterate`-style body leaves one more level. For such loops the footprint fails and `summaryHavoc_sound` does not apply.
- `γ` is not trivial anywhere. `AVal.top` and the cut state are the only states covering everything. `AState.γ` pins values, known levels, the save, conditional and input stacks, and the mode's kind (`s.mode.kind ∈ a.modes`). It does not pin the inner flag or the nest, `pending` (prefixes, the `\afterassignment` token), streams, fonts, outputs, `halted` or `fatal`.

## Model changes in this round

- **P1, P5.** A write whose save entry the model cannot decide leaves `touched l` on the save stack: any number of restore entries for `l` may sit in that frame (`SaveRel.skip`). Closing the frame makes `l` unknown, as the Rust's `unfold` widens saved slots. Levels are `Option Nat`. `AState.pinned` says whether `curLevel` is TeX's group level. It is lost where a `many` frame closes.
- **P2.** `closeFrom` collects `\aftergroup` tokens into one backed-up list, as `Concrete.unsave` does.
- **P3.** `hex` restricts letters and others only (`hexOk`).
- **P4.** `AVal.γ` of token lists requires balanced text. Token registers always hold balanced text. `Hole.γ` requires it too, so no hole stands for unbalanced unknown text, such as `\directlua` output (`Expand.lua` pushes any result). Open for holes in the input.
- **P6.** The budget cut is the state covering everything (`none`, `bγ = True`). The Rust cut (`resync`) now joins categories and makes the groups the region opened a `many` frame (test `a_cut_keeps_the_groups_the_region_opened_open`: TeX gives 31, SaTeX gave 7). Lines the skipped rest would write to open files stay an assumption of the Rust, reported as `region-writes` (domains.md "Known limits").
- `weakAssign` writes `touched` and the join of the levels (`lvJoin`). The old fold of `assign` pushed restore entries TeX never made.
- `scanDelim` is now the list of cuts of the input whose right side may begin with the delimiter (`AText.cuts`, `dropAbs`). The old scanner missed a delimiter that starts in one item and ends in a later one, and passed one-of tokens with unbalanced members whole.
- `stripBraces` keeps the argument or its tokens as one hole.
- `primitives.tsv`: the 204 `Exec.unmodeled` rows and the 11 open rows all name rules now. Exact: LuaTeX's math parameters (`mathParam`, 113 rows), `\toksapp` and relatives (`toksApp`), `\textfont` and relatives (`famFont`), `\letcharcode`, cramped styles, `\hjcode`, `\attribute`, `\hyphenationmin`, eTeX names. Over: LuaTeX direction, local box and math-noad primitives (`Exec.lua`, any assignment), backend ones (`Exec.operands`), font copies (`fontCopy`), `\ifcondition` (`Test.condition`). `Exec.unmodeled` is gone.

## Model changes after audit A (2026-11-07)

- `ACmd.none`/`CCmd.none`: the identity effect (B2).
- `Itv.full false` is the 32-bit word, as `Num::full` (B7, D10).
- `narrow_sound`, `ifNumTest_sound`, `ifNumNarrow_sound`, `ifNum_branch_sound` (B5).
- The theorem's wording: budget per run, free `Read`/`Other`, unused lemmas, the hypotheses of `loopHead_sound` and `summaryHavoc_sound` (B1, B3, B4, B6).

## Open

- `ReadCovers`: an abstract mouth and abstract scanners (`Next`, `ScanInt`, `ScanDimen`, `Internal`, `MatchParams` over `AInput`). Without it `scanDelim_sound` is not yet linked to `Expand.macro`.
- `scanUndelim_sound`: not proven. Fixed on the way: a hole of class `c` may hold only spaces, and a one-of's rest is its members' suffixes. Still open: a one-of member with an unbalanced `{` whose group ends in a later item (`groupBody` passes non-token items whole).
- `expand_sound` (item 8): needs `ReadCovers` and `scanUndelim_sound`. It must also assume the restrictions of `expandWith`: a file on top of the input and a non-macro meaning leave the state unchanged, arguments are read from the top token list only (TeX reads across levels), and `#{` (`braceDelim`, §§ 473, 476) is ignored.
- `many` save frames hold only restore entries. `\aftergroup` tokens inside repeated groups are not covered, and `closeFrom` on a `many` frame never returns them.
- Glue and box values have no widening (`AVal.widen` takes their bounded hull). Their chains end only by the round count (`loopRounds_le`).
- Families without a transfer (`Other`): expansion primitives other than macro calls, boxes and typesetting, files and streams, `\write`, `\afterassignment` (`finishAssign`), modes, the output routine.
- The `TClass` order (`TClass.le`) and the `Ty` lattice. Types add no narrowing, so soundness does not depend on them.

## Counterexamples resolved

P1–P5 of the previous status are fixed in the model as above. P6 is fixed in the Rust for categories and groups. Writes past a cut are a reported assumption. The REVIEW-2 findings R2-1 and R2-6 are covered by `scanDelim_sound`.

## How D1–D11 meet the proof

- D1 breaks `loopHead_sound`'s hypothesis `hg` (`grows = false → join ≤ head`): a loop can stop before its fixpoint.
- D4 and D5 break the footprint `summaryHavoc_sound` needs: the summary must cover every location a run may write.
- D6 and D9 break the `may_run` havoc, which needs every write.
- D7 and D8 break `scanDelim_sound`: a cut the Rust prunes may be one TeX takes.
- D10 is outside `aval_widen_sound` (`AValInWord`).
- D11: the model havocs here, so `expandWith` stays sound, but the Rust does not.
- D2 and D3 weaken the havoc used by `loopHead`'s `widen` (`hw`) and the summary.
