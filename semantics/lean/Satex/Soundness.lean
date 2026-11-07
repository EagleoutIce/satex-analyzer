/-
Satex.Soundness: the abstract run of `Satex.Abstract` over-approximates every
concrete run of `Satex.Concrete`, and it terminates.

Written with AI assistance from Claude Code (Claude Opus 5.5; build fixes with
Claude Sonnet 5.5), directed and reviewed by the author.

Intuition. Soundness is a simulation: from a covered state, every concrete
step has an abstract run that covers its result. Termination is a budget:
every abstract step charges it, and at zero the run is cut to a state that
covers everything. So precision may be lost but never soundness, and the
run always ends (`satex_sound_and_terminating`).

Part 1 is domain-independent: orders, the loop with k passes and a summary,
simulation, the budget. Part 2 instantiates it with the domains and
transfers of `Satex.Abstract` against `Satex.Concrete`'s `Step`: one lemma
per transfer, then the run theorem by family (section 8). What is still a
hypothesis: semantics/PROOF-STATUS.md. Plain core Lean 4, no mathlib.
-/
import Satex.Core
import Satex.Concrete
import Satex.Abstract

namespace Satex.Soundness

/-! ## Traces -/

/-- Reflexive transitive closure (core Lean has none). -/
inductive Star {α : Type} (r : α → α → Prop) (a : α) : α → Prop where
  | refl : Star r a a
  | tail {b c : α} : Star r a b → r b c → Star r a c

theorem Star.single {α : Type} {r : α → α → Prop} {a b : α} (h : r a b) : Star r a b :=
  Star.tail Star.refl h

theorem Star.trans {α : Type} {r : α → α → Prop} {a b c : α}
    (h₁ : Star r a b) (h₂ : Star r b c) : Star r a c := by
  induction h₂ with
  | refl => exact h₁
  | tail _ hcd ih => exact Star.tail ih hcd

/-! ## Abstract domains, generically -/

/-- What every abstract domain must provide: a preorder with a join and a
top, and a monotone concretization `γ`. -/
structure AbsDom (C A : Type) where
  le : A → A → Prop
  join : A → A → A
  top : A
  γ : A → C → Prop
  le_refl : ∀ a, le a a
  le_trans : ∀ a b c, le a b → le b c → le a c
  join_l : ∀ a b, le a (join a b)
  join_r : ∀ a b, le b (join a b)
  le_top : ∀ a, le a top
  γ_mono : ∀ a b, le a b → ∀ c, γ a c → γ b c

namespace AbsDom
variable {C A : Type} (D : AbsDom C A)

/-- The join over-approximates the union. -/
theorem γ_join (a b : A) (c : C) (h : D.γ a c ∨ D.γ b c) : D.γ (D.join a b) c := by
  cases h with
  | inl h => exact D.γ_mono _ _ (D.join_l a b) c h
  | inr h => exact D.γ_mono _ _ (D.join_r a b) c h

theorem γ_top (hfull : ∃ a, ∀ c, D.γ a c) (c : C) : D.γ D.top c := by
  obtain ⟨a, ha⟩ := hfull
  exact D.γ_mono _ _ (D.le_top a) c (ha c)

/-- The join is the least upper bound when `le` says so. -/
theorem join_mono (a a' b b' : A)
    (hlub : ∀ x y z, D.le x z → D.le y z → D.le (D.join x y) z)
    (ha : D.le a a') (hb : D.le b b') : D.le (D.join a b) (D.join a' b') :=
  hlub _ _ _ (D.le_trans _ _ _ ha (D.join_l a' b')) (D.le_trans _ _ _ hb (D.join_r a' b'))

/-! ## Transfer functions -/

/-- Local soundness of an abstract transfer `f` for a concrete step relation. -/
def Sound (f : A → A) (Step : C → C → Prop) : Prop :=
  ∀ a c c', D.γ a c → Step c c' → D.γ (f a) c'

/-- An undecided conditional: each arm runs, the results join at `\fi`. -/
theorem sound_fork (ft ff : A → A) (St Sf : C → C → Prop)
    (ht : D.Sound ft St) (hf : D.Sound ff Sf) :
    D.Sound (fun a => D.join (ft a) (ff a)) (fun c c' => St c c' ∨ Sf c c') := by
  intro a c c' hc hs
  cases hs with
  | inl h => exact D.γ_join _ _ _ (Or.inl (ht a c c' hc h))
  | inr h => exact D.γ_join _ _ _ (Or.inr (hf a c c' hc h))

/-- A decided conditional follows one arm exactly. -/
theorem sound_decided (ft ff : A → A) (St Sf : C → C → Prop) (b : Bool)
    (ht : D.Sound ft St) (hf : D.Sound ff Sf) :
    D.Sound (if b then ft else ff) (if b then St else Sf) := by
  cases b
  · exact hf
  · exact ht

/-- Sequencing. -/
theorem sound_comp (f g : A → A) (S T : C → C → Prop)
    (hf : D.Sound f S) (hg : D.Sound g T) :
    D.Sound (fun a => g (f a)) (fun c c'' => ∃ c', S c c' ∧ T c' c'') := by
  intro a c c'' hc hs
  obtain ⟨c', h1, h2⟩ := hs
  exact hg _ _ _ (hf a c c' hc h1) h2

/-- Moving up the lattice keeps a transfer sound (widening, `Unknown`). -/
theorem sound_weaken (f g : A → A) (S : C → C → Prop)
    (hf : D.Sound f S) (hle : ∀ a, D.le (f a) (g a)) : D.Sound g S := by
  intro a c c' hc hs
  exact D.γ_mono _ _ (hle a) c' (hf a c c' hc hs)

/-- The summary `top` is sound for any step. -/
theorem sound_top (S : C → C → Prop) (hfull : ∃ a, ∀ c, D.γ a c) :
    D.Sound (fun _ => D.top) S := by
  intro _ _ c' _ _
  exact D.γ_top hfull c'

/-! ## Loops: k joining passes, then one summary -/

/-- `k` passes that join each round into the head's state; a round that adds
nothing is the fixpoint. After `k` growing rounds the summary is taken. -/
def loopAbs (f : A → A) (stable : A → A → Bool) (summary : A → A) : Nat → A → A
  | 0, a => summary a
  | k + 1, a =>
    if stable (D.join a (f a)) a then a
    else loopAbs f stable summary k (D.join a (f a))

/-- The loop result covers its entry and is a post-fixpoint of the body. -/
theorem loopAbs_post (f : A → A) (stable : A → A → Bool) (summary : A → A)
    (hstable : ∀ x y, stable x y = true → D.le x y)
    (hsum_ext : ∀ a, D.le a (summary a))
    (hsum_post : ∀ a, D.le (f (summary a)) (summary a)) :
    ∀ k a, D.le a (D.loopAbs f stable summary k a) ∧
      D.le (f (D.loopAbs f stable summary k a)) (D.loopAbs f stable summary k a) := by
  intro k
  induction k with
  | zero => intro a; exact ⟨hsum_ext a, hsum_post a⟩
  | succ k ih =>
    intro a
    simp only [loopAbs]
    by_cases h : stable (D.join a (f a)) a = true
    · simp only [h, ↓reduceIte]
      exact ⟨D.le_refl a, D.le_trans _ _ _ (D.join_r a (f a)) (hstable _ _ h)⟩
    · simp only [h, Bool.false_eq_true, ↓reduceIte]
      obtain ⟨h1, h2⟩ := ih (D.join a (f a))
      exact ⟨D.le_trans _ _ _ (D.join_l a (f a)) h1, h2⟩

/-- Every number of concrete passes from a state in `γ a` ends in the
`γ` of the loop's result. -/
theorem loopAbs_sound (f : A → A) (stable : A → A → Bool) (summary : A → A)
    (Body : C → C → Prop) (hf : D.Sound f Body)
    (hstable : ∀ x y, stable x y = true → D.le x y)
    (hsum_ext : ∀ a, D.le a (summary a))
    (hsum_post : ∀ a, D.le (f (summary a)) (summary a))
    (k : Nat) (a : A) (c c' : C) (hc : D.γ a c) (hrun : Star Body c c') :
    D.γ (D.loopAbs f stable summary k a) c' := by
  have hp := D.loopAbs_post f stable summary hstable hsum_ext hsum_post k a
  induction hrun with
  | refl => exact D.γ_mono _ _ hp.1 _ hc
  | tail _ hbc ih =>
    exact D.γ_mono _ _ hp.2 _ (hf _ _ _ ih hbc)

/-- The summary `top` meets the summary conditions. -/
theorem top_summary (f : A → A) :
    (∀ a : A, D.le a ((fun _ : A => D.top) a)) ∧
      (∀ a : A, D.le (f ((fun _ : A => D.top) a)) ((fun _ : A => D.top) a)) :=
  ⟨fun a => D.le_top a, fun _ => D.le_top (f D.top)⟩

/-- The loop runs at most `k` passes plus one summary. -/
def loopPasses (f : A → A) (stable : A → A → Bool) : Nat → A → Nat
  | 0, _ => 1
  | k + 1, a =>
    if stable (D.join a (f a)) a then 1
    else 1 + loopPasses f stable k (D.join a (f a))

theorem loopPasses_le (f : A → A) (stable : A → A → Bool) :
    ∀ k a, D.loopPasses f stable k a ≤ k + 1 := by
  intro k
  induction k with
  | zero => intro a; exact Nat.le_refl 1
  | succ k ih =>
    intro a
    simp only [loopPasses]
    by_cases h : stable (D.join a (f a)) a = true
    · simp only [h, ↓reduceIte]; omega
    · simp only [h, Bool.false_eq_true, ↓reduceIte]; have := ih (D.join a (f a)); omega

end AbsDom

/-! ## Simulation: the abstract run covers every concrete run -/

section Sim
variable {C A : Type}

/-- If every concrete step from a covered state has a covering abstract step,
every concrete trace has a covering abstract trace. -/
theorem simulation (Step : C → C → Prop) (AStep : A → A → Prop) (γ : A → C → Prop)
    (hloc : ∀ a c c', γ a c → Step c c' → ∃ a', AStep a a' ∧ γ a' c') :
    ∀ c₀ c, Star Step c₀ c → ∀ a₀, γ a₀ c₀ → ∃ a, Star AStep a₀ a ∧ γ a c := by
  intro c₀ c h
  induction h with
  | refl => intro a₀ h₀; exact ⟨a₀, Star.refl, h₀⟩
  | tail _ hbc ih =>
    intro a₀ h₀
    obtain ⟨a, ha, hγ⟩ := ih a₀ h₀
    obtain ⟨a', h1, h2⟩ := hloc _ _ _ hγ hbc
    exact ⟨a', Star.tail ha h1, h2⟩

/-- Simulation where one concrete step is covered by several abstract steps
(a primitive that reads a whole argument, a join at `\fi`). -/
theorem simulation_star (Step : C → C → Prop) (AStep : A → A → Prop) (γ : A → C → Prop)
    (hloc : ∀ a c c', γ a c → Step c c' → ∃ a', Star AStep a a' ∧ γ a' c') :
    ∀ c₀ c, Star Step c₀ c → ∀ a₀, γ a₀ c₀ → ∃ a, Star AStep a₀ a ∧ γ a c := by
  intro c₀ c h
  induction h with
  | refl => intro a₀ h₀; exact ⟨a₀, Star.refl, h₀⟩
  | tail _ hbc ih =>
    intro a₀ h₀
    obtain ⟨a, ha, hγ⟩ := ih a₀ h₀
    obtain ⟨a', h1, h2⟩ := hloc _ _ _ hγ hbc
    exact ⟨a', Star.trans ha h1, h2⟩

/-! ## Termination: the never-hang guarantee -/

/-- Every abstract step charges the budget: no infinite abstract run. -/
theorem acc_of_budget (AStep : A → A → Prop) (μ : A → Nat)
    (hdec : ∀ a b, AStep a b → μ b < μ a) :
    ∀ n a, μ a < n → Acc (fun b a => AStep a b) a := by
  intro n
  induction n with
  | zero => intro a ha; exact absurd ha (Nat.not_lt_zero _)
  | succ n ih =>
    intro a ha
    constructor
    intro b hb
    have := hdec a b hb
    exact ih b (by omega)

theorem wf_of_budget (AStep : A → A → Prop) (μ : A → Nat)
    (hdec : ∀ a b, AStep a b → μ b < μ a) :
    WellFounded (fun b a => AStep a b) :=
  ⟨fun a => acc_of_budget AStep μ hdec (μ a + 1) a (Nat.lt_succ_self _)⟩

/-- Runs of exactly `n` steps. -/
inductive StarN {α : Type} (r : α → α → Prop) (a : α) : Nat → α → Prop where
  | refl : StarN r a 0 a
  | tail {n : Nat} {b c : α} : StarN r a n b → r b c → StarN r a (n + 1) c

/-- An abstract run is never longer than its starting budget. -/
theorem run_length_le_budget (AStep : A → A → Prop) (μ : A → Nat)
    (hdec : ∀ a b, AStep a b → μ b < μ a) :
    ∀ n a b, StarN AStep a n b → n + μ b ≤ μ a := by
  intro n a b h
  induction h with
  | refl => omega
  | tail _ hbc ih => have := hdec _ _ hbc; omega

end Sim


/-! # The domains and transfers of `Satex.Abstract` -/

open Satex Satex.Abstract

/-! ## Helpers -/

theorem ite_prop {p : Prop} [Decidable p] {α : Type} {P : α → Prop} {x y : α}
    (hx : p → P x) (hy : ¬p → P y) : P (if p then x else y) := by
  by_cases h : p
  · simp only [h, ↓reduceIte]; exact hx h
  · simp only [h, ↓reduceIte]; exact hy h

theorem upd_same {α β : Type} [DecidableEq α] (f : α → β) (a : α) (b : β) :
    upd f a b a = b := by
  simp [upd]

theorem upd_other {α β : Type} [DecidableEq α] {f : α → β} {a x : α} {b : β}
    (h : x ≠ a) : upd f a b x = f x := by
  simp [upd, h]

/-- A bounded join folded over a list lies above every member. -/
theorem foldl_cover {α : Type} (le : α → α → Prop) (j : α → α → α)
    (hrefl : ∀ x, le x x) (htrans : ∀ x y z, le x y → le y z → le x z)
    (hl : ∀ x y, le x (j x y)) (hr : ∀ x y, le y (j x y)) :
    ∀ (t : List α) (acc : α), le acc (t.foldl j acc) ∧ ∀ x, x ∈ t → le x (t.foldl j acc) := by
  intro t
  induction t with
  | nil => intro acc; exact ⟨hrefl acc, fun x hx => by simp at hx⟩
  | cons y t ih =>
    intro acc
    obtain ⟨h1, h2⟩ := ih (j acc y)
    refine ⟨htrans _ _ _ (hl acc y) h1, ?_⟩
    intro x hx
    rcases List.mem_cons.mp hx with e | e
    · rw [e]; exact htrans _ _ _ (hr acc y) h1
    · exact h2 x e

/-! ## 1. Intervals -/

theorem itv_le_refl (a : Itv) : Itv.le a a := by
  unfold Itv.le; exact ⟨Int.le_refl _, Int.le_refl _⟩

theorem itv_le_trans (a b c : Itv) (h1 : Itv.le a b) (h2 : Itv.le b c) : Itv.le a c := by
  unfold Itv.le at *; constructor <;> omega

/-- The join is the least upper bound. -/
theorem itv_join_lub (a b c : Itv) (ha : Itv.le a c) (hb : Itv.le b c) :
    Itv.le (Itv.join a b) c := by
  simp only [Itv.le, Itv.join] at *
  constructor <;> split <;> omega

/-- The join over-approximates the union. -/
theorem itv_γ_join (a b : Itv) (n : Int) (h : a.γ n ∨ b.γ n) : (Itv.join a b).γ n := by
  cases h with
  | inl h => exact Itv.γ_mono (Itv.le_join_left a b) h
  | inr h => exact Itv.γ_mono (Itv.le_join_right a b) h

/-- Widening is an upper bound of both sides inside the 32-bit word. -/
theorem itv_γ_widen (a b : Itv) (ha : a.inWord) (hb : b.inWord) (n : Int)
    (h : a.γ n ∨ b.γ n) : (Itv.widen a b).γ n := by
  obtain ⟨h1, h2⟩ := Itv.le_widen a b ha hb
  cases h with
  | inl h => exact Itv.γ_mono h1 h
  | inr h => exact Itv.γ_mono h2 h

/-- How many bounds can still move. -/
def itvRank (i : Itv) : Nat :=
  (if i.lo = Itv.wordMin then 0 else 1) + (if i.hi = Itv.wordMax then 0 else 1)

theorem itvRank_le (i : Itv) : itvRank i ≤ 2 := by
  unfold itvRank; split <;> split <;> omega

/-- One widening step: a change costs a moving bound. -/
theorem widen_rank_step (a b : Itv) :
    (if (Itv.widen a b).lo = a.lo ∧ (Itv.widen a b).hi = a.hi then 0 else 1) +
      itvRank (Itv.widen a b) ≤ itvRank a := by
  simp only [itvRank, Itv.widen, Itv.wlo, Itv.whi, Itv.wordMin, Itv.wordMax]
  by_cases h1 : b.lo < a.lo <;> by_cases h2 : a.hi < b.hi <;>
    by_cases h3 : a.lo = -2147483648 <;> by_cases h4 : a.hi = 2147483647 <;>
    simp only [h1, h2, h3, h4, ↓reduceIte, and_true, true_and, and_self, ite_self] <;>
    (repeat' split) <;> omega

/-- Steps of a chain that changed its interval. -/
def changes (f : Nat → Itv) : Nat → Nat
  | 0 => 0
  | n + 1 => changes f n +
      (if (f (n + 1)).lo = (f n).lo ∧ (f (n + 1)).hi = (f n).hi then 0 else 1)

/-- Termination of widening: along any chain `fₙ₊₁ = widen fₙ gₙ` the
interval changes at most twice, so a loop head that widens is stable
after at most two growing rounds. -/
theorem widen_chain (f g : Nat → Itv) (hf : ∀ n, f (n + 1) = Itv.widen (f n) (g n)) :
    ∀ n, changes f n + itvRank (f n) ≤ itvRank (f 0) := by
  intro n
  induction n with
  | zero => simp only [changes] <;> omega
  | succ n ih =>
    have h := widen_rank_step (f n) (g n)
    rw [← hf n] at h
    simp only [changes] <;> omega

theorem widen_chain_changes_le_two (f g : Nat → Itv)
    (hf : ∀ n, f (n + 1) = Itv.widen (f n) (g n)) (n : Nat) : changes f n ≤ 2 := by
  have h1 := widen_chain f g hf n
  have h2 := itvRank_le (f 0)
  omega

/-! ## 2. Glue hulls -/

abbrev ghLe (a b : GlueHull) : Prop :=
  Itv.le a.width b.width ∧ Itv.le a.stretch b.stretch ∧ Itv.le a.stretchOrder b.stretchOrder ∧
    Itv.le a.shrink b.shrink ∧ Itv.le a.shrinkOrder b.shrinkOrder

theorem ghLe_refl (a : GlueHull) : ghLe a a :=
  ⟨itv_le_refl _, itv_le_refl _, itv_le_refl _, itv_le_refl _, itv_le_refl _⟩

theorem ghLe_trans (a b c : GlueHull) (h1 : ghLe a b) (h2 : ghLe b c) : ghLe a c :=
  ⟨itv_le_trans _ _ _ h1.1 h2.1, itv_le_trans _ _ _ h1.2.1 h2.2.1,
    itv_le_trans _ _ _ h1.2.2.1 h2.2.2.1, itv_le_trans _ _ _ h1.2.2.2.1 h2.2.2.2.1,
    itv_le_trans _ _ _ h1.2.2.2.2 h2.2.2.2.2⟩

theorem gh_join_l (a b : GlueHull) : ghLe a (GlueHull.join a b) :=
  ⟨Itv.le_join_left _ _, Itv.le_join_left _ _, Itv.le_join_left _ _,
    Itv.le_join_left _ _, Itv.le_join_left _ _⟩

theorem gh_join_r (a b : GlueHull) : ghLe b (GlueHull.join a b) :=
  ⟨Itv.le_join_right _ _, Itv.le_join_right _ _, Itv.le_join_right _ _,
    Itv.le_join_right _ _, Itv.le_join_right _ _⟩

theorem gh_γ_mono {a b : GlueHull} (h : ghLe a b) {g : Glue} (hg : a.γ g) : b.γ g := by
  obtain ⟨g1, g2, g3, g4, g5⟩ := hg
  unfold GlueHull.γ
  exact ⟨Itv.γ_mono h.1 g1, Itv.γ_mono h.2.1 g2, Itv.γ_mono h.2.2.1 g3,
    Itv.γ_mono h.2.2.2.1 g4, Itv.γ_mono h.2.2.2.2 g5⟩

theorem hullItv_cover (l : List Itv) (i : Itv) (h : i ∈ l) :
    ∃ j, hullItv l = some j ∧ Itv.le i j := by
  cases l with
  | nil => simp at h
  | cons y t =>
    have hc := foldl_cover Itv.le Itv.join itv_le_refl itv_le_trans
      Itv.le_join_left Itv.le_join_right t y
    refine ⟨t.foldl Itv.join y, rfl, ?_⟩
    rcases List.mem_cons.mp h with e | e
    · subst e; exact hc.1
    · exact hc.2 i e

theorem hullGlue_cover (l : List GlueHull) (i : GlueHull) (h : i ∈ l) :
    ∃ j, hullGlue l = some j ∧ ghLe i j := by
  cases l with
  | nil => simp at h
  | cons y t =>
    have hc := foldl_cover ghLe GlueHull.join ghLe_refl ghLe_trans gh_join_l gh_join_r t y
    refine ⟨t.foldl GlueHull.join y, rfl, ?_⟩
    rcases List.mem_cons.mp h with e | e
    · subst e; exact hc.1
    · exact hc.2 i e

/-! ## 3. Values: `AVal` is an abstract domain -/

theorem γ_meanings {ms : List AMeaning} {v : Val} :
    (AVal.meanings ms).γ v ↔ ∃ a m, a ∈ ms ∧ v = .meaning m ∧ a.γ m := Iff.rfl
theorem γ_ints {ms : List Itv} {v : Val} :
    (AVal.ints ms).γ v ↔ ∃ i n, i ∈ ms ∧ v = .int n ∧ i.γ n := Iff.rfl
theorem γ_glues {ms : List GlueHull} {v : Val} :
    (AVal.glues ms).γ v ↔ ∃ h g, h ∈ ms ∧ v = .glue g ∧ h.γ g := Iff.rfl
theorem γ_toks {ms : List AText} {v : Val} :
    (AVal.toks ms).γ v ↔ ∃ a p, a ∈ ms ∧ v = .toks p ∧ AText.γ a p ∧ balanced p = true := Iff.rfl
theorem γ_box {mv : Bool} {wd ht dp : Itv} {v : Val} :
    (AVal.box mv wd ht dp).γ v ↔ (mv = true ∧ v = .box .void) ∨
      ∃ x y z, (v = .box (.hbox x y z) ∨ v = .box (.vbox x y z)) ∧
        wd.γ x ∧ ht.γ y ∧ dp.γ z := Iff.rfl
theorem γ_topv (v : Val) : AVal.top.γ v := True.intro

/-! ### The text hull

A hull forgets structure and keeps one hole whose class is the union of the
items' classes. It is sound because a class only grows under `union`, and
every token an item stands for is admitted by the item's class. -/

/-- The class order as meaning: every token `a` admits, `b` admits. -/
def ClsLe (a b : TClass) : Prop := ∀ t, a.admits t → b.admits t

/-- A class that admits a letter or other has `effHex = hex`. -/
theorem admits_eff {a : TClass} {t : Token} (h : a.admits t) :
    a.c = true ∨ (a.has (kindOf t) = true ∧ (a.effHex = true → hexOk t)) := by
  rcases h with h | ⟨h1, h2⟩
  · exact Or.inl h
  · refine Or.inr ⟨h1, fun he hk => h2 ?_ hk⟩
    unfold TClass.effHex at he
    rcases hk with k | k <;> rw [k] at h1 <;> simp only [TClass.has] at h1 <;> simpa [h1] using he

theorem admits_union {a b : TClass} {t : Token} (h : a.c = true ∨
      (a.has (kindOf t) = true ∧ (a.effHex = true → hexOk t)))
    (hab : ∀ k, a.has k = true → (a.union b).has k = true) (hc : a.c = true → (a.union b).c = true)
    (he : (a.union b).hex = true → a.effHex = true) : (a.union b).admits t := by
  rcases h with h | ⟨h1, h2⟩
  · exact Or.inl (hc h)
  · exact Or.inr ⟨hab _ h1, fun hx => h2 (he hx)⟩

theorem admits_union_l (a b : TClass) : ClsLe a (a.union b) := fun _ h =>
  admits_union (admits_eff h)
    (fun k hk => by cases k <;> simp_all [TClass.union, TClass.norm, TClass.has])
    (fun hc => by simp [TClass.union, TClass.norm, hc])
    (fun hx => by revert hx; simp only [TClass.union, TClass.norm]; cases a.effHex <;> simp)

theorem admits_union_r (a b : TClass) : ClsLe b (a.union b) := fun _ h => by
  rcases admits_eff h with hc | ⟨h1, h2⟩
  · exact Or.inl (by simp [TClass.union, TClass.norm, hc])
  · refine Or.inr ⟨by cases k : kindOf _ <;> rw [k] at h1 <;> simp_all [TClass.union, TClass.norm, TClass.has],
      fun hx => h2 ?_⟩
    revert hx; simp only [TClass.union, TClass.norm]; cases b.effHex <;> simp

theorem clsLe_trans {a b c : TClass} (h1 : ClsLe a b) (h2 : ClsLe b c) : ClsLe a c :=
  fun t h => h2 t (h1 t h)

/-- `\number` writes digits and a sign only. -/
theorem printInt_number (n : Int) : ∀ t, t ∈ printInt n → TClass.number.admits t := by
  intro t ht
  have hd : ∀ ch, ch ∈ Nat.toDigits 10 n.natAbs → TClass.number.admits (.char ch.toNat .other) := by
    intro ch hch
    have h := Nat.isDigit_of_mem_toDigits (by decide) (by decide) hch
    have hc : isDigitCode ch.toNat = true := by
      simp only [Char.isDigit, Bool.and_eq_true, decide_eq_true_eq] at h
      simp only [isDigitCode, Bool.and_eq_true, decide_eq_true_eq]
      exact ⟨h.1, h.2⟩
    right
    simp [kindOf, hc, TClass.has, TClass.number]
  unfold printInt at ht
  split at ht
  · rcases List.mem_cons.mp ht with e | e
    · subst e; right; simp [kindOf, isDigitCode, TClass.has, TClass.number]
    · obtain ⟨ch, hch, e⟩ := List.mem_map.mp e; subst e; exact hd ch hch
  · obtain ⟨ch, hch, e⟩ := List.mem_map.mp ht; subst e; exact hd ch hch

/-- Every token an item stands for is admitted by the item's class. -/
theorem item_admits (it : AItem) (p : TokList) (h : it.γ p) : ∀ t, t ∈ p → it.cls.admits t := by
  intro t ht
  cases it with
  | tok u =>
    have e : p = [u] := h
    subst e
    have e : t = u := List.mem_singleton.mp ht
    subst e
    unfold AItem.cls TClass.admits
    cases k : kindOf t <;> simp [TClass.has, k]
  | hole hl => exact h.1 t ht
  | digits i => obtain ⟨n, _, e⟩ := h; subst e; exact printInt_number n t ht
  | oneOf _ => left; rfl

theorem text_admits : ∀ (x : AText) (p : TokList), AText.γ x p →
    ∀ t, t ∈ p → ∃ it, it ∈ x ∧ it.cls.admits t
  | [], p, h, t, ht => by have e : p = [] := h; subst e; cases ht
  | it :: rest, p, h, t, ht => by
    obtain ⟨p₁, p₂, e, h1, h2⟩ := h
    subst e
    rcases List.mem_append.mp ht with h' | h'
    · exact ⟨it, List.mem_cons_self .., item_admits it p₁ h1 t h'⟩
    · obtain ⟨j, hj, ha⟩ := text_admits rest p₂ h2 t h'
      exact ⟨j, List.mem_cons_of_mem _ hj, ha⟩

/-- A fold of `union` lies above its start and above each folded class. -/
theorem fold_union_cover {β : Type} (f : β → TClass) :
    ∀ (t : List β) (k : TClass), ClsLe k (t.foldl (fun k' b => k'.union (f b)) k) ∧
      ∀ b, b ∈ t → ClsLe (f b) (t.foldl (fun k' b => k'.union (f b)) k)
  | [], k => ⟨fun _ h => h, fun _ hb => by cases hb⟩
  | b :: t, k => by
    obtain ⟨h1, h2⟩ := fold_union_cover f t (k.union (f b))
    refine ⟨clsLe_trans (admits_union_l _ _) h1, fun b' hb' => ?_⟩
    rcases List.mem_cons.mp hb' with e | e
    · subst e; exact clsLe_trans (admits_union_r _ _) h1
    · exact h2 b' e

theorem fold_text_cover :
    ∀ (xs : List AText) (k : TClass), ClsLe k (xs.foldl (fun k x =>
        x.foldl (fun (k' : TClass) (it : AItem) => k'.union it.cls) k) k) ∧
      ∀ x, x ∈ xs → ∀ it, it ∈ x → ClsLe it.cls (xs.foldl (fun k x =>
        x.foldl (fun (k' : TClass) (it : AItem) => k'.union it.cls) k) k)
  | [], k => ⟨fun _ h => h, fun _ hx => by cases hx⟩
  | x :: xs, k => by
    have hx := fold_union_cover AItem.cls x k
    obtain ⟨h1, h2⟩ := fold_text_cover xs (x.foldl (fun k' it => k'.union it.cls) k)
    refine ⟨clsLe_trans hx.1 h1, fun x' hx' it hit => ?_⟩
    rcases List.mem_cons.mp hx' with e | e
    · subst e; exact clsLe_trans (hx.2 it hit) h1
    · exact h2 x' e it hit

/-- The text hull covers every balanced member (PROOF-STATUS P3, P4 fixed:
`hex` restricts letters and others only, and token registers are balanced). -/
theorem hull_sound (xs : List AText) (x : AText) (p : TokList) (hx : x ∈ xs)
    (hp : AText.γ x p) (hb : balanced p = true) : AText.γ (AText.hull xs) p := by
  refine ⟨p, [], (List.append_nil p).symm, ⟨fun t ht => ?_, (fun h => by simp at h),
    (fun _ h => by simp at h), hb⟩, rfl⟩
  obtain ⟨it, hit, ha⟩ := text_admits x p hp t ht
  exact (fold_text_cover xs {}).2 x hx it hit t ha

theorem join_sound_meanings (L : Limits) (xs ys : List AMeaning) (v : Val)
    (h : (AVal.meanings xs).γ v ∨ (AVal.meanings ys).γ v) :
    (AVal.join L (.meanings xs) (.meanings ys)).γ v := by
  have hm : ∃ a m, a ∈ xs ++ ys ∧ v = .meaning m ∧ a.γ m := by
    rcases h with h | h
    · obtain ⟨a, m, hi, hv, hn⟩ := γ_meanings.mp h
      exact ⟨a, m, List.mem_append.mpr (Or.inl hi), hv, hn⟩
    · obtain ⟨a, m, hi, hv, hn⟩ := γ_meanings.mp h
      exact ⟨a, m, List.mem_append.mpr (Or.inr hi), hv, hn⟩
  simp only [AVal.join]
  exact ite_prop (P := fun r => AVal.γ r v) (fun _ => γ_meanings.mpr hm) (fun _ => γ_topv v)

theorem join_sound_ints (L : Limits) (xs ys : List Itv) (v : Val)
    (h : (AVal.ints xs).γ v ∨ (AVal.ints ys).γ v) :
    (AVal.join L (.ints xs) (.ints ys)).γ v := by
  have hm : ∃ i n, i ∈ xs ++ ys ∧ v = .int n ∧ i.γ n := by
    rcases h with h | h
    · obtain ⟨i, n, hi, hv, hn⟩ := γ_ints.mp h
      exact ⟨i, n, List.mem_append.mpr (Or.inl hi), hv, hn⟩
    · obtain ⟨i, n, hi, hv, hn⟩ := γ_ints.mp h
      exact ⟨i, n, List.mem_append.mpr (Or.inr hi), hv, hn⟩
  simp only [AVal.join]
  refine ite_prop (P := fun r => AVal.γ r v) (fun _ => γ_ints.mpr hm) (fun _ => ?_)
  obtain ⟨i, n, hi, hv, hn⟩ := hm
  obtain ⟨j, hj, hle⟩ := hullItv_cover (xs ++ ys) i hi
  rw [hj]
  exact γ_ints.mpr ⟨j, n, List.mem_singleton.mpr rfl, hv, Itv.γ_mono hle hn⟩

theorem join_sound_glues (L : Limits) (xs ys : List GlueHull) (v : Val)
    (h : (AVal.glues xs).γ v ∨ (AVal.glues ys).γ v) :
    (AVal.join L (.glues xs) (.glues ys)).γ v := by
  have hm : ∃ i g, i ∈ xs ++ ys ∧ v = .glue g ∧ i.γ g := by
    rcases h with h | h
    · obtain ⟨i, g, hi, hv, hn⟩ := γ_glues.mp h
      exact ⟨i, g, List.mem_append.mpr (Or.inl hi), hv, hn⟩
    · obtain ⟨i, g, hi, hv, hn⟩ := γ_glues.mp h
      exact ⟨i, g, List.mem_append.mpr (Or.inr hi), hv, hn⟩
  simp only [AVal.join]
  refine ite_prop (P := fun r => AVal.γ r v) (fun _ => γ_glues.mpr hm) (fun _ => ?_)
  obtain ⟨i, g, hi, hv, hn⟩ := hm
  obtain ⟨j, hj, hle⟩ := hullGlue_cover (xs ++ ys) i hi
  rw [hj]
  exact γ_glues.mpr ⟨j, g, List.mem_singleton.mpr rfl, hv, gh_γ_mono hle hn⟩

theorem join_sound_toks (L : Limits) (xs ys : List AText) (v : Val)
    (h : (AVal.toks xs).γ v ∨ (AVal.toks ys).γ v) :
    (AVal.join L (.toks xs) (.toks ys)).γ v := by
  have hm : ∃ x p, x ∈ xs ++ ys ∧ v = .toks p ∧ AText.γ x p ∧ balanced p = true := by
    rcases h with h | h
    · obtain ⟨x, p, hi, hv, hn⟩ := γ_toks.mp h
      exact ⟨x, p, List.mem_append.mpr (Or.inl hi), hv, hn⟩
    · obtain ⟨x, p, hi, hv, hn⟩ := γ_toks.mp h
      exact ⟨x, p, List.mem_append.mpr (Or.inr hi), hv, hn⟩
  simp only [AVal.join]
  refine ite_prop (P := fun r => AVal.γ r v) (fun _ => γ_toks.mpr hm) (fun _ => ?_)
  obtain ⟨x, p, hx, hv, hp, hb⟩ := hm
  exact γ_toks.mpr ⟨AText.hull (xs ++ ys), p, List.mem_singleton.mpr rfl, hv,
    hull_sound _ x p hx hp hb, hb⟩

theorem join_sound_box (L : Limits) (mv : Bool) (wd ht dp : Itv) (mv' : Bool) (wd' ht' dp' : Itv)
    (v : Val) (h : (AVal.box mv wd ht dp).γ v ∨ (AVal.box mv' wd' ht' dp').γ v) :
    (AVal.join L (.box mv wd ht dp) (.box mv' wd' ht' dp')).γ v := by
  simp only [AVal.join]
  apply γ_box.mpr
  rcases h with h | h
  · rcases γ_box.mp h with ⟨hm, hv⟩ | ⟨x, y, z, hk, hx, hy, hz⟩
    · left; subst hm; exact ⟨rfl, hv⟩
    · right
      exact ⟨x, y, z, hk, Itv.γ_mono (Itv.le_join_left wd wd') hx,
        Itv.γ_mono (Itv.le_join_left ht ht') hy, Itv.γ_mono (Itv.le_join_left dp dp') hz⟩
  · rcases γ_box.mp h with ⟨hm, hv⟩ | ⟨x, y, z, hk, hx, hy, hz⟩
    · left; subst hm; exact ⟨Bool.or_true mv, hv⟩
    · right
      exact ⟨x, y, z, hk, Itv.γ_mono (Itv.le_join_right wd wd') hx,
        Itv.γ_mono (Itv.le_join_right ht ht') hy, Itv.γ_mono (Itv.le_join_right dp dp') hz⟩

/-- `AVal.join_sound` (CHANGES.md item 1): the join covers the union,
unconditionally. -/
theorem aval_join_sound (L : Limits) (a b : AVal) (v : Val)
    (h : a.γ v ∨ b.γ v) : (AVal.join L a b).γ v := by
  cases a <;> cases b
  all_goals first
    | exact True.intro
    | exact join_sound_meanings L _ _ v h
    | exact join_sound_ints L _ _ v h
    | exact join_sound_glues L _ _ v h
    | exact join_sound_toks L _ _ v h
    | exact join_sound_box L _ _ _ _ _ _ _ _ v h

/-- `AVal` with the semantic order is an abstract domain in the sense of `AbsDom`. -/
def avalDom (L : Limits) : AbsDom Val AVal where
  le := AVal.le
  join := AVal.join L
  top := .top
  γ := AVal.γ
  le_refl := by intro a c h; exact h
  le_trans := by intro a b d h1 h2 c hc; exact h2 c (h1 c hc)
  join_l := by intro a b c hc; exact aval_join_sound L a b c (Or.inl hc)
  join_r := by intro a b c hc; exact aval_join_sound L a b c (Or.inr hc)
  le_top := by intro a c _; exact True.intro
  γ_mono := by intro a b h c hc; exact h c hc

/-! ### Widening

`AVal.widen` jumps a moving bound to the end of the 32-bit word. That is
sound for every value TeX can hold: counts and dimensions are words. -/

/-- Every member interval lies in the word. -/
def AValInWord : AVal → Prop
  | .ints ms => ∀ i, i ∈ ms → i.inWord
  | _ => True

theorem inWord_iff (i : Itv) : i.inWord ↔ Itv.le i Itv.word := by
  unfold Itv.inWord Itv.le Itv.word; constructor <;> intro h <;> exact ⟨h.1, h.2⟩

theorem foldl_join_le (c : Itv) : ∀ (t : List Itv) (acc : Itv), Itv.le acc c →
    (∀ i, i ∈ t → Itv.le i c) → Itv.le (t.foldl Itv.join acc) c
  | [], _, h, _ => h
  | y :: t, _, h, ht => foldl_join_le c t _
      (itv_join_lub _ _ _ h (ht y (List.mem_cons_self ..)))
      (fun i hi => ht i (List.mem_cons_of_mem _ hi))

theorem hullItv_inWord {l : List Itv} {j : Itv} (h : hullItv l = some j)
    (hw : ∀ i, i ∈ l → i.inWord) : j.inWord := by
  cases l with
  | nil => cases h
  | cons y t =>
    simp only [hullItv, Option.some.injEq] at h
    subst h
    exact (inWord_iff _).mpr (foldl_join_le _ t y ((inWord_iff _).mp (hw y (List.mem_cons_self ..)))
      (fun i hi => (inWord_iff _).mp (hw i (List.mem_cons_of_mem _ hi))))

theorem widen_other (L : Limits) (a b : AVal) (v : Val) (hjoin : (AVal.join L a b).γ v) :
    (match AVal.join L a b with | .meanings _ => AVal.top | j => j).γ v := by
  revert hjoin; cases AVal.join L a b <;> intro h <;> first | exact True.intro | exact h

/-- `AVal.widen_sound` (CHANGES item 2): widening covers both sides. -/
theorem aval_widen_sound (L : Limits) (a b : AVal) (ha : AValInWord a) (hb : AValInWord b) (v : Val)
    (h : a.γ v ∨ b.γ v) : (AVal.widen L a b).γ v := by
  have hjoin := aval_join_sound L a b v h
  cases a <;> cases b
  case ints.ints xs ys =>
    simp only [AVal.widen]
    have hw : ∀ i, i ∈ xs ++ ys → i.inWord := fun i hi => by
      rcases List.mem_append.mp hi with e | e
      · exact ha i e
      · exact hb i e
    have hm : ∃ i n, i ∈ xs ++ ys ∧ v = .int n ∧ i.γ n := by
      rcases h with h | h
      · obtain ⟨i, n, hi, hv, hn⟩ := γ_ints.mp h
        exact ⟨i, n, List.mem_append.mpr (Or.inl hi), hv, hn⟩
      · obtain ⟨i, n, hi, hv, hn⟩ := γ_ints.mp h
        exact ⟨i, n, List.mem_append.mpr (Or.inr hi), hv, hn⟩
    split
    · rename_i p q hp hq
      obtain ⟨i, n, hi, hv, hn⟩ := hm
      obtain ⟨j, hj, hle⟩ := hullItv_cover (xs ++ ys) i hi
      rw [hq] at hj; cases hj
      have hpw : p.inWord := hullItv_inWord hp ha
      have hqw : q.inWord := hullItv_inWord hq hw
      exact γ_ints.mpr ⟨_, n, List.mem_singleton.mpr rfl, hv,
        Itv.γ_mono (Itv.le_widen p q hpw hqw).2 (Itv.γ_mono hle hn)⟩
    · exact γ_ints.mpr hm
  case toks.toks xs ys =>
    simp only [AVal.widen]
    have hm : ∃ x p, x ∈ xs ++ ys ∧ v = .toks p ∧ AText.γ x p ∧ balanced p = true := by
      rcases h with h | h
      · obtain ⟨x, p, hi, hv, hn⟩ := γ_toks.mp h
        exact ⟨x, p, List.mem_append.mpr (Or.inl hi), hv, hn⟩
      · obtain ⟨x, p, hi, hv, hn⟩ := γ_toks.mp h
        exact ⟨x, p, List.mem_append.mpr (Or.inr hi), hv, hn⟩
    obtain ⟨x, p, hx, hv, hp, hb⟩ := hm
    exact γ_toks.mpr ⟨_, p, List.mem_singleton.mpr rfl, hv, hull_sound _ x p hx hp hb, hb⟩
  all_goals first
    | exact True.intro
    | (simp only [AVal.widen]; exact widen_other L _ _ v hjoin)

/-! ## 4. Assignment and groups (§§ 268–284)

One lemma per transfer, none with a pin on the group level: under a `many`
frame or after a summary the transfer leaves `touched`, and `skip` absorbs
whatever restore entries TeX made (PROOF-STATUS P1, P5). -/

/-- The semantic order on abstract states. -/
abbrev ALe (x y : AState) : Prop := ∀ s, x.γ s → y.γ s

theorem ale_refl (x : AState) : ALe x x := fun _ h => h
theorem ale_trans {x y z : AState} (h1 : ALe x y) (h2 : ALe y z) : ALe x z :=
  fun s h => h2 s (h1 s h)

/-- One location of the eqtb is covered: value and level. -/
def CovAt (a : AState) (E : Loc → Entry) (l : Loc) : Prop :=
  (a.eqtb l).γ (E l).val ∧ ∀ m, a.level l = some m → (E l).level = m

/-- A write keeps every location covered. -/
theorem covAt_upd {a : AState} {E : Loc → Entry} {l : Loc} {v : AVal} {x : Val}
    {lv : Option Nat} {n : Nat} (hv : v.γ x) (hlv : ∀ m, lv = some m → n = m)
    (h : ∀ l', l' ≠ l → CovAt a E l') (l' : Loc) :
    CovAt { a with eqtb := upd a.eqtb l v, level := upd a.level l lv }
      (upd E l { val := x, level := n }) l' := by
  by_cases e : l' = l
  · subst e; exact ⟨by simp only [upd_same]; exact hv, by simp only [upd_same]; exact hlv⟩
  · obtain ⟨h1, h2⟩ := h l' e
    exact ⟨by simp only [upd_other e]; exact h1, by simp only [upd_other e]; exact h2⟩

/-- The written state: covered eqtb, a related save stack, the rest unchanged. -/
theorem γ_written {a : AState} {s : State} {l : Loc} {v : AVal} {x : Val} {lv : Option Nat}
    {n : Nat} {sv : List ASave} {cs : List SaveEntry}
    (ha : a.γ s) (hv : v.γ x) (hlv : ∀ m, lv = some m → n = m) (hsv : SaveRel sv cs) :
    AState.γ { a with eqtb := upd a.eqtb l v, level := upd a.level l lv, save := sv }
      { s with eqtb := upd s.eqtb l { val := x, level := n }, save := cs } := by
  obtain ⟨h1, h2, h3, _, h5, h6, h7⟩ := ha
  have hc := covAt_upd (a := a) (E := s.eqtb) (l := l) hv hlv (fun l' _ => ⟨h1 l', h2 l'⟩)
  exact ⟨fun l' => (hc l').1, fun l' => (hc l').2, h3, hsv, h5, h6, h7⟩

theorem touched_head (l : Loc) (sv : List ASave) : Touched l (.touched l :: sv) := Or.inl rfl

/-- A local write whose save TeX may or may not make: `touched` covers both. -/
theorem saveRel_touch {sv : List ASave} {cs : List SaveEntry} (l : Loc) (e : Entry) (b : Bool)
    (h : SaveRel sv cs) :
    SaveRel (.touched l :: sv) (if b then .restore l e :: cs else cs) := by
  cases b
  · exact SaveRel.touched h
  · exact SaveRel.skip (touched_head l sv) (SaveRel.touched h)

theorem eqDefine_eq (s : State) (l : Loc) (x : Val) :
    Concrete.eqDefine s l x =
      { s with eqtb := upd s.eqtb l { val := x, level := s.curLevel },
               save := if decide ((s.eqtb l).level ≠ s.curLevel ∧ s.curLevel > 1) then
                 .restore l (s.eqtb l) :: s.save else s.save } := by
  simp only [Concrete.eqDefine]
  by_cases h : (s.eqtb l).level ≠ s.curLevel ∧ s.curLevel > 1 <;> simp [h]

/-- § 279 `geq_define`. -/
theorem assign_global_sound {a : AState} {s : State} {l : Loc} {v : AVal} {x : Val}
    (ha : a.γ s) (hv : v.γ x) : (a.assign true l v).γ (Concrete.geqDefine s l x) :=
  γ_written (lv := some 1) (n := 1) (sv := a.save) (cs := s.save) ha hv
    (fun _ h => (Option.some.inj h)) ha.2.2.2.1

/-- § 277 `eq_define`, at any group level and under `many` frames. -/
theorem assign_local_sound {a : AState} {s : State} {l : Loc} {v : AVal} {x : Val}
    (ha : a.γ s) (hv : v.γ x) : (a.assign false l v).γ (Concrete.eqDefine s l x) := by
  rw [eqDefine_eq]
  have hsr := ha.2.2.2.1
  have touchAny : ∀ (b : Bool) (lv : Option Nat), (∀ m, lv = some m → s.curLevel = m) →
      AState.γ { a with eqtb := upd a.eqtb l v, level := upd a.level l lv,
                        save := .touched l :: a.save }
        { s with eqtb := upd s.eqtb l { val := x, level := s.curLevel },
                 save := if b then .restore l (s.eqtb l) :: s.save else s.save } :=
    fun b lv hlv => γ_written ha hv hlv (saveRel_touch l _ b hsr)
  unfold AState.assign
  simp only [Bool.false_eq_true, ↓reduceIte]
  cases hp : a.pinned
  · simp only [Bool.not_false, ↓reduceIte]
    exact touchAny _ none (fun _ h => by cases h)
  · have hpin := ha.2.2.1 hp
    have hl1 : ∀ m, some a.curLevel = some m → s.curLevel = m :=
      fun m h => by rw [← hpin]; exact Option.some.inj h
    simp only [Bool.not_true, Bool.false_eq_true, ↓reduceIte]
    cases hl : a.level l with
    | none =>
      by_cases c : a.curLevel ≤ 1
      · simp only [c, ↓reduceIte]
        have hd : decide ((s.eqtb l).level ≠ s.curLevel ∧ s.curLevel > 1) = false := by
          simp; omega
        rw [hd]
        exact γ_written ha hv hl1 hsr
      · simp only [c, ↓reduceIte]
        exact touchAny _ _ hl1
    | some n =>
      have hn : (s.eqtb l).level = n := ha.2.1 l n hl
      by_cases c : n = a.curLevel ∨ a.curLevel ≤ 1
      · simp only [c, ↓reduceIte]
        have hd : decide ((s.eqtb l).level ≠ s.curLevel ∧ s.curLevel > 1) = false := by
          simp; omega
        rw [hd]
        exact γ_written ha hv hl1 hsr
      · simp only [c, ↓reduceIte]
        have hd : decide ((s.eqtb l).level ≠ s.curLevel ∧ s.curLevel > 1) = true := by
          simp; omega
        rw [hd]
        simp only [↓reduceIte]
        exact γ_written (a := { a with pinned := true }) (l := l)
          ⟨ha.1, ha.2.1, fun _ => hpin, ha.2.2.2⟩ hv hl1
          (SaveRel.restore (ha.1 l) (fun m h => by rw [hn]; exact Option.some.inj h) hsr)

/-- `define` with `\globaldefs` (§ 1214). -/
theorem define_sound {a : AState} {s : State} {l : Loc} {v : AVal} {x : Val}
    (ha : a.γ s) (hv : v.γ x) (glob : Bool) :
    (a.assign (Concrete.isGlobal s glob) l v).γ (Concrete.define s glob l x) := by
  unfold Concrete.define
  cases Concrete.isGlobal s glob
  · exact assign_local_sound ha hv
  · exact assign_global_sound ha hv

theorem lvJoin_some {x y : Option Nat} {m : Nat} (h : AState.lvJoin x y = some m) :
    x = some m ∧ y = some m := by
  unfold AState.lvJoin at h
  split at h
  · rename_i e; subst e; exact ⟨h, h⟩
  · cases h

theorem newLevel_sound {a : AState} {s : State} (ha : a.γ s) {glob : Bool} {m : Nat}
    (h : a.newLevel glob = some m) : (if glob then 1 else s.curLevel) = m := by
  unfold AState.newLevel at h
  cases glob
  · cases hp : a.pinned
    · simp [hp] at h
    · simp only [hp, Bool.false_eq_true, ↓reduceIte, Option.some.injEq] at h ⊢
      rw [← h]; exact (ha.2.2.1 hp).symm
  · simp only [↓reduceIte, Option.some.injEq] at h ⊢; exact h

/-- A weak write at `l` covers the state where nothing was written. -/
theorem weakAt_keep (L : Limits) {a : AState} {t : State} (glob : Bool) (l : Loc) (v : AVal)
    (ha : a.γ t) : (a.weakAt L glob l v).γ t := by
  obtain ⟨h1, h2, h3, h4, h5, h6, h7⟩ := ha
  have hc := covAt_upd (a := a) (E := t.eqtb) (l := l) (x := (t.eqtb l).val)
    (v := AVal.join L (a.eqtb l) v) (lv := AState.lvJoin (a.level l) (a.newLevel glob))
    (n := (t.eqtb l).level) (aval_join_sound L _ _ _ (Or.inl (h1 l)))
    (fun m hm => h2 l m (lvJoin_some hm).1)
    (fun l' _ => ⟨h1 l', h2 l'⟩)
  have he : upd t.eqtb l { val := (t.eqtb l).val, level := (t.eqtb l).level } = t.eqtb := by
    funext l'; by_cases e : l' = l
    · subst e; simp [upd_same]
    · simp [upd_other e]
  rw [he] at hc
  exact ⟨fun l' => (hc l').1, fun l' => (hc l').2, h3, SaveRel.touched h4, h5, h6, h7⟩

/-- A weak write at `l` covers the state where `l` was written. -/
theorem weakAt_write (L : Limits) {a : AState} {s : State} (glob : Bool) (l : Loc) {v : AVal}
    {x : Val} (ha : a.γ s) (hv : v.γ x) :
    (a.weakAt L glob l v).γ (if glob then Concrete.geqDefine s l x else Concrete.eqDefine s l x) := by
  have hj := aval_join_sound L (a.eqtb l) v x (Or.inr hv)
  have hn := fun m (hm : AState.lvJoin (a.level l) (a.newLevel glob) = some m) =>
    newLevel_sound ha (lvJoin_some hm).2
  cases glob
  · simp only [Bool.false_eq_true, ↓reduceIte] at hn ⊢
    rw [eqDefine_eq]
    exact γ_written (a := a) (l := l) ha hj hn (saveRel_touch l _ _ ha.2.2.2.1)
  · simp only [↓reduceIte] at hn ⊢
    exact γ_written (a := a) (l := l) (n := 1) ha hj hn (SaveRel.touched ha.2.2.2.1)

/-- `weakAssign_sound` (CHANGES item 4): a write to one of `ls`, unknown
which, is covered. -/
theorem weakAssign_sound (L : Limits) (glob : Bool) {v : AVal} {x : Val} {l : Loc} (hv : v.γ x)
    {s : State} :
    ∀ (ls : List Loc) (a : AState),
      (a.γ s → l ∈ ls → (a.weakAssign L glob ls v).γ
        (if glob then Concrete.geqDefine s l x else Concrete.eqDefine s l x)) ∧
      ∀ t, a.γ t → (a.weakAssign L glob ls v).γ t
  | [], a => ⟨fun _ h => by simp at h, fun _ h => h⟩
  | l' :: ls, a => by
    have ih := weakAssign_sound L glob hv (l := l) (s := s) ls
    refine ⟨fun ha hl => ?_, fun t ht => (ih _).2 t (weakAt_keep L glob l' v ht)⟩
    show (ls.foldl (fun s l => s.weakAt L glob l v) (a.weakAt L glob l' v)).γ _
    by_cases e : l' = l
    · subst e; exact (ih _).2 _ (weakAt_write L glob l' ha hv)
    · rcases List.mem_cons.mp hl with h | h
      · exact absurd h.symm e
      · exact (ih _).1 (weakAt_keep L glob l' v ha) h

/-! ### Closing a group

`closeFrom_sound` walks the related stacks entry by entry. Its invariant: a
location is covered unless a `touched` entry for it still lies ahead in the
frame, which will make it unknown. -/

/-- Covered except where a `touched` entry lies ahead in the frame. -/
def CovFrame (a : AState) (E : Loc → Entry) (as : List ASave) : Prop :=
  ∀ l, ¬ Touched l as → CovAt a E l

theorem covAt_top {a : AState} {E : Loc → Entry} {l : Loc} :
    CovAt (a.setAt l .top none) E l :=
  ⟨by simp [AState.setAt, upd_same]; exact True.intro, fun _ h => by simp [AState.setAt, upd_same] at h⟩

theorem covAt_setAt_other {a : AState} {E : Loc → Entry} {l l' : Loc} {v : AVal} {lv : Option Nat}
    (e : l' ≠ l) (h : CovAt a E l') : CovAt (a.setAt l v lv) E l' :=
  ⟨by simp [AState.setAt, upd_other e]; exact h.1, by simp [AState.setAt, upd_other e]; exact h.2⟩

theorem covAt_E_other {a : AState} {E : Loc → Entry} {l l' : Loc} {e : Entry}
    (ne : l' ≠ l) (h : CovAt a E l') : CovAt a (upd E l e) l' :=
  ⟨by simp only [upd_other ne]; exact h.1, by simp only [upd_other ne]; exact h.2⟩

/-- `forget` makes its locations unknown and keeps the rest. -/
theorem forget_cov (E : Loc → Entry) : ∀ (ls : List Loc) (a : AState) (l : Loc),
    (l ∉ ls → CovAt a E l) → CovAt (a.forget ls) E l
  | [], a, l, h => h (fun h => by cases h)
  | m :: ls, a, l, h => by
    unfold AState.forget
    simp only [List.foldl_cons]
    refine forget_cov E ls _ l (fun hl => ?_)
    by_cases e : l = m
    · subst e; exact covAt_top
    · exact covAt_setAt_other e (h (fun hm => by
        rcases List.mem_cons.mp hm with h' | h'
        · exact e h'
        · exact hl h'))

theorem forget_fields : ∀ (ls : List Loc) (a : AState),
    (a.forget ls).save = a.save ∧ (a.forget ls).input = a.input ∧ (a.forget ls).cond = a.cond ∧
      (a.forget ls).modes = a.modes
  | [], _ => ⟨rfl, rfl, rfl, rfl⟩
  | _ :: ls, a => forget_fields ls (a.setAt _ .top none)

/-- Restore entries of locations `ls` change no other location. -/
theorem unsaveAux_restores (ls : List Loc) : ∀ (pre : List SaveEntry) (E : Loc → Entry)
    (ins : TokList) (rest : List SaveEntry),
    (∀ e, e ∈ pre → ∃ l old, e = .restore l old ∧ l ∈ ls) →
    ∃ E1, Concrete.unsaveAux E ins (pre ++ rest) = Concrete.unsaveAux E1 ins rest ∧
      ∀ l, l ∉ ls → E1 l = E l
  | [], E, _, _, _ => ⟨E, rfl, fun _ _ => rfl⟩
  | e :: pre, E, ins, rest, h => by
    obtain ⟨l, old, he, hl⟩ := h e (List.mem_cons_self ..)
    subst he
    obtain ⟨E1, e1, h1⟩ := unsaveAux_restores ls pre
      (if (E l).level = 1 then E else upd E l old) ins rest
      (fun e' h' => h e' (List.mem_cons_of_mem _ h'))
    refine ⟨E1, e1, fun l' hl' => ?_⟩
    rw [h1 l' hl']
    have ne : l' ≠ l := fun e => hl' (e ▸ hl)
    split
    · rfl
    · exact upd_other ne

/-- What closing a frame gives, part by part (the closed group's `γ`). -/
def Closed (a b : AState) (E' : Loc → Entry) (ins' : TokList) (crest : List SaveEntry) : Prop :=
  (∀ l, CovAt b E' l) ∧ SaveRel b.save crest ∧ b.cond = a.cond ∧ b.modes = a.modes ∧
    b.input = (if ins' = [] then a.input else .toks .backedUp (AText.ofList ins') :: a.input) ∧
    (b.pinned = true → a.pinned = true ∧ b.curLevel = a.curLevel - 1)

theorem closeFrom_sound (L : Limits) (c : GroupCode) {as : List ASave} {cs : List SaveEntry}
    (h : SaveRel as cs) :
    ∀ (a : AState) (E : Loc → Entry) (ins : TokList) (E' : Loc → Entry) (ins' : TokList)
      (ctx : Option BoxContext) (crest : List SaveEntry),
      CovFrame a E as → Concrete.unsaveAux E ins cs = (E', ins', some (c, ctx), crest) →
      ∃ b, b ∈ AState.closeFrom L c a ins as ∧ Closed a b E' ins' crest := by
  induction h with
  | nil => intro a E ins E' ins' ctx crest _ hu; simp [Concrete.unsaveAux] at hu
  | @restore l old lv e rest crest0 hold hlv _ ih =>
    intro a E ins E' ins' ctx crest hcov hu
    simp only [Concrete.unsaveAux] at hu
    have hf : ∀ b, Closed (a.restoreOne L l old lv) b E' ins' crest → Closed a b E' ins' crest := by
      intro b hb; unfold AState.restoreOne at hb; split at hb <;> exact hb
    suffices hcf : CovFrame (a.restoreOne L l old lv) (if (E l).level = 1 then E else upd E l e) rest by
      obtain ⟨b, hb, hc⟩ := ih (a.restoreOne L l old lv) _ ins E' ins' ctx crest hcf hu
      exact ⟨b, hb, hf b hc⟩
    intro l' ht
    have hc := hcov l' ht
    by_cases e' : l' = l
    · subst e'
      obtain ⟨hv, hlvl⟩ := hc
      unfold AState.restoreOne
      cases hk : a.level l' with
      | none =>
        refine ⟨?_, fun _ h => by simp [AState.setAt, upd_same] at h⟩
        simp only [AState.setAt, upd_same]
        split
        · exact aval_join_sound L _ _ _ (Or.inl hv)
        · simp only [upd_same]; exact aval_join_sound L _ _ _ (Or.inr hold)
      | some k =>
        have hE : (E l').level = k := hlvl k hk
        by_cases k1 : k = 1
        · subst k1
          simp only [hE, ↓reduceIte]
          exact ⟨hv, hlvl⟩
        · have hne : ¬ (E l').level = 1 := by omega
          simp only [hne, ↓reduceIte]
          have hset : CovAt (a.setAt l' old lv) (upd E l' e) l' :=
            ⟨by simp [AState.setAt, upd_same]; exact hold,
             fun m h => by simp [AState.setAt, upd_same] at h ⊢; exact hlv m h⟩
          cases k with
          | zero => exact hset
          | succ k' => cases k' with
            | zero => exact absurd rfl k1
            | succ _ => exact hset
    · have hr : CovAt (a.restoreOne L l old lv) E l' := by
        unfold AState.restoreOne
        split
        · exact hc
        · exact covAt_setAt_other e' hc
        · exact covAt_setAt_other e' hc
      split
      · exact hr
      · exact covAt_E_other e' hr
  | @touched l rest crest0 _ ih =>
    intro a E ins E' ins' ctx crest hcov hu
    suffices hcf : CovFrame (a.setAt l .top none) E rest by
      obtain ⟨b, hb, hc⟩ := ih (a.setAt l .top none) E ins E' ins' ctx crest hcf hu
      exact ⟨b, hb, hc⟩
    intro l' ht
    by_cases e' : l' = l
    · subst e'; exact covAt_top
    · exact covAt_setAt_other e' (hcov l' (fun h => by
        rcases h with h | h
        · exact e' h.symm
        · exact ht h))
  | @skip l e rest crest0 ht _ ih =>
    intro a E ins E' ins' ctx crest hcov hu
    simp only [Concrete.unsaveAux] at hu
    refine ih a _ ins E' ins' ctx crest (fun l' ht' => ?_) hu
    have e' : l' ≠ l := fun h => ht' (h ▸ ht)
    split
    · exact hcov l' ht'
    · exact covAt_E_other e' (hcov l' ht')
  | @retain l rest crest0 _ ih =>
    intro a E ins E' ins' ctx crest hcov hu
    exact ih a E ins E' ins' ctx crest (fun l' ht => hcov l' ht) hu
  | @insertToken t rest crest0 _ ih =>
    intro a E ins E' ins' ctx crest hcov hu
    exact ih a E (t :: ins) E' ins' ctx crest (fun l' ht => hcov l' ht) hu
  | @group c0 ctx0 line rest crest0 hr _ =>
    intro a E ins E' ins' ctx crest hcov hu
    simp only [Concrete.unsaveAux, Prod.mk.injEq, Option.some.injEq] at hu
    obtain ⟨e1, e2, ⟨e3, -⟩, e4⟩ := hu
    subst e1 e2 e3 e4
    refine ⟨a.closed ins rest, by simp [AState.closeFrom], fun l => hcov l (fun h => h),
      hr, rfl, rfl, rfl, fun hp => ⟨hp, rfl⟩⟩
  | @manyNone c0 ls rest crest0 _ ih =>
    intro a E ins E' ins' ctx crest hcov hu
    obtain ⟨b, hb, h1, h2, h3, h4, h5, h6⟩ :=
      ih { a with pinned := false } E ins E' ins' ctx crest (fun l _ => hcov l (fun h => h)) hu
    refine ⟨b, ?_, h1, h2, h3, h4, h5, fun hp => by have h := (h6 hp).1; simp at h⟩
    simp only [AState.closeFrom]
    exact List.mem_append.mpr (Or.inr hb)
  | @manyMore c0 ls ctx0 line rest crest0 pre hr hpre _ =>
    intro a E ins E' ins' ctx crest hcov hu
    obtain ⟨E1, e1, hE1⟩ := unsaveAux_restores ls pre E ins _ hpre
    rw [e1] at hu
    simp only [Concrete.unsaveAux, Prod.mk.injEq, Option.some.injEq] at hu
    obtain ⟨e2, e3, ⟨e4, -⟩, e5⟩ := hu
    subst e2 e3 e4 e5
    obtain ⟨f1, f2, f3, f4⟩ := forget_fields ls (a.closed ins (.group c0 (some ls) :: rest))
    refine ⟨{ (a.closed ins (.group c0 (some ls) :: rest)).forget ls with
        curLevel := a.curLevel, pinned := false }, ?_, ?_, ?_, f3, f4, f2, fun h => by cases h⟩
    · simp [AState.closeFrom]
    · intro l
      have hc := forget_cov E1 ls (a.closed ins (.group c0 (some ls) :: rest)) l
        (fun hl => by
          have h := hcov l (fun h => h)
          unfold CovAt at h ⊢; rw [hE1 l hl]; exact h)
      exact ⟨hc.1, hc.2⟩
    · show SaveRel ((a.closed ins (.group c0 (some ls) :: rest)).forget ls).save _
      rw [f1]; exact hr

theorem ofList_γ : ∀ ts : TokList, AText.γ (AText.ofList ts) ts
  | [] => rfl
  | t :: ts => ⟨[t], ts, rfl, rfl, ofList_γ ts⟩

/-- `endGroup_sound` (CHANGES item 5): the concrete `unsave` (§§ 281–283)
of a group of code `c` is covered by an abstract result, for plain and
`many` frames, `\aftergroup` tokens included (PROOF-STATUS P1, P2 fixed). -/
theorem endGroup_sound (L : Limits) {a : AState} {s : State} {c : GroupCode}
    {E' : Loc → Entry} {ins : TokList} {ctx : Option BoxContext} {crest : List SaveEntry}
    (ha : a.γ s) (hu : Concrete.unsaveAux s.eqtb [] s.save = (E', ins, some (c, ctx), crest)) :
    ∃ b, b ∈ a.endGroup L c ∧ b.γ (Concrete.unsave s).1 := by
  obtain ⟨h1, h2, h3, h4, h5, h6, h7⟩ := ha
  obtain ⟨b, hb, hc1, hc2, hc3, hc4, hc5, hc6⟩ :=
    closeFrom_sound L c h4 a s.eqtb [] E' ins ctx crest (fun l _ => ⟨h1 l, h2 l⟩) hu
  refine ⟨b, hb, ?_⟩
  unfold Concrete.unsave
  rw [hu]
  dsimp only
  have hp : b.pinned = true → b.curLevel = s.curLevel - 1 := fun hp => by
    rw [(hc6 hp).2, h3 (hc6 hp).1]
  by_cases e : ins = []
  · simp only [e, ↓reduceIte] at hc5 ⊢
    exact ⟨fun l => (hc1 l).1, fun l => (hc1 l).2, hp, hc2, hc3 ▸ h5, hc5 ▸ h6, hc4 ▸ h7⟩
  · simp only [e, ↓reduceIte, Concrete.pushToks] at hc5 ⊢
    exact ⟨fun l => (hc1 l).1, fun l => (hc1 l).2, hp, hc2, hc3 ▸ h5,
      hc5 ▸ ⟨⟨rfl, ofList_γ ins⟩, h6⟩, hc4 ▸ h7⟩

/-! ### Loop and call summaries (PROOF-STATUS P5)

A summary replaces any number of runs of a body by one transfer. What it
may assume of the body is its footprint: the locations it writes, and that
it leaves the stacks as it found them, apart from restore entries for those
locations. -/

/-- What a body may change: the locations `ws`, and restore entries for
them on top of the save stack. -/
def Footprint (ws : List Loc) (s s' : State) : Prop :=
  (∀ l, l ∉ ws → s'.eqtb l = s.eqtb l) ∧
  (∃ pre, s'.save = pre ++ s.save ∧ ∀ e, e ∈ pre → ∃ l old, l ∈ ws ∧ e = .restore l old) ∧
  s'.curLevel = s.curLevel ∧ s'.cond = s.cond ∧ s'.input = s.input ∧ s'.mode = s.mode

theorem footprint_star {ws : List Loc} {s s' : State} (h : Star (Footprint ws) s s') :
    Footprint ws s s' := by
  induction h with
  | refl => exact ⟨fun _ _ => rfl, ⟨[], rfl, fun _ h => by cases h⟩, rfl, rfl, rfl, rfl⟩
  | tail _ hbc ih =>
    obtain ⟨a1, ⟨p1, e1, q1⟩, a3, a4, a5, a6⟩ := ih
    obtain ⟨b1, ⟨p2, e2, q2⟩, b3, b4, b5, b6⟩ := hbc
    refine ⟨fun l hl => (b1 l hl).trans (a1 l hl), ⟨p2 ++ p1, ?_, ?_⟩,
      b3.trans a3, b4.trans a4, b5.trans a5, b6.trans a6⟩
    · rw [e2, e1, List.append_assoc]
    · intro e he
      rcases List.mem_append.mp he with h | h
      · exact q2 e h
      · exact q1 e h

theorem touched_map {l : Loc} {ws : List Loc} (h : l ∈ ws) (sv : List ASave) :
    Touched l (ws.map ASave.touched ++ sv) := by
  induction ws with
  | nil => cases h
  | cons w ws ih =>
    rcases List.mem_cons.mp h with e | e
    · exact Or.inl e.symm
    · exact Or.inr (ih e)

theorem saveRel_touchedAll {ws : List Loc} {sv : List ASave} {cs : List SaveEntry}
    (h : SaveRel sv cs) : SaveRel (ws.map ASave.touched ++ sv) cs := by
  induction ws with
  | nil => exact h
  | cons w ws ih => exact SaveRel.touched ih

theorem saveRel_summary {ws : List Loc} {sv : List ASave} {cs : List SaveEntry}
    (h : SaveRel sv cs) :
    ∀ pre : List SaveEntry, (∀ e, e ∈ pre → ∃ l old, l ∈ ws ∧ e = .restore l old) →
      SaveRel (ws.map ASave.touched ++ sv) (pre ++ cs)
  | [], _ => by simpa using saveRel_touchedAll (ws := ws) h
  | e :: pre, hp => by
    obtain ⟨l, old, hl, he⟩ := hp e (List.mem_cons_self ..)
    subst he
    exact SaveRel.skip (touched_map hl sv)
      (saveRel_summary h pre (fun e' h' => hp e' (List.mem_cons_of_mem _ h')))

/-- `summaryHavoc_sound` (CHANGES item 9, corrected): the summary covers
every number of runs of a body with footprint `ws`, whatever the levels and
save entries the runs make. -/
theorem summaryHavoc_sound {ws : List Loc} {Body : State → State → Prop}
    (hb : ∀ s s', Body s s' → Footprint ws s s') {a : AState} {s s' : State} (ha : a.γ s)
    (hrun : Star Body s s') : (summaryHavoc ws a).γ s' := by
  have hst : Star (Footprint ws) s s' := by
    induction hrun with
    | refl => exact Star.refl
    | tail _ h ih => exact Star.tail ih (hb _ _ h)
  obtain ⟨f1, ⟨pre, e, q⟩, f3, f4, f5, f6⟩ := footprint_star hst
  obtain ⟨h1, h2, h3, h4, h5, h6, h7⟩ := ha
  refine ⟨fun l => ?_, fun l n hn => ?_, fun hp => ?_, ?_, ?_, ?_, ?_⟩
  · show (if ws.contains l then AVal.top else a.eqtb l).γ (s'.eqtb l).val
    by_cases hl : l ∈ ws
    · have : ws.contains l = true := by simpa using hl
      simp only [this, ↓reduceIte]; exact True.intro
    · have : ws.contains l = false := by simpa using hl
      simp only [this, Bool.false_eq_true, ↓reduceIte]; rw [f1 l hl]; exact h1 l
  · change (if ws.contains l then none else a.level l) = some n at hn
    by_cases hl : l ∈ ws
    · simp at hn; exact absurd hl hn.1
    · have : ws.contains l = false := by simpa using hl
      simp only [this, Bool.false_eq_true, ↓reduceIte] at hn; rw [f1 l hl]; exact h2 l n hn
  · rw [f3]; exact h3 hp
  · show SaveRel (ws.map ASave.touched ++ a.save) s'.save
    rw [e]; exact saveRel_summary h4 pre q
  · show CondRel a.cond s'.cond; rw [f4]; exact h5
  · show InputRel a.input s'.input; rw [f5]; exact h6
  · show s'.mode.kind ∈ a.modes; rw [f6]; exact h7

/-! ## 5. Undecided conditionals: fork, narrow, join at `\fi` -/

/-- `branch_sound` (CHANGES.md item 6): the concrete answer `cb` has an arm. -/
theorem branch_sound (test : AState → Option Bool) (narrow : Bool → AState → Option AState)
    (a : AState) (s : State) (cb : Bool) (ha : a.γ s)
    (htest : ∀ b, test a = some b → cb = b)
    (hnarrow : test a = none → ∃ a', narrow cb a = some a' ∧ a'.γ s) :
    ∃ a', (cb, a') ∈ branch test narrow a ∧ a'.γ s := by
  unfold branch
  cases ht : test a with
  | some b =>
    have e := htest b ht
    subst e
    exact ⟨a, List.mem_singleton.mpr rfl, ha⟩
  | none =>
    obtain ⟨a', hn, hγ⟩ := hnarrow ht
    cases cb with
    | true =>
      rw [hn]
      exact ⟨a', List.mem_append.mpr (Or.inl (List.mem_singleton.mpr rfl)), hγ⟩
    | false =>
      rw [hn]
      exact ⟨a', List.mem_append.mpr (Or.inr (List.mem_singleton.mpr rfl)), hγ⟩

/-- `\ifnum`/`\ifdim` by interval (`Num::compare`): a decided answer holds
for every member. This discharges `branch_sound`'s test hypothesis. -/
theorem compare_sound (i : Itv) (r : Itv.Rel) (k : Int) (b : Bool) (n : Int)
    (h : i.compare r (Itv.exact k) = some b) (hn : i.γ n) : r.holds n k = b := by
  obtain ⟨h1, h2⟩ := hn
  cases r with
  | lt =>
    simp only [Itv.compare, Itv.exact] at h
    by_cases c1 : i.hi < k
    · simp only [c1, ↓reduceIte] at h; cases h; simp only [Itv.Rel.holds, decide_eq_true_eq]; omega
    · simp only [c1, ↓reduceIte] at h
      by_cases c2 : k ≤ i.lo
      · simp only [c2, ↓reduceIte] at h; cases h; simp only [Itv.Rel.holds, decide_eq_false_iff_not]; omega
      · simp only [c2, ↓reduceIte] at h; cases h
  | gt =>
    simp only [Itv.compare, Itv.exact] at h
    by_cases c1 : k < i.lo
    · simp only [c1, ↓reduceIte] at h; cases h; simp only [Itv.Rel.holds, decide_eq_true_eq]; omega
    · simp only [c1, ↓reduceIte] at h
      by_cases c2 : i.hi ≤ k
      · simp only [c2, ↓reduceIte] at h; cases h; simp only [Itv.Rel.holds, decide_eq_false_iff_not]; omega
      · simp only [c2, ↓reduceIte] at h; cases h
  | eq =>
    simp only [Itv.compare, Itv.exact] at h
    split at h
    · cases h; simp only [Itv.Rel.holds, decide_eq_true_eq]; omega
    · by_cases c2 : i.hi < k ∨ k < i.lo
      · simp only [c2, ↓reduceIte] at h; cases h; simp only [Itv.Rel.holds, decide_eq_false_iff_not]; omega
      · simp only [c2, ↓reduceIte] at h; cases h

theorem narrow_some {lo hi n : Int} (h1 : lo ≤ n) (h2 : n ≤ hi) :
    ∃ j, (if lo ≤ hi then some (⟨lo, hi⟩ : Itv) else none) = some j ∧ j.γ n :=
  ⟨⟨lo, hi⟩, by simp only [show lo ≤ hi by omega, ↓reduceIte], h1, h2⟩

/-- `Num::narrow`: the member's value survives on the arm its answer takes. -/
theorem narrow_sound (i : Itv) (r : Itv.Rel) (k n : Int) (hn : i.γ n) :
    ∃ j, i.narrow r k (r.holds n k) = some j ∧ j.γ n := by
  obtain ⟨h1, h2⟩ := hn
  cases r
  · by_cases c : n < k
    · rw [show Itv.Rel.holds .lt n k = true by simp [Itv.Rel.holds, c]]
      exact narrow_some (lo := i.lo) (hi := min i.hi (k - 1)) (by omega) (by omega)
    · rw [show Itv.Rel.holds .lt n k = false by simp [Itv.Rel.holds, c]]
      exact narrow_some (lo := max i.lo k) (hi := i.hi) (by omega) (by omega)
  · by_cases c : n = k
    · rw [show Itv.Rel.holds .eq n k = true by simp [Itv.Rel.holds, c]]
      exact narrow_some (lo := max i.lo k) (hi := min i.hi k) (by omega) (by omega)
    · rw [show Itv.Rel.holds .eq n k = false by simp [Itv.Rel.holds, c]]
      exact narrow_some (lo := if i.lo = k then i.lo + 1 else i.lo)
        (hi := if i.hi = k then i.hi - 1 else i.hi) (by split <;> omega) (by split <;> omega)
  · by_cases c : k < n
    · rw [show Itv.Rel.holds .gt n k = true by simp [Itv.Rel.holds, c]]
      exact narrow_some (lo := max i.lo (k + 1)) (hi := i.hi) (by omega) (by omega)
    · rw [show Itv.Rel.holds .gt n k = false by simp [Itv.Rel.holds, c]]
      exact narrow_some (lo := i.lo) (hi := min i.hi k) (by omega) (by omega)

/-- `ifNumTest`: a decided answer is TeX's (§ 503). -/
theorem ifNumTest_sound {a : AState} {s : State} {l : Loc} {r : Itv.Rel} {k n : Int} {b : Bool}
    (ha : a.γ s) (hv : (s.eqtb l).val = .int n) (h : ifNumTest l r k a = some b) :
    r.holds n k = b := by
  have hl := ha.1 l
  rw [hv] at hl
  unfold ifNumTest at h
  cases e : a.eqtb l with
  | ints ms =>
    rw [e] at h hl
    obtain ⟨i, m, hi, hm, hγ⟩ := hl
    cases hm
    cases ms with
    | nil => cases hi
    | cons i0 rest =>
      simp only [List.map_cons] at h
      split at h
      · rename_i b0 rs heq
        simp only [List.cons.injEq] at heq
        obtain ⟨e1, e2⟩ := heq
        split at h
        · rename_i hall
          cases h
          rcases List.mem_cons.mp hi with hi | hi
          · subst hi; exact compare_sound _ _ _ _ _ e1 hγ
          · subst e2
            have := (List.all_eq_true.mp hall) _ (List.mem_map_of_mem hi)
            exact compare_sound _ _ _ _ _ (by simpa using this) hγ
        · cases h
      · cases h
  | _ => rw [e] at h; cases h

/-- `ifNumNarrow`: the state narrowed to TeX's answer still covers `s`. -/
theorem ifNumNarrow_sound {a : AState} {s : State} {l : Loc} {r : Itv.Rel} {k n : Int}
    (ha : a.γ s) (hv : (s.eqtb l).val = .int n) :
    ∃ a', ifNumNarrow l r k (r.holds n k) a = some a' ∧ a'.γ s := by
  have hl := ha.1 l
  rw [hv] at hl
  unfold ifNumNarrow
  cases e : a.eqtb l with
  | ints ms =>
    rw [e] at hl
    obtain ⟨i, m, hi, hm, hγ⟩ := hl
    cases hm
    obtain ⟨j, hj, hjγ⟩ := narrow_sound i r k n hγ
    have hmem : j ∈ ms.filterMap (fun i => i.narrow r k (r.holds n k)) :=
      List.mem_filterMap.mpr ⟨i, hi, hj⟩
    have hne : (ms.filterMap (fun i => i.narrow r k (r.holds n k))).isEmpty = false := by
      cases hq : ms.filterMap (fun i => i.narrow r k (r.holds n k)) with
      | nil => rw [hq] at hmem; cases hmem
      | cons _ _ => rfl
    simp only [hne, Bool.false_eq_true, ↓reduceIte]
    refine ⟨_, rfl, ?_⟩
    obtain ⟨h1, h2, h3, h4, h5, h6, h7⟩ := ha
    refine ⟨fun l' => ?_, h2, h3, h4, h5, h6, h7⟩
    by_cases hl' : l' = l
    · subst hl'; simp only [upd_same, hv]; exact ⟨j, n, hmem, rfl, hjγ⟩
    · simp only [upd_other hl']; exact h1 l'
  | _ => exact ⟨a, rfl, ha⟩

/-- `\ifnum⟨loc⟩⟨rel⟩⟨k⟩` by `branch`: TeX's answer has an arm that covers
the state. `branch_sound` with both hypotheses discharged. -/
theorem ifNum_branch_sound {a : AState} {s : State} {l : Loc} {r : Itv.Rel} {k n : Int}
    (ha : a.γ s) (hv : (s.eqtb l).val = .int n) :
    ∃ a', (r.holds n k, a') ∈ branch (ifNumTest l r k) (ifNumNarrow l r k) a ∧ a'.γ s :=
  branch_sound _ _ a s _ ha (fun _ h => ifNumTest_sound ha hv h)
    (fun _ => ifNumNarrow_sound ha hv)

/-- `AState.join_sound` (CHANGES.md item 10): arms meeting at `\fi` in the
same stacks join soundly. -/
theorem astate_join_sound (L : Limits) (a b : AState) (s : State)
    (hj : a.joinable b) (h : a.γ s ∨ b.γ s) : (a.join L b).γ s := by
  obtain ⟨hc, hs, hco, hi, hl⟩ := hj
  rcases h with h | h
  · obtain ⟨h1, h2, h3, h4, h5, h6, h7⟩ := h
    unfold AState.γ
    refine ⟨?_, h2, fun hf => h3 (by simp only [AState.join, Bool.and_eq_true] at hf; exact hf.1),
      h4, h5, h6, ?_⟩
    · intro l; exact aval_join_sound L _ _ _ (Or.inl (h1 l))
    · show s.mode.kind ∈ a.modes ++ b.modes.filter (fun m => !(a.modes.contains m))
      exact List.mem_append.mpr (Or.inl h7)
  · obtain ⟨h1, h2, h3, h4, h5, h6, h7⟩ := h
    unfold AState.γ
    refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_⟩
    · intro l; exact aval_join_sound L _ _ _ (Or.inr (h1 l))
    · intro l n hn; exact h2 l n ((hl l) ▸ hn)
    · intro hf
      show a.curLevel = s.curLevel
      rw [hc]; apply h3; simp only [AState.join, Bool.and_eq_true] at hf; exact hf.2
    · show SaveRel a.save s.save
      rw [hs]; exact h4
    · show CondRel a.cond s.cond
      rw [hco]; exact h5
    · show InputRel a.input s.input
      rw [hi]; exact h6
    · show s.mode.kind ∈ a.modes ++ b.modes.filter (fun m => !(a.modes.contains m))
      by_cases hm : s.mode.kind ∈ a.modes
      · exact List.mem_append.mpr (Or.inl hm)
      · refine List.mem_append.mpr (Or.inr (List.mem_filter.mpr ⟨h7, ?_⟩))
        first
          | simp [hm]
          | simp_all [List.elem_iff]

/-! ## 6. Macro expansion: substitution and known delimiters -/

theorem atext_γ_nil {p : TokList} : AText.γ [] p ↔ p = [] := Iff.rfl

theorem atext_γ_cons {it : AItem} {rest : AText} {p : TokList} :
    AText.γ (it :: rest) p ↔ ∃ p₁ p₂, p = p₁ ++ p₂ ∧ it.γ p₁ ∧ AText.γ rest p₂ := Iff.rfl

/-- Abstract texts concatenate like their meanings. -/
theorem atext_γ_append : ∀ (x y : AText) (p q : TokList),
    AText.γ x p → AText.γ y q → AText.γ (x ++ y) (p ++ q)
  | [], y, p, q, hp, hq => by
    have e : p = [] := hp
    subst e
    exact hq
  | it :: rest, y, p, q, hp, hq => by
    obtain ⟨p₁, p₂, e, h1, h2⟩ := atext_γ_cons.mp hp
    subst e
    exact atext_γ_cons.mpr ⟨p₁, p₂ ++ q, List.append_assoc p₁ p₂ q, h1,
      atext_γ_append rest y p₂ q h2 hq⟩

/-- § 390 `macro_call`: substituting covered arguments into a body covers
the concrete substitution. -/
theorem substitute_sound (args : List AText) (cargs : List TokList)
    (hargs : ∀ i, AText.γ (args.getD i []) (cargs.getD i [])) :
    ∀ body : TokList, AText.γ (substitute args body) (Concrete.subst cargs body)
  | [] => atext_γ_nil.mpr rfl
  | t :: ts => by
    have ih := substitute_sound args cargs hargs ts
    cases t with
    | outParam n =>
      show AText.γ (args.getD (n - 1) [] ++ substitute args ts)
        (cargs.getD (n - 1) [] ++ Concrete.subst cargs ts)
      exact atext_γ_append _ _ _ _ (hargs (n - 1)) ih
    | char c k =>
      show AText.γ (AItem.tok (.char c k) :: substitute args ts)
        (Token.char c k :: Concrete.subst cargs ts)
      exact atext_γ_cons.mpr ⟨[Token.char c k], Concrete.subst cargs ts, rfl, rfl, ih⟩
    | cs r =>
      show AText.γ (AItem.tok (.cs r) :: substitute args ts)
        (Token.cs r :: Concrete.subst cargs ts)
      exact atext_γ_cons.mpr ⟨[Token.cs r], Concrete.subst cargs ts, rfl, rfl, ih⟩
    | noexp r =>
      show AText.γ (AItem.tok (.noexp r) :: substitute args ts)
        (Token.noexp r :: Concrete.subst cargs ts)
      exact atext_γ_cons.mpr ⟨[Token.noexp r], Concrete.subst cargs ts, rfl, rfl, ih⟩

/-! ### Brace depth -/

/-- Brace depth after a text, `none` when a `}` closes below zero. -/
def walk : Nat → TokList → Option Nat
  | n, [] => some n
  | n, t :: ts =>
    if t.isBgroup then walk (n + 1) ts
    else if t.isEgroup then (if n = 0 then none else walk (n - 1) ts)
    else walk n ts

theorem bal_walk : ∀ (n : Nat) (ts : TokList), balancedFrom n ts = true ↔ walk n ts = some 0
  | n, [] => by simp [balancedFrom, walk]
  | n, t :: ts => by
    have ih1 := bal_walk (n + 1) ts
    have ih2 := bal_walk (n - 1) ts
    have ih3 := bal_walk n ts
    simp only [balancedFrom, walk]
    by_cases hb : t.isBgroup = true
    · simp only [hb, ↓reduceIte]; exact ih1
    · by_cases he : t.isEgroup = true
      · by_cases hn : n = 0
        · simp [hb, he, hn]
        · simp [hb, he, hn, ih2]
      · simp [hb, he, ih3]

theorem concrete_bal : ∀ (n : Nat) (ts : TokList), Concrete.balancedFrom n ts = balancedFrom n ts
  | n, [] => rfl
  | n, t :: ts => by
    simp only [Concrete.balancedFrom, balancedFrom]
    split
    · exact concrete_bal _ ts
    · split
      · split <;> simp_all [concrete_bal]
      · exact concrete_bal _ ts

theorem walk_append : ∀ (n : Nat) (p q : TokList), walk n (p ++ q) = (walk n p).bind (walk · q)
  | n, [], q => rfl
  | n, t :: p, q => by
    simp only [List.cons_append, walk]
    split
    · exact walk_append _ p q
    · split
      · split
        · rfl
        · exact walk_append _ p q
      · exact walk_append _ p q

theorem walk_shift : ∀ (p : TokList) (n k m : Nat), walk n p = some m → walk (n + k) p = some (m + k)
  | [], n, k, m, h => by simp [walk] at h ⊢; omega
  | t :: p, n, k, m, h => by
    simp only [walk] at h ⊢
    by_cases hb : t.isBgroup = true
    · simp only [hb, ↓reduceIte] at h ⊢
      rw [show n + k + 1 = n + 1 + k by omega]; exact walk_shift p (n + 1) k m h
    · by_cases he : t.isEgroup = true
      · simp only [hb, he, Bool.false_eq_true, ↓reduceIte] at h ⊢
        by_cases hn : n = 0
        · simp [hn] at h
        · simp only [hn, ↓reduceIte] at h
          rw [ite_eq_right_iff.mpr (fun h => absurd h (by omega)), show n + k - 1 = n - 1 + k by omega]
          exact walk_shift p (n - 1) k m h
      · simp only [hb, he, Bool.false_eq_true, ↓reduceIte] at h ⊢
        exact walk_shift p n k m h

/-- Text without braces leaves the depth as it is. -/
theorem walk_plain : ∀ (p : TokList) (n : Nat),
    (∀ t, t ∈ p → t.isBgroup = false ∧ t.isEgroup = false) → walk n p = some n
  | [], _, _ => rfl
  | t :: p, n, h => by
    have ⟨h1, h2⟩ := h t (List.mem_cons_self ..)
    simp only [walk, h1, h2, Bool.false_eq_true, ↓reduceIte]
    exact walk_plain p n (fun t' ht => h t' (List.mem_cons_of_mem _ ht))

/-- A prefix of a balanced text that ends at depth 0 is balanced, and so is
the rest. This is why a hole cut at a delimiter leaves balanced pieces. -/
theorem walk_cut {p m : TokList} {d : Nat} (hpm : walk 0 (p ++ m) = some 0)
    (hp : walk d p = some 0) : walk 0 p = some 0 ∧ walk 0 m = some 0 := by
  rw [walk_append] at hpm
  cases hr : walk 0 p with
  | none => rw [hr] at hpm; cases hpm
  | some r =>
    rw [hr] at hpm
    have := walk_shift p 0 d r hr
    rw [Nat.zero_add, hp] at this
    have hr0 : r = 0 := by simp at this; omega
    subst hr0
    exact ⟨rfl, hpm⟩

/-! ### Cuts and delimiters -/

theorem printInt_plain (n : Int) : ∀ t, t ∈ printInt n → t.isBgroup = false ∧ t.isEgroup = false := by
  intro t ht
  unfold printInt at ht
  split at ht
  · rcases List.mem_cons.mp ht with e | e
    · subst e; exact ⟨rfl, rfl⟩
    · obtain ⟨_, _, e⟩ := List.mem_map.mp e; subst e; exact ⟨rfl, rfl⟩
  · obtain ⟨_, _, e⟩ := List.mem_map.mp ht; subst e; exact ⟨rfl, rfl⟩

theorem mem_prefixes (p m : TokList) : p ∈ prefixes (p ++ m) :=
  List.mem_map.mpr ⟨p.length, List.mem_range.mpr (by simp; omega), by simp⟩

theorem mem_suffixes (p m : TokList) : m ∈ suffixes (p ++ m) :=
  List.mem_map.mpr ⟨p.length, List.mem_range.mpr (by simp; omega), by simp⟩

/-- A cut inside an item at brace depth 0 has covered pieces. -/
theorem item_cut (it : AItem) (hnt : ∀ t, it ≠ .tok t) {p m : TokList} {d : Nat}
    (h : it.γ (p ++ m)) (hp : walk d p = some 0) :
    ∃ c, c ∈ it.cuts ∧ c.1.γ p ∧ c.2.γ m := by
  cases it with
  | tok t => exact absurd rfl (hnt t)
  | hole hl =>
    obtain ⟨ha, _, hlen, hb⟩ := h
    have ⟨b1, b2⟩ := walk_cut ((bal_walk 0 _).mp hb) hp
    refine ⟨_, List.mem_singleton.mpr rfl, ⟨fun t ht => ha t (List.mem_append_left _ ht),
      fun h => by simp [Hole.part] at h, fun n hn => ?_, (bal_walk 0 _).mpr b1⟩,
      ⟨fun t ht => ha t (List.mem_append_right _ ht), fun h => by simp [Hole.part] at h,
      fun n hn => ?_, (bal_walk 0 _).mpr b2⟩⟩
    · have := hlen n hn; simp at this; omega
    · have := hlen n hn; simp at this; omega
  | digits i =>
    obtain ⟨n, _, e⟩ := h
    have hpl := printInt_plain n
    have hnum := printInt_number n
    rw [← e] at hpl hnum
    refine ⟨_, List.mem_singleton.mpr rfl,
      ⟨fun t ht => hnum t (List.mem_append_left _ ht), fun h => by simp at h, fun _ h => by simp at h,
        (bal_walk 0 _).mpr (walk_plain p 0 (fun t ht => hpl t (List.mem_append_left _ ht)))⟩,
      ⟨fun t ht => hnum t (List.mem_append_right _ ht), fun h => by simp at h, fun _ h => by simp at h,
        (bal_walk 0 _).mpr (walk_plain m 0 (fun t ht => hpl t (List.mem_append_right _ ht)))⟩⟩
  | oneOf alts =>
    exact ⟨_, List.mem_singleton.mpr rfl, List.mem_flatMap.mpr ⟨_, h, mem_prefixes p m⟩,
      List.mem_flatMap.mpr ⟨_, h, mem_suffixes p m⟩⟩

theorem nil_cut : ∀ xs : AText, ([], xs) ∈ xs.cuts
  | [] => List.mem_singleton.mpr rfl
  | _ :: _ => List.mem_cons_self ..

/-- Every concrete cut at brace depth 0 has an abstract cut that covers it. -/
theorem cuts_sound : ∀ (x : AText) (d : Nat) (p q : TokList),
    AText.γ x (p ++ q) → walk d p = some 0 → ∃ c, c ∈ x.cuts ∧ AText.γ c.1 p ∧ AText.γ c.2 q
  | [], d, p, q, h, _ => by
    have e : p ++ q = [] := h
    obtain ⟨e1, e2⟩ := List.append_eq_nil_iff.mp e
    subst e1 e2
    exact ⟨([], []), List.mem_singleton.mpr rfl, rfl, rfl⟩
  | it :: xs, d, p, q, h, hw => by
    obtain ⟨p₁, p₂, e, h1, h2⟩ := h
    by_cases hp0 : p = []
    · subst hp0
      exact ⟨([], it :: xs), List.mem_cons_self .., rfl, ⟨p₁, p₂, e, h1, h2⟩⟩
    rcases List.append_eq_append_iff.mp e with ⟨m, e1, e2⟩ | ⟨m, e1, e2⟩
    · -- the cut lies inside `it`: `p₁ = p ++ m`
      subst e1 e2
      cases it with
      | tok t =>
        have ht : p ++ m = [t] := h1
        have hm : m = [] := by
          cases p with
          | nil => exact absurd rfl hp0
          | cons _ p' =>
            simp only [List.cons_append, List.cons.injEq] at ht
            exact (List.append_eq_nil_iff.mp ht.2).2
        subst hm
        rw [List.append_nil] at ht
        exact ⟨(AItem.tok t :: [], xs), List.mem_cons_of_mem _ (List.mem_append_right _
          (List.mem_map.mpr ⟨([], xs), nil_cut xs, rfl⟩)), ⟨p, [], by simp, ht, rfl⟩, h2⟩
      | hole hl => exact inside (.hole hl) (fun _ h => by cases h) h1 hw h2
      | digits i => exact inside (.digits i) (fun _ h => by cases h) h1 hw h2
      | oneOf alts => exact inside (.oneOf alts) (fun _ h => by cases h) h1 hw h2
    · -- the cut lies in `xs`: `p = p₁ ++ m`
      subst e1
      rw [walk_append] at hw
      cases hd : walk d p₁ with
      | none => rw [hd] at hw; cases hw
      | some d' =>
        rw [hd] at hw
        obtain ⟨c, hc, hc1, hc2⟩ := cuts_sound xs d' m q (e2 ▸ h2) hw
        exact ⟨(it :: c.1, c.2), List.mem_cons_of_mem _ (List.mem_append_right _
          (List.mem_map.mpr ⟨c, hc, rfl⟩)), ⟨p₁, m, rfl, h1, hc1⟩, hc2⟩
where
  inside (it : AItem) (hnt : ∀ t, it ≠ .tok t) {p m p₂ : TokList} {xs : AText} {d : Nat}
      (h1 : it.γ (p ++ m)) (hw : walk d p = some 0) (h2 : AText.γ xs p₂) :
      ∃ c, c ∈ AText.cuts (it :: xs) ∧ AText.γ c.1 p ∧ AText.γ c.2 (m ++ p₂) := by
    obtain ⟨c, hc, hc1, hc2⟩ := item_cut it hnt h1 hw
    exact ⟨([c.1], c.2 :: xs), List.mem_cons_of_mem _ (List.mem_append_left _
      (List.mem_map.mpr ⟨c, hc, rfl⟩)), ⟨p, [], by simp, hc1, rfl⟩, ⟨m, p₂, rfl, hc2, h2⟩⟩

/-- Delimiters and leading tokens hold no braces (§ 473: a `{` ends the
parameter text, a `}` there is an error). -/
def NoBraces (δ : TokList) : Prop := ∀ t, t ∈ δ → t.isBgroup = false ∧ t.isEgroup = false

/-- The step of `dropAbs_sound` at an item that is not a known token. -/
theorem dropAbs_item (it : AItem) (hnt : ∀ t, it ≠ .tok t) (x : AText) (d : Token) (ds rest : TokList)
    (hδ : NoBraces (d :: ds)) (h : AText.γ (it :: x) (d :: ds ++ rest))
    (ih : ∀ δ rest, NoBraces δ → AText.γ x (δ ++ rest) → ∃ r, r ∈ dropAbs δ x ∧ AText.γ r rest)
    (heq : dropAbs (d :: ds) (it :: x) = it.cuts.map (fun c => c.2 :: x) ++
      (List.range (ds.length + 2)).flatMap (fun i => dropAbs ((d :: ds).drop i) x)) :
    ∃ r, r ∈ dropAbs (d :: ds) (it :: x) ∧ AText.γ r rest := by
  rw [heq]
  obtain ⟨p₁, p₂, e, h1, h2⟩ := h
  rcases List.append_eq_append_iff.mp e.symm with ⟨m, e1, e2⟩ | ⟨m, e1, e2⟩
  · -- `it` holds a part of `δ` whole: `δ = p₁ ++ m`
    obtain ⟨r, hr, hγ⟩ := ih m rest (fun t ht => hδ t (e1 ▸ List.mem_append_right _ ht))
      (e2 ▸ h2)
    refine ⟨r, List.mem_append_right _ (List.mem_flatMap.mpr ⟨p₁.length, ?_, ?_⟩), hγ⟩
    · have : (d :: ds).length = p₁.length + m.length := by rw [e1]; simp
      exact List.mem_range.mpr (by simp at this; omega)
    · rw [e1]; simpa using hr
  · -- `δ` ends inside `it`: `p₁ = δ ++ m`
    subst e1
    obtain ⟨c, hc, _, hc2⟩ := item_cut it hnt (d := 0) h1 (walk_plain _ 0 hδ)
    exact ⟨c.2 :: x, List.mem_append_left _ (List.mem_map.mpr ⟨c, hc, rfl⟩),
      ⟨m, p₂, e2 ▸ rfl, hc2, h2⟩⟩

/-- Known tokens at the head of covered text: some rest covers what follows. -/
theorem dropAbs_sound : ∀ (x : AText) (δ rest : TokList), NoBraces δ →
    AText.γ x (δ ++ rest) → ∃ r, r ∈ dropAbs δ x ∧ AText.γ r rest
  | x, [], rest, _, h => ⟨x, by cases x <;> exact List.mem_singleton.mpr rfl, h⟩
  | [], d :: ds, rest, _, h => by have : d :: ds ++ rest = [] := h; simp at this
  | .tok t :: x, d :: ds, rest, hδ, h => by
    obtain ⟨p₁, p₂, e, h1, h2⟩ := h
    have e1 : p₁ = [t] := h1
    subst e1
    simp only [List.cons_append, List.cons.injEq] at e
    obtain ⟨e1, e2⟩ := e
    subst e1
    obtain ⟨r, hr, hγ⟩ := dropAbs_sound x ds rest (fun t ht => hδ t (List.mem_cons_of_mem _ ht))
      (by simp at e2; exact e2 ▸ h2)
    exact ⟨r, by simp only [dropAbs, ↓reduceIte]; exact hr, hγ⟩
  | .hole hl :: x, d :: ds, rest, hδ, h =>
    dropAbs_item (.hole hl) (fun _ h => by cases h) x d ds rest hδ h
      (fun δ r hδ' h' => dropAbs_sound x δ r hδ' h') rfl
  | .digits i :: x, d :: ds, rest, hδ, h =>
    dropAbs_item (.digits i) (fun _ h => by cases h) x d ds rest hδ h
      (fun δ r hδ' h' => dropAbs_sound x δ r hδ' h') rfl
  | .oneOf alts :: x, d :: ds, rest, hδ, h =>
    dropAbs_item (.oneOf alts) (fun _ h => by cases h) x d ds rest hδ h
      (fun δ r hδ' h' => dropAbs_sound x δ r hδ' h') rfl

/-- `scanDelim_sound` (CHANGES item 7): TeX's delimited argument (§ 392, the
shortest balanced text before the delimiter) and the rest after the
delimiter are covered by a pair the abstract scan lists, also over typed
holes, digits and one-of tokens (REVIEW-2 R2-1, R2-6: the delimiter may
start in one item and end in a later one). -/
theorem scanDelim_sound {x : AText} {arg δ rest : TokList} (hδ : NoBraces δ)
    (h : AText.γ x (arg ++ δ ++ rest)) (harg : Concrete.firstDelim arg δ) :
    ∃ c, c ∈ scanDelim δ x ∧ AText.γ c.1 arg ∧ AText.γ c.2 rest := by
  have hb : walk 0 arg = some 0 := by
    have := harg.1; unfold Concrete.balanced at this; rw [concrete_bal] at this
    exact (bal_walk 0 arg).mp this
  rw [List.append_assoc] at h
  obtain ⟨c, hc, hc1, hc2⟩ := cuts_sound x 0 arg (δ ++ rest) h hb
  obtain ⟨r, hr, hγ⟩ := dropAbs_sound c.2 δ rest hδ hc2
  exact ⟨(c.1, r), List.mem_flatMap.mpr ⟨c, hc, List.mem_map.mpr ⟨r, hr, rfl⟩⟩, hc1, hγ⟩

/-- § 400: the argument after brace stripping is covered. -/
theorem stripBraces_sound {xa : AText} {ts : TokList} (h : AText.γ xa ts)
    (hb : balanced ts = true) :
    ∃ y, y ∈ stripBraces xa ∧ AText.γ y (Concrete.stripBraces ts) := by
  unfold Concrete.stripBraces
  split
  · rename_i b rest
    split
    · rename_i e he
      split
      · rename_i hc
        simp only [Bool.and_eq_true] at hc
        refine ⟨AText.hull [xa], List.mem_cons_of_mem _ (List.mem_singleton.mpr rfl), ?_⟩
        refine ⟨rest.dropLast, [], (List.append_nil _).symm, ⟨fun t ht => ?_,
          (fun h => by simp at h), (fun _ h => by simp at h), ?_⟩, rfl⟩
        · have hm : t ∈ b :: rest := List.mem_cons_of_mem _ (List.dropLast_subset _ ht)
          obtain ⟨it, hit, ha⟩ := text_admits xa _ h t hm
          exact (fold_text_cover [xa] {}).2 xa (List.mem_singleton.mpr rfl) it hit t ha
        · have := hc.2; unfold Concrete.balanced at this; rw [concrete_bal] at this; exact this
      · exact ⟨xa, List.mem_cons_self .., h⟩
    · exact ⟨xa, List.mem_cons_self .., h⟩
  · exact ⟨xa, List.mem_cons_self .., h⟩

/-- A widened macro (`AMeaning.text`) has no parameters: its expansion
pushes its body, which `γ` covers. -/
theorem text_meaning_body {lg ou pr : Bool} {body : AText} {m : Meaning}
    (h : (AMeaning.text lg ou pr body).γ m) :
    ∃ mc : Macro, m = .call mc ∧ mc.params = [] ∧ AText.γ body mc.body := by
  obtain ⟨mc, e, hp, _, _, _, _, hb⟩ := h
  exact ⟨mc, e, hp, hb⟩

/-! ## 7. Loop heads: k passes, widening, the forced summary -/

/-- The state a round of `loopHead` joins into the head. -/
def headJoin (L : Limits) (join widen : AState → AState → AState) (body : AState → AState)
    (passes : Nat) (a : AState) : AState :=
  if passes ≥ L.widenAfter then widen a (join a (body a)) else join a (body a)

theorem loopHead_succ (L : Limits) (join widen : AState → AState → AState)
    (grows : AState → AState → Bool) (body summary : AState → AState) (n passes : Nat)
    (a : AState) :
    loopHead L join widen grows body summary (n + 1) passes a =
      (if !grows a (headJoin L join widen body passes a) then a
       else if passes ≥ 2 * L.widenAfter then summary (headJoin L join widen body passes a)
       else loopHead L join widen grows body summary n (passes + 1)
         (headJoin L join widen body passes a)) := rfl

/-- The loop head's result covers its entry and is a post-fixpoint of the
body, whatever the fuel: so it covers every number of passes. -/
theorem loopHead_post (L : Limits) (join widen : AState → AState → AState)
    (grows : AState → AState → Bool) (body summary : AState → AState)
    (hj : ∀ x y, ALe x (join x y) ∧ ALe y (join x y))
    (hw : ∀ x y, ALe x (widen x y) ∧ ALe y (widen x y))
    (hg : ∀ x y, grows x y = false → ALe y x)
    (hs : ∀ x, ALe x (summary x) ∧ ALe (body (summary x)) (summary x)) :
    ∀ fuel passes a,
      ALe a (loopHead L join widen grows body summary fuel passes a) ∧
      ALe (body (loopHead L join widen grows body summary fuel passes a))
        (loopHead L join widen grows body summary fuel passes a) := by
  intro fuel
  induction fuel with
  | zero => intro passes a; exact hs a
  | succ n ih =>
    intro passes a
    have hJ : ALe a (headJoin L join widen body passes a) ∧
        ALe (body a) (headJoin L join widen body passes a) := by
      unfold headJoin
      refine ite_prop (P := fun r => ALe a r ∧ ALe (body a) r) (fun _ => ?_) (fun _ => ?_)
      · exact ⟨ale_trans (hj a (body a)).1 (hw a (join a (body a))).2,
          ale_trans (hj a (body a)).2 (hw a (join a (body a))).2⟩
      · exact ⟨(hj a (body a)).1, (hj a (body a)).2⟩
    rw [loopHead_succ]
    refine ite_prop (P := fun r => ALe a r ∧ ALe (body r) r) (fun hgr => ?_) (fun _ => ?_)
    · have hf : grows a (headJoin L join widen body passes a) = false := by
        revert hgr
        cases grows a (headJoin L join widen body passes a) <;> decide
      exact ⟨ale_refl a, ale_trans hJ.2 (hg _ _ hf)⟩
    · refine ite_prop (P := fun r => ALe a r ∧ ALe (body r) r) (fun _ => ?_) (fun _ => ?_)
      · exact ⟨ale_trans hJ.1 (hs _).1, (hs _).2⟩
      · obtain ⟨h1, h2⟩ := ih (passes + 1) (headJoin L join widen body passes a)
        exact ⟨ale_trans hJ.1 h1, h2⟩

/-- Loop soundness: every concrete run of the body from a covered state
ends covered by the loop head's result. -/
theorem loopHead_sound (L : Limits) (join widen : AState → AState → AState)
    (grows : AState → AState → Bool) (body summary : AState → AState)
    (Body : State → State → Prop)
    (hbody : ∀ x s s', x.γ s → Body s s' → (body x).γ s')
    (hj : ∀ x y, ALe x (join x y) ∧ ALe y (join x y))
    (hw : ∀ x y, ALe x (widen x y) ∧ ALe y (widen x y))
    (hg : ∀ x y, grows x y = false → ALe y x)
    (hs : ∀ x, ALe x (summary x) ∧ ALe (body (summary x)) (summary x))
    (fuel passes : Nat) (a : AState) (s s' : State) (ha : a.γ s) (hrun : Star Body s s') :
    (loopHead L join widen grows body summary fuel passes a).γ s' := by
  have hp := loopHead_post L join widen grows body summary hj hw hg hs fuel passes a
  induction hrun with
  | refl => exact hp.1 _ ha
  | tail _ hbc ih => exact hp.2 _ (hbody _ _ _ ih hbc)

/-- The never-hang half of a loop: at most `fuel + 1` rounds (job 2's
`loopRounds_le`), whatever the precision. -/
theorem loopHead_rounds (L : Limits) (join widen : AState → AState → AState)
    (grows : AState → AState → Bool) (body : AState → AState) (fuel passes : Nat) (a : AState) :
    loopRounds L join widen grows body fuel passes a ≤ fuel + 1 :=
  loopRounds_le L join widen grows body fuel passes a

/-! ## 8. Global theorems

Intuition. Soundness of a whole run is induction on the concrete trace:
each concrete step from a covered state has a covering abstract run
(`run_covered`). The local obligation is split by family. A step first
reads a command and its operands (the mouth and the scanners), then runs
the command's effect. The effects of assignment and grouping are proven
below (`fam_covered`); conditionals, loops and delimited arguments have
their lemmas above. What is left is the abstract reading itself and the
families without a transfer (PROOF-STATUS "Open"). -/

/-- `T` covers every step of `R` that starts in a covered state. -/
def Covered {A C : Type} (γ : A → C → Prop) (T : A → A → Prop) (R : C → C → Prop) : Prop :=
  ∀ a c c', γ a c → R c c' → ∃ a', Star T a a' ∧ γ a' c'

/-- Families combine: a step in one of them is covered. -/
theorem covered_or {A C : Type} {γ : A → C → Prop} {T : A → A → Prop} {R S : C → C → Prop}
    (hR : Covered γ T R) (hS : Covered γ T S) : Covered γ T (fun c c' => R c c' ∨ S c c') :=
  fun a c c' ha h => h.elim (hR a c c' ha) (hS a c c' ha)

/-- A read, then an effect: covered when both are. -/
theorem covered_comp {A C : Type} {γ : A → C → Prop} {T : A → A → Prop} {R S : C → C → Prop}
    (hR : Covered γ T R) (hS : Covered γ T S) :
    Covered γ T (fun c c'' => ∃ c', R c c' ∧ S c' c'') := by
  intro a c c'' ha ⟨c', h1, h2⟩
  obtain ⟨a1, t1, g1⟩ := hR a c c' ha h1
  obtain ⟨a2, t2, g2⟩ := hS a1 c' c'' g1 h2
  exact ⟨a2, Star.trans t1 t2, g2⟩

/-- A command as TeX reads it, with its operand: an assignment (§ 1211),
`\begingroup`/`{` (§ 1063), a group's end (§ 1086), or `none`: the step's
whole change is in its reading (expansion, `\relax`, a test). -/
inductive CCmd where
  | assign (glob : Bool) (l : Loc) (x : Val)
  | begin (c : GroupCode) (ctx : Option BoxContext)
  | close (c : GroupCode)
  | none

/-- The same command as the abstract run reads it. -/
inductive ACmd where
  | assign (glob : Bool) (l : Loc) (v : AVal)
  | begin (c : GroupCode)
  | close (c : GroupCode)
  | none

def ACmd.covers : ACmd → CCmd → Prop
  | .assign g l v, .assign g' l' x => g = g' ∧ l = l' ∧ v.γ x
  | .begin c, .begin c' _ => c = c'
  | .close c, .close c' => c = c'
  | .none, .none => True
  | _, _ => False

/-- The concrete effect of a command. `glob` is the globality after
`\globaldefs` (`Concrete.isGlobal`, § 1214). -/
inductive Effect : State → CCmd → State → Prop
  | assign {s : State} {glob : Bool} {l : Loc} {x : Val} :
      Effect s (.assign glob l x)
        (if glob then Concrete.geqDefine s l x else Concrete.eqDefine s l x)
  | begin {s : State} {c : GroupCode} {ctx : Option BoxContext} :
      Effect s (.begin c ctx) (Concrete.newSaveLevel c ctx s)
  | close {s : State} {c : GroupCode} {E : Loc → Entry} {ins : TokList} {ctx : Option BoxContext}
      {crest : List SaveEntry} :
      Concrete.unsaveAux s.eqtb [] s.save = (E, ins, some (c, ctx), crest) →
      Effect s (.close c) (Concrete.unsave s).1
  | none {s : State} : Effect s .none s

/-- The abstract effect: the model's transfers. -/
def aeffect (L : Limits) (a : AState) : ACmd → List AState
  | .assign glob l v => [a.assign glob l v]
  | .begin c => [a.beginGroup c]
  | .close c => a.endGroup L c
  | .none => [a]

/-- The abstract step: read a command (`read`, the abstract mouth and
scanners), then run its effect. -/
def AStep (L : Limits) (read : AState → List (AState × ACmd)) (a b : AState) : Prop :=
  ∃ a1 k, (a1, k) ∈ read a ∧ b ∈ aeffect L a1 k

/-- What the abstract reading must satisfy: it covers TeX's reading, state
and command. -/
def ReadCovers (Read : State → State → CCmd → Prop) (read : AState → List (AState × ACmd)) : Prop :=
  ∀ a s s1 k, a.γ s → Read s s1 k → ∃ a1 k', (a1, k') ∈ read a ∧ a1.γ s1 ∧ k'.covers k

/-- § 274 `new_save_level`. -/
theorem beginGroup_sound {a : AState} {s : State} (ha : a.γ s) (c : GroupCode)
    (ctx : Option BoxContext) : (a.beginGroup c).γ (Concrete.newSaveLevel c ctx s) := by
  obtain ⟨h1, h2, h3, h4, h5, h6, h7⟩ := ha
  exact ⟨h1, h2, fun hp => by simp only [AState.beginGroup, Concrete.newSaveLevel]; rw [h3 hp],
    SaveRel.group h4, h5, h6, h7⟩

/-- Every effect of a command is covered by the abstract effect of a
covering command. -/
theorem effect_sound (L : Limits) {a : AState} {s s' : State} {k : CCmd} {k' : ACmd}
    (ha : a.γ s) (hk : k'.covers k) (h : Effect s k s') : ∃ b, b ∈ aeffect L a k' ∧ b.γ s' := by
  cases h with
  | @assign glob l x =>
    cases k' with
    | assign g l' v =>
      obtain ⟨e1, e2, hv⟩ := hk; subst e1 e2
      refine ⟨_, List.mem_singleton.mpr rfl, ?_⟩
      cases g
      · exact assign_local_sound ha hv
      · exact assign_global_sound ha hv
    | _ => cases hk
  | @begin c ctx =>
    cases k' with
    | begin c' => subst hk; exact ⟨_, List.mem_singleton.mpr rfl, beginGroup_sound ha _ _⟩
    | _ => cases hk
  | @close c E ins ctx crest hu =>
    cases k' with
    | close c' => subst hk; exact endGroup_sound L ha hu
    | _ => cases hk
  | none =>
    cases k' with
    | none => exact ⟨_, List.mem_singleton.mpr rfl, ha⟩
    | _ => cases hk

/-- The families with a transfer are covered, given the abstract reading. -/
theorem fam_covered (L : Limits) (Read : State → State → CCmd → Prop)
    (read : AState → List (AState × ACmd)) (hread : ReadCovers Read read) :
    Covered AState.γ (AStep L read) (fun s s' => ∃ s1 k, Read s s1 k ∧ Effect s1 k s') := by
  intro a s s' ha ⟨s1, k, hr, he⟩
  obtain ⟨a1, k', hm, h1, hk⟩ := hread a s s1 k ha hr
  obtain ⟨b, hb, hγ⟩ := effect_sound L h1 hk he
  exact ⟨b, Star.single ⟨a1, k', hm, hb⟩, hγ⟩

/-- The abstract run covers every concrete run (by induction on the
concrete trace), given local soundness of the abstract transfer `T` for
job 1's `Step`. -/
theorem run_covered (E : Concrete.Engine) (T : AState → AState → Prop)
    (hloc : Covered AState.γ T (Concrete.Step E)) :
    ∀ s s', Concrete.Steps E s s' → ∀ a, a.γ s → ∃ a', Star T a a' ∧ a'.γ s' := by
  intro s s' h
  induction h with
  | refl => intro a ha; exact ⟨a, Star.refl, ha⟩
  | step h1 _ ih =>
    intro a ha
    obtain ⟨a1, ht, h1'⟩ := hloc a _ _ ha h1
    obtain ⟨a', ht', h'⟩ := ih a1 h1'
    exact ⟨a', Star.trans ht ht', h'⟩

/-- The run theorem by families: a step is a read command with a modelled
effect, or of another family `Other` whose coverage is still an obligation
(PROOF-STATUS lists them). -/
theorem run_covered_families (E : Concrete.Engine) (L : Limits)
    (Read : State → State → CCmd → Prop) (read : AState → List (AState × ACmd))
    (hread : ReadCovers Read read) (Other : State → State → Prop)
    (hother : Covered AState.γ (AStep L read) Other)
    (hsplit : ∀ s s', Concrete.Step E s s' →
      (∃ s1 k, Read s s1 k ∧ Effect s1 k s') ∨ Other s s') :
    ∀ s s', Concrete.Steps E s s' → ∀ a, a.γ s → ∃ a', Star (AStep L read) a a' ∧ a'.γ s' :=
  run_covered E (AStep L read) (fun a s s' ha h =>
    covered_or (fam_covered L Read read hread) hother a s s' ha (hsplit s s' h))

/-- Paths: a transfer that maps a path to its successor paths. -/
theorem paths_covered (E : Concrete.Engine) (trans : AState → List AState)
    (hloc : ∀ a s s', a.γ s → Concrete.Step E s s' → ∃ a', a' ∈ trans a ∧ a'.γ s') :
    ∀ s s', Concrete.Steps E s s' → ∀ a, a.γ s →
      ∃ a', Star (fun x y => y ∈ trans x) a a' ∧ a'.γ s' := by
  apply run_covered E
  intro a s s' ha hs
  obtain ⟨a', h1, h2⟩ := hloc a s s' ha hs
  exact ⟨a', Star.single h1, h2⟩

/-! ### The budget: never hang, and stay sound

`Machine::charge` pays every step from `limits.steps`. Modelled: one
counter per run, in the state, that each abstract step decreases; at zero
the run is cut to a state covering everything (`none`). A budget per run,
not per step or per line (the Rust's line guard is not modelled). Past the
cut `bγ` is `True`: nothing is claimed of the rest of the run. Sound only if
the cut really is a top (P6 in PROOF-STATUS.md). -/

inductive BStep {A : Type} (T : A → A → Prop) : Nat × Option A → Nat × Option A → Prop where
  | step {n : Nat} {a a' : A} : T a a' → BStep T (n + 1, some a) (n, some a')
  | cut {a : A} : BStep T (0, some a) (0, none)

def bγ {A C : Type} (γ : A → C → Prop) : Nat × Option A → C → Prop
  | (_, some a), c => γ a c
  | (_, none), _ => True

def bμ {A : Type} : Nat × Option A → Nat
  | (n, some _) => n + 1
  | (_, none) => 0

theorem bstep_dec {A : Type} (T : A → A → Prop) (x y : Nat × Option A)
    (h : BStep T x y) : bμ y < bμ x := by
  cases h <;> simp only [bμ] <;> omega

/-- An abstract run under the budget: it reaches its end, or the cut. -/
theorem budget_star {A : Type} (T : A → A → Prop) {a b : A} (h : Star T a b) :
    ∀ n, ∃ y, Star (BStep T) (n, some a) y ∧ ((∃ m, y = (m, some b)) ∨ y.2 = none) := by
  induction h with
  | refl => intro n; exact ⟨(n, some _), Star.refl, Or.inl ⟨n, rfl⟩⟩
  | tail _ hbc ih =>
    intro n
    obtain ⟨y, hy, hb⟩ := ih n
    rcases hb with ⟨m, e⟩ | e
    · subst e
      cases m with
      | zero => exact ⟨(0, none), Star.tail hy BStep.cut, Or.inr rfl⟩
      | succ m => exact ⟨(m, some _), Star.tail hy (BStep.step hbc), Or.inl ⟨m, rfl⟩⟩
    · exact ⟨y, hy, Or.inr e⟩

theorem budget_covers {A C : Type} (γ : A → C → Prop) (Step : C → C → Prop)
    (T : A → A → Prop) (hloc : Covered γ T Step) :
    ∀ x c c', bγ γ x c → Step c c' → ∃ y, Star (BStep T) x y ∧ bγ γ y c' := by
  intro x c c' hx hs
  obtain ⟨n, oa⟩ := x
  cases oa with
  | none => exact ⟨(n, none), Star.refl, True.intro⟩
  | some a =>
    obtain ⟨a', ht, hγ⟩ := hloc a c c' hx hs
    obtain ⟨y, hy, hb⟩ := budget_star T ht n
    refine ⟨y, hy, ?_⟩
    rcases hb with ⟨m, e⟩ | e
    · subst e; exact hγ
    · obtain ⟨m, o⟩ := y; simp only at e; subst e; exact True.intro

/-- The main theorem: with one budget per run the abstract run covers every
concrete run, and it terminates (no infinite abstract run, and no run
longer than the budget). The cut goes to a state that covers everything:
the budget never makes the run unsound, only coarser, and after the cut
nothing is claimed. Conditional on `hloc`. -/
theorem sound_and_terminating {A C : Type} (γ : A → C → Prop) (Step : C → C → Prop)
    (T : A → A → Prop) (hloc : Covered γ T Step) :
    (∀ c₀ c, Star Step c₀ c → ∀ x₀, bγ γ x₀ c₀ → ∃ x, Star (BStep T) x₀ x ∧ bγ γ x c) ∧
    WellFounded (fun y x => BStep T x y) ∧
    (∀ n x y, StarN (BStep T) x n y → n + bμ y ≤ bμ x) :=
  ⟨simulation_star Step (BStep T) (bγ γ) (budget_covers γ Step T hloc),
   wf_of_budget (BStep T) bμ (bstep_dec T),
   run_length_le_budget (BStep T) bμ (bstep_dec T)⟩

/-- The main theorem for SaTeX's model, its local obligation split by
family: the abstract reading of commands (`hread`) and the families without
a transfer (`hother`). Both are hypotheses, and `Read`, `Other` are free:
with `Other := Concrete.Step E` the whole obligation is `hother`, and with
`Read := ⊥` `hread` is trivial. What is proven is the induction, the budget
and the effects of `ACmd` (assign, begin, close, none). -/
theorem satex_sound_and_terminating (E : Concrete.Engine) (L : Limits)
    (Read : State → State → CCmd → Prop) (read : AState → List (AState × ACmd))
    (hread : ReadCovers Read read) (Other : State → State → Prop)
    (hother : Covered AState.γ (AStep L read) Other)
    (hsplit : ∀ s s', Concrete.Step E s s' → (∃ s1 k, Read s s1 k ∧ Effect s1 k s') ∨ Other s s') :
    (∀ s₀ s, Star (Concrete.Step E) s₀ s → ∀ x₀, bγ AState.γ x₀ s₀ →
      ∃ x, Star (BStep (AStep L read)) x₀ x ∧ bγ AState.γ x s) ∧
    WellFounded (fun y x => BStep (AStep L read) x y) ∧
    (∀ n x y, StarN (BStep (AStep L read)) x n y → n + bμ y ≤ bμ x) :=
  sound_and_terminating AState.γ (Concrete.Step E) (AStep L read) (fun a s s' ha h =>
    covered_or (fam_covered L Read read hread) hother a s s' ha (hsplit s s' h))

end Satex.Soundness
