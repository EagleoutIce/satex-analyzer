# Semantics changes

Coordination log for the three semantics jobs. Newest first. Core names are
stable once published; a change to one is announced here first.

## Lean round (2026-11-04)

* Core: `Cmd.mathParam`, `toksApp`, `famFont`, `letCharcode`, `fontCopy`
  added; `Cmd.unmodeled` removed (no primitive uses it); `IfOp.condition`
  added (LuaTeX `\ifcondition`).
* Concrete: rules `Exec.mathParam`, `toksApp`, `famFont`, `letCharcode`,
  `fontCopy`, `Internal.mathParam`, `famFont`, `Test.condition`; helpers
  `MathStyleArg`, `ToksReg`, `styleIndex`, `famCount`, `boxKind` (`\hpack`,
  `\vpack`, `\tpack`). `Exec.unmodeled` removed.
* Abstract: `AState.level : Loc → Option Nat`, `AState.pinned`;
  `ASave.restore` takes `Option Nat`, `ASave.touched` added, `ASave.group`
  takes `Option (List Loc)` (the locations a `many` frame's copies save);
  `SaveRel.skip`; `Touched`. `unsave`/`endGroupN`/`widenSaved` replaced by
  `closeFrom` (and `closed`, `forget`); `weakAssign` via `weakAt`,
  `newLevel`, `lvJoin`. `TClass.admits` via `hexOk`; `AVal.γ` of token lists
  requires `balanced`. Scanning: `AItem.cuts`, `AText.cuts`, `dropAbs`,
  `prefixes`, `suffixes`; `scanDelim δ x` (no depth argument);
  `readLead`, `afterItem`, `dropToks`, `mayHold` removed; `stripBraces`
  keeps the argument or its hull. `summaryHavoc` also forgets levels and
  leaves `touched`.
* Soundness: see PROOF-STATUS.md. `run_covered` and `sound_and_terminating`
  take `Covered γ T Step` (a covering abstract run per step).
  `satex_sound_and_terminating` splits the obligation by family.

## Job 1 (concrete semantics)

* Published `lean/Satex/Core.lean`: `Cat`, `CsRef`, `Token`, `TokList`,
  `Glue`, `Level`, `BoxVal`, `Item`, `Mode`, `ListState`, `GroupCode`,
  `BoxContext`, `RegKind`, `CodeTable`, `Operand`, `ConvOp`, `IfOp`,
  `FiCode`, `LastOp`, `Cmd`, `ParamItem`, `Macro`, `Meaning`, `Loc`, `Val`,
  `Entry`, `SaveEntry`, `MouthState`, `TokListKind`, `FileLevel`,
  `InputLevel`, `IfLimit`, `CondLevel`, `ReadStream`, `Output`, `Pending`,
  `State` (with `get`, `meaningOf`, `intAt`, `catOf`, `intPar`, `toksAt`,
  `mode`), `upd`.
* Lake project: `lean/lakefile.lean` (package `Satex`, lib `Satex`),
  `lean/lean-toolchain`. `lean/Satex.lean` is the root: add your module's
  import there.
* Core changes after publishing (no other job used them yet):
  `CondLevel.unless` is now `CondLevel.negated` (`unless` is a Lean
  keyword); `Cmd.gletCmd` added (LuaTeX `\glet`); `State.interaction`
  defaults to 1 (nonstop, § 73 numbering).
* `lean/Satex/Concrete.lean` (namespace `Satex.Concrete`):
  `Step E s s'` (`E : Engine` holds file resolution, font metrics, operand
  grammars), `Exec E` (stomach rules), `Expand E` (gullet), `Next`
  (mouth), `XNext`, scanners (`ScanInt`, `ScanDimen`, `ScanGlue`,
  `Internal`, `ScanExpr`, `Keyword`, `GeneralText`, `ExpandedText`),
  `MatchParams`, `PassText`, `Steps`, `WellFormed`. Group/eqtb helpers:
  `eqDefine`, `geqDefine`, `define`, `unsave`, `newSaveLevel`. Rules
  marked **[over]** over-approximate; every primitive is mapped in
  `primitives.tsv` (columns name, rule, source, kind).

## Job 2 (abstract semantics)

* Published `lean/Satex/Abstract.lean` (namespace `Satex.Abstract`, imported
  from `lean/Satex.lean`). Domains with `γ`: `Itv` (`Itv.γ : Itv → Int →
  Prop`), `GlueHull` (`→ Glue`), `TClass`/`Hole`/`AItem`/`AText`
  (`AText.γ : AText → TokList → Prop`), `AMeaning` (`→ Meaning`), `AVal`
  (`AVal.γ : AVal → Val → Prop`), `Ty` (γ a parameter, `Ty.Admissible`),
  `AState` (`AState.γ : AState → State → Prop`, via `SaveRel`, `CondRel`,
  `InputRel`), `APaths` (`APaths.γ`). Orders: `Itv.le`, `TClass.le`,
  `Ty.le`, else `semLe γ`. Joins: `Itv.join`, `GlueHull.join`,
  `TClass.union`, `AVal.join L`, `Ty.join`, `AState.join L`. Widening:
  `Itv.widen`, `AVal.widen L`.
* Proven here: `Itv.γ_mono`, `Itv.le_join_left/right`, `Itv.le_widen`
  (needs `inWord`), `Itv.wlo_cases/whi_cases/wlo_fix/whi_fix` (each bound
  moves at most once: the widening chain argument), `loopRounds_le`.
  Nothing is built yet: no Lean toolchain.
* `AbsOps C A` holds `le join top γ`, field names as in job 3's `AbsDom`, so
  each domain fills `AbsDom` by adding the laws.
* Local soundness lemmas for job 3 (statements; `Conc*` are job 1's
  concrete functions, to be named once `Concrete.lean` exists):
  1. `AVal.join_sound : a.γ v ∨ b.γ v → (AVal.join L a b).γ v`
  2. `AVal.widen_sound : (∀ ints in word) → a.γ v ∨ b.γ v → (AVal.widen L a b).γ v`
  3. `assign_sound : a.γ s → v.γ x → (a.assign glob l v).γ (ConcDefine s glob l x)`
     (§§ 277, 279)
  4. `weakAssign_sound : a.γ s → l ∈ ls → v.γ x → (a.weakAssign L glob ls v).γ (ConcDefine s glob l x)`
  5. `endGroup_sound : a.γ s → ConcUnsave s = some s' → ∃ a', a' ∈ a.endGroup L c ∧ a'.γ s'`
  6. `branch_sound : (test a = some b → a.γ s → ConcTest s = b) →
     (a.γ s → ConcTest s = b → ∃ a', narrow b a = some a' ∧ a'.γ s) →
     a.γ s → ∃ a', (ConcTest s, a') ∈ branch test narrow a ∧ a'.γ s`
  7. `scanDelim_sound : AText.γ x p → ConcScanDelim δ p = some (arg, rest) →
     ∃ q, q ∈ stripAll (scanDelim δ 0 x) ∧ AText.γ q.1 arg ∧ AText.γ q.2 rest`
     (same for `scanUndelim`, `scanParams`)
  8. `expand_sound : a.γ s → ConcExpand s r s' → ∃ a', a' ∈ expandWith havocAll a r ∧ a'.γ s'`
  9. `summaryHavoc_sound : (∀ s s', Body s s' → ∀ l, l ∉ writes → s'.eqtb l = s.eqtb l) →
     Sound (summaryHavoc writes) (Star Body)` (and `loopAbs_sound` with it)
  10. `AState.join_sound : a.joinable b → a.γ s ∨ b.γ s → (a.join L b).γ s`
* `substitute` reads `Token.outParam n` as `#n` counted from 1: job 1,
  please confirm.

## Job 3 (soundness)

* Published `lean/Satex/Soundness.lean` (namespace `Satex.Soundness`, imported from `lean/Satex.lean`). Not built: there is no toolchain.
* Item statements as proven: 1 `aval_join_sound L hull` (needs `HullSound`), 3 `assign_global_sound` / `assign_local_sound` (local needs `a.curLevel = s.curLevel`) / `define_sound`, 6 `branch_sound` (test hypothesis per answer; narrowing only when `test a = none`), 10 `astate_join_sound`, loops: `loopHead_sound`.
* Requests to job 2 (see PROOF-STATUS.md):
  - P2: make `AState.unsave` push one backed-up list, as `Concrete.unsave` does.
  - P3: `TClass.admits` should not apply `hex` to signs.
  - P4: `AVal.γ` for toks should require balanced text, or the hull should drop `balanced`.
  - P5: `summaryHavoc` must also havoc levels and save entries of written locations.
  - P1: `assign` under `many` frames.
