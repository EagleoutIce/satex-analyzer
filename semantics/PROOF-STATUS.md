# Soundness proof status

> Written with AI assistance from Claude Code (Claude Opus 5.5; build fixes with Claude Sonnet 5.5), directed and reviewed by the author.

Files: `lean/Satex/Soundness.lean`, `lean/Satex/Mouth.lean` (the abstract mouth), `lean/Satex/Operands.lean` (the global state and operand readers), `lean/Satex/Conds.lean` (conditionals) and `lean/Satex/Global.lean` (main theorem), imported from `lean/Satex.lean`.
`lake build` succeeds (Lean 4.35.0, core only). No `sorry` in any file.
`lean/Satex/Axioms.lean` checks with `#guard_msgs` that the main theorems use only `propext`, `Classical.choice` and `Quot.sound`. The build fails if that changes.

## The main theorem

`satex_sound_and_terminating` (`lean/Satex/Global.lean`) has no hypothesis. The reader `read` and the run `OStep` are definitions.

The abstract run is over `Option GState`. `GState` is an `AState` with the pending prefixes, the `\afterassignment` token and `force_eof` pinned exactly (audit C15). `none` is `lost`: the named havoc, γ = every state.

### The mouth

`read` takes the next token with `GState.next`, which runs `amouth` (`Mouth.lean`):

- from a token list: the next literal token. Used-up lists are popped (§ 324).
- from a file: TeX's tokenizer as a function (`alineTok`, §§ 343–356). Letters, others, spaces, `{`, `}`, math shift, alignment tab, parameter, superscript and subscript characters, active characters, control words and control symbols, ignored, invalid and comment characters, and the end of a line (a space, `\par` or nothing, by the scanner state). Each catcode comes from the abstract eqtb and must be one known number (`acat`).
- at the end of a line: the next line with the `\endlinechar` in force (§ 362), if that is one known number.
- at the end of a file: the file level goes, or `\everyeof` comes first when it is a known literal list (etex.ch). `force_eof` decides first, as in § 362.

Intuition: a file level is concrete in SaTeX. Its name, lines, rest of line and scanner state are known, so `AInput.γ` pins it exactly (`f = f'`). Only the catcode table is abstract. So the tokenizer can run as it is, asking the abstract eqtb for each catcode it needs. The alternative, an abstract tokenizer over catcode sets, would fork at every character whose catcode is not known. SaTeX does not do that either.

The choice to make the tokenizer a function, not a relation like `LineTok`: the reader must compute its answer. `alineTok_sound` shows that every `LineTok` derivation gives the function's answer. So the relation is deterministic wherever the function answers. `^^` reductions are not followed. They rewrite the buffer, and documents rarely use them.

Why `force_eof` is in `GState`: `\endinput` sets it and the end of a line reads it. Left free, every line end would split into "next line" and "file ended".

### Commands

`read` follows a token whose meaning is one exact meaning and runs these transfers:

- a macro without parameters (§ 389): its body is pushed, used-up lists popped first (§ 390);
- an undefined name (§ 370), `\relax` (§ 1045), a prefix (§ 1211), a space (§ 1045): glue in a paragraph, nothing elsewhere, and γ does not see the list;
- `\begingroup` and `{` outside math (`AState.beginGroup`, § 1063);
- `\endgroup` and `}` when the abstract save stack names the group (`atopGroup`): a simple or semi-simple group closes (`AState.endGroup`, §§ 281–283), the error cases change nothing γ sees;
- `\aftergroup` (save entry, § 1271) with a pinned group level, `\afterassignment` (token kept, § 1269);
- `\let` (§ 1221) whose right side has a known meaning, with at most `\global` pending: `define_sound` through `gdefine_sound`;
- `\def` and `\gdef` (§ 1218) without parameters: the replacement text up to the matching `}` (`abody`), then `define_sound`;
- an integer parameter (§ 1228), a `\count` register (§ 1224) and a code table entry (§ 1232: `\catcode`, `\lccode`, …) set to a number the reader follows, such as `\tolerance=200`, `\count0=5` or `` \catcode`\@=11 ``: `assign_local_sound` and `assign_global_sound` through `gdefine_sound`. An invalid code is an error and gives 0, as in TeX. For `\lccode` and `\uccode` the limit depends on the engine, so only codes up to 255 (valid in all) or past 1114111 (valid in none) are followed;
- `\endinput` (§ 378): `force_eof` is set, so the file ends at its line end (§ 362);
- `\endcsname` without `\csname`: an error (§ 1135);
- `\iftrue` and `\iffalse` (§§ 498, 500), also under a name `\let` to them (as `\newif` makes), and the `\else`, `\or` and `\fi` of a level the stack knows (§ 510). A false test skips to its `\else` or `\fi` without expansion, nesting by meaning (`apass`, `askip`, `askipFi` in `Conds.lean`). The stack must be exact (no `many` frame). One path per test: the run does not fork.

The operand readers live in `Operands.lean`: `rtoken` (§ 1215), `letRhs` (§ 1221), `abody` (§ 473), `optEquals` (§ 405) and `scanInt` (§ 440: decimal digits or an alphabetic constant up to 255). Each one reads with `GState.next` and has a lemma that the concrete relation (`RToken`, `LetRhs`, `ParamText`, `DefBody`, `OptEquals`, `ScanInt`) gives the same result.

Numbers are read after expansion (`XNext`). The reader does not expand. It follows a number only while each token it reads has a known meaning that does not expand. One lemma makes this cheap: every `Expand` rule starts with an expandable token (`expand_head`). So a token known not to expand rules out all of them at once, and `XNext` is then just `Next`. `GState.define` ends every assignment as `finishAssign` does (§ 1269): the prefixes go, the `\afterassignment` token is pushed. `\globaldefs` must be one known number.

Anything else gives `read = none` and the run goes to `lost`:

- the mouth: a hole, digits or a one-of on top of a token list, a file character whose catcode is not one known number, a `^^` reduction, a line end when `\endlinechar` is not known, a file end when `\everyeof` is not a known literal list;
- the meaning: an unknown or `may` meaning, a primitive matched by name (alignments, § 1046);
- the command: a box or list group's `}`, and every command not listed above. Among them: macros with parameters, `\edef`, `\futurelet`, other assignments, conditionals other than `\iftrue`/`\iffalse` (their tests read numbers, dimensions or tokens), `\unless`, a `\fi` while a test is read or with no level open, `\par`, and a letter or other character in vertical mode (it starts a paragraph);
- skipping: a skipped token whose meaning is not known (inner conditionals are found by meaning);
- the operands: a space token before the name or before `=` (only a token list holds one there), `\long`, `\outer` or `\protected` before an assignment, a parameter or `#` in a `\def`, a body longer than the input (`isize`), an unknown `\globaldefs`;
- numbers: a sign, an octal or hexadecimal constant, an alphabetic constant past 255, an internal quantity (`\count1=\count2`), a token that would expand next to a number, two spaces in a row before it. Dimension, glue and token registers, `\chardef` and relatives, `\advance`, `\multiply` and `\divide` are not followed.

The theorem says:

1. every concrete run is covered by an `OStep` run, at most one abstract step per concrete step (`step_covered`, `ostep_covered`);
2. the run reaches `lost` only where `read` answers `none`;
3. with a budget at least the run's length, the budgeted run covers it without the cut (`budget_enough`, audit C14);
4. with any budget the budgeted run covers it, has no infinite run and none longer than its budget;
5. every abstract state has finitely many successors.

The statement is the same as before this round. What changed is `read`: it answers `none` in fewer cases, and `step_covered` proves the new ones.

`step_covered` is the work: for each state `read` follows, every `Exec` rule (and every `Expand` rule) that could fire is either the one the reader models, with its result covered, or refuted by the head token's meaning, the prefixes, the mode or the group code.

Counter-model check: `Read`, `Other` and `hsplit` are gone. `read` and `OStep` are fixed, so neither `Read := ⊥` nor `Other := Concrete.Step E` can be chosen. A reader that always answered `none` would make the theorem trivial. This one answers `none` only in the cases listed above, and `step_covered` must hold in all others.

What this does not say:

- After the first command `read` does not follow, the run is `lost` and nothing more is claimed. Files are now read, so a run no longer is `lost` at the first token of a file. It is `lost` at the first command outside the list. Code made only of the commands above is followed until the input runs out: definitions without parameters, `\let`, groups, spaces, `\catcode` and `\count` assignments, `\iftrue` and `\iffalse`. In a LaTeX document it is early: `\documentclass` is a macro with parameters, and the first letter of text starts a paragraph.
- Only integer parameters, `\count` registers and code tables are followed, with numbers `scanInt` reads. `weakAssign_sound` is not used. A `\catcode` change the reader follows is seen by the tokenizer at once: the catcode it asks for is the new exact value.
- `branch_sound`/`ifNum_branch_sound` are not used: `\iftrue` and `\iffalse` need no narrowing. `\ifnum` is not followed yet: it needs a reader for the relation, and documents mostly compare internal quantities, which `scanInt` does not read. `\ifx` compares primitives by `E.primId`, which the abstract state does not know. `scanDelim_sound`, `stripBraces_sound`, `substitute_sound`: macros with parameters need `scanUndelim_sound` (open) and reading across levels. `loopHead_sound`, `summaryHavoc_sound`, `aval_widen_sound`: no join or widening in `OStep` yet (one path per `read` result). These stay guarded in `Axioms.lean` only.
- The old split (`run_covered_families`, `fam_covered`, `ReadCovers`) remains as a generic lemma. No main theorem uses it.
- Read streams and the nest beyond the mode kind are still free in γ (C15). `force_eof` is pinned in `GState`, and file levels (`eofSeen`, `text`, `pseudo` included) are pinned exactly in `AInput.γ`.
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
| global | `run_covered`, `run_covered_families`, `fam_covered` (generic), `budget_star`, `sound_and_terminating`; Mouth: `aint_sound`, `alit_γ`, `atoks_sound`, `acat_sound`, `ascan_sound`, `alineTok_sound`, `amouth_sound`, `amouth_not_drained`; Operands: `ameaning_sound`, `gnext_sound`, `gnext_not_drained`, `rtoken_sound`, `letRhs_sound`, `letOps_sound`, `abody_sound`, `defOps_sound`, `finish_sound`, `gdefine_sound`, `expand_head`, `xnext_sound`, `xnonblank_sound`, `adigits_sound`, `signs_plain`, `optSpace_sound`, `scanInt_sound`, `optEquals_sound`, `back_sound`, `acodeOk_sound`; Conds: `condRel_length`, `setLimitAt_top`, `apass_sound`, `askip_sound`, `askipFi_sound`; Global: `appendItem_γ`, `atopGroup_sound`, `unsaveAux_top`, `unsave_code`, `apopDone_sound`, `close_sound`, `step_covered`, `ostep_covered`, `budget_enough`, `satex_sound_and_terminating` (no hypothesis) |

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

- `read` beyond what it follows now: the rest of `ScanInt` (signs, constants, internal quantities), `ScanDimen`, `Internal`, `MatchParams` over `AInput`. Each one moves cases from `lost` into `step_covered`. The file tokenizer (`alineTok`), `RToken`, `LetRhs`, `OptEquals` and `ScanInt` for decimal digits and alphabetic constants are done (2026-11-18). Without them `scanDelim_sound` is not linked to `Expand.call`.
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
