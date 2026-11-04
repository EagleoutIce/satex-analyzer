/-
Satex.Soundness: the abstract run of `Satex.Abstract` over-approximates every
concrete run of `Satex.Concrete`, and it terminates.

Part 1 is domain-independent: orders, the loop with k passes and a summary,
simulation, the budget. Part 2 instantiates it with job 2's domains and
transfers against job 1's `Step`. Open items and counterexamples:
semantics/PROOF-STATUS.md. Plain core Lean 4, no mathlib.
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
    · rw [if_pos h]
      exact ⟨D.le_refl a, D.le_trans _ _ _ (D.join_r a (f a)) (hstable _ _ h)⟩
    · rw [if_neg h]
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
  ⟨fun a => D.le_top a, fun a => D.le_top (f D.top)⟩

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
    · rw [if_pos h]; omega
    · rw [if_neg h]; have := ih (D.join a (f a)); omega

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
  · rw [if_pos h]; exact hx h
  · rw [if_neg h]; exact hy h

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
    simp only [h1, h2, h3, h4, ↓reduceIte, Int.reduceEq, and_true, true_and, and_self, and_false, false_and, if_true, if_false, ite_self] <;>
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
    (AVal.toks ms).γ v ↔ ∃ a p, a ∈ ms ∧ v = .toks p ∧ AText.γ a p := Iff.rfl
theorem γ_box {mv : Bool} {wd ht dp : Itv} {v : Val} :
    (AVal.box mv wd ht dp).γ v ↔ (mv = true ∧ v = .box .void) ∨
      ∃ x y z, (v = .box (.hbox x y z) ∨ v = .box (.vbox x y z)) ∧
        wd.γ x ∧ ht.γ y ∧ dp.γ z := Iff.rfl
theorem γ_topv (v : Val) : AVal.top.γ v := True.intro

/-- What a text hull must satisfy to be sound. **False** for `AText.hull`
as defined: see PROOF-STATUS.md (P3, P4). Every theorem that needs it
takes it as a hypothesis. -/
def HullSound : Prop :=
  ∀ (xs : List AText) (x : AText) (p : TokList), x ∈ xs → AText.γ x p → AText.γ (AText.hull xs) p

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

theorem join_sound_toks (L : Limits) (hull : HullSound) (xs ys : List AText) (v : Val)
    (h : (AVal.toks xs).γ v ∨ (AVal.toks ys).γ v) :
    (AVal.join L (.toks xs) (.toks ys)).γ v := by
  have hm : ∃ x p, x ∈ xs ++ ys ∧ v = .toks p ∧ AText.γ x p := by
    rcases h with h | h
    · obtain ⟨x, p, hi, hv, hn⟩ := γ_toks.mp h
      exact ⟨x, p, List.mem_append.mpr (Or.inl hi), hv, hn⟩
    · obtain ⟨x, p, hi, hv, hn⟩ := γ_toks.mp h
      exact ⟨x, p, List.mem_append.mpr (Or.inr hi), hv, hn⟩
  simp only [AVal.join]
  refine ite_prop (P := fun r => AVal.γ r v) (fun _ => γ_toks.mpr hm) (fun _ => ?_)
  obtain ⟨x, p, hx, hv, hp⟩ := hm
  exact γ_toks.mpr ⟨AText.hull (xs ++ ys), p, List.mem_singleton.mpr rfl, hv, hull _ x p hx hp⟩

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
given a sound text hull. -/
theorem aval_join_sound (L : Limits) (hull : HullSound) (a b : AVal) (v : Val)
    (h : a.γ v ∨ b.γ v) : (AVal.join L a b).γ v := by
  cases a <;> cases b
  all_goals first
    | exact True.intro
    | exact join_sound_meanings L _ _ v h
    | exact join_sound_ints L _ _ v h
    | exact join_sound_glues L _ _ v h
    | exact join_sound_toks L hull _ _ v h
    | exact join_sound_box L _ _ _ _ _ _ _ _ v h

/-- `AVal` with the semantic order is an abstract domain in the sense of `AbsDom`. -/
def avalDom (L : Limits) (hull : HullSound) : AbsDom Val AVal where
  le := AVal.le
  join := AVal.join L
  top := .top
  γ := AVal.γ
  le_refl := by intro a c h; exact h
  le_trans := by intro a b d h1 h2 c hc; exact h2 c (h1 c hc)
  join_l := by intro a b c hc; exact aval_join_sound L hull a b c (Or.inl hc)
  join_r := by intro a b c hc; exact aval_join_sound L hull a b c (Or.inr hc)
  le_top := by intro a c _; exact True.intro
  γ_mono := by intro a b h c hc; exact h c hc

/-! ## 4. Assignment with groups (§§ 277, 279) -/

/-- The semantic order on abstract states. -/
abbrev ALe (x y : AState) : Prop := ∀ s, x.γ s → y.γ s

theorem ale_refl (x : AState) : ALe x x := fun _ h => h
theorem ale_trans {x y z : AState} (h1 : ALe x y) (h2 : ALe y z) : ALe x z :=
  fun s h => h2 s (h1 s h)

/-- A write of a covered value at a matching level keeps `γ`. -/
theorem γ_write {a : AState} {s : State} {l : Loc} {v : AVal} {x : Val} {lv n : Nat}
    {sv : List ASave} {cs : List SaveEntry}
    (ha : a.γ s) (hv : v.γ x) (hsv : SaveRel sv cs)
    (hcur : (∀ f, f ∈ sv → ∀ c, f ≠ .group c true) → a.curLevel = s.curLevel)
    (hlv : lv = n) :
    AState.γ { a with eqtb := upd a.eqtb l v, level := upd a.level l lv, save := sv }
      { s with eqtb := upd s.eqtb l { val := x, level := n }, save := cs } := by
  obtain ⟨h1, h2, _, _, h5, h6, h7⟩ := ha
  unfold AState.γ
  refine ⟨?_, ?_, hcur, hsv, h5, h6, h7⟩
  · intro l'
    show (upd a.eqtb l v l').γ (upd s.eqtb l { val := x, level := n } l').val
    by_cases h : l' = l
    · subst h; rw [upd_same, upd_same]; exact hv
    · rw [upd_other h, upd_other h]; exact h1 l'
  · intro l'
    show upd a.level l lv l' = (upd s.eqtb l { val := x, level := n } l').level
    by_cases h : l' = l
    · subst h; rw [upd_same, upd_same]; exact hlv
    · rw [upd_other h, upd_other h]; exact h2 l'

theorem eqDefine_keep (s : State) (l : Loc) (x : Val)
    (h : ¬((s.eqtb l).level ≠ s.curLevel ∧ s.curLevel > 1)) :
    Concrete.eqDefine s l x =
      { s with eqtb := upd s.eqtb l { val := x, level := s.curLevel }, save := s.save } := by
  simp only [Concrete.eqDefine]
  rw [if_neg h]

theorem eqDefine_push (s : State) (l : Loc) (x : Val)
    (h : (s.eqtb l).level ≠ s.curLevel ∧ s.curLevel > 1) :
    Concrete.eqDefine s l x =
      { s with eqtb := upd s.eqtb l { val := x, level := s.curLevel },
               save := .restore l (s.eqtb l) :: s.save } := by
  simp only [Concrete.eqDefine]
  rw [if_pos h]

theorem assign_keep {a : AState} {l : Loc} {v : AVal} (h : a.level l = a.curLevel) :
    a.assign false l v =
      { a with eqtb := upd a.eqtb l v, level := upd a.level l a.curLevel, save := a.save } := by
  unfold AState.assign
  rw [if_neg (show ¬(false = true) by decide), if_pos h] <;> rfl

theorem assign_push {a : AState} {l : Loc} {v : AVal} (h1 : ¬ a.level l = a.curLevel)
    (h2 : a.curLevel > 1) :
    a.assign false l v =
      { a with eqtb := upd a.eqtb l v, level := upd a.level l a.curLevel,
               save := .restore l (a.eqtb l) (a.level l) :: a.save } := by
  unfold AState.assign
  rw [if_neg (show ¬(false = true) by decide), if_neg h1, if_pos h2] <;> rfl

theorem assign_low {a : AState} {l : Loc} {v : AVal} (h1 : ¬ a.level l = a.curLevel)
    (h2 : ¬ a.curLevel > 1) :
    a.assign false l v =
      { a with eqtb := upd a.eqtb l v, level := upd a.level l a.curLevel, save := a.save } := by
  unfold AState.assign
  rw [if_neg (show ¬(false = true) by decide), if_neg h1, if_neg h2] <;> rfl

/-- § 279 `geq_define`: a global assignment is sound. -/
theorem assign_global_sound {a : AState} {s : State} {l : Loc} {v : AVal} {x : Val}
    (ha : a.γ s) (hv : v.γ x) :
    (a.assign true l v).γ (Concrete.geqDefine s l x) := by
  have ha' := ha
  obtain ⟨_, _, h3, h4, _, _, _⟩ := ha'
  have e : a.assign true l v =
      { a with eqtb := upd a.eqtb l v, level := upd a.level l 1, save := a.save } := rfl
  rw [e]
  exact γ_write (lv := 1) (n := 1) ha hv h4 h3 rfl

/-- § 277 `eq_define`: a local assignment is sound **when the group level
is pinned** (no `many` frame open). Without the pin it fails: P1 in
PROOF-STATUS.md. -/
theorem assign_local_sound {a : AState} {s : State} {l : Loc} {v : AVal} {x : Val}
    (ha : a.γ s) (hv : v.γ x) (hpin : a.curLevel = s.curLevel) :
    (a.assign false l v).γ (Concrete.eqDefine s l x) := by
  have ha' := ha
  obtain ⟨h1, h2, _, h4, _, _, _⟩ := ha'
  have hlev : a.level l = (s.eqtb l).level := h2 l
  by_cases c1 : a.level l = a.curLevel
  · have hc : ¬((s.eqtb l).level ≠ s.curLevel ∧ s.curLevel > 1) := by omega
    rw [assign_keep c1, eqDefine_keep s l x hc]
    exact γ_write ha hv h4 (fun _ => hpin) hpin
  · by_cases c2 : a.curLevel > 1
    · have hc : (s.eqtb l).level ≠ s.curLevel ∧ s.curLevel > 1 := by omega
      rw [assign_push c1 c2, eqDefine_push s l x hc]
      exact γ_write ha hv (SaveRel.restore (h1 l) hlev.symm h4) (fun _ => hpin) hpin
    · have hc : ¬((s.eqtb l).level ≠ s.curLevel ∧ s.curLevel > 1) := by omega
      rw [assign_low c1 c2, eqDefine_keep s l x hc]
      exact γ_write ha hv h4 (fun _ => hpin) hpin

/-- `define` with `\globaldefs` (§ 1214): the abstract assignment is told
the same globality the concrete one computes. -/
theorem define_sound {a : AState} {s : State} {l : Loc} {v : AVal} {x : Val}
    (ha : a.γ s) (hv : v.γ x) (hpin : a.curLevel = s.curLevel) (glob : Bool) :
    (a.assign (Concrete.isGlobal s glob) l v).γ (Concrete.define s glob l x) := by
  unfold Concrete.define
  cases hg : Concrete.isGlobal s glob
  · rw [if_neg (show ¬(false = true) by decide)]; exact assign_local_sound ha hv hpin
  · rw [if_pos (show true = true from rfl)]; exact assign_global_sound ha hv

/-- With no `many` group frame the level is pinned. -/
theorem pinned_of_plain {a : AState} {s : State} (ha : a.γ s)
    (hplain : ∀ f, f ∈ a.save → ∀ c, f ≠ .group c true) : a.curLevel = s.curLevel := by
  obtain ⟨_, _, h3, _, _, _, _⟩ := ha
  exact h3 hplain

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

/-- `AState.join_sound` (CHANGES.md item 10): arms meeting at `\fi` in the
same stacks join soundly. -/
theorem astate_join_sound (L : Limits) (hull : HullSound) (a b : AState) (s : State)
    (hj : a.joinable b) (h : a.γ s ∨ b.γ s) : (a.join L b).γ s := by
  obtain ⟨hc, hs, hco, hi, hl⟩ := hj
  rcases h with h | h
  · obtain ⟨h1, h2, h3, h4, h5, h6, h7⟩ := h
    unfold AState.γ
    refine ⟨?_, h2, h3, h4, h5, h6, ?_⟩
    · intro l; exact aval_join_sound L hull _ _ _ (Or.inl (h1 l))
    · show s.mode.kind ∈ a.modes ++ b.modes.filter (fun m => !(a.modes.contains m))
      exact List.mem_append.mpr (Or.inl h7)
  · obtain ⟨h1, h2, h3, h4, h5, h6, h7⟩ := h
    unfold AState.γ
    refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_⟩
    · intro l; exact aval_join_sound L hull _ _ _ (Or.inr (h1 l))
    · intro l; exact (hl l).trans (h2 l)
    · intro hf
      show a.curLevel = s.curLevel
      rw [hc]; apply h3; rw [← hs]; exact hf
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

/-- Leading delimiters as known tokens (§ 397): what is left is covered. -/
theorem dropToks_sound : ∀ (ds : TokList) (xs r : AText) (p : TokList),
    dropToks ds xs = some r → AText.γ xs p → ∃ q, p = ds ++ q ∧ AText.γ r q
  | [], xs, r, p, h, hp => by
    have e : xs = r := Option.some.inj h
    subst e
    exact ⟨p, rfl, hp⟩
  | _ :: _, [], _, _, h, _ => by cases h
  | d :: ds, AItem.tok t :: xs, r, p, h, hp => by
    obtain ⟨p₁, p₂, hpe, h1, h2⟩ := atext_γ_cons.mp hp
    have hp1 : p₁ = [t] := h1
    by_cases ht : t = d
    · have e : dropToks (d :: ds) (AItem.tok t :: xs) = dropToks ds xs := by
        simp [dropToks, ht]
      rw [e] at h
      obtain ⟨q, hq, hr⟩ := dropToks_sound ds xs r p₂ h h2
      refine ⟨q, ?_, hr⟩
      rw [hpe, hp1, hq, ht]
      all_goals rfl
    · have e : dropToks (d :: ds) (AItem.tok t :: xs) = none := by
        simp [dropToks, ht]
      rw [e] at h
      cases h
  | _ :: _, AItem.hole _ :: _, _, _, h, _ => by cases h
  | _ :: _, AItem.digits _ :: _, _, _, h, _ => by cases h
  | _ :: _, AItem.oneOf _ :: _, _, _, h, _ => by cases h

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

/-! ## 8. Global theorems -/

/-- The abstract run covers every concrete run (by induction on the
concrete trace), given local soundness of the abstract transfer `T` for
job 1's `Step`. -/
theorem run_covered (E : Concrete.Engine) (T : AState → AState → Prop)
    (hloc : ∀ a s s', a.γ s → Concrete.Step E s s' → ∃ a', Star T a a' ∧ a'.γ s') :
    ∀ s s', Concrete.Steps E s s' → ∀ a, a.γ s → ∃ a', Star T a a' ∧ a'.γ s' := by
  intro s s' h
  induction h with
  | refl => intro a ha; exact ⟨a, Star.refl, ha⟩
  | step h1 _ ih =>
    intro a ha
    obtain ⟨a1, ht, h1'⟩ := hloc a _ _ ha h1
    obtain ⟨a', ht', h'⟩ := ih a1 h1'
    exact ⟨a', Star.trans ht ht', h'⟩

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

`Machine::check_budget` charges every step. Modelled: a counter that each
step decreases; at zero the run is cut to a state covering everything
(`none`). Sound only if the cut really is a top (P6 in PROOF-STATUS.md). -/

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

theorem budget_covers {A C : Type} (γ : A → C → Prop) (Step : C → C → Prop)
    (T : A → A → Prop) (hloc : ∀ a c c', γ a c → Step c c' → ∃ a', T a a' ∧ γ a' c') :
    ∀ x c c', bγ γ x c → Step c c' → ∃ y, Star (BStep T) x y ∧ bγ γ y c' := by
  intro x c c' hx hs
  obtain ⟨n, oa⟩ := x
  cases oa with
  | none => exact ⟨(n, none), Star.refl, True.intro⟩
  | some a =>
    cases n with
    | zero => exact ⟨(0, none), Star.single BStep.cut, True.intro⟩
    | succ n =>
      obtain ⟨a', ht, hγ⟩ := hloc a c c' hx hs
      exact ⟨(n, some a'), Star.single (BStep.step ht), hγ⟩

/-- The main theorem: with a per-step budget the abstract run covers every
concrete run, and it terminates (no infinite abstract run, and no run
longer than the budget). -/
theorem sound_and_terminating {A C : Type} (γ : A → C → Prop) (Step : C → C → Prop)
    (T : A → A → Prop) (hloc : ∀ a c c', γ a c → Step c c' → ∃ a', T a a' ∧ γ a' c') :
    (∀ c₀ c, Star Step c₀ c → ∀ x₀, bγ γ x₀ c₀ → ∃ x, Star (BStep T) x₀ x ∧ bγ γ x c) ∧
    WellFounded (fun y x => BStep T x y) ∧
    (∀ n x y, StarN (BStep T) x n y → n + bμ y ≤ bμ x) :=
  ⟨simulation_star Step (BStep T) (bγ γ) (budget_covers γ Step T hloc),
   wf_of_budget (BStep T) bμ (bstep_dec T),
   run_length_le_budget (BStep T) bμ (bstep_dec T)⟩

end Satex.Soundness
