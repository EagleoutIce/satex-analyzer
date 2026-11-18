/-
Satex.Conds: conditionals with a test the reader decides (§§ 487–510).

Intuition. A conditional pushes a level on the condition stack, runs its
test, and on a false test skips tokens to its `\else` or `\fi`. Skipping
does not expand: it reads tokens and looks only at their meanings, to
nest inner conditionals (`pass_text`, § 494). So the abstract skip is a
loop over `GState.next` that needs each skipped token's meaning.

Choices. Only tests the reader decides without reading anything are
followed: `\iftrue` and `\iffalse`, also under names `\let` to them (as
`\newif` makes). The condition stack must be exact (no `many` frame):
then its length is TeX's, and `setLimitAt` hits the top level. One path
per test: the abstract run never forks here. Joining undecided arms at
`\fi` (`branch_sound`) is the alternative for tests the reader cannot
decide. It needs a join in `OStep`, which the run does not have yet.

Written with AI assistance from Claude Code (Claude Opus 5.5), directed and
reviewed by the author.
-/
import Satex.Operands

namespace Satex.Soundness

open Satex Satex.Abstract

/-! ## 1. The condition stack -/

/-- No `many` frame: the stack is TeX's, level for level. -/
def condExact : List ACond → Bool
  | [] => true
  | a :: rest => !a.many && condExact rest

theorem condRel_length : ∀ {ac : List ACond} {cc : List CondLevel}, CondRel ac cc →
    condExact ac = true → cc.length = ac.length := by
  intro ac cc h
  induction h with
  | nil => intro _; rfl
  | one hm _ _ _ ih =>
    intro he
    simp only [condExact, hm, Bool.not_false, Bool.true_and] at he
    simp only [List.length_cons, ih he]
  | manyNone hm _ _ => intro he; simp [condExact, hm] at he
  | manyMore hm _ _ _ => intro he; simp [condExact, hm] at he

theorem setLimitAt_go_id (depth : Nat) (l : IfLimit) (n : Nat) :
    ∀ (cs : List CondLevel) (i : Nat), 1 ≤ i → i + cs.length = n → n = depth + 1 →
      Concrete.setLimitAt.go depth l n i cs = cs
  | [], _, _, _, _ => rfl
  | c :: cs, i, hi, hn, hd => by
    simp only [Concrete.setLimitAt.go]
    have : n - 1 - i ≠ depth := by simp at hn; omega
    simp only [this, ↓reduceIte]
    rw [setLimitAt_go_id depth l n cs (i + 1) (by omega) (by simp at hn; omega) hd]

/-- `setLimitAt` at the depth of the top level changes the top level. -/
theorem setLimitAt_top (k : CondLevel) (cs : List CondLevel) (l : IfLimit) :
    Concrete.setLimitAt (k :: cs) cs.length l = { k with limit := l } :: cs := by
  simp only [Concrete.setLimitAt, Concrete.setLimitAt.go, List.length_cons]
  simp only [Nat.add_sub_cancel, Nat.sub_zero, ↓reduceIte, List.cons.injEq, true_and]
  exact setLimitAt_go_id cs.length l (cs.length + 1) cs 1 (Nat.le_refl _) (by omega) rfl

/-- An exact top level: TeX's top level has its limit. -/
theorem condRel_cons {k : ACond} {ks : List ACond} {cc : List CondLevel}
    (h : CondRel (k :: ks) cc) (hk : k.many = false) :
    ∃ c crest, cc = c :: crest ∧ k.limit = c.limit ∧ CondRel ks crest := by
  cases h with
  | one _ hl _ hrest => exact ⟨_, _, rfl, hl, hrest⟩
  | manyNone hm _ => simp [hm] at hk
  | manyMore hm _ _ => simp [hm] at hk

/-! ## 2. Skipping (§ 494) -/

/-- The command of a meaning (`cmdOf` after `meaningTok`). -/
def cmdOfM : Meaning → Option Cmd
  | .prim _ c => some c
  | _ => none

theorem cmdOf_eq (s : State) (t : Token) : Concrete.cmdOf s t = cmdOfM (Concrete.meaningTok s t) := by
  unfold Concrete.cmdOf cmdOfM; rfl

/-- `pass_text` (§ 494): skip to the `\fi`, `\else` or `\or` of level `l`.
`fuel` makes it total.
source: tex.web § 494 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web; SaTeX `Machine::pass_text` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/analysis/machine.rs -/
def apass : Nat → GState → Nat → Option (FiCode × GState)
  | 0, _, _ => none
  | fuel + 1, g, l =>
    match g.next with
    | some (t, g1) =>
      match ameaning g1.a t with
      | some m =>
        match Concrete.fiCodeOf (cmdOfM m) with
        | some c =>
          if l = 0 then some (c, g1)
          else if c = .fi then apass fuel g1 (l - 1) else apass fuel g1 l
        | none => if Concrete.isIfTest (cmdOfM m) then apass fuel g1 (l + 1) else apass fuel g1 l
      | none => none
    | none => none

theorem apass_sound : ∀ {s : State} {l : Nat} {c : FiCode} {s1 : State}, Concrete.PassText s l c s1 →
    ∀ (fuel : Nat) (g g' : GState) (c' : FiCode), g.γ s → apass fuel g l = some (c', g') →
      c = c' ∧ g'.γ s1 ∧ s1.cond = s.cond := by
  intro s l c s1 h
  induction h with
  | @stop s s1 t c hn hf =>
    intro fuel g g' c' hg ha
    cases fuel with
    | zero => cases ha
    | succ f =>
      simp only [apass] at ha
      cases hx : g.next with
      | none => simp [hx] at ha
      | some p =>
        obtain ⟨u, g1⟩ := p
        obtain ⟨rfl, hs1, hg1⟩ := gnext_sound hg hn hx
        rw [hx] at ha
        simp only at ha
        split at ha
        · rename_i m hm
          have e := ameaning_sound hg1.1 hm
          rw [cmdOf_eq, e] at hf
          simp only [hf, ↓reduceIte, Option.some.injEq, Prod.mk.injEq] at ha
          obtain ⟨rfl, rfl⟩ := ha
          exact ⟨rfl, hg1, by rw [hs1]⟩
        · cases ha
  | @close s s1 s2 t l c hn hf _ ih =>
    intro fuel g g' c' hg ha
    cases fuel with
    | zero => cases ha
    | succ f =>
      simp only [apass] at ha
      cases hx : g.next with
      | none => simp [hx] at ha
      | some p =>
        obtain ⟨u, g1⟩ := p
        obtain ⟨rfl, hs1, hg1⟩ := gnext_sound hg hn hx
        rw [hx] at ha
        simp only at ha
        split at ha
        · rename_i m hm
          have e := ameaning_sound hg1.1 hm
          rw [cmdOf_eq, e] at hf
          simp only [hf, Nat.add_one_ne_zero, ↓reduceIte, Nat.add_sub_cancel] at ha
          obtain ⟨h1, h2, h3⟩ := ih f g1 g' c' hg1 ha
          exact ⟨h1, h2, by rw [h3, hs1]⟩
        · cases ha
  | @innerElse s s1 s2 t l c c0 hn hf hne _ ih =>
    intro fuel g g' c' hg ha
    cases fuel with
    | zero => cases ha
    | succ f =>
      simp only [apass] at ha
      cases hx : g.next with
      | none => simp [hx] at ha
      | some p =>
        obtain ⟨u, g1⟩ := p
        obtain ⟨rfl, hs1, hg1⟩ := gnext_sound hg hn hx
        rw [hx] at ha
        simp only at ha
        split at ha
        · rename_i m hm
          have e := ameaning_sound hg1.1 hm
          rw [cmdOf_eq, e] at hf
          simp only [hf, Nat.add_one_ne_zero, hne, ↓reduceIte] at ha
          obtain ⟨h1, h2, h3⟩ := ih f g1 g' c' hg1 ha
          exact ⟨h1, h2, by rw [h3, hs1]⟩
        · cases ha
  | @openIf s s1 s2 t l c hn hi _ ih =>
    intro fuel g g' c' hg ha
    cases fuel with
    | zero => cases ha
    | succ f =>
      simp only [apass] at ha
      cases hx : g.next with
      | none => simp [hx] at ha
      | some p =>
        obtain ⟨u, g1⟩ := p
        obtain ⟨rfl, hs1, hg1⟩ := gnext_sound hg hn hx
        rw [hx] at ha
        simp only at ha
        split at ha
        · rename_i m hm
          have e := ameaning_sound hg1.1 hm
          rw [cmdOf_eq, e] at hi
          have hf : Concrete.fiCodeOf (cmdOfM m) = none := by
            revert hi; cases cmdOfM m with
            | none => intro; rfl
            | some cm => cases cm <;> simp [Concrete.isIfTest, Concrete.fiCodeOf]
          simp only [hf, hi, ↓reduceIte] at ha
          obtain ⟨h1, h2, h3⟩ := ih f g1 g' c' hg1 ha
          exact ⟨h1, h2, by rw [h3, hs1]⟩
        · cases ha
  | @other s s1 s2 t l c hn hi hf _ ih =>
    intro fuel g g' c' hg ha
    cases fuel with
    | zero => cases ha
    | succ f =>
      simp only [apass] at ha
      cases hx : g.next with
      | none => simp [hx] at ha
      | some p =>
        obtain ⟨u, g1⟩ := p
        obtain ⟨rfl, hs1, hg1⟩ := gnext_sound hg hn hx
        rw [hx] at ha
        simp only at ha
        split at ha
        · rename_i m hm
          have e := ameaning_sound hg1.1 hm
          rw [cmdOf_eq, e] at hi hf
          simp only [hf, hi, Bool.false_eq_true, ↓reduceIte] at ha
          obtain ⟨h1, h2, h3⟩ := ih f g1 g' c' hg1 ha
          exact ⟨h1, h2, by rw [h3, hs1]⟩
        · cases ha

/-- After a false test (§ 500): skip to the `\else` or `\fi` of level 0.
An `\or` there is "Extra \or" and skipping goes on. -/
def askip : Nat → GState → Option (FiCode × GState)
  | 0, _ => none
  | fuel + 1, g =>
    match apass (isize g.a.input + 1) g 0 with
    | some (.orCode, g1) => askip fuel g1
    | some (c, g1) => some (c, g1)
    | none => none

theorem askip_sound : ∀ {s : State} {d : Nat} {s3 : State}, Concrete.SkipFalse s d s3 →
    s.cond.length = d + 1 →
    ∀ (fuel : Nat) (g g1 : GState) (c : FiCode), g.γ s → askip fuel g = some (c, g1) →
      ∃ s1, g1.γ s1 ∧ s1.cond = s.cond ∧
        (c = .fi ∧ s3 = { s1 with cond := s1.cond.drop 1 } ∨
         c = .elseCode ∧ s3 = { s1 with cond := Concrete.setLimitAt s1.cond d .fiCode }) := by
  intro s d s3 h
  induction h with
  | @toElse s s1 d hp hl =>
    intro _ fuel g g1 c hg ha
    cases fuel with
    | zero => cases ha
    | succ f =>
      simp only [askip] at ha
      split at ha
      · rename_i g2 hx
        obtain ⟨e, -, -⟩ := apass_sound hp _ g g2 _ hg hx
        cases e
      · rename_i c0 g2 hor hx
        simp only [Option.some.injEq, Prod.mk.injEq] at ha
        obtain ⟨rfl, rfl⟩ := ha
        obtain ⟨rfl, hg2, hc⟩ := apass_sound hp _ g _ _ hg hx
        exact ⟨s1, hg2, hc, Or.inr ⟨rfl, rfl⟩⟩
      · cases ha
  | @toFi s s1 d hp hl =>
    intro _ fuel g g1 c hg ha
    cases fuel with
    | zero => cases ha
    | succ f =>
      simp only [askip] at ha
      split at ha
      · rename_i g2 hx
        obtain ⟨e, -, -⟩ := apass_sound hp _ g g2 _ hg hx
        cases e
      · rename_i c0 g2 hor hx
        simp only [Option.some.injEq, Prod.mk.injEq] at ha
        obtain ⟨rfl, rfl⟩ := ha
        obtain ⟨rfl, hg2, hc⟩ := apass_sound hp _ g _ _ hg hx
        exact ⟨s1, hg2, hc, Or.inl ⟨rfl, rfl⟩⟩
      · cases ha
  | @extraOr s s1 s2 d hp hl _ ih =>
    intro hd fuel g g1 c hg ha
    cases fuel with
    | zero => cases ha
    | succ f =>
      simp only [askip] at ha
      split at ha
      · rename_i g2 hx
        obtain ⟨-, hg2, hc⟩ := apass_sound hp _ g g2 _ hg hx
        have hr : (Concrete.raise "Extra \\or" s1).cond = s1.cond := by
          unfold Concrete.raise Concrete.logOut; dsimp only; split <;> rfl
        obtain ⟨s4, h1, h2, h3⟩ := ih (by rw [hr, hc]; exact hd) f g2 g1 c (raise_γ _ hg2) ha
        exact ⟨s4, h1, by rw [h2, hr, hc], h3⟩
      · rename_i c0 g2 hor hx
        obtain ⟨rfl, -, -⟩ := apass_sound hp _ g _ _ hg hx
        exact absurd rfl hor
      · cases ha
  | @innerFi s s1 s2 d hp hl _ _ =>
    intro hd fuel g g1 c hg ha
    cases fuel with
    | zero => cases ha
    | succ f =>
      simp only [askip] at ha
      split at ha
      · rename_i g2 hx
        obtain ⟨-, -, hc⟩ := apass_sound hp _ g g2 _ hg hx
        rw [hc] at hl; omega
      · rename_i c0 g2 hor hx
        obtain ⟨-, -, hc⟩ := apass_sound hp _ g _ _ hg hx
        rw [hc] at hl; omega
      · cases ha
  | @innerOther s s1 s2 d c hp hne hl _ _ =>
    intro hd fuel g g1 c' hg ha
    cases fuel with
    | zero => cases ha
    | succ f =>
      simp only [askip] at ha
      split at ha
      · rename_i g2 hx
        obtain ⟨-, -, hc⟩ := apass_sound hp _ g g2 _ hg hx
        rw [hc] at hl; omega
      · rename_i c0 g2 hor hx
        obtain ⟨-, -, hc⟩ := apass_sound hp _ g _ _ hg hx
        rw [hc] at hl; omega
      · cases ha

/-- `\else` or `\or` met in a branch taken (§ 510): skip to the `\fi`. -/
def askipFi : Nat → GState → Option GState
  | 0, _ => none
  | fuel + 1, g =>
    match apass (isize g.a.input + 1) g 0 with
    | some (.fi, g1) => some g1
    | some (_, g1) => askipFi fuel g1
    | none => none

theorem askipFi_sound : ∀ {s s2 : State}, Concrete.SkipToFi s s2 →
    ∀ (fuel : Nat) (g g1 : GState), g.γ s → askipFi fuel g = some g1 → g1.γ s2 ∧ s2.cond = s.cond := by
  intro s s2 h
  induction h with
  | @fi s s1 hp =>
    intro fuel g g1 hg ha
    cases fuel with
    | zero => cases ha
    | succ f =>
      simp only [askipFi] at ha
      split at ha
      · rename_i g2 hx
        simp only [Option.some.injEq] at ha
        subst ha
        obtain ⟨-, hg2, hc⟩ := apass_sound hp _ g _ _ hg hx
        exact ⟨hg2, hc⟩
      · rename_i c0 g2 hfi hx
        obtain ⟨rfl, -, -⟩ := apass_sound hp _ g _ _ hg hx
        exact absurd rfl hfi
      · cases ha
  | @more s s1 s2 c hp hne _ ih =>
    intro fuel g g1 hg ha
    cases fuel with
    | zero => cases ha
    | succ f =>
      simp only [askipFi] at ha
      split at ha
      · rename_i g2 hx
        obtain ⟨e, -, -⟩ := apass_sound hp _ g _ _ hg hx
        exact absurd e hne
      · rename_i c0 g2 hfi hx
        obtain ⟨-, hg2, hc⟩ := apass_sound hp _ g _ _ hg hx
        obtain ⟨h1, h2⟩ := ih f g2 g1 hg2 ha
        exact ⟨h1, by rw [h2, hc]⟩
      · cases ha

end Satex.Soundness
