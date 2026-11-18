/-
Satex.Operands: the global abstract state and the operands the reader
takes from the input: a control sequence name (§ 1215), `\let`'s right
side (§ 1221) and a replacement text without parameters (§§ 473–479).

Intuition. Each reader reads tokens with the abstract mouth, one at a
time, and checks them as TeX's scanner would. It gives up (`none`) where a
token's meaning is not known or a case is not modelled. Each one comes
with a lemma: the concrete relation (`RToken`, `LetRhs`, `ParamText`,
`DefBody`) gives the same result, and the abstract state after it covers
the concrete one.

Written with AI assistance from Claude Code (Claude Opus 5.5), directed and
reviewed by the author.
-/
import Lean
import Satex.Mouth

namespace Satex.Soundness

open Satex Satex.Abstract

/-! ## 1. The global abstract state

`AState.γ` leaves the pending prefixes, the `\afterassignment` token and
`force_eof` free (audit C15). The reader must decide them, so the global
state pins them exactly. -/

/-- An abstract state with the pending prefixes, the `\afterassignment`
token and `force_eof` (§ 1211, § 1269, § 361).
source: tex.web § 1211 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L22720, § 1269 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L23473
source: SaTeX `Machine::prefixes`, `Machine::after_assignment` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/analysis/machine.rs -/
structure GState where
  a : AState
  pfx : Nat := 0
  after : Option Token := none
  feof : Bool := false

def GState.γ (g : GState) (s : State) : Prop :=
  g.a.γ s ∧ s.pending.prefixes = g.pfx ∧ s.pending.afterAssignment = g.after ∧
    s.forceEof = g.feof

/-! ## 2. The abstract mouth

`amouth` (Satex.Mouth) reads the next token from a token list or a file
line. Only the input and `force_eof` move. -/

/-- `get_next` (§ 357) over the global state: the token and the state after it.
-- source: tex.web § 357 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L7480
-- source: SaTeX `next_token` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/analysis/machine.rs -/
def GState.next (g : GState) : Option (Token × GState) :=
  match amouth g.a g.feof g.a.input with
  | some (t, ai, fe) => some (t, { g with a := { g.a with input := ai }, feof := fe })
  | none => none

theorem next_eq {g g1 : GState} {t : Token} (h : g.next = some (t, g1)) :
    ∃ ai fe, g1 = { g with a := { g.a with input := ai }, feof := fe } := by
  unfold GState.next at h
  split at h
  · simp only [Option.some.injEq, Prod.mk.injEq] at h
    exact ⟨_, _, h.2.symm⟩
  · cases h

/-- A step of the abstract mouth covers the concrete one: same token, only
the input and `force_eof` moved. -/
theorem gnext_sound {g g1 : GState} {s s1 : State} {t' t : Token}
    (hg : g.γ s) (hn : Concrete.Next s t' s1) (ha : g.next = some (t, g1)) :
    t' = t ∧ s1 = { s with input := s1.input, forceEof := s1.forceEof } ∧ g1.γ s1 := by
  unfold GState.next at ha
  split at ha
  · rename_i t0 ai fe hm
    simp only [Option.some.injEq, Prod.mk.injEq] at ha
    obtain ⟨rfl, rfl⟩ := ha
    obtain ⟨⟨h1, h2, h3, h4, h5, h6, h7⟩, hp, hq, hf⟩ := hg
    obtain ⟨e1, e2, e3, e4⟩ := amouth_sound hn g.a g.feof g.a.input t0 ai fe h1 h6 hf hm
    refine ⟨e1, e2, ?_⟩
    rw [e2]
    exact ⟨⟨h1, h2, h3, h4, h5, e3, h7⟩, hp, hq, e4⟩
  · cases ha

/-- With a token next, the input is not drained (§ 360). -/
theorem gnext_not_drained {g g1 : GState} {s : State} {t : Token} (hd : Concrete.Drained s)
    (hg : g.γ s) : g.next ≠ some (t, g1) := by
  unfold GState.next
  split
  · rename_i t0 ai fe hm
    exact absurd hm (amouth_not_drained hd g.a g.feof g.a.input _ hg.1.1 hg.1.2.2.2.2.2.1 hg.2.2.2)
  · simp

/-! ## 3. Meanings the reader knows

A meaning is known when the binding is one exact meaning
(`Binding.meaning` without `may`). -/

def ameaningCs (a : AState) (r : CsRef) : Option Meaning :=
  match a.eqtb (.meaning r) with
  | .meanings [.exact m] => some m
  | _ => none

/-- `meaningTok` (§ 357, § 358) over the abstract state. -/
def ameaning (a : AState) : Token → Option Meaning
  | .char c k => some (.char c k)
  | .cs r => ameaningCs a r
  | .noexp r => (ameaningCs a r).map (fun m => if m.expandable then Concrete.notExpanded else m)
  | .outParam _ => some .undefined

theorem ameaningCs_sound {a : AState} {s : State} {r : CsRef} {m : Meaning} (ha : a.γ s)
    (h : ameaningCs a r = some m) : s.meaningOf r = m := by
  have hv := ha.1 (.meaning r)
  unfold ameaningCs at h
  split at h
  · rename_i e
    rw [e] at hv
    simp only [Option.some.injEq] at h
    subst h
    obtain ⟨am, m', hm, hval, hγ⟩ := hv
    simp only [List.mem_singleton] at hm
    subst hm
    simp only [AMeaning.γ] at hγ
    subst hγ
    simp only [State.meaningOf, State.get, hval]
  · cases h

theorem ameaning_sound {a : AState} {s : State} {t : Token} {m : Meaning} (ha : a.γ s)
    (h : ameaning a t = some m) : Concrete.meaningTok s t = m := by
  cases t with
  | char c k => simp only [ameaning, Option.some.injEq] at h; exact h
  | cs r => exact ameaningCs_sound ha h
  | noexp r =>
    simp only [ameaning, Option.map_eq_some_iff] at h
    obtain ⟨m', h1, h2⟩ := h
    simp only [Concrete.meaningTok, ameaningCs_sound ha h1]
    exact h2
  | outParam n => simp only [ameaning, Option.some.injEq] at h; exact h

theorem ameaning_input (a : AState) (x : List AInput) (t : Token) :
    ameaning { a with input := x } t = ameaning a t := by
  cases t <;> rfl

/-! ## 4. Operands

Each reader below takes its tokens with `GState.next` and mirrors one
concrete relation. The lemmas do not need the frame of the state: the
γ of the state after the last token carries everything. -/

/-- `get_r_token` (§ 1215) where the next token is a control sequence. A
space before it (only a token list can hold one there) is not followed. -/
-- source: tex.web § 1215 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web
-- source: SaTeX `read_r_token` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/analysis/machine.rs
def GState.rtoken (g : GState) : Option (CsRef × GState) :=
  match g.next with
  | some (.cs r, g1) => some (r, g1)
  | some (.noexp r, g1) => some (r, g1)
  | _ => none

theorem rtoken_sound {g g1 : GState} {s s1 : State} {r r' : CsRef} (hg : g.γ s)
    (h : Concrete.RToken s r s1) (ha : g.rtoken = some (r', g1)) : r = r' ∧ g1.γ s1 := by
  unfold GState.rtoken at ha
  cases hn : g.next with
  | none => simp [hn] at ha
  | some p =>
    obtain ⟨t, g2⟩ := p
    rw [hn] at ha
    cases t <;> simp only [Option.some.injEq, Prod.mk.injEq, reduceCtorEq] at ha
    all_goals
      obtain ⟨rfl, rfl⟩ := ha
      cases h with
      | cs hc => obtain ⟨e, -, hg1⟩ := gnext_sound hg hc hn; cases e <;> exact ⟨rfl, hg1⟩
      | noexp hc => obtain ⟨e, -, hg1⟩ := gnext_sound hg hc hn; cases e <;> exact ⟨rfl, hg1⟩
      | space hc _ => obtain ⟨e, -, -⟩ := gnext_sound hg hc hn; cases e

/-- `\let`'s right side (§ 1221): an optional `=`, then one optional space.
A space before the `=` (only a token list can hold one) is not followed. -/
-- source: tex.web § 1221 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L22838
-- source: SaTeX `primitive_let` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
def GState.letRhs (g : GState) : Option (Token × GState) :=
  match g.next with
  | some (u, g1) =>
    if u = .char 61 .other then
      match g1.next with
      | some (u2, g2) =>
        match ameaning g2.a u2 with
        | some m2 => if Concrete.isSpacer m2 then g2.next else some (u2, g2)
        | none => none
      | none => none
    else
      match ameaning g1.a u with
      | some m => if Concrete.isSpacer m then none else some (u, g1)
      | none => none
  | none => none

theorem letRhs_sound {g g1 : GState} {s s1 : State} {u u' : Token} (hg : g.γ s)
    (h : Concrete.LetRhs s u s1) (ha : g.letRhs = some (u', g1)) : u = u' ∧ g1.γ s1 := by
  unfold GState.letRhs at ha
  cases hn : g.next with
  | none => simp [hn] at ha
  | some p =>
    obtain ⟨v, g2⟩ := p
    rw [hn] at ha
    simp only at ha
    cases h with
    | plain hn1 hsp hne =>
      obtain ⟨rfl, -, hg2⟩ := gnext_sound hg hn1 hn
      simp only [hne, ↓reduceIte] at ha
      cases hm : ameaning g2.a u with
      | none => simp [hm] at ha
      | some m =>
        have e := ameaning_sound hg2.1 hm
        simp only [Concrete.spacerTok, e] at hsp
        simp only [hm, hsp, Bool.false_eq_true, ↓reduceIte, Option.some.injEq, Prod.mk.injEq] at ha
        obtain ⟨rfl, rfl⟩ := ha
        exact ⟨rfl, hg2⟩
    | eq hn1 hn2 hsp =>
      obtain ⟨rfl, -, hg2⟩ := gnext_sound hg hn1 hn
      simp only [↓reduceIte] at ha
      cases hn' : g2.next with
      | none => simp [hn'] at ha
      | some q =>
        obtain ⟨w, g3⟩ := q
        obtain ⟨rfl, -, hg3⟩ := gnext_sound hg2 hn2 hn'
        rw [hn'] at ha
        simp only at ha
        cases hm : ameaning g3.a u with
        | none => simp [hm] at ha
        | some m =>
          have e := ameaning_sound hg3.1 hm
          simp only [Concrete.spacerTok, e] at hsp
          simp only [hm, hsp, Bool.false_eq_true, ↓reduceIte, Option.some.injEq, Prod.mk.injEq] at ha
          obtain ⟨rfl, rfl⟩ := ha
          exact ⟨rfl, hg3⟩
    | eqSpace hn1 hn2 hsp hn3 =>
      obtain ⟨rfl, -, hg2⟩ := gnext_sound hg hn1 hn
      simp only [↓reduceIte] at ha
      cases hn' : g2.next with
      | none => simp [hn'] at ha
      | some q =>
        obtain ⟨w, g3⟩ := q
        obtain ⟨rfl, -, hg3⟩ := gnext_sound hg2 hn2 hn'
        rw [hn'] at ha
        simp only at ha
        split at ha
        · rename_i m hm
          have e := ameaning_sound hg3.1 hm
          simp only [Concrete.spacerTok, e] at hsp
          simp only [hsp, ↓reduceIte] at ha
          obtain ⟨rfl, -, hg4⟩ := gnext_sound hg3 hn3 ha
          exact ⟨rfl, hg4⟩
        · cases ha
    | space hn1 hsp _ =>
      obtain ⟨rfl, -, hg2⟩ := gnext_sound hg hn1 hn
      split at ha
      · rename_i he
        subst he
        simp [Concrete.spacerTok, Concrete.meaningTok, Concrete.isSpacer] at hsp
      · split at ha
        · rename_i m hm
          have e := ameaning_sound hg2.1 hm
          simp only [Concrete.spacerTok, e] at hsp
          simp [hsp] at ha
        · cases ha

/-- `\let`'s operands: the name, and the meaning of the right side. -/
def GState.letOps (g : GState) : Option (CsRef × Meaning × GState) :=
  match g.rtoken with
  | some (r, g2) =>
    match g2.letRhs with
    | some (u, g3) => (ameaning g3.a u).map fun m => (r, m, g3)
    | none => none
  | none => none

theorem letOps_sound {g g' : GState} {s s1 s2 : State} {r r' : CsRef} {u : Token} {m : Meaning}
    (hg : g.γ s) (h1 : Concrete.RToken s r s1) (h2 : Concrete.LetRhs s1 u s2)
    (ha : g.letOps = some (r', m, g')) : r = r' ∧ Concrete.meaningTok s2 u = m ∧ g'.γ s2 := by
  unfold GState.letOps at ha
  split at ha
  · rename_i r0 g2 hr
    obtain ⟨rfl, hg2⟩ := rtoken_sound hg h1 hr
    split at ha
    · rename_i u0 g3 hl
      obtain ⟨rfl, hg3⟩ := letRhs_sound hg2 h2 hl
      simp only [Option.map_eq_some_iff, Prod.mk.injEq] at ha
      obtain ⟨m0, hm, rfl, rfl, rfl⟩ := ha
      exact ⟨rfl, ameaning_sound hg3.1 hm, hg3⟩
    · cases ha
  · cases ha

/-- Levels plus tokens and characters still in the input: enough fuel for
any replacement text in it (`\everyeof` aside). -/
def isize : List AInput → Nat
  | [] => 0
  | .toks _ t :: rest => t.length + 1 + isize rest
  | .file f :: rest => f.cur.length + (f.lines.map (·.length + 2)).sum + 1 + isize rest

/-- A replacement text without parameters (§§ 473, 477): the tokens up to
the matching `}`. A parameter character (`##`, `#1`) is not followed.
`fuel` bounds the tokens read: this makes the function total. -/
-- source: tex.web § 473 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L9292
-- source: SaTeX `make_macro` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
def abody : Nat → GState → Nat → Option (TokList × GState)
  | 0, _, _ => none
  | fuel + 1, g, d =>
    match g.next with
    | some (t, g1) =>
      if t.isEgroup then
        if d = 0 then some ([], g1)
        else (abody fuel g1 (d - 1)).map fun p => (t :: p.1, p.2)
      else if t.isBgroup then (abody fuel g1 (d + 1)).map fun p => (t :: p.1, p.2)
      else match t with
        | .char _ .param => none
        | _ => (abody fuel g1 d).map fun p => (t.stored :: p.1, p.2)
    | none => none

theorem abody_sound {np : Nat} : ∀ {s : State} {d : Nat} {ts : TokList} {s1 : State},
    Concrete.DefBody np s d ts s1 →
    ∀ (fuel : Nat) (g g' : GState) (ts' : TokList), g.γ s → abody fuel g d = some (ts', g') →
      ts = ts' ∧ g'.γ s1 := by
  intro s d ts s1 h
  induction h with
  | @close s s1 t hn he =>
    intro fuel g g' ts' hg ha
    cases fuel with
    | zero => cases ha
    | succ f =>
      simp only [abody] at ha
      cases hx : g.next with
      | none => simp [hx] at ha
      | some p =>
        obtain ⟨t0, g1⟩ := p
        obtain ⟨e, -, hg1⟩ := gnext_sound hg hn hx
        subst e
        simp only [hx, he, ↓reduceIte, Option.some.injEq, Prod.mk.injEq] at ha
        obtain ⟨rfl, rfl⟩ := ha
        exact ⟨rfl, hg1⟩
  | @closeInner s s1 s2 d ts t hn he _ ih =>
    intro fuel g g' ts' hg ha
    cases fuel with
    | zero => cases ha
    | succ f =>
      simp only [abody] at ha
      cases hx : g.next with
      | none => simp [hx] at ha
      | some p =>
        obtain ⟨t0, g1⟩ := p
        obtain ⟨e, -, hg1⟩ := gnext_sound hg hn hx
        subst e
        simp only [hx, he, ↓reduceIte, Nat.add_one_ne_zero, Nat.add_sub_cancel,
          Option.map_eq_some_iff, Prod.mk.injEq] at ha
        obtain ⟨q, hq, rfl, rfl⟩ := ha
        obtain ⟨rfl, hg'⟩ := ih f g1 q.2 q.1 hg1 hq
        exact ⟨rfl, hg'⟩
  | @openGroup s s1 s2 d ts t hn hb _ ih =>
    intro fuel g g' ts' hg ha
    cases fuel with
    | zero => cases ha
    | succ f =>
      simp only [abody] at ha
      cases hx : g.next with
      | none => simp [hx] at ha
      | some p =>
        obtain ⟨t0, g1⟩ := p
        obtain ⟨e, -, hg1⟩ := gnext_sound hg hn hx
        subst e
        have he : t.isEgroup = false := by
          cases t with
          | char c k => cases k <;> simp_all [Token.isBgroup, Token.isEgroup]
          | _ => rfl
        simp only [hx, he, hb, Bool.false_eq_true, ↓reduceIte,
          Option.map_eq_some_iff, Prod.mk.injEq] at ha
        obtain ⟨q, hq, rfl, rfl⟩ := ha
        obtain ⟨rfl, hg'⟩ := ih f g1 q.2 q.1 hg1 hq
        exact ⟨rfl, hg'⟩
  | @hashHash s s1 s2 s3 d ts c c' hn _ _ _ =>
    intro fuel g g' ts' hg ha
    cases fuel with
    | zero => cases ha
    | succ f =>
      simp only [abody] at ha
      cases hx : g.next with
      | none => simp [hx] at ha
      | some p =>
        obtain ⟨t0, g1⟩ := p
        obtain ⟨rfl, -, -⟩ := gnext_sound hg hn hx
        simp [hx, Token.isBgroup, Token.isEgroup] at ha
  | @argRef s s1 s2 s3 d k ts c hn _ _ _ _ _ =>
    intro fuel g g' ts' hg ha
    cases fuel with
    | zero => cases ha
    | succ f =>
      simp only [abody] at ha
      cases hx : g.next with
      | none => simp [hx] at ha
      | some p =>
        obtain ⟨t0, g1⟩ := p
        obtain ⟨rfl, -, -⟩ := gnext_sound hg hn hx
        simp [hx, Token.isBgroup, Token.isEgroup] at ha
  | @tok s s1 s2 d ts t hn hb he hp _ ih =>
    intro fuel g g' ts' hg ha
    cases fuel with
    | zero => cases ha
    | succ f =>
      simp only [abody] at ha
      cases hx : g.next with
      | none => simp [hx] at ha
      | some p =>
        obtain ⟨t0, g1⟩ := p
        obtain ⟨e, -, hg1⟩ := gnext_sound hg hn hx
        subst e
        simp only [hx, he, hb, Bool.false_eq_true, ↓reduceIte] at ha
        split at ha
        · simp at hp
        · simp only [Option.map_eq_some_iff, Prod.mk.injEq] at ha
          obtain ⟨q, hq, rfl, rfl⟩ := ha
          obtain ⟨rfl, hg'⟩ := ih f g1 q.2 q.1 hg1 hq
          exact ⟨rfl, hg'⟩

/-- `\def`'s operands without parameters: the name, then `{`, then the
replacement text (§ 473). -/
def GState.defOps (g : GState) : Option (CsRef × TokList × GState) :=
  match g.rtoken with
  | some (r, g2) =>
    match g2.next with
    | some (t, g3) =>
      if t.isBgroup then (abody (isize g3.a.input) g3 0).map fun p => (r, p.1, p.2) else none
    | none => none
  | none => none

theorem defOps_sound {g g' : GState} {s s1 s2 s3 : State} {r r' : CsRef} {ps : List ParamItem}
    {b : Option Token} {body body' : TokList}
    (hg : g.γ s) (h1 : Concrete.RToken s r s1) (h2 : Concrete.ParamText s1 0 ps b s2)
    (h3 : Concrete.DefBody (Concrete.countArgs ps) s2 0 body s3)
    (ha : g.defOps = some (r', body', g')) :
    r = r' ∧ ps = [] ∧ b = none ∧ body = body' ∧ g'.γ s3 := by
  unfold GState.defOps at ha
  split at ha
  · rename_i r0 g2 hr
    obtain ⟨rfl, hg2⟩ := rtoken_sound hg h1 hr
    split at ha
    · rename_i t g3 hn
      split at ha
      · rename_i hb
        simp only [Option.map_eq_some_iff, Prod.mk.injEq] at ha
        obtain ⟨q, hq, rfl, rfl, rfl⟩ := ha
        cases h2 with
        | brace hn1 _ =>
          obtain ⟨rfl, -, hg3⟩ := gnext_sound hg2 hn1 hn
          obtain ⟨rfl, hg'⟩ := abody_sound h3 _ g3 q.2 q.1 hg3 hq
          exact ⟨rfl, rfl, rfl, rfl, hg'⟩
        | arg hn1 _ _ _ =>
          obtain ⟨rfl, -, -⟩ := gnext_sound hg2 hn1 hn
          simp [Token.isBgroup] at hb
        | hashBrace hn1 _ _ =>
          obtain ⟨rfl, -, -⟩ := gnext_sound hg2 hn1 hn
          simp [Token.isBgroup] at hb
        | tok hn1 hb1 _ _ _ =>
          obtain ⟨rfl, -, -⟩ := gnext_sound hg2 hn1 hn
          rw [hb] at hb1; cases hb1
      · cases ha
    · cases ha
  · cases ha

/-! ## 5. Ending an assignment

`define` and `finishAssign` (§§ 1214, 1269) over the global state:
`\globaldefs` must be one known number. -/

/-- `\globaldefs` decides (§ 1214). -/
def globalWith (n : Int) (glob : Bool) : Bool :=
  if n > 0 then true else if n < 0 then false else glob

/-- `finishAssign` (§ 1269): prefixes go, the `\afterassignment` token comes. -/
def GState.finish (g : GState) : GState :=
  match g.after with
  | some t => { g with pfx := 0, after := none,
                       a := { g.a with input := .toks .backedUp (AText.ofList [t.stored]) :: g.a.input } }
  | none => { g with pfx := 0, after := none }

theorem finish_sound {g : GState} {s : State} (hg : g.γ s) : g.finish.γ (Concrete.finishAssign s) := by
  obtain ⟨⟨h1, h2, h3, h4, h5, h6, h7⟩, hp, hq, hf⟩ := hg
  unfold GState.finish Concrete.finishAssign
  cases e : g.after with
  | none =>
    rw [e] at hq
    simp only [hq]
    exact ⟨⟨h1, h2, h3, h4, h5, h6, h7⟩, rfl, rfl, hf⟩
  | some t =>
    rw [e] at hq
    simp only [hq]
    exact ⟨⟨h1, h2, h3, h4, h5, ⟨⟨rfl, ofList_γ _⟩, h6⟩, h7⟩, rfl, rfl, hf⟩

/-- An assignment to `l` of a value `v`, then its end. -/
def GState.define (g : GState) (glob : Bool) (l : Loc) (v : AVal) : Option GState :=
  (aint g.a (.intPar "globaldefs")).map fun n =>
    GState.finish { g with a := g.a.assign (globalWith n glob) l v }

theorem gdefine_sound {g g' : GState} {s : State} {glob : Bool} {l : Loc} {v : AVal} {x : Val}
    (hg : g.γ s) (hv : v.γ x) (h : g.define glob l v = some g') :
    g'.γ (Concrete.finishAssign (Concrete.define s glob l x)) := by
  unfold GState.define at h
  simp only [Option.map_eq_some_iff] at h
  obtain ⟨n, hn, rfl⟩ := h
  apply finish_sound
  have hgl : Concrete.isGlobal s glob = globalWith n glob := by
    rw [← aint_sound hg.1.1 hn]; rfl
  have hd := define_sound (l := l) hg.1 hv glob
  rw [hgl] at hd
  obtain ⟨-, hp, hq, hf⟩ := hg
  refine ⟨hd, ?_, ?_, ?_⟩ <;>
    (unfold Concrete.define Concrete.geqDefine Concrete.eqDefine; split <;> assumption)

/-! ## 6. Numbers

`scan_int` (§ 440) for explicit decimal digits, and what it needs:
`get_x_token` where nothing expands, the next non-blank token, the
optional `=`. A token that would expand stops the reader (`none`): the
number would then depend on a macro body or a conditional.

Why `expand_head` matters: `XNext` may expand first, by any `Expand`
rule. Every such rule starts with an expandable token, so a token the
reader knows to be unexpandable rules all of them out at once. -/

theorem exp_of_cmd {s : State} {t : Token} {c : Cmd} (h : Concrete.cmdOf s t = some c)
    (hc : c.expandable = true) : Concrete.expandableTok s t = true := by
  unfold Concrete.cmdOf at h
  unfold Concrete.expandableTok
  split at h
  · rename_i n c' e
    simp only [Option.some.injEq] at h
    subst h
    rw [e]
    exact hc
  · cases h

theorem expand_head {E : Concrete.Engine} {s s' : State} (h : Concrete.Expand E s s') :
    ∃ t s1, Concrete.Next s t s1 ∧ Concrete.expandableTok s1 t = true := by
  cases h
  all_goals
    first
    | exact ⟨_, _, ‹Concrete.Next s _ _›, exp_of_cmd ‹Concrete.cmdOf _ _ = some _› rfl⟩
    | exact ⟨_, _, ‹Concrete.Next s _ _›, exp_of_cmd ‹Concrete.cmdOf _ _ = some _› ‹_›⟩
    | exact ⟨_, _, ‹Concrete.Next s _ _›, by
        simp [Concrete.expandableTok, ‹Concrete.meaningTok _ _ = _›, Meaning.expandable]⟩
    | (rename_i hT
       cases hT <;> exact ⟨_, _, ‹Concrete.Next s _ _›, exp_of_cmd ‹Concrete.cmdOf _ _ = some _› rfl⟩)
    | (have hC := ‹Concrete.CondHead s _ _ _ _›
       cases hC <;> exact ⟨_, _, ‹Concrete.Next s _ _›, exp_of_cmd ‹Concrete.cmdOf _ _ = some _› rfl⟩)

theorem raise_γ {g : GState} {s : State} (msg : String) (hg : g.γ s) : g.γ (Concrete.raise msg s) := by
  unfold Concrete.raise Concrete.logOut
  dsimp only
  split <;> exact hg

/-- `back_input` (§ 325) over the global state. -/
def GState.back (g : GState) (t : Token) : GState :=
  { g with a := { g.a with input := .toks .backedUp (AText.ofList [t.stored]) :: g.a.input } }

theorem back_sound {g : GState} {s : State} (t : Token) (hg : g.γ s) :
    (g.back t).γ (Concrete.backInput t s) := by
  obtain ⟨⟨h1, h2, h3, h4, h5, h6, h7⟩, hp⟩ := hg
  exact ⟨⟨h1, h2, h3, h4, h5, ⟨⟨rfl, ofList_γ _⟩, h6⟩, h7⟩, hp⟩

/-- `get_x_token` (§ 380) where the next token is not expandable: no
expansion happens. -/
def GState.xnext (g : GState) : Option (Token × GState) :=
  match g.next with
  | some (t, g1) =>
    match ameaning g1.a t with
    | some m => if m.expandable then none else some (t, g1)
    | none => none
  | none => none

theorem xnext_sound {E : Concrete.Engine} {g g1 : GState} {s s1 : State} {t t' : Token}
    (hg : g.γ s) (h : Concrete.XNext E s t s1) (ha : g.xnext = some (t', g1)) :
    t = t' ∧ g1.γ s1 := by
  unfold GState.xnext at ha
  cases hn : g.next with
  | none => simp [hn] at ha
  | some p =>
    obtain ⟨u, g2⟩ := p
    rw [hn] at ha
    simp only at ha
    split at ha
    · rename_i m hm
      split at ha
      · cases ha
      · rename_i hne
        simp only [Option.some.injEq, Prod.mk.injEq] at ha
        obtain ⟨rfl, rfl⟩ := ha
        cases h with
        | plain hn' _ =>
          obtain ⟨rfl, -, hg2⟩ := gnext_sound hg hn' hn
          exact ⟨rfl, hg2⟩
        | expand hE _ =>
          obtain ⟨v, s1', hn', hx⟩ := expand_head hE
          obtain ⟨rfl, -, hg2⟩ := gnext_sound hg hn' hn
          have e := ameaning_sound hg2.1 hm
          simp only [Concrete.expandableTok, e] at hx
          exact absurd hx hne
    · cases ha

/-- The next non-blank token after expansion (§ 406). One space may come
first. Two spaces in a row only come from a token list: not followed. -/
def GState.xnonblank (g : GState) : Option (Token × GState) :=
  match g.xnext with
  | some (t, g1) =>
    match ameaning g1.a t with
    | some m =>
      if Concrete.isSpacer m then
        match g1.xnext with
        | some (u, g2) =>
          match ameaning g2.a u with
          | some m2 => if Concrete.isSpacer m2 then none else some (u, g2)
          | none => none
        | none => none
      else some (t, g1)
    | none => none
  | none => none

theorem xnonblank_sound {E : Concrete.Engine} {g g1 : GState} {s s1 : State} {t t' : Token}
    (hg : g.γ s) (h : Concrete.XNonBlank E s t s1) (ha : g.xnonblank = some (t', g1)) :
    t = t' ∧ g1.γ s1 := by
  unfold GState.xnonblank at ha
  cases hn : g.xnext with
  | none => simp [hn] at ha
  | some p =>
    obtain ⟨u, g2⟩ := p
    rw [hn] at ha
    simp only at ha
    split at ha
    · rename_i m hm
      cases h with
      | here hx hsp =>
        obtain ⟨rfl, hg2⟩ := xnext_sound hg hx hn
        have e := ameaning_sound hg2.1 hm
        simp only [Concrete.spacerTok, e] at hsp
        simp only [hsp, Bool.false_eq_true, ↓reduceIte, Option.some.injEq, Prod.mk.injEq] at ha
        obtain ⟨rfl, rfl⟩ := ha
        exact ⟨rfl, hg2⟩
      | skip hx hsp hrest =>
        obtain ⟨rfl, hg2⟩ := xnext_sound hg hx hn
        have e := ameaning_sound hg2.1 hm
        simp only [Concrete.spacerTok, e] at hsp
        simp only [hsp, ↓reduceIte] at ha
        cases hn2 : g2.xnext with
        | none => simp [hn2] at ha
        | some q =>
          obtain ⟨w, g3⟩ := q
          rw [hn2] at ha
          simp only at ha
          split at ha
          · rename_i m2 hm2
            cases hrest with
            | here hx2 hsp2 =>
              obtain ⟨rfl, hg3⟩ := xnext_sound hg2 hx2 hn2
              have e2 := ameaning_sound hg3.1 hm2
              simp only [Concrete.spacerTok, e2] at hsp2
              simp only [hsp2, Bool.false_eq_true, ↓reduceIte, Option.some.injEq, Prod.mk.injEq] at ha
              obtain ⟨rfl, rfl⟩ := ha
              exact ⟨rfl, hg3⟩
            | skip hx2 hsp2 _ =>
              obtain ⟨rfl, hg3⟩ := xnext_sound hg2 hx2 hn2
              have e2 := ameaning_sound hg3.1 hm2
              simp only [Concrete.spacerTok, e2] at hsp2
              simp [hsp2] at ha
          · cases ha
    · cases ha

/-- Decimal digits (§§ 444–445), then one optional space (§ 443). -/
def adigits : Nat → GState → Nat → Option (Nat × GState)
  | 0, _, _ => none
  | fuel + 1, g, acc =>
    match g.xnext with
    | some (t, g1) =>
      match Concrete.isDigitTok 10 t with
      | some d => adigits fuel g1 (acc * 10 + d)
      | none =>
        match ameaning g1.a t with
        | some m => if Concrete.isSpacer m then some (acc, g1) else some (acc, g1.back t)
        | none => none
    | none => none

theorem adigits_sound {E : Concrete.Engine} : ∀ (fuel : Nat) {g g' : GState} {s s1 : State}
    {acc n : Nat} {ds : List Nat},
    g.γ s → Concrete.Digits E 10 s acc ds s1 → adigits fuel g acc = some (n, g') →
      ds = [n] ∧ g'.γ s1
  | 0, _, _, _, _, _, _, _, _, _, ha => by cases ha
  | fuel + 1, g, g', s, s1, acc, n, ds, hg, h, ha => by
    simp only [adigits] at ha
    cases hn : g.xnext with
    | none => simp [hn] at ha
    | some p =>
      obtain ⟨u, g2⟩ := p
      rw [hn] at ha
      simp only at ha
      cases h with
      | more hx hd hrest =>
        obtain ⟨rfl, hg2⟩ := xnext_sound hg hx hn
        rw [hd] at ha
        exact adigits_sound fuel hg2 hrest ha
      | endSpace hx hd hsp =>
        obtain ⟨rfl, hg2⟩ := xnext_sound hg hx hn
        rw [hd] at ha
        simp only at ha
        split at ha
        · rename_i m hm
          have e := ameaning_sound hg2.1 hm
          simp only [Concrete.spacerTok, e] at hsp
          simp only [hsp, ↓reduceIte, Option.some.injEq, Prod.mk.injEq] at ha
          obtain ⟨rfl, rfl⟩ := ha
          exact ⟨rfl, hg2⟩
        · cases ha
      | endOther hx hd hsp =>
        obtain ⟨rfl, hg2⟩ := xnext_sound hg hx hn
        rw [hd] at ha
        simp only at ha
        split at ha
        · rename_i m hm
          have e := ameaning_sound hg2.1 hm
          simp only [Concrete.spacerTok, e] at hsp
          simp only [hsp, Bool.false_eq_true, ↓reduceIte, Option.some.injEq, Prod.mk.injEq] at ha
          obtain ⟨rfl, rfl⟩ := ha
          exact ⟨rfl, back_sound _ hg2⟩
        · cases ha

theorem digit_not_num {s : State} {t : Token} {d : Nat} (h : Concrete.isDigitTok 10 t = some d) :
    Concrete.numTok s t = false := by
  cases t with
  | char c k => cases k <;> simp_all [Concrete.isDigitTok, Concrete.numTok, Concrete.internalTok, Concrete.meaningTok]
  | _ => simp [Concrete.isDigitTok] at h

/-- One optional space after a constant (§ 443). -/
def GState.optSpace (g : GState) : Option GState :=
  match g.xnext with
  | some (t, g1) =>
    match ameaning g1.a t with
    | some m => if Concrete.isSpacer m then some g1 else some (g1.back t)
    | none => none
  | none => none

theorem optSpace_sound {E : Concrete.Engine} {g g' : GState} {s s1 : State}
    (hg : g.γ s) (h : Concrete.OptSpace E s s1) (ha : g.optSpace = some g') : g'.γ s1 := by
  unfold GState.optSpace at ha
  split at ha
  · rename_i t g1 hx
    split at ha
    · rename_i m hm
      cases h with
      | space hx' hsp =>
        obtain ⟨rfl, hg1⟩ := xnext_sound hg hx' hx
        have e := ameaning_sound hg1.1 hm
        simp only [Concrete.spacerTok, e] at hsp
        simp only [hsp, ↓reduceIte, Option.some.injEq] at ha
        subst ha; exact hg1
      | other hx' hsp =>
        obtain ⟨rfl, hg1⟩ := xnext_sound hg hx' hx
        have e := ameaning_sound hg1.1 hm
        simp only [Concrete.spacerTok, e] at hsp
        simp only [hsp, Bool.false_eq_true, ↓reduceIte, Option.some.injEq] at ha
        subst ha; exact back_sound _ hg1
    · cases ha
  · cases ha

/-- The code of an alphabetic constant's token (§ 442): a character, an
active character or a one-letter name. -/
def alphaCode : Token → Option Nat
  | .char d _ => some d
  | .cs (.active d) => some d
  | .noexp (.active d) => some d
  | .cs (.name n) => match Concrete.strCodes n with | [c] => some c | _ => none
  | .noexp (.name n) => match Concrete.strCodes n with | [c] => some c | _ => none
  | _ => none

theorem charMax_ge (E : Concrete.Engine) : 255 ≤ Concrete.charMax E := by
  unfold Concrete.charMax; split <;> omega

/-- `scan_int` (§ 440) for explicit decimal digits or an alphabetic
constant (`` `c ``, § 442) with a code up to 255, without a sign. -/
def GState.scanInt (g : GState) : Option (Int × GState) :=
  match g.xnonblank with
  | some (t, g1) =>
    if t = Concrete.otherChar '`' then
      match g1.next with
      | some (u, g2) =>
        match alphaCode u with
        | some c => if c ≤ 255 then g2.optSpace.map fun g3 => (Int.ofNat c, g3) else none
        | none => none
      | none => none
    else
      match Concrete.isDigitTok 10 t with
      | some d =>
        (adigits (isize g1.a.input + 1) g1 d).map fun p => (min (Int.ofNat p.1) infinity, p.2)
      | none => none
  | none => none

theorem signs_plain {E : Concrete.Engine} {g g1 : GState} {s s1 : State} {neg : Bool} {t t' : Token}
    (hg : g.γ s) (h : Concrete.Signs E s neg t s1) (ha : g.xnonblank = some (t', g1))
    (hp : t' ≠ Concrete.otherChar '+') (hm : t' ≠ Concrete.otherChar '-') :
    neg = false ∧ t = t' ∧ g1.γ s1 := by
  cases h with
  | done hx _ _ =>
    obtain ⟨rfl, hg1⟩ := xnonblank_sound hg hx ha
    exact ⟨rfl, rfl, hg1⟩
  | plus hx _ =>
    obtain ⟨rfl, -⟩ := xnonblank_sound hg hx ha
    exact absurd rfl hp
  | minus hx _ =>
    obtain ⟨rfl, -⟩ := xnonblank_sound hg hx ha
    exact absurd rfl hm

theorem digit_not_sign {t : Token} {d : Nat} (hd : Concrete.isDigitTok 10 t = some d) :
    t ≠ Concrete.otherChar '+' ∧ t ≠ Concrete.otherChar '-' := by
  constructor <;> (rintro rfl; simp [Concrete.isDigitTok, Concrete.otherChar] at hd)

theorem scanInt_sound {E : Concrete.Engine} {g g' : GState} {s s1 : State} {n n' : Int}
    (hg : g.γ s) (h : Concrete.ScanInt E s n s1) (ha : g.scanInt = some (n', g')) :
    n = n' ∧ g'.γ s1 := by
  unfold GState.scanInt at ha
  split at ha
  · rename_i t g1 hx
    split at ha
    · -- an alphabetic constant
      rename_i hq
      subst hq
      have hpm : Concrete.otherChar '`' ≠ Concrete.otherChar '+' ∧
          Concrete.otherChar '`' ≠ Concrete.otherChar '-' := by
        simp [Concrete.otherChar]
      split at ha
      · rename_i u g2 hn
        split at ha
        · rename_i c hc
          split at ha
          · rename_i hle
            simp only [Option.map_eq_some_iff, Prod.mk.injEq] at ha
            obtain ⟨g3, ho, rfl, rfl⟩ := ha
            cases h with
            | internal hs hnum _ =>
              obtain ⟨-, rfl, -⟩ := signs_plain hg hs hx hpm.1 hpm.2
              simp [Concrete.numTok, Concrete.internalTok, Concrete.meaningTok, Concrete.otherChar] at hnum
            | @alpha _ s1' s2' s3' neg' u' c' hs hn' hm _ hsp =>
              obtain ⟨rfl, -, hg1⟩ := signs_plain hg hs hx hpm.1 hpm.2
              obtain ⟨e1, -, hg2⟩ := gnext_sound hg1 hn' hn
              subst e1
              have e : c' = c := by
                clear hn hn' hg2 hsp ho
                revert hm hc
                rcases u' with ⟨d, k⟩ | ⟨n | d⟩ | ⟨n | d⟩ | n <;> intro hm hc <;>
                  simp only [alphaCode, Option.some.injEq, reduceCtorEq] at hc hm
                all_goals first
                  | omega
                  | (rw [hm] at hc; simpa using hc)
              subst e
              exact ⟨by simp, optSpace_sound hg2 hsp ho⟩
            | @alphaBad _ s1' s2' neg' u' hs hn' hm =>
              obtain ⟨-, -, hg1⟩ := signs_plain hg hs hx hpm.1 hpm.2
              obtain ⟨e1, -, -⟩ := gnext_sound hg1 hn' hn
              subst e1
              have := charMax_ge E
              clear hn hn' ho
              revert hm hc
              rcases u' with ⟨d, k⟩ | ⟨n | d⟩ | ⟨n | d⟩ | n <;> intro hm hc <;>
                simp only [alphaCode, Option.some.injEq, reduceCtorEq] at hc hm
              all_goals first
                | (have hm' : Concrete.charMax E < (d : Nat) := hm; omega)
                | (split at hc
                   · rename_i c0 e
                     simp only [Option.some.injEq] at hc
                     subst hc
                     simp only [e, List.length_singleton, ne_eq, not_true_eq_false, List.cons.injEq,
                       and_true, forall_eq', false_or] at hm
                     have hm' : Concrete.charMax E < (c0 : Nat) := hm
                     omega
                   · cases hc)
            | decimal hs hd' _ =>
              obtain ⟨-, rfl, -⟩ := signs_plain hg hs hx hpm.1 hpm.2
              simp [Concrete.isDigitTok, Concrete.otherChar] at hd'
            | octal hs _ _ _ =>
              obtain ⟨-, e, -⟩ := signs_plain hg hs hx hpm.1 hpm.2
              simp [Concrete.otherChar] at e
            | hex hs _ _ _ =>
              obtain ⟨-, e, -⟩ := signs_plain hg hs hx hpm.1 hpm.2
              simp [Concrete.otherChar] at e
            | vacuous hq' hs _ _ =>
              obtain ⟨-, rfl, -⟩ := signs_plain hg hs hx hpm.1 hpm.2
              rcases hq' with ⟨e, -⟩ | ⟨e, -⟩ <;> simp [Concrete.otherChar] at e
            | missing hs _ hne _ _ _ =>
              obtain ⟨-, rfl, -⟩ := signs_plain hg hs hx hpm.1 hpm.2
              exact absurd rfl hne
          · cases ha
        · cases ha
      · cases ha
    · -- decimal digits
      split at ha
      · rename_i d hd
        have hpm := digit_not_sign hd
        simp only [Option.map_eq_some_iff, Prod.mk.injEq] at ha
        obtain ⟨q, hq, rfl, rfl⟩ := ha
        cases h with
        | internal hs hnum _ =>
          obtain ⟨-, rfl, -⟩ := signs_plain hg hs hx hpm.1 hpm.2
          rw [digit_not_num hd] at hnum; cases hnum
        | alpha hs _ _ _ _ =>
          obtain ⟨-, rfl, -⟩ := signs_plain hg hs hx hpm.1 hpm.2
          simp [Concrete.isDigitTok, Concrete.otherChar] at hd
        | alphaBad hs _ _ =>
          obtain ⟨-, rfl, -⟩ := signs_plain hg hs hx hpm.1 hpm.2
          simp [Concrete.isDigitTok, Concrete.otherChar] at hd
        | decimal hs hd' hdg =>
          obtain ⟨rfl, rfl, hg1⟩ := signs_plain hg hs hx hpm.1 hpm.2
          rw [hd] at hd'
          cases hd'
          obtain ⟨e, hg2⟩ := adigits_sound _ hg1 hdg hq
          cases e
          refine ⟨by simp, ?_⟩
          split
          · exact raise_γ _ hg2
          · exact hg2
        | octal hs _ _ _ =>
          obtain ⟨-, rfl, -⟩ := signs_plain hg hs hx hpm.1 hpm.2
          simp [Concrete.isDigitTok, Concrete.otherChar] at hd
        | hex hs _ _ _ =>
          obtain ⟨-, rfl, -⟩ := signs_plain hg hs hx hpm.1 hpm.2
          simp [Concrete.isDigitTok, Concrete.otherChar] at hd
        | vacuous hq' hs _ _ =>
          obtain ⟨-, rfl, -⟩ := signs_plain hg hs hx hpm.1 hpm.2
          rcases hq' with ⟨rfl, -⟩ | ⟨rfl, -⟩ <;> simp [Concrete.isDigitTok, Concrete.otherChar] at hd
        | missing hs hnd _ _ _ _ =>
          obtain ⟨-, rfl, -⟩ := signs_plain hg hs hx hpm.1 hpm.2
          rw [hd] at hnd; cases hnd
      · cases ha
  · cases ha

/-- `scan_optional_equals` (§ 405). -/
def GState.optEquals (g : GState) : Option GState :=
  match g.xnonblank with
  | some (t, g1) => if t = Concrete.otherChar '=' then some g1 else some (g1.back t)
  | none => none

theorem optEquals_sound {E : Concrete.Engine} {g g' : GState} {s s1 : State}
    (hg : g.γ s) (h : Concrete.OptEquals E s s1) (ha : g.optEquals = some g') : g'.γ s1 := by
  unfold GState.optEquals at ha
  split at ha
  · rename_i t g1 hx
    cases h with
    | eq hxb =>
      obtain ⟨rfl, hg1⟩ := xnonblank_sound hg hxb hx
      simp only [↓reduceIte, Option.some.injEq] at ha
      subst ha; exact hg1
    | absent hxb hne =>
      obtain ⟨rfl, hg1⟩ := xnonblank_sound hg hxb hx
      simp only [hne, ↓reduceIte, Option.some.injEq] at ha
      subst ha; exact back_sound _ hg1
  · cases ha

theorem raise_pending {s : State} (msg : String) : (Concrete.raise msg s).pending = s.pending := by
  unfold Concrete.raise Concrete.logOut
  dsimp only
  split <;> rfl

/-- Whether `v` is a valid code for `tab` (§ 1232), where that does not
depend on the engine: `\lccode` and `\uccode` reach 255 or 1114111. -/
def acodeOk (tab : CodeTable) (v : Int) : Option Bool :=
  match tab with
  | .lc | .uc => if v ≤ 255 then some (decide (0 ≤ v)) else if v > 1114111 then some false else none
  | .cat => some (decide (0 ≤ v ∧ v ≤ 15))
  | .sf => some (decide (0 ≤ v ∧ v ≤ 32767))
  | .math => some (decide (0 ≤ v ∧ v ≤ 32768))
  | .del => some (decide (v ≤ 16777215))

theorem acodeOk_sound (E : Concrete.Engine) {tab : CodeTable} {v : Int} {b : Bool}
    (h : acodeOk tab v = some b) :
    ((0 ≤ v ∨ tab = .del) ∧ v ≤ Concrete.codeMax E tab) ↔ b = true := by
  cases tab <;> simp only [acodeOk, Concrete.codeMax] at h ⊢
  all_goals first
    | (simp only [Option.some.injEq] at h; subst h; simp <;> omega)
    | (split at h
       · simp only [Option.some.injEq] at h; subst h; split <;> simp <;> omega
       · split at h
         · simp only [Option.some.injEq] at h; subst h; split <;> simp <;> omega
         · cases h)

end Satex.Soundness
