# Soundness proof status

> Written with AI assistance from Claude Code (Claude Opus 5.5; build fixes with Claude Sonnet 5.5), directed and reviewed by the author.

Files: `lean/Satex/Soundness.lean` and `lean/Satex/Global.lean` (main theorem), imported from `lean/Satex.lean`.
`lake build` succeeds (Lean 4.35.0, core only). No `sorry` in any file.
`lean/Satex/Axioms.lean` checks with `#guard_msgs` that the main theorems use only `propext`, `Classical.choice` and `Quot.sound`. The build fails if that changes.

## The main theorem

`satex_sound_and_terminating` (`lean/Satex/Global.lean`) has no hypothesis. The reader `read` and the run `OStep` are definitions.

The abstract run is over `Option GState`. `GState` is an `AState` with the pending prefixes and the `\afterassignment` token pinned exactly (audit C15). `none` is `lost`: the named havoc, γ = every state.
`read` follows the next literal token on top of a token list (exhausted lists are popped, § 324) whose meaning is one exact meaning. It runs these transfers:

- a macro without parameters (§ 389): its body is pushed, used-up lists popped first (§ 390);
- an undefined name (§ 370), `\relax` (§ 1045), a prefix (§ 1211);
- `\begingroup` and `{` outside math (`AState.beginGroup`, § 1063);
- `\endgroup` and `}` when the abstract save stack names the group (`atopGroup`): a simple or semi-simple group closes (`AState.endGroup`, §§ 281–283), the error cases change nothing γ sees;
- `\aftergroup` (save entry, § 1271) with a pinned group level, `\afterassignment` (token kept, § 1269).

Anything else gives `read = none` and the run goes to `lost`: a file on top of the input, a hole, digits or a one-of on top, an unknown or `may` meaning, a primitive matched by name (alignments, §1046), a box or list group's `}`, and every command not listed above, assignments included.

The theorem says:

1. every concrete run is covered by an `OStep` run, at most one abstract step per concrete step (`step_covered`, `ostep_covered`);
2. the run reaches `lost` only where `read` answers `none`;
3. with a budget at least the run's length, the budgeted run covers it without the cut (`budget_enough`, audit C14);
4. with any budget the budgeted run covers it, has no infinite run and none longer than its budget;
5. every abstract state has finitely many successors.

`step_covered` is the work: for each state `read` follows, every `Exec` rule (and every `Expand` rule) that could fire is either the one the reader models, with its result covered, or refuted by the head token's meaning, the prefixes, the mode or the group code.

Counter-model check: `Read`, `Other` and `hsplit` are gone. `read` and `OStep` are fixed, so neither `Read := ⊥` nor `Other := Concrete.Step E` can be chosen. A reader that always answered `none` would make the theorem trivial; this one answers `none` only in the cases listed above, and `step_covered` must hold in all others.

What this does not say:

- After the first command `read` does not follow, the run is `lost` and nothing more is claimed. A real document starts from a file, so today `lost` comes at the first token of a file. The theorem is free of assumptions but weak.
- The proven assignment transfers (`assign_*_sound`, `define_sound`, `weakAssign_sound`) are not used: an assignment reads operands (`ScanInt`, `Value`, `RToken`, `LetRhs`), and `read` has no abstract scanner yet. `\let` with literal tokens is the cheapest next step.
- `branch_sound`/`ifNum_branch_sound`: conditionals read their test (`ScanInt`, `\ifx` tokens); not followed. `scanDelim_sound`, `stripBraces_sound`, `substitute_sound`: macros with parameters need `scanUndelim_sound` (open) and reading across levels. `loopHead_sound`, `summaryHavoc_sound`, `aval_widen_sound`: no join or widening in `OStep` yet (one path per `read` result). These stay guarded in `Axioms.lean` only.
- The old split (`run_covered_families`, `fam_covered`, `ReadCovers`) remains as a generic lemma; no main theorem uses it.
- `State.forceEof`, `eofSeen`, `pseudo`, `text`, read streams and the nest beyond the mode kind are still free in γ (C15). `read` never follows a file, so it never needs them.
- The termination conjunct is the budget's: it holds for any relation (C14). What is specific to the model: (2), (3) and (5).

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
| global | `run_covered`, `run_covered_families`, `fam_covered` (generic), `budget_star`, `sound_and_terminating`; Global: `anext_sound`, `anext_not_drained`, `gnext_sound`, `ameaning_sound`, `atopGroup_sound`, `unsaveAux_top`, `unsave_code`, `apopDone_sound`, `close_sound`, `step_covered`, `ostep_covered`, `budget_enough`, `satex_sound_and_terminating` (no hypothesis) |

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
- Concrete.lean follows tex.web closer (audit A1–A55). No proof uses the changed rules. A macro call now pops used-up token lists first (§ 390, A43), so a tail-recursive loop keeps its input depth, which `Footprint` (B6) asks for. Core gained `Item.vbox`, `Cmd.alter`, `Macro.brace`, `FileLevel.eofSeen` and `Font`; each rule cites tex.web and the SaTeX code it maps to.

## Model changes after audit C (2026-11-07)

Concrete.lean and Core.lean follow tex.web for audit C1–C13. Each changed rule cites its § and the SaTeX code; the pdfTeX/XeTeX runs are target/scratch/audit-c/tNN.tex. No proof uses the changed rules.

- **C1.** A token register, `\every…`, a font or `\font` read as a number (`tokLevelTok`, § 415): "Missing number", the token is read again, 0 at `dimen_val`. `ScanInt.missing`, `ScanUnit.tokLevel`, `ScanDimen.tokLevel`/`tokLevelMu`, `ScanGlue.tokLevel` (t01, t07).
- **C2.** `\ignorespaces` pushes the next token with its `\noexpand` marker, so it acts as `\relax` (§ 1045, t05).
- **C3.** `ParamItem.arg c` keeps the parameter character (§ 476): `\ifx` tells `#1` from `!1` (§ 508, t11). `\meaning` prints it, and the body's `#n` uses `match_chr` (§ 294). Abstract.lean only matches `.arg _`.
- **C4.** `scanLetters` keeps a `^^` reduction after a control word's letters in the buffer (§ 356, t10).
- **C5.** `force_eof` is one flag, `State.forceEof` (§ 361). `\endinput` sets it, the next file level that wants a line ends and clears it (§ 362, § 378, t04). `FileLevel.forceEof` is gone.
- **C6.** `splitLines`: no line for an empty `\scantokens` text, none after a final `\newlinechar` (etex.ch `pseudo_start`, t13).
- **C7.** Register glue passes `trap_zero_glue` (`trapVal`, § 1236, t06).
- **C8.** File names (§ 526, § 516, TeX Live tex.ch): any token whose meaning is a character ≤ 255 counts, implicit ones too; code 32 ends the name in any category; `\input{…}` reads an expanded balanced text (`FileName.braced`, t02, t08). The TeX Live rule that a space at the end of a buffer line ends even a quoted name is not modelled.
- **C9.** Already fixed (`dimVal`/`dimState`, t12).
- **C10.** `\everyeof` counts the fake empty line: `\inputlineno` is one past the last line (§ 362, t09).
- **C11.** `\long`, `\outer` or `\protected` before a command other than `\def` (`longMisplaced`, § 1213, etex.ch): `Exec.prefixLong` raises the error, keeps only `\global` and runs the command (t14). `Head` excludes that case.
- **C12.** `\ifx` compares primitives by identity: `Engine.primId` names one primitive per (`cur_cmd`, `cur_chr`), so engine aliases are equal (§ 507, t15).
- **C13.** `nameOf` is injective on codes up to `0x10FFFF`: `U+FFFF` and surrogates are escaped, `strCodes` undoes it (§ 372, t16).

## Open

- `read` beyond literal token lists: an abstract tokenizer for files (`LineTok` over known catcodes) and abstract scanners (`ScanInt`, `ScanDimen`, `Internal`, `MatchParams`, `RToken`, `LetRhs` over `AInput`). Each one moves cases from `lost` into `step_covered`. Without them `scanDelim_sound` is not linked to `Expand.call`.
- `scanUndelim_sound`: not proven. Fixed on the way: a hole of class `c` may hold only spaces, and a one-of's rest is its members' suffixes. Still open: a one-of member with an unbalanced `{` whose group ends in a later item (`groupBody` passes non-token items whole).
- `expand_sound` (item 8): needs abstract `MatchParams` in `read` and `scanUndelim_sound`. It must also assume the restrictions of `expandWith`: a file on top of the input and a non-macro meaning leave the state unchanged, arguments are read from the top token list only (TeX reads across levels), and `#{` (`braceDelim`, §§ 473, 476) is ignored.
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
