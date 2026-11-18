/-
Satex.Mouth: the abstract mouth. It reads the next token from a token
list or from a file line, by the catcodes the abstract state knows.

Intuition. A file level is concrete data in SaTeX: the name, the lines,
the rest of the current line and the scanner state are all known. What is
abstract is the catcode table. So the mouth runs TeX's tokenizer
(§§ 343–356) as a function, and asks the abstract eqtb for each catcode it
needs. Where a catcode is not one known number it gives up (`none`).

Choices. The tokenizer is a function, not a relation like `LineTok`. The
proof shows that it agrees with every `LineTok` derivation (so `LineTok`
is deterministic where the function answers). `^^` reductions are not
followed: they rewrite the buffer, and documents rarely use them. A
control word stops at the first non-letter, as in § 356.

Written with AI assistance from Claude Code (Claude Opus 5.5), directed and
reviewed by the author.
-/
import Satex.Soundness

namespace Satex.Soundness

open Satex Satex.Abstract Satex.Concrete

/-! ## 1. Values the abstract eqtb knows exactly -/

/-- An integer the abstract eqtb pins to one value: one interval of width 0. -/
def aint (a : AState) (l : Loc) : Option Int :=
  match a.eqtb l with
  | .ints [i] => if i.lo = i.hi then some i.lo else none
  | _ => none

/-- Every entry of `a` covers `s`'s. -/
abbrev EqtbRel (a : AState) (s : State) : Prop := ∀ l, (a.eqtb l).γ (s.eqtb l).val

theorem aint_sound {a : AState} {s : State} {l : Loc} {n : Int} (hE : EqtbRel a s)
    (h : aint a l = some n) : s.intAt l = n := by
  have hv := hE l
  unfold aint at h
  split at h
  · rename_i i e
    rw [e] at hv
    obtain ⟨i', m, hi, hval, hlo, hhi⟩ := hv
    simp only [List.mem_singleton] at hi
    subst hi
    split at h
    · simp only [Option.some.injEq] at h
      subst h
      simp only [State.intAt, State.get, hval]
      omega
    · cases h
  · cases h

/-- A token list of literal tokens only. -/
def alit : AText → Option TokList
  | [] => some []
  | .tok t :: xs => (alit xs).map (t :: ·)
  | _ => none

theorem alit_γ : ∀ {x : AText} {ts p : TokList}, alit x = some ts → AText.γ x p → p = ts
  | [], ts, p, h, hp => by simp only [alit, Option.some.injEq] at h; subst h; exact hp
  | .tok t :: xs, ts, p, h, hp => by
    simp only [alit, Option.map_eq_some_iff] at h
    obtain ⟨ts', h1, rfl⟩ := h
    obtain ⟨p1, p2, rfl, e1, e2⟩ := hp
    simp only [AItem.γ] at e1
    subst e1
    rw [alit_γ h1 e2]
    rfl
  | .hole _ :: _, _, _, h, _ => by simp [alit] at h
  | .digits _ :: _, _, _, h, _ => by simp [alit] at h
  | .oneOf _ :: _, _, _, h, _ => by simp [alit] at h

/-- A token register the abstract eqtb pins to one literal list. -/
def atoks (a : AState) (l : Loc) : Option TokList :=
  match a.eqtb l with
  | .toks [x] => alit x
  | _ => none

theorem atoks_sound {a : AState} {s : State} {l : Loc} {ts : TokList} (hE : EqtbRel a s)
    (h : atoks a l = some ts) : s.toksAt l = ts := by
  have hv := hE l
  unfold atoks at h
  split at h
  · rename_i x e
    rw [e] at hv
    obtain ⟨x', p, hx, hval, hγ, -⟩ := hv
    simp only [List.mem_singleton] at hx
    subst hx
    simp only [State.toksAt, State.get, hval]
    exact alit_γ h hγ
  · cases h

/-- A catcode the abstract eqtb knows (§ 230 `cat_code`). -/
def acat (a : AState) (c : CharCode) : Option Cat :=
  (aint a (.code .cat c)).map fun n => Cat.ofCode n.toNat

theorem acat_sound {a : AState} {s : State} (hE : EqtbRel a s) :
    ∀ c k, acat a c = some k → s.catOf c = k := by
  intro c k h
  simp only [acat, Option.map_eq_some_iff] at h
  obtain ⟨n, hn, rfl⟩ := h
  simp only [State.catOf, aint_sound hE hn]

/-! ## 2. The line tokenizer (§§ 343–356)

`alineTok cat st cs` reads one token from the rest `cs` of a line in
scanner state `st`. It answers the token (`none` when the line is used
up), the new state and the rest. The outer `none`: a catcode is unknown,
or a `^^` reduction would apply. -/

/-- The letters of a control word after its first letter (§ 356). -/
def ascan (cat : CharCode → Option Cat) : List CharCode → Option (List CharCode × List CharCode)
  | [] => some ([], [])
  | c :: rest =>
    match cat c with
    | some .letter => (ascan cat rest).map fun p => (c :: p.1, p.2)
    | some .sup => if reduceHat c rest = none then some ([], c :: rest) else none
    | some _ => some ([], c :: rest)
    | none => none

theorem ascan_sound {acat : CharCode → Option Cat} {ccat : CharCode → Cat}
    (hc : ∀ c k, acat c = some k → ccat c = k) :
    ∀ (cs : List CharCode) (n : Nat) (p : List CharCode × List CharCode),
      cs.length ≤ n → ascan acat cs = some p → scanLetters ccat n cs = p
  | [], n, p, _, h => by
    simp only [ascan, Option.some.injEq] at h; subst h
    cases n <;> rfl
  | c :: rest, n, p, hn, h => by
    obtain ⟨m, rfl⟩ : ∃ m, n = m + 1 := ⟨n - 1, by simp at hn; omega⟩
    simp only [ascan] at h
    cases hk : acat c with
    | none => simp [hk] at h
    | some k =>
      have e := hc c k hk
      cases k <;> simp only [hk, Option.map_eq_some_iff, Option.some.injEq] at h
      case letter =>
        obtain ⟨q, hq, rfl⟩ := h
        have := ascan_sound hc rest m q (by simp at hn; omega) hq
        simp only [scanLetters, e, ↓reduceIte, this]
      case sup =>
        split at h
        · rename_i hr
          simp only [Option.some.injEq] at h; subst h
          simp [scanLetters, e, hr]
        · cases h
      all_goals (subst h; simp [scanLetters, e])

/-- One token from the rest of a line (§§ 343–356).
source: SaTeX `Mouth::next_with` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/tex.rs -/
def alineTok (cat : CharCode → Option Cat) :
    MouthState → List CharCode → Option (Option Token × MouthState × List CharCode)
  | st, [] => some (none, st, [])
  | st, c :: rest =>
    match cat c with
    | none => none
    | some .escape =>
      match rest with
      | [] => some (some (.cs (.name "")), .skipBlanks, [])
      | c1 :: rest' =>
        match cat c1 with
        | some .letter =>
          (ascan cat rest').map fun p => (some (.cs (.name (nameOf (c1 :: p.1)))), .skipBlanks, p.2)
        | some .sup =>
          if reduceHat c1 rest' = none then some (some (.cs (.name (nameOf [c1]))), .midLine, rest')
          else none
        | some .space => some (some (.cs (.name (nameOf [c1]))), .skipBlanks, rest')
        | some _ => some (some (.cs (.name (nameOf [c1]))), .midLine, rest')
        | none => none
    | some .sup => if reduceHat c rest = none then some (some (.char c .sup), .midLine, rest) else none
    | some .active => some (some (.cs (.active c)), .midLine, rest)
    | some .space =>
      if st = .midLine then some (some Token.space, .skipBlanks, rest) else alineTok cat st rest
    | some .eol =>
      match st with
      | .newLine => some (some parToken, .newLine, [])
      | .midLine => some (some Token.space, .newLine, [])
      | .skipBlanks => some (none, .newLine, [])
    | some .ignored => alineTok cat st rest
    | some .invalid => alineTok cat st rest
    | some .comment => some (none, .newLine, [])
    | some k => some (some (.char c k), .midLine, rest)

/-- The tokenizer agrees with every `LineTok` derivation. -/
theorem alineTok_sound {acat : CharCode → Option Cat} {ccat : CharCode → Cat}
    (hc : ∀ c k, acat c = some k → ccat c = k) :
    ∀ {st cs r st' cs'}, LineTok ccat st cs r st' cs' →
      ∀ {x}, alineTok acat st cs = some x → x = (r, st', cs') := by
  intro st cs r st' cs' h
  induction h with
  | done => intro x hx; simp only [alineTok, Option.some.injEq] at hx; exact hx.symm
  | @hat st st' c c' rest rest' rest'' r hk hr _ _ =>
    intro x hx
    cases ha : acat c with
    | none => simp [alineTok, ha] at hx
    | some k =>
      rw [hc c k ha] at hk; subst hk
      simp [alineTok, ha, hr] at hx
  | @escWord st c c1 rest hk hk1 =>
    intro x hx
    cases ha : acat c with
    | none => simp [alineTok, ha] at hx
    | some k =>
      rw [hc c k ha] at hk; subst hk
      cases ha1 : acat c1 with
      | none => simp [alineTok, ha, ha1] at hx
      | some k1 =>
        rw [hc c1 k1 ha1] at hk1; subst hk1
        simp only [alineTok, ha, ha1, Option.map_eq_some_iff] at hx
        obtain ⟨q, hq, rfl⟩ := hx
        have := ascan_sound hc rest rest.length q (Nat.le_refl _) hq
        have hc1 := hc c1 _ ha1
        simp only [scanLetters, hc1, ↓reduceIte, this]
  | @escSymbol st c c1 rest hk hk1 hsup =>
    intro x hx
    cases ha : acat c with
    | none => simp [alineTok, ha] at hx
    | some k =>
      rw [hc c k ha] at hk; subst hk
      cases ha1 : acat c1 with
      | none => simp [alineTok, ha, ha1] at hx
      | some k1 =>
        have e1 := hc c1 k1 ha1
        rw [e1] at hk1 hsup ⊢
        cases k1 <;> simp_all [alineTok]
  | @escHat st c c1 c' rest rest' r st' rest'' hk hk1 hr _ _ =>
    intro x hx
    cases ha : acat c with
    | none => simp [alineTok, ha] at hx
    | some k =>
      rw [hc c k ha] at hk; subst hk
      cases ha1 : acat c1 with
      | none => simp [alineTok, ha, ha1] at hx
      | some k1 =>
        rw [hc c1 k1 ha1] at hk1; subst hk1
        simp [alineTok, ha, ha1, hr] at hx
  | @escEmpty st c hk =>
    intro x hx
    cases ha : acat c with
    | none => simp [alineTok, ha] at hx
    | some k =>
      rw [hc c k ha] at hk; subst hk
      simp only [alineTok, ha, Option.some.injEq] at hx; exact hx.symm
  | @active st c rest hk =>
    intro x hx
    cases ha : acat c with
    | none => simp [alineTok, ha] at hx
    | some k =>
      rw [hc c k ha] at hk; subst hk
      simp only [alineTok, ha, Option.some.injEq] at hx; exact hx.symm
  | @plain st c rest ht hns hsup =>
    intro x hx
    cases ha : acat c with
    | none => simp [alineTok, ha] at hx
    | some k =>
      have e := hc c k ha
      rw [e] at ht hns hsup ⊢
      cases k <;> simp_all [alineTok, Cat.tokenable]
  | @spaceMid c rest hk =>
    intro x hx
    cases ha : acat c with
    | none => simp [alineTok, ha] at hx
    | some k =>
      rw [hc c k ha] at hk; subst hk
      simp only [alineTok, ha, ↓reduceIte, Option.some.injEq] at hx; exact hx.symm
  | @spaceSkip st st' c rest rest' r hk hst _ ih =>
    intro x hx
    cases ha : acat c with
    | none => simp [alineTok, ha] at hx
    | some k =>
      rw [hc c k ha] at hk; subst hk
      simp only [alineTok, ha, hst, ↓reduceIte] at hx
      exact ih hx
  | @eolPar c rest hk =>
    intro x hx
    cases ha : acat c with
    | none => simp [alineTok, ha] at hx
    | some k =>
      rw [hc c k ha] at hk; subst hk
      simp only [alineTok, ha, Option.some.injEq] at hx; exact hx.symm
  | @eolSpace c rest hk =>
    intro x hx
    cases ha : acat c with
    | none => simp [alineTok, ha] at hx
    | some k =>
      rw [hc c k ha] at hk; subst hk
      simp only [alineTok, ha, Option.some.injEq] at hx; exact hx.symm
  | @eolSkip c rest hk =>
    intro x hx
    cases ha : acat c with
    | none => simp [alineTok, ha] at hx
    | some k =>
      rw [hc c k ha] at hk; subst hk
      simp only [alineTok, ha, Option.some.injEq] at hx; exact hx.symm
  | @ignored st st' c rest rest' r hk _ ih =>
    intro x hx
    cases ha : acat c with
    | none => simp [alineTok, ha] at hx
    | some k =>
      rw [hc c k ha] at hk; subst hk
      simp only [alineTok, ha] at hx
      exact ih hx
  | @comment st c rest hk =>
    intro x hx
    cases ha : acat c with
    | none => simp [alineTok, ha] at hx
    | some k =>
      rw [hc c k ha] at hk; subst hk
      simp only [alineTok, ha, Option.some.injEq] at hx; exact hx.symm
  | @invalid st st' c rest rest' r hk _ ih =>
    intro x hx
    cases ha : acat c with
    | none => simp [alineTok, ha] at hx
    | some k =>
      rw [hc c k ha] at hk; subst hk
      simp only [alineTok, ha] at hx
      exact ih hx

/-! ## 3. The mouth over the input stack (§§ 357–362)

`amouth a fe ai` is `get_next` over the abstract input `ai` with
`force_eof = fe`. It pops used-up token lists (§ 324), reads file lines,
moves to the next line with the `\endlinechar` in force (§ 362), ends a
file and inserts `\everyeof` (etex.ch). It answers the token, the input
after it and the new `force_eof`.

Why `fe` is an argument: `\endinput` sets it and the end of a line reads
it. `γ` would otherwise leave it free, and every line end would split
into "next line" and "file ended". -/

/-- Levels plus the lines still to read: what `amouth` uses up. -/
def weight : List AInput → Nat
  | [] => 0
  | .file f :: rest => f.lines.length + 1 + weight rest
  | .toks _ _ :: rest => 1 + weight rest

/-- `prepareLine` with the `\endlinechar` given. -/
def prepareWith (e : Int) (line : List CharCode) : List CharCode :=
  let trimmed := (line.reverse.dropWhile (· == 32)).reverse
  if 0 ≤ e ∧ e < 256 then trimmed ++ [e.toNat] else trimmed

theorem prepareLine_eq (s : State) (line : List CharCode) :
    prepareLine s line = prepareWith (s.intPar "endlinechar") line := rfl

-- source: tex.web § 357 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L7480, § 362 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L7563
-- source: SaTeX `next_token`, `end_input_frame` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/analysis/machine.rs
-- source: SaTeX `Mouth::next_with` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/tex.rs
def amouth (a : AState) : Bool → List AInput → Option (Token × List AInput × Bool)
  | fe, .toks _ [] :: rest => amouth a fe rest
  | fe, .toks k (.tok t :: xs) :: rest => some (t, .toks k xs :: rest, fe)
  | fe, .file f :: rest =>
    match alineTok (acat a) f.state f.cur with
    | some (some t, st, cur) => some (t, .file { f with state := st, cur := cur } :: rest, fe)
    | some (none, _, _) =>
      if fe then amouth a false rest
      else match _h : f.lines with
        | line :: lines =>
          match aint a (.intPar "endlinechar") with
          | some e =>
            amouth a fe (.file { f with cur := prepareWith e line, lines := lines,
                                        state := .newLine, line := f.line + 1 } :: rest)
          | none => none
        | [] =>
          if f.eofSeen || f.text then amouth a false rest
          else match atoks a (.toksPar "everyeof") with
            | some [] => amouth a false rest
            | some (u :: us) =>
              some (u, .toks (.every "everyeof") (AText.ofList us) ::
                .file { f with eofSeen := true, cur := [], state := .newLine, line := f.line + 1 } :: rest,
                false)
            | none => none
    | none => none
  | _, _ => none
termination_by _ ai => weight ai
decreasing_by all_goals simp_all [weight] <;> omega

theorem amouth_sound : ∀ {s : State} {t' : Token} {s1 : State}, Concrete.Next s t' s1 →
    ∀ (a : AState) (fe : Bool) (ai : List AInput) (t : Token) (ai' : List AInput) (fe' : Bool),
    EqtbRel a s → InputRel ai s.input → s.forceEof = fe →
    amouth a fe ai = some (t, ai', fe') →
    t' = t ∧ s1 = { s with input := s1.input, forceEof := s1.forceEof } ∧
      InputRel ai' s1.input ∧ s1.forceEof = fe' := by
  intro s t' s1 h
  induction h with
  | @tok s k t0 ts rest hin =>
    intro a fe ai t ai' fe' _ hr hfe ha
    rw [hin] at hr
    match ai, hr, ha with
    | .toks k' [] :: ar, hr, _ =>
      obtain ⟨⟨_, hx⟩, _⟩ := hr
      cases hx
    | .toks k' (.tok u :: xs) :: ar, hr, ha =>
      obtain ⟨⟨hk, p1, p2, he, h1, h2⟩, hrest⟩ := hr
      simp only [amouth, Option.some.injEq, Prod.mk.injEq] at ha
      obtain ⟨rfl, rfl, rfl⟩ := ha
      simp only [AItem.γ] at h1
      subst h1 hk
      simp only [List.singleton_append, List.cons.injEq] at he
      obtain ⟨rfl, rfl⟩ := he
      exact ⟨rfl, rfl, ⟨⟨rfl, h2⟩, hrest⟩, hfe⟩
    | .toks _ (.hole _ :: _) :: _, _, ha => simp [amouth] at ha
    | .toks _ (.digits _ :: _) :: _, _, ha => simp [amouth] at ha
    | .toks _ (.oneOf _ :: _) :: _, _, ha => simp [amouth] at ha
    | .file _ :: _, hr, _ => exact hr.1.elim
    | [], hr, _ => exact hr.elim
  | @tokEnd s s' k rest t0 hin _ ih =>
    intro a fe ai t ai' fe' hE hr hfe ha
    rw [hin] at hr
    match ai, hr, ha with
    | .toks k' [] :: ar, hr, ha =>
      simp only [amouth] at ha
      obtain ⟨e1, e2, e3, e4⟩ := ih a fe ar t ai' fe' hE hr.2 hfe ha
      exact ⟨e1, by rw [e2], e3, e4⟩
    | .toks k' (.tok u :: xs) :: ar, hr, _ =>
      obtain ⟨⟨_, p1, p2, he, h1, _⟩, _⟩ := hr
      simp only [AItem.γ] at h1
      subst h1
      cases he
    | .toks _ (.hole _ :: _) :: _, _, ha => simp [amouth] at ha
    | .toks _ (.digits _ :: _) :: _, _, ha => simp [amouth] at ha
    | .toks _ (.oneOf _ :: _) :: _, _, ha => simp [amouth] at ha
    | .file _ :: _, hr, _ => exact hr.1.elim
    | [], hr, _ => exact hr.elim
  | @fileTok s f rest t0 st cur hin hl =>
    intro a fe ai t ai' fe' hE hr hfe ha
    rw [hin] at hr
    match ai, hr, ha with
    | .toks _ _ :: _, hr, _ => exact hr.1.elim
    | .file f' :: ar, hr, ha =>
      obtain ⟨hf, hrest⟩ := hr
      simp only [AInput.γ] at hf
      subst hf
      rw [amouth.eq_3] at ha
      split at ha
      · rename_i t1 st1 cur1 hx
        have := alineTok_sound (acat_sound hE) hl hx
        simp only [Prod.mk.injEq, Option.some.injEq] at this
        obtain ⟨rfl, rfl, rfl⟩ := this
        simp only [Option.some.injEq, Prod.mk.injEq] at ha
        obtain ⟨rfl, rfl, rfl⟩ := ha
        exact ⟨rfl, rfl, ⟨rfl, hrest⟩, hfe⟩
      · rename_i st1 cur1 hx
        have := alineTok_sound (acat_sound hE) hl hx
        simp at this
      · cases ha
    | [], hr, _ => exact hr.elim
  | @nextLine s s' f rest t0 st line lines hin hl hls hfe0 _ ih =>
    intro a fe ai t ai' fe' hE hr hfe ha
    rw [hin] at hr
    match ai, hr, ha with
    | .toks _ _ :: _, hr, _ => exact hr.1.elim
    | .file f' :: ar, hr, ha =>
      obtain ⟨hf, hrest⟩ := hr
      simp only [AInput.γ] at hf
      subst hf
      rw [hfe0] at hfe; subst hfe
      rw [amouth.eq_3] at ha
      split at ha
      · rename_i t1 st1 cur1 hx
        have := alineTok_sound (acat_sound hE) hl hx
        simp at this
      · simp only [Bool.false_eq_true, ↓reduceIte] at ha
        split at ha
        · rename_i line' lines' hls'
          rw [hls] at hls'
          simp only [List.cons.injEq] at hls'
          obtain ⟨rfl, rfl⟩ := hls'
          split at ha
          · rename_i e he
            have hel : s.intPar "endlinechar" = e := aint_sound hE he
            obtain ⟨e1, e2, e3, e4⟩ := ih a false _ t ai' fe' hE
              (by simp only [InputRel]; exact ⟨by simp only [AInput.γ, prepareLine_eq, hel], hrest⟩) hfe0 ha
            exact ⟨e1, by rw [e2], e3, e4⟩
          · cases ha
        · rename_i hls'
          rw [hls] at hls'
          cases hls'
      · cases ha
    | [], hr, _ => exact hr.elim
  | @endFile s s' f rest t0 st hin hl hend _ ih =>
    intro a fe ai t ai' fe' hE hr hfe ha
    rw [hin] at hr
    match ai, hr, ha with
    | .toks _ _ :: _, hr, _ => exact hr.1.elim
    | .file f' :: ar, hr, ha =>
      obtain ⟨hf, hrest⟩ := hr
      simp only [AInput.γ] at hf
      subst hf
      rw [amouth.eq_3] at ha
      have fin : amouth a false ar = some (t, ai', fe') →
          t0 = t ∧ s' = { s with input := s'.input, forceEof := s'.forceEof } ∧
            InputRel ai' s'.input ∧ s'.forceEof = fe' := by
        intro hx
        obtain ⟨e1, e2, e3, e4⟩ := ih a false ar t ai' fe' hE hrest rfl hx
        exact ⟨e1, by rw [e2], e3, e4⟩
      split at ha
      · rename_i t1 st1 cur1 hx
        have := alineTok_sound (acat_sound hE) hl hx
        simp at this
      · split at ha
        · exact fin ha
        · rename_i hfe1
          have hfs : s.forceEof = false := by simpa using hfe.trans (by simpa using hfe1)
          rw [hfs] at hend
          simp only [Bool.false_eq_true, or_false] at hend
          obtain ⟨hls, hcase⟩ := hend
          split at ha
          · rename_i line' lines' hls'
            rw [hls] at hls'; cases hls'
          · split at ha
            · exact fin ha
            · rename_i hnot
              split at ha
              · exact fin ha
              · rename_i u us hev
                have := atoks_sound hE hev
                simp only [Bool.or_eq_true, not_or, Bool.not_eq_true] at hnot
                rcases hcase with h1 | h1 | h1
                · simp_all
                · simp_all
                · rw [this] at h1; cases h1
              · cases ha
      · cases ha
    | [], hr, _ => exact hr.elim
  | @everyEof s s' f rest t0 st hin hl hls hfe0 hseen htext hev _ ih =>
    intro a fe ai t ai' fe' hE hr hfe ha
    rw [hin] at hr
    match ai, hr, ha with
    | .toks _ _ :: _, hr, _ => exact hr.1.elim
    | .file f' :: ar, hr, ha =>
      obtain ⟨hf, hrest⟩ := hr
      simp only [AInput.γ] at hf
      subst hf
      rw [hfe0] at hfe; subst hfe
      rw [amouth.eq_3] at ha
      split at ha
      · rename_i t1 st1 cur1 hx
        have := alineTok_sound (acat_sound hE) hl hx
        simp at this
      · simp only [Bool.false_eq_true, ↓reduceIte] at ha
        split at ha
        · rename_i line' lines' hls'
          rw [hls] at hls'; cases hls'
        · rw [show (f'.eofSeen || f'.text) = false by simp [hseen, htext]] at ha
          simp only [Bool.false_eq_true, ↓reduceIte] at ha
          split at ha
          · rename_i hev'
            have := atoks_sound hE hev'
            exact absurd this hev
          · rename_i u us hev'
            have hts := atoks_sound hE hev'
            simp only [Option.some.injEq, Prod.mk.injEq] at ha
            obtain ⟨h1, h2, h3⟩ := ha
            subst h1 h2 h3
            simp only [insertEvery, State.toksAt, State.get] at ih hts
            simp only [hts, reduceCtorEq, ↓reduceIte, pushToks] at ih
            obtain ⟨e1, e2, e3, e4⟩ := ih a false
              (.toks (.every "everyeof") (AText.ofList (u :: us)) ::
                .file { f' with eofSeen := true, cur := [], state := .newLine, line := f'.line + 1 } :: ar)
              u _ false hE ⟨⟨rfl, ofList_γ _⟩, rfl, hrest⟩ hfe0
              (by simp [amouth, AText.ofList]; rfl)
            exact ⟨e1, by rw [e2], e3, e4⟩
          · cases ha
      · cases ha
    | [], hr, _ => exact hr.elim

/-- With a token next, the input is not drained (§ 360). -/
theorem amouth_not_drained : ∀ {s : State}, Concrete.Drained s →
    ∀ (a : AState) (fe : Bool) (ai : List AInput) (x : Token × List AInput × Bool),
    EqtbRel a s → InputRel ai s.input → s.forceEof = fe → amouth a fe ai ≠ some x := by
  intro s h
  induction h with
  | @empty s hin =>
    intro a fe ai x _ hr _ ha
    rw [hin] at hr
    match ai, hr with
    | [], _ => simp [amouth] at ha
  | @tokEnd s k rest hin _ ih =>
    intro a fe ai x hE hr hfe ha
    rw [hin] at hr
    match ai, hr, ha with
    | .toks _ [] :: ar, hr, ha => simp only [amouth] at ha; exact ih a fe ar x hE hr.2 hfe ha
    | .toks _ (.tok u :: xs) :: ar, hr, _ =>
      obtain ⟨⟨_, p1, p2, he, h1, _⟩, _⟩ := hr
      simp only [AItem.γ] at h1
      subst h1
      cases he
    | .toks _ (.hole _ :: _) :: _, _, ha => simp [amouth] at ha
    | .toks _ (.digits _ :: _) :: _, _, ha => simp [amouth] at ha
    | .toks _ (.oneOf _ :: _) :: _, _, ha => simp [amouth] at ha
    | .file _ :: _, hr, _ => exact hr.1.elim
  | @nextLine s f rest st line lines hin hl hls hfe0 _ ih =>
    intro a fe ai x hE hr hfe ha
    rw [hin] at hr
    match ai, hr, ha with
    | .toks _ _ :: _, hr, _ => exact hr.1.elim
    | .file f' :: ar, hr, ha =>
      obtain ⟨hf, hrest⟩ := hr
      simp only [AInput.γ] at hf
      subst hf
      rw [hfe0] at hfe; subst hfe
      rw [amouth.eq_3] at ha
      split at ha
      · rename_i t1 st1 cur1 hx
        have := alineTok_sound (acat_sound hE) hl hx
        simp at this
      · simp only [Bool.false_eq_true, ↓reduceIte] at ha
        split at ha
        · rename_i line' lines' hls'
          rw [hls] at hls'
          simp only [List.cons.injEq] at hls'
          obtain ⟨rfl, rfl⟩ := hls'
          split at ha
          · rename_i e he
            have hel : s.intPar "endlinechar" = e := aint_sound hE he
            exact ih a false _ x hE (by simp only [InputRel]; exact ⟨by simp only [AInput.γ, prepareLine_eq, hel], hrest⟩) hfe0 ha
          · cases ha
        · rename_i hls'
          rw [hls] at hls'
          cases hls'
      · cases ha
  | @endFile s f rest st hin hl hend _ ih =>
    intro a fe ai x hE hr hfe ha
    rw [hin] at hr
    match ai, hr, ha with
    | .toks _ _ :: _, hr, _ => exact hr.1.elim
    | .file f' :: ar, hr, ha =>
      obtain ⟨hf, hrest⟩ := hr
      simp only [AInput.γ] at hf
      subst hf
      rw [amouth.eq_3] at ha
      split at ha
      · rename_i t1 st1 cur1 hx
        have := alineTok_sound (acat_sound hE) hl hx
        simp at this
      · split at ha
        · exact ih a false ar x hE hrest rfl ha
        · rename_i hfe1
          have hfs : s.forceEof = false := by simpa using hfe.trans (by simpa using hfe1)
          rw [hfs] at hend
          simp only [Bool.false_eq_true, or_false] at hend
          obtain ⟨hls, hcase⟩ := hend
          split at ha
          · rename_i line' lines' hls'
            rw [hls] at hls'; cases hls'
          · split at ha
            · exact ih a false ar x hE hrest rfl ha
            · rename_i hnot
              split at ha
              · exact ih a false ar x hE hrest rfl ha
              · rename_i u us hev
                have := atoks_sound hE hev
                simp only [Bool.or_eq_true, not_or, Bool.not_eq_true] at hnot
                rcases hcase with h1 | h1 | h1
                · simp_all
                · simp_all
                · rw [this] at h1; cases h1
              · cases ha
      · cases ha

end Satex.Soundness
