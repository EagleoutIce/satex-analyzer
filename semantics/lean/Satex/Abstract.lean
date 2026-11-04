/-
Satex.Abstract: what SaTeX's abstract interpreter computes.

Written with AI assistance from Claude Code (Claude Opus 5.5; build fixes with
Claude Sonnet 5.5), directed and reviewed by the author.

Each domain is given with its order, join, widening and concretization `γ`
into the objects of `Satex.Core`. Transfers follow the concrete rule
families of `Satex.Concrete`: assignment and grouping (§§ 268–284),
conditionals (§§ 487–510), macro expansion with argument scanning
(§§ 389–399), loops, and call summaries (SEM § 14).

`semantics/mapping.md` maps every definition here to the Rust code that
implements it and lists where the Rust deviates.

Plain core Lean 4, no mathlib. Sets are predicates (`α → Prop`).
-/
import Satex.Core

namespace Satex.Abstract
open Satex

/-! ## 0. Domain interface

`AbsOps C A`: the operations a domain `A` over concrete objects `C` has.
Job 3 bundles them with their laws into `Soundness.AbsDom`. -/

structure AbsOps (C A : Type) where
  le : A → A → Prop
  join : A → A → A
  top : A
  γ : A → C → Prop

/-- The order every domain here can fall back on: inclusion of meanings. -/
def semLe {C A : Type} (γ : A → C → Prop) (a b : A) : Prop := ∀ c, γ a c → γ b c

/-- The limits that bound sets (`limits.*` in src/project/config.rs). -/
structure Limits where
  meaningSet : Nat := 8       -- limits.meaning_set
  valueSet : Nat := 8         -- limits.value_set
  intervalSet : Nat := 4      -- limits.interval_set
  meaningSplits : Nat := 4    -- limits.meaning_splits
  choices : Nat := 8          -- limits.choices
  widenAfter : Nat := 3       -- limits.widen_after
  callSummaryAfter : Nat := 2 -- limits.call_summary_after
  deriving Inhabited

/-! ## 1. Intervals: `Num` (src/engine/value.rs) -/

structure Itv where
  lo : Int
  hi : Int
  deriving DecidableEq, Repr, Inhabited

namespace Itv

def wordMin : Int := -2147483648
def wordMax : Int := 2147483647

def exact (n : Int) : Itv := ⟨n, n⟩
def γ (i : Itv) (n : Int) : Prop := i.lo ≤ n ∧ n ≤ i.hi
def le (a b : Itv) : Prop := b.lo ≤ a.lo ∧ a.hi ≤ b.hi

/-- `Num::join`. -/
def join (a b : Itv) : Itv :=
  ⟨if a.lo ≤ b.lo then a.lo else b.lo, if a.hi ≤ b.hi then b.hi else a.hi⟩

/-- The whole 32-bit word: the top every widening reaches. -/
def word : Itv := ⟨wordMin, wordMax⟩

/-- `Num::full`: every value of a kind, `±maxDimen` or `±infinity`. -/
def full (dimen : Bool) : Itv :=
  if dimen then ⟨-maxDimen, maxDimen⟩ else ⟨-infinity, infinity⟩

/-- One bound of `Num::widen`: a bound that moved goes to the word's end. -/
def wlo (a b : Int) : Int := if b < a then wordMin else a
def whi (a b : Int) : Int := if a < b then wordMax else a

/-- `Num::widen`. -/
def widen (a b : Itv) : Itv := ⟨wlo a.lo b.lo, whi a.hi b.hi⟩

def inWord (i : Itv) : Prop := wordMin ≤ i.lo ∧ i.hi ≤ wordMax

/-- The relations `\ifnum`/`\ifdim` test (§ 503). -/
inductive Rel where
  | lt | eq | gt
  deriving DecidableEq, Repr, Inhabited

def Rel.holds : Rel → Int → Int → Bool
  | .lt, x, y => decide (x < y)
  | .eq, x, y => decide (x = y)
  | .gt, x, y => decide (y < x)

/-- `Num::compare`: the answer when every pair of members gives it. -/
def compare (a : Itv) (r : Rel) (b : Itv) : Option Bool :=
  match r with
  | .lt => if a.hi < b.lo then some true else if b.hi ≤ a.lo then some false else none
  | .gt => if b.hi < a.lo then some true else if a.hi ≤ b.lo then some false else none
  | .eq =>
    if a.lo = a.hi ∧ b.lo = b.hi ∧ a.lo = b.lo then some true
    else if a.hi < b.lo ∨ b.hi < a.lo then some false else none

/-- `Num::narrow`: what `x r bound = holds` leaves of `i`; `none` is empty. -/
def narrow (i : Itv) (r : Rel) (bound : Int) (holds : Bool) : Option Itv :=
  let p : Int × Int :=
    (match r, holds with
    | .lt, true => (i.lo, min i.hi (bound - 1))
    | .lt, false => (max i.lo bound, i.hi)
    | .gt, true => (max i.lo (bound + 1), i.hi)
    | .gt, false => (i.lo, min i.hi bound)
    | .eq, true => (max i.lo bound, min i.hi bound)
    | .eq, false =>
      (if i.lo = bound then i.lo + 1 else i.lo, if i.hi = bound then i.hi - 1 else i.hi))
  if p.1 ≤ p.2 then some ⟨p.1, p.2⟩ else none

theorem γ_mono {a b : Itv} (h : le a b) {n : Int} (hn : γ a n) : γ b n := by
  unfold Itv.le at h
  unfold Itv.γ at hn ⊢
  omega

theorem le_join_left (a b : Itv) : le a (join a b) := by
  simp only [Itv.le, Itv.join]
  constructor <;> split <;> omega

theorem le_join_right (a b : Itv) : le b (join a b) := by
  simp only [Itv.le, Itv.join]
  constructor <;> split <;> omega

/-- A widened bound either stays or is the word's end. -/
theorem wlo_cases (a b : Int) : wlo a b = a ∨ wlo a b = wordMin := by
  unfold Itv.wlo
  split
  · exact Or.inr rfl
  · exact Or.inl rfl

theorem whi_cases (a b : Int) : whi a b = a ∨ whi a b = wordMax := by
  unfold Itv.whi
  split
  · exact Or.inr rfl
  · exact Or.inl rfl

/-- At the word's end a bound never moves again. -/
theorem wlo_fix (b : Int) : wlo wordMin b = wordMin := by
  unfold Itv.wlo
  split <;> rfl

theorem whi_fix (b : Int) : whi wordMax b = wordMax := by
  unfold Itv.whi
  split <;> rfl

/-- Termination of widening: by `wlo_cases`/`wlo_fix` (and the same for
`hi`), along any chain `aₙ₊₁ = widen aₙ bₙ` each bound changes at most once,
so a chain has at most three distinct members.

Soundness of widening: the widened interval covers both sides when the new
values lie in the 32-bit word (`inWord`). -/
theorem le_widen (a b : Itv) (ha : inWord a) (hb : inWord b) :
    le a (widen a b) ∧ le b (widen a b) := by
  unfold Itv.inWord at ha hb
  simp only [Itv.le, Itv.widen, Itv.wlo, Itv.whi, Itv.wordMin, Itv.wordMax] at *
  refine ⟨⟨?_, ?_⟩, ⟨?_, ?_⟩⟩ <;> split <;> omega

end Itv

/-! ## 2. Glue hulls: `GlueHull` (src/engine/value.rs) -/

structure GlueHull where
  width : Itv
  stretch : Itv
  stretchOrder : Itv
  shrink : Itv
  shrinkOrder : Itv
  deriving DecidableEq, Repr, Inhabited

namespace GlueHull

def γ (h : GlueHull) (g : Glue) : Prop :=
  h.width.γ g.width ∧ h.stretch.γ g.stretch ∧ h.stretchOrder.γ (Int.ofNat g.stretchOrder) ∧
    h.shrink.γ g.shrink ∧ h.shrinkOrder.γ (Int.ofNat g.shrinkOrder)

def exact (g : Glue) : GlueHull :=
  ⟨.exact g.width, .exact g.stretch, .exact (Int.ofNat g.stretchOrder),
    .exact g.shrink, .exact (Int.ofNat g.shrinkOrder)⟩

def join (a b : GlueHull) : GlueHull :=
  ⟨a.width.join b.width, a.stretch.join b.stretch, a.stretchOrder.join b.stretchOrder,
    a.shrink.join b.shrink, a.shrinkOrder.join b.shrinkOrder⟩

end GlueHull

/-! ## 3. Text classes: `TextClass` (src/engine/tex.rs)

A class is the set of token kinds unknown text may hold. `hex` is a
restriction: its letters and others are hex digits. -/

inductive Kind where
  | digit | sign | letter | other | space | brace | cs
  deriving DecidableEq, Repr, Inhabited

structure TClass where
  d : Bool := false
  s : Bool := false
  l : Bool := false
  o : Bool := false
  sp : Bool := false
  b : Bool := false
  c : Bool := false
  hex : Bool := false
  deriving DecidableEq, Repr, Inhabited

namespace TClass

def has (k : TClass) : Kind → Bool
  | .digit => k.d | .sign => k.s | .letter => k.l | .other => k.o
  | .space => k.sp | .brace => k.b | .cs => k.c

def any : TClass := { d := true, s := true, l := true, o := true, sp := true, b := true, c := true }
def number : TClass := { d := true, s := true }
def digits : TClass := { d := true }

/-- `TextClass::hex`: true also when no letter, other or name may occur. -/
def effHex (k : TClass) : Bool := k.hex || !(k.l || k.o || k.c)

/-- `TextClass::new`: `hex` only where it restricts something. -/
def norm (k : TClass) : TClass :=
  { k with hex := k.hex && (k.l || k.o) && !k.c }

/-- `TextClass::union`. -/
def union (a b : TClass) : TClass :=
  norm { d := a.d || b.d, s := a.s || b.s, l := a.l || b.l, o := a.o || b.o,
         sp := a.sp || b.sp, b := a.b || b.b, c := a.c || b.c,
         hex := a.effHex && b.effHex }

/-- `TextClass::contains`, as an order. -/
def le (a b : TClass) : Bool :=
  (!a.d || b.d) && (!a.s || b.s) && (!a.l || b.l) && (!a.o || b.o) &&
    (!a.sp || b.sp) && (!a.b || b.b) && (!a.c || b.c) && (!b.effHex || a.effHex)

end TClass

def isDigitCode (c : CharCode) : Bool := decide (48 ≤ c) && decide (c ≤ 57)
def isHexCode (c : CharCode) : Bool :=
  isDigitCode c || (decide (65 ≤ c) && decide (c ≤ 70)) || (decide (97 ≤ c) && decide (c ≤ 102))

/-- `TextClass::of` for a known token. -/
def kindOf : Token → Kind
  | .char c .other =>
    if isDigitCode c then .digit
    else if c == 43 || c == 45 || c == 46 || c == 44 then .sign else .other
  | .char _ .letter => .letter
  | .char _ .space => .space
  | .char _ .bgroup => .brace
  | .char _ .egroup => .brace
  | .char _ .active => .cs
  | .char _ .param => .cs
  | .char _ _ => .other
  | _ => .cs

/-- Whether text of class `k` may hold token `t`. A class with names may
hold anything (`CS` is "anything" in tex.rs). -/
def TClass.admits (k : TClass) (t : Token) : Prop :=
  k.c = true ∨
    (k.has (kindOf t) = true ∧
      (k.hex = true →
        match t with
        | .char c .letter => isHexCode c = true
        | .char c .other => isHexCode c = true
        | _ => True))

/-- Braces balanced, never closing below zero (§ 473). -/
def balancedFrom : Nat → TokList → Bool
  | n, [] => n == 0
  | n, t :: ts =>
    if t.isBgroup then balancedFrom (n + 1) ts
    else if t.isEgroup then (if n == 0 then false else balancedFrom (n - 1) ts)
    else balancedFrom n ts

def balanced (ts : TokList) : Bool := balancedFrom 0 ts

/-! ## 4. Abstract text: typed holes, typed digits, one-of tokens

An abstract token list is a list of items. A `hole` is unknown text of a
class (`Machine::unknown_text`), with "non-empty" and an optional length
bound (`LEN_MARK`). `digits i` is `\the` of a number in `i` (typed digits,
src/engine/scan.rs). `oneOf` is a one-of token (`split_one_of_groups`). -/

structure Hole where
  cls : TClass
  nonempty : Bool := false
  maxLen : Option Nat := none
  deriving DecidableEq, Repr, Inhabited

inductive AItem where
  | tok (t : Token)
  | hole (h : Hole)
  | digits (i : Itv)
  | oneOf (alts : List TokList)
  deriving Inhabited

abbrev AText := List AItem

/-- `\number` of `n` (§ 65): an optional `-`, then the digits, category 12. -/
def printInt (n : Int) : TokList :=
  let ds := (Nat.toDigits 10 n.natAbs).map (fun ch => Token.char ch.toNat .other)
  if n < 0 then Token.char 45 .other :: ds else ds

def Hole.γ (h : Hole) (p : TokList) : Prop :=
  (∀ t, t ∈ p → h.cls.admits t) ∧ (h.nonempty = true → p ≠ []) ∧
    (∀ n, h.maxLen = some n → p.length ≤ n) ∧ balanced p = true

def AItem.γ : AItem → TokList → Prop
  | .tok t, p => p = [t]
  | .hole h, p => h.γ p
  | .digits i, p => ∃ n, i.γ n ∧ p = printInt n
  | .oneOf alts, p => p ∈ alts

def AText.γ : AText → TokList → Prop
  | [], p => p = []
  | it :: rest, p => ∃ p₁ p₂, p = p₁ ++ p₂ ∧ it.γ p₁ ∧ AText.γ rest p₂

def AText.ofList (ts : TokList) : AText := ts.map AItem.tok

/-- Typed text standing for any text of `cls` (the havoc of a token list). -/
def AText.any (cls : TClass) : AText := [.hole { cls := cls }]

/-- The class of what an item may hold. -/
def AItem.cls : AItem → TClass
  | .tok t =>
    (match kindOf t with
    | .digit => { d := true } | .sign => { s := true } | .letter => { l := true }
    | .other => { o := true } | .space => { sp := true } | .brace => { b := true }
    | .cs => { c := true })
  | .hole h => h.cls
  | .digits _ => TClass.number
  | .oneOf _ => TClass.any

/-- `run_hull` without structure: one hole for the whole text (what a
`Reach::Havoc` widening keeps of a text it does not recognize). -/
def AText.hull (xs : List AText) : AText :=
  let cls := xs.foldl (fun (k : TClass) (x : AText) =>
    x.foldl (fun (k' : TClass) (it : AItem) => k'.union it.cls) k) ({} : TClass)
  [.hole { cls := cls }]

/-! ## 5. Meaning sets: `Binding.may` (src/engine/env.rs)

A member is a known meaning or a parameterless macro whose body is
abstract text (`macro_hull`, a widened macro). -/

inductive AMeaning where
  | exact (m : Meaning)
  | text (long outer prot : Bool) (body : AText)
  deriving Inhabited

def AMeaning.γ : AMeaning → Meaning → Prop
  | .exact m, m' => m' = m
  | .text lg ou pr body, m' =>
    ∃ mc : Macro, m' = .call mc ∧ mc.params = [] ∧ mc.braceDelim = false ∧
      mc.long = lg ∧ mc.outer = ou ∧ mc.prot = pr ∧ AText.γ body mc.body

/-! ## 6. Values: `Value` and `OneOf` (src/engine/value.rs) -/

inductive AVal where
  /-- `Value::Unknown`, `Meaning::Unknown` without `may`. -/
  | top
  /-- `Binding.meaning`/`may`: one of these meanings. -/
  | meanings (ms : List AMeaning)
  /-- `Int`, `Dimen`, `Num`, `OneOf` of numbers: one of these intervals. -/
  | ints (ms : List Itv)
  /-- `Glue`, `GlueNum`, `OneOf` of glue. -/
  | glues (ms : List GlueHull)
  /-- `Toks`, `OneOf` of token lists, and a widened list's frame. -/
  | toks (ms : List AText)
  /-- A box register: void or not, and its dimensions (`Dim`). -/
  | box (mayVoid : Bool) (w h d : Itv)
  deriving Inhabited

def AVal.γ : AVal → Val → Prop
  | .top, _ => True
  | .meanings ms, v => ∃ a m, a ∈ ms ∧ v = .meaning m ∧ a.γ m
  | .ints ms, v => ∃ i n, i ∈ ms ∧ v = .int n ∧ i.γ n
  | .glues ms, v => ∃ h g, h ∈ ms ∧ v = .glue g ∧ h.γ g
  | .toks ms, v => ∃ a p, a ∈ ms ∧ v = .toks p ∧ AText.γ a p
  | .box mv w h d, v =>
    (mv = true ∧ v = .box .void) ∨
      ∃ x y z, (v = .box (.hbox x y z) ∨ v = .box (.vbox x y z)) ∧ w.γ x ∧ h.γ y ∧ d.γ z

def AVal.le (a b : AVal) : Prop := semLe AVal.γ a b

def hullItv : List Itv → Option Itv
  | [] => none
  | i :: is => some (is.foldl Itv.join i)

def hullGlue : List GlueHull → Option GlueHull
  | [] => none
  | g :: gs => some (gs.foldl GlueHull.join g)

/-- `Value::join` and the meaning half of `env::join`: bounded union, past
the bound the hull; kinds that differ give `top`. -/
def AVal.join (L : Limits) : AVal → AVal → AVal
  | .meanings a, .meanings b =>
    if (a ++ b).length ≤ L.meaningSet then .meanings (a ++ b) else .top
  | .ints a, .ints b =>
    if (a ++ b).length ≤ L.intervalSet then .ints (a ++ b)
    else (match hullItv (a ++ b) with
      | some i => .ints [i]
      | none => .ints [])
  | .glues a, .glues b =>
    if (a ++ b).length ≤ L.intervalSet then .glues (a ++ b)
    else (match hullGlue (a ++ b) with
      | some g => .glues [g]
      | none => .glues [])
  | .toks a, .toks b =>
    if (a ++ b).length ≤ L.valueSet then .toks (a ++ b) else .toks [AText.hull (a ++ b)]
  | .box v w h d, .box v' w' h' d' => .box (v || v') (w.join w') (h.join h') (d.join d')
  | _, _ => .top

/-- Widening at a loop head (`widen_slot`): numbers widen each moving bound
to the word, everything else that still grows goes to `top`, except text,
which keeps only its class. -/
def AVal.widen (L : Limits) : AVal → AVal → AVal
  | .ints a, .ints b =>
    (match hullItv a, hullItv (a ++ b) with
    | some p, some q => .ints [p.widen q]
    | _, _ => .ints (a ++ b))
  | .toks a, .toks b => .toks [AText.hull (a ++ b)]
  | a, b => match AVal.join L a b with
    | .meanings _ => .top
    | j => j

/-! ## 7. Argument types: the `Ty` lattice (src/analysis/roles.rs)

A tree by `parent`; the join is the nearest common ancestor. Types are an
editor aid and add no narrowing to the run, so their `γ` is a parameter:
the texts an argument of that type may hold, monotone along `parent`. -/

inductive RegK where
  | count | dimen | skip | muskip | toks | box | read | write
  deriving DecidableEq, Repr, Inhabited

inductive KeyK where
  | label | cite | option | pgf | environment | declared | entry | color | any
  deriving DecidableEq, Repr, Inhabited

inductive FileK where
  | tex | sty | cls | bib | bst | bbx | cbx | graphic | any
  deriving DecidableEq, Repr, Inhabited

inductive VarK where
  | tl | str | clist | seq | prop | fp | bool | regex | coffin | flag | intarray | cctab
  deriving DecidableEq, Repr, Inhabited

inductive Ty where
  | bottom | number | dimen | glue | muglue | quantity
  | reg (k : RegK) | register | box | named (k : RegK)
  | key (k : KeyK) | file (k : FileK) | name | cs | switch | var (k : VarK)
  | options | date | body | text | top
  deriving DecidableEq, Repr, Inhabited

namespace Ty

/-- `Ty::parent`, case for case. -/
def parent : Ty → Ty
  | bottom => bottom
  | dimen => glue
  | reg .skip => glue
  | number => quantity
  | glue => quantity
  | muglue => quantity
  | reg .count => number
  | box => number
  | reg .dimen => dimen
  | reg .muskip => muglue
  | named _ => name
  | key .any => name
  | file .any => name
  | name => text
  | body => text
  | date => text
  | key _ => key .any
  | file _ => file .any
  | switch => cs
  | var .str => var .tl
  | var .clist => var .tl
  | var _ => cs
  | reg _ => top
  | register => top
  | quantity => top
  | text => top
  | cs => top
  | options => top
  | top => top

/-- The chain up to `top`; the tree is at most 6 deep. -/
def ancestors (t : Ty) : List Ty :=
  let rec go : Nat → Ty → List Ty
    | 0, _ => [top]
    | n + 1, u => if u = top then [top] else u :: go n u.parent
  go 6 t

/-- `Ty::join`. -/
def join (a b : Ty) : Ty :=
  if a = bottom then b
  else if b = bottom then a
  else ((ancestors b).find? (fun u => (ancestors a).contains u)).getD top

/-- `Ty::leq`. -/
def le (a b : Ty) : Bool := join a b == b

/-- A concretization is admissible when it grows along `parent`. -/
def Admissible {Text : Type} (γ : Ty → Text → Prop) : Prop :=
  (∀ t x, γ t x → γ t.parent x) ∧ (∀ x, γ top x) ∧ (∀ x, ¬ γ bottom x)

end Ty

/-! ## 8. The abstract state

`eqtb` maps each location to an abstract value; levels are kept exactly
(src/engine/env.rs keeps `level` per slot). Group and conditional frames
may stand for any number of copies (`many`, "stacks as intervals"). The
modes are a set (`Modes`, src/engine/mode.rs). -/

inductive ASave where
  | restore (l : Loc) (old : AVal) (lvl : Nat)
  | retain (l : Loc)
  | insertToken (t : Token)
  | group (c : GroupCode) (many : Bool)
  deriving Inhabited

inductive AInput where
  | toks (kind : TokListKind) (t : AText)
  | file (f : FileLevel)
  deriving Inhabited

structure ACond where
  limit : IfLimit
  op : IfOp
  many : Bool := false
  deriving Inhabited

structure AState where
  eqtb : Loc → AVal
  level : Loc → Nat
  curLevel : Nat := 1
  save : List ASave := []
  input : List AInput := []
  cond : List ACond := []
  modes : List ModeKind := [.vertical]

/-- The save stack: a `many` group frame stands for zero or more concrete
groups of its code, whose restore entries are any. -/
inductive SaveRel : List ASave → List SaveEntry → Prop
  | nil : SaveRel [] []
  | restore {l : Loc} {a : AVal} {lv : Nat} {e : Entry} {rest : List ASave} {crest : List SaveEntry} :
      a.γ e.val → e.level = lv → SaveRel rest crest →
      SaveRel (.restore l a lv :: rest) (.restore l e :: crest)
  | retain {l : Loc} {rest : List ASave} {crest : List SaveEntry} :
      SaveRel rest crest → SaveRel (.retain l :: rest) (.retain l :: crest)
  | insertToken {t : Token} {rest : List ASave} {crest : List SaveEntry} :
      SaveRel rest crest → SaveRel (.insertToken t :: rest) (.insertToken t :: crest)
  | group {c : GroupCode} {ctx : Option BoxContext} {line : Nat} {rest : List ASave}
      {crest : List SaveEntry} :
      SaveRel rest crest → SaveRel (.group c false :: rest) (.group c ctx line :: crest)
  | manyNone {c : GroupCode} {rest : List ASave} {crest : List SaveEntry} :
      SaveRel rest crest → SaveRel (.group c true :: rest) crest
  | manyMore {c : GroupCode} {ctx : Option BoxContext} {line : Nat} {rest : List ASave}
      {crest pre : List SaveEntry} :
      SaveRel (.group c true :: rest) crest →
      (∀ e, e ∈ pre → ∃ l old, e = .restore l old) →
      SaveRel (.group c true :: rest) (pre ++ (SaveEntry.group c ctx line :: crest))

/-- The conditional stack, `many` frames as above. -/
inductive CondRel : List ACond → List CondLevel → Prop
  | nil : CondRel [] []
  | one {a : ACond} {c : CondLevel} {rest : List ACond} {crest : List CondLevel} :
      a.many = false → a.limit = c.limit → a.op = c.op → CondRel rest crest →
      CondRel (a :: rest) (c :: crest)
  | manyNone {a : ACond} {rest : List ACond} {crest : List CondLevel} :
      a.many = true → CondRel rest crest → CondRel (a :: rest) crest
  | manyMore {a : ACond} {c : CondLevel} {rest : List ACond} {crest : List CondLevel} :
      a.many = true → a.op = c.op → CondRel (a :: rest) crest → CondRel (a :: rest) (c :: crest)

def AInput.γ : AInput → InputLevel → Prop
  | .toks k t, .toks k' p => k = k' ∧ AText.γ t p
  | .file f, .file f' => f.name = f'.name ∧ f.cur = f'.cur ∧ f.lines = f'.lines ∧
      f.state = f'.state ∧ f.line = f'.line
  | _, _ => False

def InputRel : List AInput → List InputLevel → Prop
  | [], [] => True
  | a :: xs, c :: ys => a.γ c ∧ InputRel xs ys
  | _, _ => False

/-- `γ` of an abstract state. With `many` frames the group level is
any level the frames allow, so it is not pinned. -/
def AState.γ (a : AState) (s : State) : Prop :=
  (∀ l, (a.eqtb l).γ (s.eqtb l).val) ∧
  (∀ l, a.level l = (s.eqtb l).level) ∧
  ((∀ f, f ∈ a.save → ∀ c, f ≠ .group c true) → a.curLevel = s.curLevel) ∧
  SaveRel a.save s.save ∧
  CondRel a.cond s.cond ∧
  InputRel a.input s.input ∧
  s.mode.kind ∈ a.modes

/-- Paths: the abstract state is a list of path states (`Paths` in
src/analysis/machine/explore.rs). A concrete state is covered by some path. -/
abbrev APaths := List AState

def APaths.γ (ps : APaths) (s : State) : Prop := ∃ a, a ∈ ps ∧ a.γ s

/-! ## 9. Transfer: assignment and grouping (§§ 268–284)

Mirrors Core's `eq_define`/`geq_define`; Rust: `Env::set`, `Env::pop_group`. -/

namespace AState

def setAt (a : AState) (l : Loc) (v : AVal) (lv : Nat) : AState :=
  { a with eqtb := upd a.eqtb l v, level := upd a.level l lv }

/-- § 277 `eq_define` (local) and § 279 `geq_define` (global). -/
def assign (a : AState) (glob : Bool) (l : Loc) (v : AVal) : AState :=
  if glob then a.setAt l v 1
  else if a.level l = a.curLevel then a.setAt l v a.curLevel
  else if a.curLevel > 1 then
    { a.setAt l v a.curLevel with save := .restore l (a.eqtb l) (a.level l) :: a.save }
  else a.setAt l v a.curLevel

/-- A write through an index not pinned to one location (`weaken_registers`,
src/engine/exec.rs): each location it may name joins the new value. -/
def weakAssign (L : Limits) (a : AState) (glob : Bool) (ls : List Loc) (v : AVal) : AState :=
  ls.foldl (fun (s : AState) (l : Loc) => s.assign glob l (AVal.join L (s.eqtb l) v)) a

/-- Past `limits.value_set` locations: every register of the kind may hold
anything (`register_top`). Levels: a weak write may be local or global, so
no restore entry is trusted for these; kept exact here, see mapping.md. -/
def havocRegs (a : AState) (k : RegKind) : AState :=
  { a with eqtb := fun l => match l with
      | .reg k' i => if k' = k then .top else a.eqtb (.reg k' i)
      | l' => a.eqtb l' }

def beginGroup (a : AState) (c : GroupCode) : AState :=
  { a with save := .group c false :: a.save, curLevel := a.curLevel + 1 }

/-- One restore (§ 283): a global write since keeps its value. -/
def restoreOne (a : AState) (l : Loc) (old : AVal) (lv : Nat) : AState :=
  if a.level l = 1 then a else a.setAt l old lv

/-- Unwind to the group frame (§ 282). `none`: no frame (an error). -/
def unsave : Nat → AState → List ASave → Option (AState × List ASave)
  | 0, _, _ => none
  | _, _, [] => none
  | n + 1, a, .restore l old lv :: rest => unsave n (a.restoreOne l old lv) rest
  | n + 1, a, .retain _ :: rest => unsave n a rest
  | n + 1, a, .insertToken t :: rest =>
    unsave n { a with input := AInput.toks .backedUp [AItem.tok t] :: a.input } rest
  | _, a, .group c m :: rest => some (a, .group c m :: rest)

/-- The value a location may hold after a `many` frame closes once: its
saved value or its current one (saved names widened). -/
def widenSaved (L : Limits) (a : AState) (fr : List ASave) : AState :=
  fr.foldl (fun (s : AState) (f : ASave) => match f with
    | .restore l old _ => { s with eqtb := upd s.eqtb l (AVal.join L (s.eqtb l) old) }
    | _ => s) a

/-- `}` / `\endgroup`: a plain frame closes. A `many` frame forks: one copy
closes and the frame stays (the saved names widened), or no copy is left
and the closer acts below it. `n` bounds the frames looked through. -/
def endGroupN (L : Limits) : Nat → AState → GroupCode → List AState
  | 0, _, _ => []
  | n + 1, a, c =>
    match unsave (a.save.length + 1) a a.save with
    | some (a', .group c' false :: rest) =>
      if c' = c then [{ a' with save := rest, curLevel := a'.curLevel - 1 }] else []
    | some (a', .group c' true :: rest) =>
      if c' = c then
        { widenSaved L a a.save with save := ASave.group c' true :: rest } ::
          endGroupN L n { a' with save := rest } c
      else []
    | _ => []

def endGroup (L : Limits) (a : AState) (c : GroupCode) : List AState :=
  endGroupN L (a.save.length + 1) a c

end AState

/-! ## 10. Transfer: conditionals (§§ 487–510)

A test answers `some b` when every concrete state agrees, else `none`; the
narrowing keeps on each arm what that arm's answer allows. Arms run on
paths of their own and join where they meet (`fi_mark`, `partner`). -/

/-- `Machine::branch`: decided, one arm; undecided, both arms on narrowed
states. Infeasible arms (`narrow` gives `none`) are dropped. -/
def branch (test : AState → Option Bool) (narrow : Bool → AState → Option AState)
    (a : AState) : List (Bool × AState) :=
  match test a with
  | some b => [(b, a)]
  | none =>
    (match narrow true a with | some t => [(true, t)] | none => []) ++
    (match narrow false a with | some f => [(false, f)] | none => [])

/-- `\ifnum⟨loc⟩⟨rel⟩⟨bound⟩`: decide member by member; narrow each member. -/
def ifNumTest (l : Loc) (r : Itv.Rel) (bound : Int) (a : AState) : Option Bool :=
  match a.eqtb l with
  | .ints ms =>
    let rs := ms.map (fun i => i.compare r (.exact bound))
    (match rs with
    | some b :: rest => if rest.all (fun x => x == some b) then some b else none
    | _ => none)
  | _ => none

def ifNumNarrow (l : Loc) (r : Itv.Rel) (bound : Int) (holds : Bool) (a : AState) :
    Option AState :=
  match a.eqtb l with
  | .ints ms =>
    let ms' := ms.filterMap (fun i => i.narrow r bound holds)
    if ms'.isEmpty then none else some { a with eqtb := upd a.eqtb l (.ints ms') }
  | _ => some a

/-- Where a conditional's arms meet, at its `\fi` or the next common place:
paths at the same place join (`join_configs`). Paths whose stacks or levels
differ are kept apart here (see mapping.md for what the Rust does). -/
def AState.joinable (a b : AState) : Prop :=
  a.curLevel = b.curLevel ∧ a.save = b.save ∧ a.cond = b.cond ∧ a.input = b.input ∧
    (∀ l, a.level l = b.level l)

def AState.join (L : Limits) (a b : AState) : AState :=
  { a with
    eqtb := fun l => AVal.join L (a.eqtb l) (b.eqtb l),
    modes := a.modes ++ b.modes.filter (fun m => !(a.modes.contains m)) }

/-! ## 11. Transfer: macro expansion and argument scanning (§§ 389–399)

Arguments are read from the top token-list level. Over a typed hole a scan
cannot know where its argument ends; it forks over the places that remain
possible, and a hole that cannot hold the delimiter is passed whole. -/

/-- Whether a hole may hold token `t`: its class admits `t`'s kind. -/
def Hole.mayHold (h : Hole) (t : Token) : Bool :=
  h.cls.c || h.cls.has (kindOf t)

/-- What is left of a hole after a scan took a part of it. -/
def Hole.part (h : Hole) : Hole := { h with nonempty := false }

/-- Whether an item that is not one known token may hold `t`. -/
def AItem.mayHold : AItem → Token → Bool
  | .tok t, t' => t == t'
  | .hole h, t => h.mayHold t
  | .digits _, t => TClass.number.has (kindOf t)
  | .oneOf alts, t => alts.any (fun p => p.contains t)

/-- A part of an item a scan took or left: unknown text of its class. -/
def AItem.part : AItem → AItem
  | .hole h => .hole h.part
  | it => .hole { cls := it.cls }

/-- Tokens `ds` at the head of `xs` as known tokens: the rest, if so. -/
def dropToks : TokList → AText → Option AText
  | [], xs => some xs
  | d :: ds, AItem.tok t :: xs => if t = d then dropToks ds xs else none
  | _ :: _, _ => none

/-- The rests a delimiter `δ` may leave when it may start inside an item:
every suffix of `δ` may be what lies after the item. -/
def afterItem (δ : TokList) (rest : AText) : List AText :=
  (List.range (δ.length + 1)).filterMap (fun i => dropToks (δ.drop i) rest)

/-- A delimited argument (§ 392): the pairs (argument, rest). `depth` is the
brace depth so far; the delimiter counts only at depth 0. An unmatched `}`
or the end of the text gives no pair (TeX's error, under assume_compiles).
An item that may hold a delimiter token forks: the delimiter starts in it,
or the scan reads on past it. -/
def scanDelim (δ : TokList) : Nat → AText → List (AText × AText)
  | _, [] => []
  | depth, AItem.tok t :: rest =>
    let here : List (AText × AText) :=
      (match depth, δ with
      | 0, d :: ds =>
        if t = d then (match dropToks ds rest with
          | some r => [([], r)]
          | none => [])
        else []
      | _, _ => [])
    let deeper : List (AText × AText) :=
      if t.isBgroup then scanDelim δ (depth + 1) rest
      else if t.isEgroup then (if depth = 0 then [] else scanDelim δ (depth - 1) rest)
      else scanDelim δ depth rest
    here ++ deeper.map (fun (p : AText × AText) => (AItem.tok t :: p.1, p.2))
  | depth, it :: rest =>
    let inside : List (AText × AText) :=
      if δ.any (fun d => it.mayHold d) then
        (afterItem δ rest).map (fun (r : AText) => ([it.part], it.part :: r))
      else []
    inside ++ (scanDelim δ depth rest).map (fun (p : AText × AText) => (it :: p.1, p.2))

/-- The body of a group whose `{` was read: up to its matching `}`. -/
def groupBody : Nat → AText → Option (AText × AText)
  | _, [] => none
  | 0, AItem.tok t :: rest =>
    if t.isEgroup then some ([], rest)
    else (groupBody (if t.isBgroup then 1 else 0) rest).map
      (fun (p : AText × AText) => (AItem.tok t :: p.1, p.2))
  | n + 1, AItem.tok t :: rest =>
    let n' := if t.isBgroup then n + 2 else if t.isEgroup then n else n + 1
    (groupBody n' rest).map (fun (p : AText × AText) => (AItem.tok t :: p.1, p.2))
  | n, it :: rest => (groupBody n rest).map (fun (p : AText × AText) => (it :: p.1, p.2))

/-- An undelimited argument (§ 393): spaces skipped, then one token or one
balanced group. Over an item that is not a known token: a part of it, or
(when it may be empty or spaces) what comes after it. -/
def scanUndelim : AText → List (AText × AText)
  | [] => []
  | AItem.tok t :: rest =>
    if t.isSpace then scanUndelim rest
    else if t.isBgroup then (match groupBody 0 rest with | some p => [p] | none => [])
    else [([AItem.tok t], rest)]
  | AItem.hole h :: rest =>
    ([AItem.hole h.part], AItem.hole h.part :: rest) ::
      (if h.nonempty = false || h.cls.sp then scanUndelim rest else [])
  | AItem.digits _ :: rest =>
    [([AItem.hole { cls := TClass.number, nonempty := true, maxLen := some 1 }],
      AItem.hole { cls := TClass.digits } :: rest)]
  | AItem.oneOf alts :: rest =>
    ([AItem.hole { cls := TClass.any }], AItem.hole { cls := TClass.any } :: rest) ::
      (if alts.any (fun p => p.all (fun t => t.isSpace)) then scanUndelim rest else [])

/-- `#n` replaced by the n-th argument (§ 390 `macro_call`; `outParam n`
is `#n`, counted from 1). -/
def substitute (args : List AText) : TokList → AText
  | [] => []
  | Token.outParam n :: ts => (args.getD (n - 1) []) ++ substitute args ts
  | t :: ts => AItem.tok t :: substitute args ts

/-- A parameter text as its leading tokens and, per `#`, the tokens that
delimit it (empty: undelimited). -/
def splitParams : List ParamItem → TokList × List TokList
  | [] => ([], [])
  | .tok t :: ps => let r := splitParams ps; (t :: r.1, r.2)
  | .arg :: ps => let r := splitParams ps; ([], r.1 :: r.2)

/-- The leading tokens of a parameter text must come first (§ 397). Over an
item that may hold them, they may end in it or past it. -/
def readLead (lead : TokList) (rest : AText) : List AText :=
  match dropToks lead rest with
  | some r => [r]
  | none =>
    match rest with
    | it :: r =>
      if lead.any (fun d => it.mayHold d) && !(match it with | AItem.tok _ => true | _ => false)
      then (it.part :: r) :: afterItem lead r
      else []
    | [] => []

/-- § 400: a delimited argument that is one `{…}` group loses its braces.
Both readings are kept where the frame is known tokens. -/
def stripBraces (xa : AText) : List AText :=
  match xa with
  | AItem.tok t :: mid =>
    if t.isBgroup then
      match mid.reverse with
      | AItem.tok u :: midRev => if u.isEgroup then [xa, midRev.reverse] else [xa]
      | _ => [xa]
    else [xa]
  | _ => [xa]

def stripAll (ps : List (AText × AText)) : List (AText × AText) :=
  ps.foldr (fun (p : AText × AText) (acc : List (AText × AText)) =>
    (stripBraces p.1).map (fun (x : AText) => (x, p.2)) ++ acc) []

def scanArgs : List TokList → AText → List (List AText × AText)
  | [], rest => [([], rest)]
  | δ :: ds, rest =>
    let firsts := if δ.isEmpty then scanUndelim rest else stripAll (scanDelim δ 0 rest)
    firsts.foldr (fun (p : AText × AText) (acc : List (List AText × AText)) =>
      (scanArgs ds p.2).map (fun (q : List AText × AText) => (p.1 :: q.1, q.2)) ++ acc) []

/-- All the ways a macro's parameter text may match the abstract input. -/
def scanParams (ps : List ParamItem) (rest : AText) : List (List AText × AText) :=
  let sp := splitParams ps
  (readLead sp.1 rest).foldr (fun (r : AText) (acc : List (List AText × AText)) =>
    scanArgs sp.2 r ++ acc) []

/-- Expanding a control sequence (§ 366): split per member of its meaning
set (`limits.meaning_splits`), each macro called with every scan the
abstract input allows. A widened macro inserts its typed body. `top`: its
output is unknown text and what it does is unknown (`havoc`, mapping.md). -/
def expandWith (havoc : AState → AState) (a : AState) (r : CsRef) : List AState :=
  match a.input with
  | AInput.toks k rest :: below =>
    (match a.eqtb (.meaning r) with
    | .meanings ms =>
      ms.foldr (fun (am : AMeaning) (acc : List AState) =>
        (match am with
          | .exact (.call mc) =>
            (scanParams mc.params rest).map (fun (p : List AText × AText) =>
              { a with input := AInput.toks (.macroBody r) (substitute p.1 mc.body) ::
                  AInput.toks k p.2 :: below })
          | .text _ _ _ body =>
            [{ a with input := AInput.toks (.macroBody r) body :: AInput.toks k rest :: below }]
          | .exact _ => [a]) ++ acc) []
    | _ =>
      [{ havoc a with
          input := AInput.toks .inserted (AText.any TClass.any) :: AInput.toks k rest :: below }])
  | _ => [a]

/-- The sound havoc for unknown code: every location may hold anything. -/
def havocAll (a : AState) : AState := { a with eqtb := fun _ => .top }

/-! ## 12. Loops: k passes, widening, the forced summary

`loopHead` mirrors explore.rs: each round joins the pass into the head;
from round `widenAfter` the join is widened; a head still growing at
`2·widenAfter` widened rounds is summarized. `fuel` makes it structural. -/

def loopHead (L : Limits) (join widen : AState → AState → AState)
    (grows : AState → AState → Bool) (body : AState → AState)
    (summary : AState → AState) : Nat → Nat → AState → AState
  | 0, _, a => summary a
  | fuel + 1, passes, a =>
    let j := if passes ≥ L.widenAfter then widen a (join a (body a)) else join a (body a)
    if !grows a j then a
    else if passes ≥ 2 * L.widenAfter then summary j
    else loopHead L join widen grows body summary fuel (passes + 1) j

/-- Rounds `loopHead` runs, at most `fuel` plus the summary. -/
def loopRounds (L : Limits) (join widen : AState → AState → AState)
    (grows : AState → AState → Bool) (body : AState → AState) : Nat → Nat → AState → Nat
  | 0, _, _ => 1
  | fuel + 1, passes, a =>
    let j := if passes ≥ L.widenAfter then widen a (join a (body a)) else join a (body a)
    if !grows a j then 1
    else if passes ≥ 2 * L.widenAfter then 1
    else loopRounds L join widen grows body fuel (passes + 1) j + 1

theorem loopRounds_le (L : Limits) (join widen : AState → AState → AState)
    (grows : AState → AState → Bool) (body : AState → AState) :
    ∀ fuel passes a, loopRounds L join widen grows body fuel passes a ≤ fuel + 1 := by
  intro fuel
  induction fuel with
  | zero => intro _ _; exact Nat.le_refl 1
  | succ n ih =>
    intro passes a
    simp only [loopRounds]
    repeat' split
    all_goals first | omega | exact Nat.succ_le_succ (ih _ _)

/-- The sound loop summary: every location the body may write (`writes`
must cover every pass, not only the passes seen) holds anything. -/
def summaryHavoc (writes : List Loc) (a : AState) : AState :=
  { a with eqtb := fun l => if writes.contains l then .top else a.eqtb l }

/-! ## 13. Call summaries (src/analysis/machine/calls.rs)

The Rust takes a call met with unknown operands after `call_summary_after`
recorded runs as the join of those runs' writes. Sound only when the runs
cover every run of the call; `callSummarySound` is the model's summary. -/

/-- What calls.rs computes: the recorded writes, joined and widened. -/
def callSummaryRust (L : Limits) (runs : List (List (Loc × AVal))) (a : AState) : AState :=
  let locs := runs.foldl (fun (acc : List Loc) (r : List (Loc × AVal)) => acc ++ r.map Prod.fst) []
  locs.foldl (fun (s : AState) (l : Loc) =>
    let vs := runs.map (fun (r : List (Loc × AVal)) => match r.find? (fun w => w.1 == l) with
      | some w => w.2
      | none => s.eqtb l)
    match vs with
    | v :: rest => s.assign false l (rest.foldl (AVal.widen L) v)
    | [] => s) a

/-- Sound: every location any run of the call may write holds anything. -/
def callSummarySound (writes : List Loc) (a : AState) : AState := summaryHavoc writes a

/-! ## 14. Code a join makes unknown: `may_run` (src/analysis/machine.rs)

Pending code joined into unknown text may run. Sound: every location the
code may write holds its old value or anything. -/
def mayRunSound (writes : List Loc) (a : AState) : AState := summaryHavoc writes a

/-! ## 15. assume_compiles

A path that raises a certain error in library code is dropped. The model
states this as a restriction of the concrete runs: `γ` covers only runs in
which `Compiles` holds. -/
def dropErrors (certainLibError : AState → Bool) (ps : APaths) : APaths :=
  ps.filter (fun a => !certainLibError a)

/-- What job 3 proves for `dropErrors`: a run that compiles is never a run
whose state a dropped path covers. -/
def DropSound (certainLibError : AState → Bool) (Compiles : State → Prop) : Prop :=
  ∀ a s, certainLibError a = true → a.γ s → ¬ Compiles s

end Satex.Abstract
