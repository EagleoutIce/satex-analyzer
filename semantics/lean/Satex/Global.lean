/-
Satex.Global: the main theorem with no free assumption. The abstract
reader `read` is fixed. It takes the next token from a token list or a
file (Satex.Mouth) and runs the command's transfer. Where it cannot follow
TeX's reading it says so (`none`), and the run goes to `lost`, the named
havoc that covers every state. Everything else is proven.

Written with AI assistance from Claude Code (Claude Opus 5.5), directed and
reviewed by the author.
-/
import Lean
import Satex.Conds

namespace Satex.Soundness

open Satex Satex.Abstract

/-! ## 1. Group codes and used-up lists the reader knows -/

/-- The code of the innermost group (§ 1064 `cur_group`) if the abstract
save stack decides it: a plain frame, or none at all (`bottom`).
source: tex.web § 1064 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L20648; SaTeX `Env::group_kind` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/env.rs -/
def atopGroup : List ASave → Option GroupCode
  | [] => some .bottom
  | .group c none :: _ => some c
  | .group _ (some _) :: _ => none
  | _ :: rest => atopGroup rest

theorem atopGroup_sound : ∀ {as : List ASave} {cs : List SaveEntry}, SaveRel as cs →
    ∀ {c : GroupCode}, atopGroup as = some c → Concrete.topGroup.go cs = c := by
  intro as cs h
  induction h with
  | nil => intro c hc; simp only [atopGroup, Option.some.injEq] at hc; subst hc; rfl
  | restore _ _ _ ih => intro c hc; exact ih hc
  | touched _ ih => intro c hc; exact ih hc
  | skip _ _ ih => intro c hc; exact ih hc
  | retain _ ih => intro c hc; exact ih hc
  | insertToken _ ih => intro c hc; exact ih hc
  | group _ _ =>
    intro c hc; simp only [atopGroup, Option.some.injEq] at hc; subst hc; rfl
  | manyNone _ _ => intro c hc; simp [atopGroup] at hc
  | manyMore _ _ _ => intro c hc; simp [atopGroup] at hc

/-- A group on top (§ 281): `unsave` finds its boundary. -/
theorem unsaveAux_top : ∀ (cs : List SaveEntry) (E : Loc → Entry) (ins : TokList) (c : GroupCode),
    Concrete.topGroup.go cs = c → c ≠ .bottom →
    ∃ E' ins' ctx crest, Concrete.unsaveAux E ins cs = (E', ins', some (c, ctx), crest)
  | [], _, _, c, h, hb => by simp only [Concrete.topGroup.go] at h; exact absurd h.symm hb
  | .group c' ctx _ :: rest, E, ins, c, h, _ => by
    simp only [Concrete.topGroup.go] at h; subst h
    exact ⟨E, ins, ctx, rest, rfl⟩
  | .restore l old :: rest, E, ins, c, h, hb => by
    simp only [Concrete.topGroup.go] at h
    simp only [Concrete.unsaveAux]
    exact unsaveAux_top rest _ ins c h hb
  | .retain _ :: rest, E, ins, c, h, hb => by
    simp only [Concrete.topGroup.go] at h
    simp only [Concrete.unsaveAux]
    exact unsaveAux_top rest E ins c h hb
  | .insertToken t :: rest, E, ins, c, h, hb => by
    simp only [Concrete.topGroup.go] at h
    simp only [Concrete.unsaveAux]
    exact unsaveAux_top rest E (t :: ins) c h hb

/-- `popDone` (§ 390) over abstract input, where it is decided.
-- source: tex.web § 390 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L7978
-- source: SaTeX `expand_macro` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/analysis/machine.rs -/
def apopDone : List AInput → Option (List AInput)
  | .toks k [] :: rest => if k = .vTemplate then some (.toks k [] :: rest) else apopDone rest
  | .toks k (.tok t :: xs) :: rest => some (.toks k (.tok t :: xs) :: rest)
  | .file f :: rest => some (.file f :: rest)
  | [] => some []
  | _ => none

theorem apopDone_sound : ∀ (ai : List AInput) (ci : List InputLevel) (ai' : List AInput),
    InputRel ai ci → apopDone ai = some ai' → InputRel ai' (Concrete.popDone ci)
  | [], [], _, _, h => by simp only [apopDone, Option.some.injEq] at h; subst h; trivial
  | .toks k [] :: rest, .toks k' p :: crest, ai', hr, h => by
    obtain ⟨⟨hk, hp⟩, hr'⟩ := hr
    simp only [AText.γ] at hp
    subst hk hp
    simp only [apopDone] at h
    by_cases e : k = .vTemplate
    · simp only [e, ↓reduceIte, Option.some.injEq] at h
      subst h e
      simp only [Concrete.popDone, ↓reduceIte]
      exact ⟨⟨rfl, rfl⟩, hr'⟩
    · simp only [e, ↓reduceIte] at h
      simp only [Concrete.popDone, e, ↓reduceIte]
      exact apopDone_sound rest crest ai' hr' h
  | .toks k (.tok t :: xs) :: rest, .toks k' p :: crest, ai', hr, h => by
    simp only [apopDone, Option.some.injEq] at h
    subst h
    obtain ⟨⟨hk, p1, p2, he, h1, h2⟩, hr'⟩ := hr
    simp only [AItem.γ] at h1
    subst h1 he
    exact ⟨⟨hk, [t], p2, rfl, rfl, h2⟩, hr'⟩
  | .file f :: rest, .file f' :: crest, ai', hr, h => by
    simp only [apopDone, Option.some.injEq] at h
    subst h
    exact hr
  | .toks _ (.hole _ :: _) :: _, _, _, _, h => by simp [apopDone] at h
  | .toks _ (.digits _ :: _) :: _, _, _, _, h => by simp [apopDone] at h
  | .toks _ (.oneOf _ :: _) :: _, _, _, _, h => by simp [apopDone] at h
  | .toks _ _ :: _, .file _ :: _, _, hr, _ => hr.1.elim
  | .file _ :: _, .toks _ _ :: _, _, hr, _ => hr.1.elim
  | [], _ :: _, _, hr, _ => hr.elim
  | _ :: _, [], _, hr, _ => hr.elim

/-! ## 2. The reader

`read` takes the next token (`GState.next`) and runs the command's transfer.
It answers `none` where it does not follow: the run goes to `lost`. -/

/-- Primitive names some rules match by name (alignments, § 1046): the
reader does not follow them. -/
def byName : List String :=
  ["halign", "valign", "noalign", "cr", "crcr", "span", "omit"] ++ Concrete.mathOnly ++ Concrete.nonMath

def bodyName : Token → CsRef
  | .cs r => r
  | .noexp r => r
  | _ => .name ""

/-- The macro `\def` stores (§ 1218): prefix bits 1 `\long`, 2 `\outer`,
8 `\protected`, no parameters. -/
def macroOf (p : Nat) (body : TokList) : Macro :=
  { long := Nat.testBit p 0, outer := Nat.testBit p 1, prot := Nat.testBit p 3, params := [],
    braceDelim := false, brace := .char 123 .bgroup, body := body }

/-- The command of a known meaning. `g1` is the state after the token. -/
def readCmd (L : Limits) (g : GState) (t : Token) (g1 : GState) : Meaning → Option (List GState)
  /- A macro without parameters (§ 389): its body comes next.
  source: tex.web § 389 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L7938; SaTeX `expand_macro` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/analysis/machine.rs -/
  | .call mc =>
    if mc.params = [] ∧ mc.braceDelim = false then
      match apopDone g1.a.input with
      | some ai2 =>
        let body : AText := AText.ofList (Concrete.subst [] mc.body ++ [])
        some [{ g1 with a := { g1.a with input := AInput.toks (.macroBody (bodyName t)) body :: ai2 } }]
      | none => none
    else none
  /- "Undefined control sequence" (§ 370): an error, nothing else.
  source: tex.web § 370 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L7723; SaTeX `expand_macro` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/analysis/machine.rs -/
  | .undefined => some [g1]
  | .prim n c =>
    if n ∈ byName then none else
    match c with
    /- § 1045: `\relax` does nothing; with prefixes pending it is skipped (§ 1211).
    source: tex.web § 1045 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L20377, § 1211 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L22720; SaTeX `drop_prefixes` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs -/
    | .relax => some [g1]
    /- § 1211: a prefix is kept.
    source: tex.web § 1211 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L22720; SaTeX `Machine::prefixes`, `take_prefixes` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/analysis/machine.rs -/
    | .pfx p => some [{ g1 with pfx := Nat.lor g.pfx p }]
    /- `\begingroup` (§ 1063).
    source: tex.web § 1063 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L20637; SaTeX `Env::push_group` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/env.rs -/
    | .beginGroup => if g.pfx = 0 then some [{ g1 with a := g1.a.beginGroup .semiSimple }] else none
    /- `\endgroup` (§ 1069): closes a semi-simple group; at the bottom an error.
    source: tex.web § 1069 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L20735, § 281 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L6035; SaTeX `Env::pop_group` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/env.rs -/
    | .endGroup =>
      if g.pfx = 0 then
        match atopGroup g.a.save with
        | some .semiSimple => some ((g1.a.endGroup L .semiSimple).map fun b => { g1 with a := b })
        | some .bottom => some [g1]
        | _ => none
      else none
    /- `\aftergroup` (§ 1271): the token goes on the save stack above level one.
    source: tex.web § 1271 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L23490; SaTeX `defer_after_group` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/analysis/machine.rs -/
    | .aftergroup =>
      if g.pfx = 0 ∧ g.a.pinned = true then
        match g1.next with
        | some (u, g2) =>
          some [{ g2 with a := if g.a.curLevel > 1 then
                    { g2.a with save := .insertToken u.stored :: g2.a.save }
                  else g2.a }]
        | none => none
      else none
    /- `\afterassignment` (§ 1269): the token is kept.
    source: tex.web § 1269 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L23473; SaTeX `Machine::after_assignment` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/analysis/machine.rs -/
    | .afterassignment =>
      if g.pfx = 0 then
        match g1.next with
        | some (u, g2) => some [{ g2 with after := some u.stored }]
        | none => none
      else none
    /- `\let` (§ 1221) with a right side the reader follows. Only `\global`
    may be pending: `\long`, `\outer` and `\protected` make it an error (§ 1213).
    source: tex.web § 1221 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L22838; SaTeX `primitive_let` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs -/
    | .letCmd false =>
      if Nat.land g.pfx 11 = 0 then
        match g1.letOps with
        | some (r, m, g3) =>
          match g3.define (Nat.testBit g3.pfx 2) (.meaning r) (.meanings [.exact m]) with
          | some g4 => some [g4]
          | none => none
        | none => none
      else none
    /- `\def`, `\gdef` (§ 1218) without parameters: the prefixes go into the macro.
    source: tex.web § 1218 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web; SaTeX `primitive_def` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs -/
    | .defCmd glob false =>
      match g1.defOps with
      | some (r, body, g4) =>
        match g4.define (glob || Nat.testBit g4.pfx 2) (.meaning r)
            (.meanings [.exact (.call (macroOf g4.pfx body))]) with
        | some g5 => some [g5]
        | none => none
      | none => none
    /- `\endinput` (§ 378): `force_eof` is set, the file ends at its line end (§ 362).
    source: tex.web § 378 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L7807; SaTeX `take_pending_eof` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/analysis/machine.rs -/
    | .endinput => some [{ g1 with feof := true }]
    /- "Extra \endcsname" (§ 1135): an error, nothing else.
    source: tex.web § 1135 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web; SaTeX `execute` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs -/
    | .endcsname => if g.pfx = 0 then some [g1] else none
    /- An integer parameter (§ 1228) set to explicit decimal digits.
    source: tex.web § 1228 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web; SaTeX `scan_register_value` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs -/
    | .intPar ip =>
      if Nat.land g.pfx 11 = 0 then
        match g1.optEquals with
        | some g2 =>
          match g2.scanInt with
          | some (nv, g3) =>
            match g3.define (Nat.testBit g3.pfx 2) (.intPar ip) (.ints [Itv.exact nv]) with
            | some g4 => some [g4]
            | none => none
          | none => none
        | none => none
      else none
    /- `\iftrue`, `\iffalse` (§§ 498, 500): a level is pushed; a false test skips
    to its `\else` or `\fi` (Satex.Conds). The stack must be exact.
    source: tex.web § 498 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web; SaTeX `conditional` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs -/
    | .ifTest op =>
      if condExact g.a.cond = true then
        match op with
        | .iftrue => some [{ g1 with a := { g1.a with cond := ⟨.elseCode, .iftrue, false⟩ :: g1.a.cond } }]
        | .iffalse =>
          match askip (isize g1.a.input + 1)
              { g1 with a := { g1.a with cond := ⟨.ifCode, .iffalse, false⟩ :: g1.a.cond } } with
          | some (.fi, g2) => some [{ g2 with a := { g2.a with cond := g1.a.cond } }]
          | some (.elseCode, g2) =>
            some [{ g2 with a := { g2.a with cond := ⟨.fiCode, .iffalse, false⟩ :: g1.a.cond } }]
          | _ => none
        | _ => none
      else none
    /- `\fi`, `\else`, `\or` (§ 510) of a known level: `\fi` pops it, `\else`
    and `\or` skip to the `\fi` first.
    source: tex.web § 510 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web; SaTeX `conditional_else`, `conditional_fi` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs -/
    | .fiOrElse fc =>
      match g.a.cond with
      | k :: ks =>
        if k.many = false ∧ k.limit ≠ .ifCode ∧ fc.toNat ≤ k.limit.toNat then
          if fc = .fi then some [{ g1 with a := { g1.a with cond := ks } }]
          else
            match askipFi (isize g1.a.input + 1) g1 with
            | some g2 => some [{ g2 with a := { g2.a with cond := ks } }]
            | none => none
        else none
      | [] => none
    /- `\catcode`, `\lccode`, … (§ 1232) with explicit digits or an alphabetic
    constant. An invalid code is an error and gives 0.
    source: tex.web § 1232 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L23040; SaTeX `assign_char_code` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs -/
    | .codeTable tab =>
      if Nat.land g.pfx 11 = 0 then
        match g1.scanInt with
        | some (ch, g2) =>
          match g2.optEquals with
          | some g3 =>
            match g3.scanInt with
            | some (v, g4) =>
              match acodeOk tab v with
              | some b =>
                match g4.define (Nat.testBit g4.pfx 2) (.code tab ch.toNat)
                    (.ints [Itv.exact (if b then v else 0)]) with
                | some g5 => some [g5]
                | none => none
              | none => none
            | none => none
          | none => none
        | none => none
      else none
    /- `\count n = v` (§ 1224) with explicit decimal digits.
    source: tex.web § 1224 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L22902; SaTeX `scan_register_value` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs -/
    | .register .count =>
      if Nat.land g.pfx 11 = 0 then
        match g1.scanInt with
        | some (i, g2) =>
          match g2.optEquals with
          | some g3 =>
            match g3.scanInt with
            | some (nv, g4) =>
              match g4.define (Nat.testBit g4.pfx 2) (.reg .count i.toNat) (.ints [Itv.exact nv]) with
              | some g5 => some [g5]
              | none => none
            | none => none
          | none => none
        | none => none
      else none
    | _ => none
  /- `{` (§ 1063) outside math.
  source: tex.web § 1063 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L20637; SaTeX `Env::push_group` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/env.rs -/
  | .char _ .bgroup =>
    if g.pfx = 0 ∧ ModeKind.math ∉ g.a.modes then
      some [{ g1 with a := g1.a.beginGroup .simple }]
    else none
  /- `}` (§§ 1068–1069): a simple group closes; bottom, semi-simple and math
  groups give an error. Box and list groups: not followed.
  source: tex.web § 1068 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L20715, § 1069 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L20735; SaTeX `Env::pop_group` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/env.rs -/
  | .char _ .egroup =>
    if g.pfx = 0 then
      match atopGroup g.a.save with
      | some .simple => some ((g1.a.endGroup L .simple).map fun b => { g1 with a := b })
      | some .bottom | some .semiSimple | some .mathShift | some .mathLeft => some [g1]
      | _ => none
    else none
  /- A space (§ 1045): glue in a paragraph, nothing elsewhere. `γ` does not
  see the list, so the state after the token covers every case.
  source: tex.web § 1045 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L20377; SaTeX `space_glue` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs -/
  | .char _ .space => some [g1]
  | _ => none

/-- SaTeX's abstract reader: the next token, its known meaning, the
command's transfer. `none`: the reader does not follow (unknown text or a
catcode it does not know next, an unknown meaning, a command without a
transfer here). -/
def read (L : Limits) (g : GState) : Option (List GState) :=
  match g.next with
  | some (t, g1) =>
    match ameaning g.a t with
    | some m => readCmd L g t g1 m
    | none => none
  | none => none

/-! ## 3. The reader covers TeX's step -/

theorem topGroup_eq (s : State) : Concrete.topGroup s = Concrete.topGroup.go s.save := rfl

/-- `unsave` closes the group `topGroup` names (§ 281). -/
theorem unsaveAux_code : ∀ (cs : List SaveEntry) (E : Loc → Entry) (ins : TokList),
    ((Concrete.unsaveAux E ins cs).2.2.1 = none ∧ Concrete.topGroup.go cs = .bottom) ∨
      ∃ ctx, (Concrete.unsaveAux E ins cs).2.2.1 = some (Concrete.topGroup.go cs, ctx)
  | [], _, _ => Or.inl ⟨rfl, rfl⟩
  | .group c ctx _ :: _, _, _ => Or.inr ⟨ctx, rfl⟩
  | .restore l old :: rest, E, ins => by
    exact unsaveAux_code rest _ ins
  | .retain _ :: rest, E, ins => by
    exact unsaveAux_code rest E ins
  | .insertToken t :: rest, E, ins => by
    exact unsaveAux_code rest E (t :: ins)

theorem unsave_code (s : State) {c : GroupCode} {ctx : Option BoxContext}
    (h : (Concrete.unsave s).2 = some (c, ctx)) : c = Concrete.topGroup s := by
  rw [topGroup_eq]
  have hc := unsaveAux_code s.save s.eqtb []
  unfold Concrete.unsave at h
  revert h hc
  generalize Concrete.unsaveAux s.eqtb [] s.save = r
  obtain ⟨E, ins, o, rest⟩ := r
  intro h hc
  simp only at h hc
  subst h
  rcases hc with ⟨h1, _⟩ | ⟨ctx', h1⟩
  · cases h1
  · simp only [Option.some.injEq, Prod.mk.injEq] at h1
    exact h1.1

theorem unsave_pending (s : State) : (Concrete.unsave s).1.pending = s.pending := by
  unfold Concrete.unsave
  split
  rename_i E ins g rest _
  dsimp only
  split <;> rfl

theorem unsave_forceEof (s : State) : (Concrete.unsave s).1.forceEof = s.forceEof := by
  unfold Concrete.unsave
  split
  rename_i E ins g rest _
  dsimp only
  split <;> rfl

/-- A list item is not in `γ`: only the mode's kind is, and it stays. -/
theorem appendItem_γ {g : GState} {s : State} (i : Item) (hg : g.γ s) :
    g.γ (Concrete.appendItem i s) := by
  unfold Concrete.appendItem
  split
  · rename_i l rest hn
    obtain ⟨⟨h1, h2, h3, h4, h5, h6, h7⟩, hp⟩ := hg
    refine ⟨⟨h1, h2, h3, h4, h5, h6, ?_⟩, hp⟩
    simp only [State.mode, hn] at h7 ⊢
    exact h7
  · exact hg

/-- Closing a group of a code the abstract save stack names. -/
theorem close_sound (L : Limits) {g : GState} {s : State} {c : GroupCode} (hg : g.γ s)
    (htop : atopGroup g.a.save = some c) (hb : c ≠ .bottom) :
    ∃ b, b ∈ (g.a.endGroup L c).map (fun b => ({ g with a := b } : GState)) ∧
      b.γ (Concrete.unsave s).1 := by
  have ht := atopGroup_sound hg.1.2.2.2.1 htop
  obtain ⟨E', ins, ctx, crest, hu⟩ := unsaveAux_top s.save s.eqtb [] c ht hb
  obtain ⟨b, hb', hγ⟩ := endGroup_sound L hg.1 hu
  obtain ⟨hp, hq, hf⟩ := hg.2
  refine ⟨{ g with a := b }, List.mem_map.mpr ⟨b, hb', rfl⟩, hγ, ?_, ?_, ?_⟩
  · rw [unsave_pending]; exact hp
  · rw [unsave_pending]; exact hq
  · rw [unsave_forceEof]; exact hf

theorem begin_sound {g : GState} {s : State} (c : GroupCode) (hg : g.γ s) :
    ({ g with a := g.a.beginGroup c } : GState).γ (Concrete.newSaveLevel c none s) :=
  ⟨beginGroup_sound hg.1 c none, hg.2⟩

open Lean Meta Elab Tactic in
/-- Revert the first hypothesis whose type unifies with `t`; fail if none. -/
elab "revert_hyp " t:term : tactic => withMainContext do
  let ty ← elabTerm t none (mayPostpone := true)
  for ld in (← getLCtx) do
    if ld.isImplementationDetail then continue
    let st ← saveState
    if ← isDefEq ld.type ty then
      liftMetaTactic fun g => do
        let (_, g') ← g.revert #[ld.fvarId]
        return [g']
      return
    st.restore
  throwError "revert_hyp: no hypothesis"

/- The contradiction cases: a rule whose head token's meaning is not the
one the reader saw, or whose input is drained. -/
syntax "hcSimp" : tactic
macro_rules
  | `(tactic| hcSimp) => `(tactic|
      (try subst_vars); simp_all [Concrete.cmdOf, Concrete.expandableTok, Concrete.spacerTok, Meaning.expandable,
         Cmd.expandable, byName, Concrete.charOfMeaning, Concrete.isSpacer, Concrete.isRelax,
         Concrete.nameOfMeaning, Concrete.assignmentMeaning, Concrete.mathOnly, Concrete.nonMath,
         topGroup_eq, Concrete.isExtension, Concrete.isBoxCmd, Concrete.longMisplaced])
syntax "hcD " ident ident term : tactic
macro_rules
  | `(tactic| hcD $hg $ha $s) => `(tactic|
      (revert_hyp Concrete.Drained $s
       intro hd
       exact absurd $ha (gnext_not_drained hd $hg)))
syntax "hcNP " ident ident ident term : tactic
macro_rules
  | `(tactic| hcNP $hg $ha $hm $s) => `(tactic|
      (revert_hyp Concrete.HeadNP $s _ _
       rintro ⟨⟨hn, hxe, hlm⟩, hp0⟩
       obtain ⟨rfl, hs1, hg1⟩ := gnext_sound $hg hn $ha
       have hm1 := ameaning_sound hg1.1 ((ameaning_input _ _ _).trans $hm)
       have hsv := congrArg State.save hs1
       have hnv := congrArg State.nest hs1
       have hpf := $hg.2.1
       have hmd := hg1.1.2.2.2.2.2.2
       exfalso
       clear hg1 $hg $ha hs1
       hcSimp))
syntax "hcH " ident ident ident term : tactic
macro_rules
  | `(tactic| hcH $hg $ha $hm $s) => `(tactic|
      (revert_hyp Concrete.Head $s _ _
       rintro ⟨hn, hxe, hlm⟩
       obtain ⟨rfl, hs1, hg1⟩ := gnext_sound $hg hn $ha
       have hm1 := ameaning_sound hg1.1 ((ameaning_input _ _ _).trans $hm)
       have hsv := congrArg State.save hs1
       have hnv := congrArg State.nest hs1
       have hpf := $hg.2.1
       have hmd := hg1.1.2.2.2.2.2.2
       exfalso
       clear hg1 $hg $ha hs1
       hcSimp))
syntax "hcN " ident ident ident term : tactic
macro_rules
  | `(tactic| hcN $hg $ha $hm $s) => `(tactic|
      (revert_hyp Concrete.Next $s _ _
       intro hn
       obtain ⟨rfl, hs1, hg1⟩ := gnext_sound $hg hn $ha
       have hm1 := ameaning_sound hg1.1 ((ameaning_input _ _ _).trans $hm)
       have hsv := congrArg State.save hs1
       have hnv := congrArg State.nest hs1
       have hpf := $hg.2.1
       have hmd := hg1.1.2.2.2.2.2.2
       exfalso
       clear hg1 $hg $ha hs1
       hcSimp))
syntax "hc " ident ident ident term : tactic
macro_rules
  | `(tactic| hc $hg $ha $hm $s) => `(tactic|
      first | hcD $hg $ha $s | hcNP $hg $ha $hm $s | hcH $hg $ha $hm $s | hcN $hg $ha $hm $s)
syntax "hcE " ident ident ident term : tactic
macro_rules
  | `(tactic| hcE $hg $ha $hm $s) => `(tactic|
      first
      | hc $hg $ha $hm $s
      | (revert_hyp Concrete.TheToks _ $s _ _; intro hT; cases hT <;> hc $hg $ha $hm $s)
      | (revert_hyp Concrete.CondHead $s _ _ _ _; intro hC; cases hC <;> hc $hg $ha $hm $s))
/-- The covered cases: the head token, the state after it. -/
syntax "hd " ident ident ident ident ident term : tactic
macro_rules
  | `(tactic| hd $hg $ha $hm $hg1 $hm1 $s) => `(tactic|
      first
      | (revert_hyp Concrete.HeadNP $s _ _
         rintro ⟨⟨hn, -, -⟩, -⟩
         obtain ⟨rfl, -, $hg1⟩ := gnext_sound $hg hn $ha
         have $hm1 := ameaning_sound ($hg1).1 ((ameaning_input _ _ _).trans $hm))
      | (revert_hyp Concrete.Head $s _ _
         rintro ⟨hn, -, -⟩
         obtain ⟨rfl, -, $hg1⟩ := gnext_sound $hg hn $ha
         have $hm1 := ameaning_sound ($hg1).1 ((ameaning_input _ _ _).trans $hm))
      | (revert_hyp Concrete.Next $s _ _
         intro hn
         obtain ⟨rfl, -, $hg1⟩ := gnext_sound $hg hn $ha
         have $hm1 := ameaning_sound ($hg1).1 ((ameaning_input _ _ _).trans $hm)))

theorem bodyName_eq (t : Token) : TokListKind.macroBody (bodyName t) =
    TokListKind.macroBody (match t with | .cs r => r | .noexp r => r | _ => .name "") := by
  cases t <;> rfl

set_option maxHeartbeats 20000000 in
theorem step_covered (E : Concrete.Engine) (L : Limits) {g : GState} {s s' : State} {rs : List GState}
    (hg : g.γ s) (hs : Concrete.Step E s s') (hr : read L g = some rs) : ∃ g', g' ∈ rs ∧ g'.γ s' := by
  obtain ⟨-, -, hx⟩ := hs
  simp only [read] at hr
  split at hr
  case h_2 => cases hr
  rename_i t g1 ha
  obtain ⟨ai, fe, rfl⟩ := next_eq ha
  split at hr
  case h_2 => cases hr
  rename_i m hm
  cases m
  case prim n c =>
    simp only [readCmd] at hr
    split at hr
    · cases hr
    rename_i hnb
    split at hr
    · -- \relax
      simp only [Option.some.injEq] at hr; subst hr
      cases hx
      case relax =>
        hd hg ha hm hg1 hm1 s
        exact ⟨_, List.mem_singleton.mpr rfl, hg1⟩
      case prefixSkip =>
        hd hg ha hm hg1 hm1 s
        exact ⟨_, List.mem_singleton.mpr rfl, hg1⟩
      case expand => cases ‹Concrete.Expand E s s'› <;> hcE hg ha hm s
      all_goals hc hg ha hm s
    · -- a prefix
      rename_i p
      simp only [Option.some.injEq] at hr; subst hr
      cases hx
      case prefix_ =>
        rename_i s1 t' p' hh hcp
        obtain ⟨hn, -, -⟩ := hh
        obtain ⟨rfl, -, hg1⟩ := gnext_sound hg hn ha
        have hm1 := ameaning_sound hg1.1 ((ameaning_input _ _ _).trans hm)
        rw [Concrete.cmdOf, hm1] at hcp
        cases hcp
        exact ⟨_, List.mem_singleton.mpr rfl, hg1.1, by simp [hg1.2.1], hg1.2.2⟩
      case expand => cases ‹Concrete.Expand E s s'› <;> hcE hg ha hm s
      all_goals hc hg ha hm s
    · -- \begingroup
      split at hr
      · simp only [Option.some.injEq] at hr; subst hr
        cases hx
        case begingroup =>
          hd hg ha hm hg1 hm1 s
          exact ⟨_, List.mem_singleton.mpr rfl, begin_sound _ hg1⟩
        case expand => cases ‹Concrete.Expand E s s'› <;> hcE hg ha hm s
        all_goals hc hg ha hm s
      · cases hr
    · -- \endgroup
      split at hr
      · split at hr
        · rename_i htop
          simp only [Option.some.injEq] at hr; subst hr
          have htg := atopGroup_sound hg.1.2.2.2.1 htop
          cases hx
          case endgroup =>
            hd hg ha hm hg1 hm1 s
            exact close_sound L hg1 htop (by decide)
          case expand => cases ‹Concrete.Expand E s s'› <;> hcE hg ha hm s
          all_goals hc hg ha hm s
        · rename_i htop
          simp only [Option.some.injEq] at hr; subst hr
          have htg := atopGroup_sound hg.1.2.2.2.1 htop
          cases hx
          case endgroupBottom =>
            hd hg ha hm hg1 hm1 s
            exact ⟨_, List.mem_singleton.mpr rfl, raise_γ _ hg1⟩
          case expand => cases ‹Concrete.Expand E s s'› <;> hcE hg ha hm s
          all_goals hc hg ha hm s
        · cases hr
      · cases hr
    · -- \aftergroup
      split at hr
      · rename_i hcond
        split at hr
        · rename_i u g2 hai2
          obtain ⟨ai2, fe2, rfl⟩ := next_eq hai2
          simp only [Option.some.injEq] at hr; subst hr
          cases hx
          case aftergroup =>
            hd hg ha hm hg1 hm1 s
            revert_hyp Concrete.Next _ _ _
            intro hn2
            obtain ⟨rfl, hs2, hg2⟩ := gnext_sound hg1 hn2 hai2
            refine ⟨_, List.mem_singleton.mpr rfl, ?_⟩
            have hcl := hg2.1.2.2.1 hcond.2
            obtain ⟨⟨h1, h2, h3, h4, h5, h6, h7⟩, hp, hq⟩ := hg2
            simp only at hcl
            split <;> split
            · exact ⟨⟨h1, h2, h3, SaveRel.insertToken h4, h5, h6, h7⟩, hp, hq⟩
            · omega
            · omega
            · exact ⟨⟨h1, h2, h3, h4, h5, h6, h7⟩, hp, hq⟩
          case expand => cases ‹Concrete.Expand E s s'› <;> hcE hg ha hm s
          all_goals hc hg ha hm s
        · cases hr
      · cases hr
    · -- \afterassignment
      split at hr
      · split at hr
        · rename_i u g2 hai2
          obtain ⟨ai2, fe2, rfl⟩ := next_eq hai2
          simp only [Option.some.injEq] at hr; subst hr
          cases hx
          case afterassignment =>
            hd hg ha hm hg1 hm1 s
            revert_hyp Concrete.Next _ _ _
            intro hn2
            obtain ⟨rfl, hs2, hg2⟩ := gnext_sound hg1 hn2 hai2
            exact ⟨_, List.mem_singleton.mpr rfl, hg2.1, hg2.2.1, rfl, hg2.2.2.2⟩
          case expand => cases ‹Concrete.Expand E s s'› <;> hcE hg ha hm s
          all_goals hc hg ha hm s
        · cases hr
      · cases hr
    · -- \let
      split at hr
      · rename_i hpf11
        split at hr
        · rename_i r mu g3 hops
          split at hr
          · rename_i g4 hdef
            simp only [Option.some.injEq] at hr; subst hr
            cases hx
            case let_ =>
              rename_i s1 s2 s3 s4 t' u r' hh hcmd hrt hrhs hs4
              obtain ⟨hn, -, -⟩ := hh
              obtain ⟨rfl, -, hg1⟩ := gnext_sound hg hn ha
              obtain ⟨rfl, hmu, hg3⟩ := letOps_sound hg1 hrt hrhs hops
              subst hs4
              refine ⟨_, List.mem_singleton.mpr rfl, ?_⟩
              unfold Concrete.assignTo Concrete.pfxGlobal
              rw [hg3.2.1]
              exact gdefine_sound (v := .meanings [.exact mu]) hg3
                ⟨_, _, List.mem_singleton.mpr rfl, rfl, hmu⟩ hdef
            case expand => cases ‹Concrete.Expand E s s'› <;> hcE hg ha hm s
            all_goals hc hg ha hm s
          · cases hr
        · cases hr
      · cases hr
    · -- \def
      rename_i glob
      split at hr
      · rename_i r body g4 hops
        split at hr
        · rename_i g5 hdef
          simp only [Option.some.injEq] at hr; subst hr
          cases hx
          case def_ =>
            rename_i s1 s2 s3 s4 t' glob' r' ps b body' hh hcmd hrt hpt hdb
            obtain ⟨hn, -, -⟩ := hh
            obtain ⟨rfl, -, hg1⟩ := gnext_sound hg hn ha
            have hm1 := ameaning_sound hg1.1 ((ameaning_input _ _ _).trans hm)
            rw [Concrete.cmdOf, hm1] at hcmd
            cases hcmd
            obtain ⟨rfl, rfl, rfl, rfl, hg4⟩ := defOps_sound hg1 hrt hpt hdb hops
            refine ⟨_, List.mem_singleton.mpr rfl, ?_⟩
            unfold Concrete.pfxGlobal
            rw [hg4.2.1]
            exact gdefine_sound (v := .meanings [.exact (.call (macroOf g4.pfx body'))]) hg4
              ⟨_, _, List.mem_singleton.mpr rfl, rfl, rfl⟩ hdef
          case expand => cases ‹Concrete.Expand E s s'› <;> hcE hg ha hm s
          all_goals hc hg ha hm s
        · cases hr
      · cases hr
    · -- \endinput
      simp only [Option.some.injEq] at hr; subst hr
      cases hx
      case expand =>
        cases ‹Concrete.Expand E s s'›
        case endinput =>
          revert_hyp Concrete.Next s _ _
          intro hn
          obtain ⟨rfl, -, hg1⟩ := gnext_sound hg hn ha
          exact ⟨_, List.mem_singleton.mpr rfl, hg1.1, hg1.2.1, hg1.2.2.1, rfl⟩
        all_goals hcE hg ha hm s
      all_goals hc hg ha hm s
    · -- \endcsname
      split at hr
      · simp only [Option.some.injEq] at hr; subst hr
        cases hx
        case endcsname =>
          hd hg ha hm hg1 hm1 s
          exact ⟨_, List.mem_singleton.mpr rfl, raise_γ _ hg1⟩
        case expand => cases ‹Concrete.Expand E s s'› <;> hcE hg ha hm s
        all_goals hc hg ha hm s
      · cases hr
    · -- an integer parameter
      split at hr
      · split at hr
        · rename_i g2 hoe
          split at hr
          · rename_i nv g3 hsi
            split at hr
            · rename_i g4 hdef
              simp only [Option.some.injEq] at hr; subst hr
              cases hx
              case intPar =>
                rename_i s1 s2 s3 t' ip' v hh hcmd hoe' hval
                obtain ⟨hn, -, -⟩ := hh
                obtain ⟨rfl, -, hg1⟩ := gnext_sound hg hn ha
                have hm1 := ameaning_sound hg1.1 ((ameaning_input _ _ _).trans hm)
                rw [Concrete.cmdOf, hm1] at hcmd
                cases hcmd
                have hg2 := optEquals_sound hg1 hoe' hoe
                cases hval with
                | int hsc =>
                  obtain ⟨rfl, hg3⟩ := scanInt_sound hg2 hsc hsi
                  refine ⟨_, List.mem_singleton.mpr rfl, ?_⟩
                  unfold Concrete.assignTo Concrete.pfxGlobal
                  rw [hg3.2.1]
                  exact gdefine_sound (v := .ints [Itv.exact _]) hg3
                    ⟨_, _, List.mem_singleton.mpr rfl, rfl, Int.le_refl _, Int.le_refl _⟩ hdef
              case expand => cases ‹Concrete.Expand E s s'› <;> hcE hg ha hm s
              all_goals hc hg ha hm s
            · cases hr
          · cases hr
        · cases hr
      · cases hr
    · -- \iftrue, \iffalse
      split at hr
      · rename_i hex
        split at hr
        · -- \iftrue
          simp only [Option.some.injEq] at hr; subst hr
          cases hx
          case expand =>
            cases ‹Concrete.Expand E s s'›
            case ifTrue =>
              have hch := ‹Concrete.CondHead _ _ _ _ _›
              have htest := ‹Concrete.Test _ _ _ _ _›
              have hb := ‹(_ != _) = _›
              cases hch with
              | plain hn hcmd =>
                obtain ⟨rfl, -, hg1⟩ := gnext_sound hg hn ha
                have hm1 := ameaning_sound hg1.1 ((ameaning_input _ _ _).trans hm)
                rw [Concrete.cmdOf, hm1] at hcmd
                cases hcmd
                cases htest
                case iftrue =>
                  refine ⟨_, List.mem_singleton.mpr rfl, ?_⟩
                  simp only [setLimitAt_top]
                  obtain ⟨⟨h1, h2, h3, h4, h5, h6, h7⟩, hp⟩ := hg1
                  exact ⟨⟨h1, h2, h3, h4, CondRel.one rfl rfl rfl h5, h6, h7⟩, hp⟩
                all_goals (exfalso; simp_all)
              | unless_ hn hcmd _ _ _ =>
                obtain ⟨rfl, -, hg1⟩ := gnext_sound hg hn ha
                have hm1 := ameaning_sound hg1.1 ((ameaning_input _ _ _).trans hm)
                rw [Concrete.cmdOf, hm1] at hcmd
                cases hcmd
            case ifFalse =>
              have hch := ‹Concrete.CondHead _ _ _ _ _›
              have htest := ‹Concrete.Test _ _ _ _ _›
              have hb := ‹(_ != _) = _›
              have hskip := ‹Concrete.SkipFalse _ _ _›
              cases hch with
              | plain hn hcmd =>
                obtain ⟨rfl, -, hg1⟩ := gnext_sound hg hn ha
                have hm1 := ameaning_sound hg1.1 ((ameaning_input _ _ _).trans hm)
                rw [Concrete.cmdOf, hm1] at hcmd
                cases hcmd
                cases htest
                case iftrue => simp at hb
                all_goals (exfalso; simp_all)
              | unless_ hn hcmd _ _ _ =>
                obtain ⟨rfl, -, hg1⟩ := gnext_sound hg hn ha
                have hm1 := ameaning_sound hg1.1 ((ameaning_input _ _ _).trans hm)
                rw [Concrete.cmdOf, hm1] at hcmd
                cases hcmd
            all_goals hcE hg ha hm s
          all_goals hc hg ha hm s
        · -- \iffalse
          split at hr
          · rename_i g2 hask
            simp only [Option.some.injEq] at hr; subst hr
            cases hx
            case expand =>
              cases ‹Concrete.Expand E s s'›
              case ifFalse =>
                have hch := ‹Concrete.CondHead _ _ _ _ _›
                have htest := ‹Concrete.Test _ _ _ _ _›
                have hb := ‹(_ != _) = _›
                have hskip := ‹Concrete.SkipFalse _ _ _›
                cases hch with
                | plain hn hcmd =>
                  obtain ⟨rfl, -, hg1⟩ := gnext_sound hg hn ha
                  have hm1 := ameaning_sound hg1.1 ((ameaning_input _ _ _).trans hm)
                  rw [Concrete.cmdOf, hm1] at hcmd
                  cases hcmd
                  cases htest
                  case iffalse =>
                    have hcr := hg1.1.2.2.2.2.1
                    obtain ⟨⟨h1, h2, h3, h4, h5, h6, h7⟩, hp⟩ := hg1
                    obtain ⟨s1', hg2, hc1, hfi | hel⟩ := askip_sound hskip (by simp) _ _ g2 .fi
                      (by exact ⟨⟨h1, h2, h3, h4, CondRel.one rfl rfl rfl h5, h6, h7⟩, hp⟩) hask
                    · obtain ⟨-, rfl⟩ := hfi
                      refine ⟨_, List.mem_singleton.mpr rfl, ?_⟩
                      obtain ⟨⟨k1, k2, k3, k4, -, k6, k7⟩, kp⟩ := hg2
                      refine ⟨⟨k1, k2, k3, k4, ?_, k6, k7⟩, kp⟩
                      simp only [hc1, List.drop_succ_cons, List.drop_zero]
                      exact hcr
                    · cases hel.1
                  all_goals (exfalso; simp_all)
                | unless_ hn hcmd _ _ _ =>
                  obtain ⟨rfl, -, hg1⟩ := gnext_sound hg hn ha
                  have hm1 := ameaning_sound hg1.1 ((ameaning_input _ _ _).trans hm)
                  rw [Concrete.cmdOf, hm1] at hcmd
                  cases hcmd
              case ifTrue =>
                have hch := ‹Concrete.CondHead _ _ _ _ _›
                have htest := ‹Concrete.Test _ _ _ _ _›
                have hb := ‹(_ != _) = _›
                cases hch with
                | plain hn hcmd =>
                  obtain ⟨rfl, -, hg1⟩ := gnext_sound hg hn ha
                  have hm1 := ameaning_sound hg1.1 ((ameaning_input _ _ _).trans hm)
                  rw [Concrete.cmdOf, hm1] at hcmd
                  cases hcmd
                  cases htest
                  case iffalse => simp at hb
                  all_goals (exfalso; simp_all)
                | unless_ hn hcmd _ _ _ =>
                  obtain ⟨rfl, -, hg1⟩ := gnext_sound hg hn ha
                  have hm1 := ameaning_sound hg1.1 ((ameaning_input _ _ _).trans hm)
                  rw [Concrete.cmdOf, hm1] at hcmd
                  cases hcmd
              all_goals hcE hg ha hm s
            all_goals hc hg ha hm s
          · rename_i g2 hask
            simp only [Option.some.injEq] at hr; subst hr
            cases hx
            case expand =>
              cases ‹Concrete.Expand E s s'›
              case ifFalse =>
                have hch := ‹Concrete.CondHead _ _ _ _ _›
                have htest := ‹Concrete.Test _ _ _ _ _›
                have hb := ‹(_ != _) = _›
                have hskip := ‹Concrete.SkipFalse _ _ _›
                cases hch with
                | plain hn hcmd =>
                  obtain ⟨rfl, -, hg1⟩ := gnext_sound hg hn ha
                  have hm1 := ameaning_sound hg1.1 ((ameaning_input _ _ _).trans hm)
                  rw [Concrete.cmdOf, hm1] at hcmd
                  cases hcmd
                  cases htest
                  case iffalse =>
                    have hcr := hg1.1.2.2.2.2.1
                    obtain ⟨⟨h1, h2, h3, h4, h5, h6, h7⟩, hp⟩ := hg1
                    obtain ⟨s1', hg2, hc1, hfi | hel⟩ := askip_sound hskip (by simp) _ _ g2 .elseCode
                      (by exact ⟨⟨h1, h2, h3, h4, CondRel.one rfl rfl rfl h5, h6, h7⟩, hp⟩) hask
                    · cases hfi.1
                    · obtain ⟨-, rfl⟩ := hel
                      refine ⟨_, List.mem_singleton.mpr rfl, ?_⟩
                      obtain ⟨⟨k1, k2, k3, k4, -, k6, k7⟩, kp⟩ := hg2
                      refine ⟨⟨k1, k2, k3, k4, ?_, k6, k7⟩, kp⟩
                      simp only [hc1, setLimitAt_top]
                      exact CondRel.one rfl rfl rfl hcr
                  all_goals (exfalso; simp_all)
                | unless_ hn hcmd _ _ _ =>
                  obtain ⟨rfl, -, hg1⟩ := gnext_sound hg hn ha
                  have hm1 := ameaning_sound hg1.1 ((ameaning_input _ _ _).trans hm)
                  rw [Concrete.cmdOf, hm1] at hcmd
                  cases hcmd
              case ifTrue =>
                have hch := ‹Concrete.CondHead _ _ _ _ _›
                have htest := ‹Concrete.Test _ _ _ _ _›
                have hb := ‹(_ != _) = _›
                cases hch with
                | plain hn hcmd =>
                  obtain ⟨rfl, -, hg1⟩ := gnext_sound hg hn ha
                  have hm1 := ameaning_sound hg1.1 ((ameaning_input _ _ _).trans hm)
                  rw [Concrete.cmdOf, hm1] at hcmd
                  cases hcmd
                  cases htest
                  case iffalse => simp at hb
                  all_goals (exfalso; simp_all)
                | unless_ hn hcmd _ _ _ =>
                  obtain ⟨rfl, -, hg1⟩ := gnext_sound hg hn ha
                  have hm1 := ameaning_sound hg1.1 ((ameaning_input _ _ _).trans hm)
                  rw [Concrete.cmdOf, hm1] at hcmd
                  cases hcmd
              all_goals hcE hg ha hm s
            all_goals hc hg ha hm s
          · cases hr
        · cases hr
      · cases hr
    · -- \fi, \else, \or
      split at hr
      · rename_i k ks hcd
        split at hr
        · rename_i hk
          split at hr
          · rename_i hfi
            subst hfi
            simp only [Option.some.injEq] at hr; subst hr
            cases hx
            case expand =>
              cases ‹Concrete.Expand E s s'›
              all_goals first | (hcE hg ha hm s; done) | skip
              all_goals
                have hn := ‹Concrete.Next s _ _›
                obtain ⟨e, -, hg1⟩ := gnext_sound hg hn ha
                subst e
                have hm1 := ameaning_sound hg1.1 ((ameaning_input _ _ _).trans hm)
                have hcr := hg1.1.2.2.2.2.1
                simp only [hcd] at hcr
                obtain ⟨c0, crest, hcc, hl, hrest⟩ := condRel_cons hcr hk.1
                have hcmd := ‹Concrete.cmdOf _ _ = some (Cmd.fiOrElse _)›
                rw [Concrete.cmdOf, hm1] at hcmd
                simp only [Option.some.injEq, Cmd.fiOrElse.injEq] at hcmd
              case fi =>
                simp only [hcc, List.cons.injEq] at *
                obtain ⟨-, rfl⟩ := ‹c0 = _ ∧ crest = _›
                refine ⟨_, List.mem_singleton.mpr rfl, ?_⟩
                obtain ⟨⟨h1, h2, h3, h4, -, h6, h7⟩, hp⟩ := hg1
                exact ⟨⟨h1, h2, h3, h4, hrest, h6, h7⟩, hp⟩
              case fiExtra =>
                simp only [hcc] at *
                obtain ⟨hx, -⟩ := ‹_ > _ ∧ _›
                subst hcmd
                rw [← hl] at hx
                exact absurd hx (Nat.not_lt.mpr hk.2.2)
              case fiInsertRelax =>
                simp only [hcc, List.cons.injEq] at *
                obtain ⟨rfl, -⟩ := ‹c0 = _ ∧ crest = _›
                simp only [← hl] at *
                exact absurd ‹k.limit = IfLimit.ifCode› hk.2.1
            all_goals hc hg ha hm s
          · split at hr
            · rename_i g2 hsk
              simp only [Option.some.injEq] at hr; subst hr
              cases hx
              case expand =>
                cases ‹Concrete.Expand E s s'›
                all_goals first | (hcE hg ha hm s; done) | skip
                all_goals
                  have hn := ‹Concrete.Next s _ _›
                  obtain ⟨e, -, hg1⟩ := gnext_sound hg hn ha
                  subst e
                  have hm1 := ameaning_sound hg1.1 ((ameaning_input _ _ _).trans hm)
                  have hcr := hg1.1.2.2.2.2.1
                  simp only [hcd] at hcr
                  obtain ⟨c0, crest, hcc, hl, hrest⟩ := condRel_cons hcr hk.1
                  have hcmd := ‹Concrete.cmdOf _ _ = some (Cmd.fiOrElse _)›
                  rw [Concrete.cmdOf, hm1] at hcmd
                  simp only [Option.some.injEq, Cmd.fiOrElse.injEq] at hcmd
                case elseOr =>
                  have hskip := ‹Concrete.SkipToFi _ _›
                  obtain ⟨hg2, hc2⟩ := askipFi_sound hskip _ _ g2 hg1 hsk
                  refine ⟨_, List.mem_singleton.mpr rfl, ?_⟩
                  obtain ⟨⟨h1, h2, h3, h4, -, h6, h7⟩, hp⟩ := hg2
                  refine ⟨⟨h1, h2, h3, h4, ?_, h6, h7⟩, hp⟩
                  simp only [hc2, hcc, List.drop_succ_cons, List.drop_zero]
                  exact hrest
                case fiExtra =>
                  simp only [hcc] at *
                  obtain ⟨hx, -⟩ := ‹_ > _ ∧ _›
                  subst hcmd
                  rw [← hl] at hx
                  exact absurd hx (Nat.not_lt.mpr hk.2.2)
                case fiInsertRelax =>
                  simp only [hcc, List.cons.injEq] at *
                  obtain ⟨rfl, -⟩ := ‹c0 = _ ∧ crest = _›
                  simp only [← hl] at *
                  exact absurd ‹k.limit = IfLimit.ifCode› hk.2.1
              all_goals hc hg ha hm s
            · cases hr
        · cases hr
      · cases hr
    · -- a code table
      split at hr
      · split at hr
        · rename_i ch g2 hsi1
          split at hr
          · rename_i g3 hoe
            split at hr
            · rename_i v g4 hsi
              split at hr
              · rename_i b hok
                split at hr
                · rename_i g5 hdef
                  simp only [Option.some.injEq] at hr; subst hr
                  cases hx
                  case code =>
                    rename_i s1 s2 s3 s4 t' tab' c' v' hh hcmd hsc1 h0 hmax hoe' hsc2
                    obtain ⟨hn, -, -⟩ := hh
                    obtain ⟨rfl, -, hg1⟩ := gnext_sound hg hn ha
                    have hm1 := ameaning_sound hg1.1 ((ameaning_input _ _ _).trans hm)
                    rw [Concrete.cmdOf, hm1] at hcmd
                    cases hcmd
                    obtain ⟨rfl, hg2⟩ := scanInt_sound hg1 hsc1 hsi1
                    have hg3 := optEquals_sound hg2 hoe' hoe
                    obtain ⟨rfl, hg4⟩ := scanInt_sound hg3 hsc2 hsi
                    have hb := acodeOk_sound E hok
                    refine ⟨_, List.mem_singleton.mpr rfl, ?_⟩
                    split
                    · rename_i hcond
                      have e : b = true := hb.mp hcond
                      subst e
                      unfold Concrete.assignTo Concrete.pfxGlobal
                      rw [hg4.2.1]
                      exact gdefine_sound (v := .ints [Itv.exact _]) hg4
                        ⟨_, _, List.mem_singleton.mpr rfl, rfl, Int.le_refl _, Int.le_refl _⟩ hdef
                    · rename_i hcond
                      have e : b = false := by
                        cases b
                        · rfl
                        · exact absurd (hb.mpr rfl) hcond
                      subst e
                      unfold Concrete.assignTo Concrete.pfxGlobal
                      rw [raise_pending, hg4.2.1]
                      exact gdefine_sound (v := .ints [Itv.exact _]) (raise_γ _ hg4)
                        ⟨_, _, List.mem_singleton.mpr rfl, rfl, Int.le_refl _, Int.le_refl _⟩ hdef
                  case expand => cases ‹Concrete.Expand E s s'› <;> hcE hg ha hm s
                  all_goals hc hg ha hm s
                · cases hr
              · cases hr
            · cases hr
          · cases hr
        · cases hr
      · cases hr
    · -- a count register
      split at hr
      · split at hr
        · rename_i i g2 hsi1
          split at hr
          · rename_i g3 hoe
            split at hr
            · rename_i nv g4 hsi
              split at hr
              · rename_i g5 hdef
                simp only [Option.some.injEq] at hr; subst hr
                cases hx
                case register =>
                  rename_i s1 s2 s3 s4 t' k i' v hh hcmd hk hsc1 h0 hmax hoe' hval
                  obtain ⟨hn, -, -⟩ := hh
                  obtain ⟨rfl, -, hg1⟩ := gnext_sound hg hn ha
                  have hm1 := ameaning_sound hg1.1 ((ameaning_input _ _ _).trans hm)
                  rw [Concrete.cmdOf, hm1] at hcmd
                  cases hcmd
                  obtain ⟨rfl, hg2⟩ := scanInt_sound hg1 hsc1 hsi1
                  have hg3 := optEquals_sound hg2 hoe' hoe
                  cases hval with
                  | int hsc =>
                    obtain ⟨rfl, hg4⟩ := scanInt_sound hg3 hsc hsi
                    refine ⟨_, List.mem_singleton.mpr rfl, ?_⟩
                    unfold Concrete.assignTo Concrete.pfxGlobal Concrete.trapVal
                    rw [hg4.2.1]
                    exact gdefine_sound (v := .ints [Itv.exact _]) hg4
                      ⟨_, _, List.mem_singleton.mpr rfl, rfl, Int.le_refl _, Int.le_refl _⟩ hdef
                case expand => cases ‹Concrete.Expand E s s'› <;> hcE hg ha hm s
                all_goals hc hg ha hm s
              · cases hr
            · cases hr
          · cases hr
        · cases hr
      · cases hr
    · cases hr
  case call mc =>
    simp only [readCmd] at hr
    split at hr
    · rename_i hpar
      split at hr
      · rename_i ai2 hpop
        simp only [Option.some.injEq] at hr; subst hr
        cases hx
        case expand =>
          cases ‹Concrete.Expand E s s'›
          case call =>
            revert_hyp Concrete.Next s _ _
            intro hn
            revert_hyp (Concrete.meaningTok _ _ = Meaning.call _)
            intro hmc
            revert_hyp Concrete.MatchParams _ _ _ _ _
            intro hmp
            obtain ⟨rfl, -, hg1⟩ := gnext_sound hg hn ha
            have hm1 := ameaning_sound hg1.1 ((ameaning_input _ _ _).trans hm)
            rw [hm1] at hmc
            cases hmc
            simp only [hpar.1, hpar.2, Bool.false_eq_true, ↓reduceIte] at hmp
            cases hmp
            refine ⟨_, List.mem_singleton.mpr rfl, ?_⟩
            obtain ⟨⟨h1, h2, h3, h4, h5, h6, h7⟩, hp, hq⟩ := hg1
            refine ⟨⟨h1, h2, h3, h4, h5, ?_, h7⟩, hp, hq⟩
            simp only [Concrete.pushToks, hpar.2, Bool.false_eq_true, ↓reduceIte]
            exact ⟨⟨bodyName_eq _, ofList_γ _⟩, apopDone_sound _ _ _ h6 hpop⟩
          all_goals hcE hg ha hm s
        all_goals hc hg ha hm s
      · cases hr
    · cases hr
  case undefined =>
    simp only [readCmd, Option.some.injEq] at hr; subst hr
    cases hx
    case expand =>
      cases ‹Concrete.Expand E s s'›
      case undefined =>
        revert_hyp Concrete.Next s _ _
        intro hn
        obtain ⟨rfl, -, hg1⟩ := gnext_sound hg hn ha
        exact ⟨_, List.mem_singleton.mpr rfl, raise_γ _ hg1⟩
      all_goals hcE hg ha hm s
    all_goals hc hg ha hm s
  case char c k =>
    cases k <;> simp only [readCmd] at hr
    all_goals try (cases hr; done)
    case bgroup =>
      split at hr
      · rename_i hcond
        simp only [Option.some.injEq] at hr; subst hr
        cases hx
        case bgroup =>
          rename_i s1 t' c' hh hmb
          obtain ⟨⟨hn, -, -⟩, -⟩ := hh
          obtain ⟨rfl, -, hg1⟩ := gnext_sound hg hn ha
          have hnm : ¬ s1.mode.kind = .math := fun e => hcond.2 (e ▸ hg1.1.2.2.2.2.2.2)
          simp only [hnm, ↓reduceIte]
          exact ⟨_, List.mem_singleton.mpr rfl, begin_sound .simple hg1⟩
        case expand => cases ‹Concrete.Expand E s s'› <;> hcE hg ha hm s
        all_goals hc hg ha hm s
      · cases hr
    case space =>
      simp only [Option.some.injEq] at hr; subst hr
      cases hx
      case spaceH =>
        hd hg ha hm hg1 hm1 s
        exact ⟨_, List.mem_singleton.mpr rfl, appendItem_γ _ hg1⟩
      case spaceOther =>
        hd hg ha hm hg1 hm1 s
        exact ⟨_, List.mem_singleton.mpr rfl, hg1⟩
      case prefixSkip =>
        hd hg ha hm hg1 hm1 s
        exact ⟨_, List.mem_singleton.mpr rfl, hg1⟩
      case expand => cases ‹Concrete.Expand E s s'› <;> hcE hg ha hm s
      all_goals hc hg ha hm s
    case egroup =>
      split at hr
      · split at hr
        all_goals try (cases hr; done)
        all_goals
          rename_i htop
          simp only [Option.some.injEq] at hr; subst hr
          have htg := atopGroup_sound hg.1.2.2.2.1 htop
          cases hx
          case egroupSimple =>
            rename_i s1 t' c' hh hme htp
            obtain ⟨⟨hn, -, -⟩, -⟩ := hh
            obtain ⟨rfl, hs1, hg1⟩ := gnext_sound hg hn ha
            have hsv : s1.save = s.save := by rw [hs1]
            rw [topGroup_eq, hsv, htg] at htp
            cases htp
            all_goals exact close_sound L hg1 htop (by decide)
          case egroupBottom =>
            first
            | (rename_i s1 t' c' hh hme htp
               obtain ⟨⟨hn, -, -⟩, -⟩ := hh
               obtain ⟨rfl, -, hg1⟩ := gnext_sound hg hn ha
               exact ⟨_, List.mem_singleton.mpr rfl, raise_γ _ hg1⟩)
            | hc hg ha hm s
          case egroupExtra =>
            first
            | (rename_i s1 t' c' hh hme htp
               obtain ⟨⟨hn, -, -⟩, -⟩ := hh
               obtain ⟨rfl, -, hg1⟩ := gnext_sound hg hn ha
               exact ⟨_, List.mem_singleton.mpr rfl, raise_γ _ hg1⟩)
            | hc hg ha hm s
          case egroupBox =>
            exfalso
            revert_hyp Concrete.HeadNP s _ _
            rintro ⟨⟨hn, -, -⟩, -⟩
            revert_hyp ((Concrete.unsave _).2 = some _)
            intro hun
            revert_hyp (_ = GroupCode.hbox ∨ _ = GroupCode.adjustedHbox ∨ _ = GroupCode.vbox ∨ _ = GroupCode.vtop)
            intro hg'
            obtain ⟨rfl, hs1, -⟩ := gnext_sound hg hn ha
            have e := unsave_code _ hun
            rw [topGroup_eq, congrArg State.save hs1, htg] at e
            subst e
            clear hs1 hn hg ha
            simp_all
          case expand => cases ‹Concrete.Expand E s s'› <;> hcE hg ha hm s
          all_goals hc hg ha hm s
      · cases hr
  all_goals (simp only [readCmd] at hr; cases hr)

/-! ## 4. The run

The abstract run is over `Option GState`. `none` is `lost`: the named
havoc, γ = every state. The run goes there only where `read` says it does
not follow (`lose`), never by choice. -/

inductive OStep (L : Limits) : Option GState → Option GState → Prop
  | follow {g g' : GState} {rs : List GState} : read L g = some rs → g' ∈ rs → OStep L (some g) (some g')
  | lose {g : GState} : read L g = none → OStep L (some g) none

def oγ : Option GState → State → Prop
  | none, _ => True
  | some g, s => g.γ s

/-- One concrete step: at most one abstract step. -/
theorem ostep_covered1 (E : Concrete.Engine) (L : Limits) {x : Option GState} {s s' : State}
    (hx : oγ x s) (h : Concrete.Step E s s') :
    ∃ x', (x' = x ∨ OStep L x x') ∧ oγ x' s' := by
  cases x with
  | none => exact ⟨none, Or.inl rfl, trivial⟩
  | some g =>
    cases hr : read L g with
    | none => exact ⟨none, Or.inr (.lose hr), trivial⟩
    | some rs =>
      obtain ⟨g', hm, hγ⟩ := step_covered E L hx h hr
      exact ⟨some g', Or.inr (.follow hr hm), hγ⟩

theorem ostep_covered (E : Concrete.Engine) (L : Limits) :
    Covered oγ (OStep L) (Concrete.Step E) := by
  intro x s s' hx h
  obtain ⟨x', h1 | h1, h2⟩ := ostep_covered1 E L hx h
  · exact ⟨x, Star.refl, h1 ▸ h2⟩
  · exact ⟨x', Star.single h1, h2⟩

/-- With a budget at least the length of the concrete run, the budgeted
abstract run covers it without the cut (audit C14: the cut is not what
makes it sound). -/
theorem budget_enough (E : Concrete.Engine) (L : Limits) :
    ∀ k s₀ s, StarN (Concrete.Step E) s₀ k s → ∀ n x₀, oγ x₀ s₀ → k ≤ n →
      ∃ x m, Star (BStep (OStep L)) (n, some x₀) (m, some x) ∧ n - k ≤ m ∧ oγ x s := by
  intro k s₀ s h
  induction h with
  | refl => intro n x₀ hx _; exact ⟨x₀, n, Star.refl, by omega, hx⟩
  | @tail k b c _ hbc ih =>
    intro n x₀ hx hk
    obtain ⟨x, m, hst, hm, hb⟩ := ih n x₀ hx (by omega)
    obtain ⟨x', h1 | h1, h2⟩ := ostep_covered1 E L hb hbc
    · exact ⟨x, m, hst, by omega, h1 ▸ h2⟩
    · obtain ⟨m', rfl⟩ : ∃ m', m = m' + 1 := ⟨m - 1, by omega⟩
      exact ⟨x', m', Star.tail hst (BStep.step h1), by omega, h2⟩

/-- **The main theorem**, with no hypothesis. For every engine and limits:
1. every concrete run is covered by an abstract run of SaTeX's fixed
   reader (`read`), one abstract step per concrete step at most;
2. the run is `lost` (covers everything) only where `read` does not follow;
3. with a budget at least the run's length, the budgeted run covers it
   without the cut;
4. with any budget the budgeted run covers it (past the cut: nothing
   claimed), it has no infinite run and none longer than its budget;
5. every abstract state has finitely many successors.
What makes it weak: `read` follows files and token lists, but only a few
commands (PROOF-STATUS "The main theorem"). Counter-model
check: `read` and `OStep` are definitions, so neither `Read := ⊥` nor
`Other := Concrete.Step E` can be chosen; a step from a state the reader
follows must be one of the rules `step_covered` matches, all others are
refuted there. -/
theorem satex_sound_and_terminating (E : Concrete.Engine) (L : Limits) :
    (∀ s₀ s, Star (Concrete.Step E) s₀ s → ∀ x₀, oγ x₀ s₀ → ∃ x, Star (OStep L) x₀ x ∧ oγ x s) ∧
    (∀ g, OStep L (some g) none → read L g = none) ∧
    (∀ k s₀ s, StarN (Concrete.Step E) s₀ k s → ∀ n x₀, oγ x₀ s₀ → k ≤ n →
      ∃ x m, Star (BStep (OStep L)) (n, some x₀) (m, some x) ∧ oγ x s) ∧
    ((∀ s₀ s, Star (Concrete.Step E) s₀ s → ∀ x₀, bγ oγ x₀ s₀ →
        ∃ x, Star (BStep (OStep L)) x₀ x ∧ bγ oγ x s) ∧
      WellFounded (fun y x => BStep (OStep L) x y) ∧
      (∀ n x y, StarN (BStep (OStep L)) x n y → n + bμ y ≤ bμ x)) ∧
    (∀ x, ∃ l : List (Option GState), ∀ y, OStep L x y → y ∈ l) := by
  refine ⟨simulation_star _ _ _ (ostep_covered E L), ?_, ?_,
    sound_and_terminating oγ (Concrete.Step E) (OStep L) (ostep_covered E L), ?_⟩
  · intro g h; cases h; assumption
  · intro k s₀ s h n x₀ hx hk
    obtain ⟨x, m, h1, -, h2⟩ := budget_enough E L k s₀ s h n x₀ hx hk
    exact ⟨x, m, h1, h2⟩
  · intro x
    cases x with
    | none => exact ⟨[], fun y h => by cases h⟩
    | some g =>
      cases hr : read L g with
      | none => exact ⟨[none], fun y h => by cases h <;> simp_all⟩
      | some rs => exact ⟨rs.map some, fun y h => by cases h <;> simp_all⟩

end Satex.Soundness
