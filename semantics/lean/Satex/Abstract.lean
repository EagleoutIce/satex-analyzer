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

/-! ## 1. Intervals: `Num` (src/engine/value.rs)

Intuition. Counts and dimensions are 32-bit words, and documents test them
against constants (`\ifnum\count0>3`). An interval answers such a test
whenever all its members agree, and narrows on each arm. Relational domains
(octagons, polyhedra) would relate registers, but TeX code rarely compares
two unknowns, and they cost far more per join. A small set of intervals
(`limits.interval_set`) keeps disjoint cases apart before taking the hull.
Widening jumps a moving bound to the word's end, so a chain changes at most
twice (`widen_chain_changes_le_two`). -/

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
restriction: its letters and others are hex digits. Kinds rather than
character sets: a scan only asks whether text may hold a digit, a sign, a
brace or a name, and seven bits answer that. -/

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

/-- Whether a token passes the `hex` restriction: letters and others must
be hex digits; digits, signs, spaces, braces and names are not restricted
(`TextClass::holds` keeps SIGN apart from OTHER, tex.rs). -/
def hexOk (t : Token) : Prop :=
  (kindOf t = .letter ∨ kindOf t = .other) → ∃ c k, t = .char c k ∧ isHexCode c = true

/-- Whether text of class `k` may hold token `t`. A class with names may
hold anything (`CS` is "anything" in tex.rs). -/
def TClass.admits (k : TClass) (t : Token) : Prop :=
  k.c = true ∨ (k.has (kindOf t) = true ∧ (k.hex = true → hexOk t))

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
abstract text (`macro_hull`, a widened macro). A name defined on two arms
keeps both meanings, so a later call runs each. Going to `top` at once
would lose every macro a conditional defines. Past `limits.meaning_set` the
set does go to `top`. -/

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
  | .toks ms, v => ∃ a p, a ∈ ms ∧ v = .toks p ∧ AText.γ a p ∧ balanced p = true
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

Intuition. TeX's eqtb is a total map with a level per entry, and a save
stack that undoes local writes at `}` (§§ 268–284). SaTeX keeps both, with
abstract values. Two things make them inexact, and each has one device:

- A loop that opens groups and does not close them leaves a stack of unknown
  height. A `many` frame stands for zero or more alike groups ("stacks as
  intervals"), listing the locations its copies may have saved.
- A write whose save entry SaTeX cannot decide (an unknown level under a
  `many` frame, or a summarized loop) leaves a `touched` entry: any number
  of restore entries for that location, with any values, may sit in this
  frame. Closing the frame makes the location unknown.

Alternatives: tracking the exact level of every copy (unbounded), or
widening the whole state at an unknown write (too coarse: one write would
lose every value). `touched` loses only the written location, and only at
the frame's end, which is what the Rust does (`unfold` widens the saved
slots, env.rs). -/

inductive ASave where
  /-- A restore entry the run made for sure: old value and level. -/
  | restore (l : Loc) (old : AVal) (lv : Option Nat)
  /-- Any number of restore entries for `l`, any values. -/
  | touched (l : Loc)
  | retain (l : Loc)
  | insertToken (t : Token)
  /-- A group; `many = some ls`: zero or more alike groups whose copies may
  save the locations `ls`. -/
  | group (c : GroupCode) (many : Option (List Loc))
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

/-- Levels: `some n` is exact, `none` is any level. `pinned`: `curLevel` is
TeX's group level; it is lost where a `many` frame closes. -/
structure AState where
  eqtb : Loc → AVal
  level : Loc → Option Nat
  curLevel : Nat := 1
  pinned : Bool := true
  save : List ASave := []
  input : List AInput := []
  cond : List ACond := []
  modes : List ModeKind := [.vertical]

/-- Whether the frame on top of `fr` has a `touched l` entry. -/
def Touched (l : Loc) : List ASave → Prop
  | [] => False
  | .group _ _ :: _ => False
  | .touched l' :: rest => l' = l ∨ Touched l rest
  | _ :: rest => Touched l rest

/-- The save stack. A `touched l` entry lets restore entries for `l`
appear anywhere above it in its frame (`skip`). A `many` frame stands for
zero or more groups of its code, each with restore entries for its
locations. -/
inductive SaveRel : List ASave → List SaveEntry → Prop
  | nil : SaveRel [] []
  | restore {l : Loc} {a : AVal} {lv : Option Nat} {e : Entry} {rest : List ASave}
      {crest : List SaveEntry} :
      a.γ e.val → (∀ n, lv = some n → e.level = n) → SaveRel rest crest →
      SaveRel (.restore l a lv :: rest) (.restore l e :: crest)
  | touched {l : Loc} {rest : List ASave} {crest : List SaveEntry} :
      SaveRel rest crest → SaveRel (.touched l :: rest) crest
  | skip {l : Loc} {e : Entry} {rest : List ASave} {crest : List SaveEntry} :
      Touched l rest → SaveRel rest crest → SaveRel rest (.restore l e :: crest)
  | retain {l : Loc} {rest : List ASave} {crest : List SaveEntry} :
      SaveRel rest crest → SaveRel (.retain l :: rest) (.retain l :: crest)
  | insertToken {t : Token} {rest : List ASave} {crest : List SaveEntry} :
      SaveRel rest crest → SaveRel (.insertToken t :: rest) (.insertToken t :: crest)
  | group {c : GroupCode} {ctx : Option BoxContext} {line : Nat} {rest : List ASave}
      {crest : List SaveEntry} :
      SaveRel rest crest → SaveRel (.group c none :: rest) (.group c ctx line :: crest)
  | manyNone {c : GroupCode} {ls : List Loc} {rest : List ASave} {crest : List SaveEntry} :
      SaveRel rest crest → SaveRel (.group c (some ls) :: rest) crest
  | manyMore {c : GroupCode} {ls : List Loc} {ctx : Option BoxContext} {line : Nat}
      {rest : List ASave} {crest pre : List SaveEntry} :
      SaveRel (.group c (some ls) :: rest) crest →
      (∀ e, e ∈ pre → ∃ l old, e = .restore l old ∧ l ∈ ls) →
      SaveRel (.group c (some ls) :: rest) (pre ++ (SaveEntry.group c ctx line :: crest))

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

/-- `γ` of an abstract state. -/
def AState.γ (a : AState) (s : State) : Prop :=
  (∀ l, (a.eqtb l).γ (s.eqtb l).val) ∧
  (∀ l n, a.level l = some n → (s.eqtb l).level = n) ∧
  (a.pinned = true → a.curLevel = s.curLevel) ∧
  SaveRel a.save s.save ∧
  CondRel a.cond s.cond ∧
  InputRel a.input s.input ∧
  s.mode.kind ∈ a.modes

/-- Paths: the abstract state is a list of path states (`Paths` in
src/analysis/machine/explore.rs). A concrete state is covered by some path. -/
abbrev APaths := List AState

def APaths.γ (ps : APaths) (s : State) : Prop := ∃ a, a ∈ ps ∧ a.γ s

/-! ## 9. Transfer: assignment and grouping (§§ 268–284)

Mirrors Core's `eq_define`/`geq_define`; Rust: `Env::set`, `Env::pop_group`.
A local write saves the old entry exactly when TeX would; where the level
is not known it leaves `touched` instead (see section 8). -/

namespace AState

def setAt (a : AState) (l : Loc) (v : AVal) (lv : Option Nat) : AState :=
  { a with eqtb := upd a.eqtb l v, level := upd a.level l lv }

/-- `touched l` on the save stack. -/
def touch (a : AState) (l : Loc) : AState := { a with save := .touched l :: a.save }

/-- § 277 `eq_define` (local) and § 279 `geq_define` (global). -/
def assign (a : AState) (glob : Bool) (l : Loc) (v : AVal) : AState :=
  if glob then a.setAt l v (some 1)
  else if !a.pinned then (a.touch l).setAt l v none
  else match a.level l with
    | some n =>
      if n = a.curLevel ∨ a.curLevel ≤ 1 then a.setAt l v (some a.curLevel)
      else { a with save := .restore l (a.eqtb l) (some n) :: a.save }.setAt l v (some a.curLevel)
    | none =>
      if a.curLevel ≤ 1 then a.setAt l v (some a.curLevel)
      else (a.touch l).setAt l v (some a.curLevel)

/-- The level a write gives, if known. -/
def newLevel (a : AState) (glob : Bool) : Option Nat :=
  if glob then some 1 else if a.pinned then some a.curLevel else none

/-- Two levels as one: equal, or unknown. -/
def lvJoin (x y : Option Nat) : Option Nat := if x = y then x else none

/-- A write that may or may not happen at `l`: the old value or `v`, the old
level or the new one, and maybe a save entry (`touched`). -/
def weakAt (L : Limits) (a : AState) (glob : Bool) (l : Loc) (v : AVal) : AState :=
  (a.touch l).setAt l (AVal.join L (a.eqtb l) v) (lvJoin (a.level l) (a.newLevel glob))

/-- A write through an index not pinned to one location (`weaken_registers`,
src/engine/exec.rs): each location it may name may be the one written. -/
def weakAssign (L : Limits) (a : AState) (glob : Bool) (ls : List Loc) (v : AVal) : AState :=
  ls.foldl (fun (s : AState) (l : Loc) => s.weakAt L glob l v) a

/-- Past `limits.value_set` locations: every register of the kind may hold
anything (`register_top`). -/
def havocRegs (a : AState) (k : RegKind) : AState :=
  { a with eqtb := fun l => match l with
      | .reg k' i => if k' = k then .top else a.eqtb (.reg k' i)
      | l' => a.eqtb l' }

def beginGroup (a : AState) (c : GroupCode) : AState :=
  { a with save := .group c none :: a.save, curLevel := a.curLevel + 1 }

/-- One restore (§ 283): a location written globally since (level one)
keeps its value; an unknown level may go either way. -/
def restoreOne (L : Limits) (a : AState) (l : Loc) (old : AVal) (lv : Option Nat) : AState :=
  match a.level l with
  | some 1 => a
  | some _ => a.setAt l old lv
  | none => a.setAt l (AVal.join L (a.eqtb l) old) none

/-- The state after a frame closed: the tokens saved by `\aftergroup` come
back as one list (§ 282). -/
def closed (a : AState) (ins : TokList) (rest : List ASave) : AState :=
  { a with save := rest, curLevel := a.curLevel - 1,
           input := if ins = [] then a.input else .toks .backedUp (AText.ofList ins) :: a.input }

/-- Every location of `ls` unknown. -/
def forget (a : AState) (ls : List Loc) : AState := ls.foldl (fun s l => s.setAt l .top none) a

/-- `}` / `\endgroup` closing a group of code `c` (§§ 281–283): undo the
entries down to the frame and collect the `\aftergroup` tokens in order. A
plain frame closes. A `many` frame forks: one copy closes and the frame
stays, its locations unknown; or no copy is left and the closer acts below
it. Either way the group level is no longer pinned. -/
def closeFrom (L : Limits) (c : GroupCode) : AState → TokList → List ASave → List AState
  | _, _, [] => []
  | a, ins, .restore l old lv :: rest => closeFrom L c (a.restoreOne L l old lv) ins rest
  | a, ins, .touched l :: rest => closeFrom L c (a.setAt l .top none) ins rest
  | a, ins, .retain _ :: rest => closeFrom L c a ins rest
  | a, ins, .insertToken t :: rest => closeFrom L c a (t :: ins) rest
  | a, ins, .group c' none :: rest => if c' = c then [a.closed ins rest] else []
  | a, ins, .group c' (some ls) :: rest =>
    (if c' = c then
      [{ (a.closed ins (.group c' (some ls) :: rest)).forget ls with
          curLevel := a.curLevel, pinned := false }] else []) ++
      closeFrom L c { a with pinned := false } ins rest

def endGroup (L : Limits) (a : AState) (c : GroupCode) : List AState := closeFrom L c a [] a.save

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
    pinned := a.pinned && b.pinned,
    modes := a.modes ++ b.modes.filter (fun m => !(a.modes.contains m)) }

/-! ## 11. Transfer: macro expansion and argument scanning (§§ 389–399)

Intuition. A delimited argument is a cut of the input: the text before the
delimiter and the text after it (§ 392). Over abstract text a cut falls
between two items or inside one, so the scan lists every cut whose right
side may begin with the delimiter. There are at most two cuts per item, so
the list is finite. A hole's pieces are holes of its class, and they are
balanced because TeX only matches a delimiter at brace depth 0.

Alternatives: matching inside holes precisely (needs contents a hole does
not have), or stopping at the first hole (loses every macro call whose
argument holds unknown text). The Rust prunes the cuts further: by brace
depth, and by whether an item may hold a delimiter token (`hides_delimiter`).
Arguments are read from the top token-list level. -/

/-- What is left of a hole after a scan took a part of it. -/
def Hole.part (h : Hole) : Hole := { h with nonempty := false }

def prefixes (p : TokList) : List TokList := (List.range (p.length + 1)).map p.take
def suffixes (p : TokList) : List TokList := (List.range (p.length + 1)).map p.drop

/-- The cuts inside an item: its left and right pieces. A known token has
none. Typed digits split into number text; a one-of splits each member. -/
def AItem.cuts : AItem → List (AItem × AItem)
  | .tok _ => []
  | .hole h => [(.hole h.part, .hole h.part)]
  | .digits _ => [(.hole { cls := TClass.number }, .hole { cls := TClass.number })]
  | .oneOf alts => [(.oneOf (alts.flatMap prefixes), .oneOf (alts.flatMap suffixes))]

/-- Every cut of an abstract text: before an item, or inside one. -/
def AText.cuts : AText → List (AText × AText)
  | [] => [([], [])]
  | it :: xs => ([], it :: xs) :: (it.cuts.map (fun c => ([c.1], c.2 :: xs)) ++
      (AText.cuts xs).map (fun c => (it :: c.1, c.2)))

/-- Known tokens `δ` at the head of an abstract text: the rests (§ 397, the
leading and the delimiting tokens of a parameter text). A known token must
match. Another item may end `δ` inside it, or hold a part of `δ` whole. -/
def dropAbs : TokList → AText → List AText
  | [], x => [x]
  | _ :: _, [] => []
  | d :: ds, .tok t :: x => if t = d then dropAbs ds x else []
  | d :: ds, it :: x =>
    it.cuts.map (fun c => c.2 :: x) ++
      (List.range (ds.length + 2)).flatMap (fun i => dropAbs ((d :: ds).drop i) x)

/-- A delimited argument (§ 392): the pairs (argument, rest). -/
def scanDelim (δ : TokList) (x : AText) : List (AText × AText) :=
  (AText.cuts x).flatMap (fun c => (dropAbs δ c.2).map (fun r => (c.1, r)))

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
      (if h.nonempty = false || h.cls.sp || h.cls.c then scanUndelim rest else [])
  | AItem.digits _ :: rest =>
    [([AItem.hole { cls := TClass.number, nonempty := true, maxLen := some 1 }],
      AItem.hole { cls := TClass.digits } :: rest)]
  | AItem.oneOf alts :: rest =>
    ([AItem.hole { cls := TClass.any }], AItem.oneOf (alts.flatMap suffixes) :: rest) ::
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

/-- § 400: a delimited argument that is one `{…}` group loses its braces.
Either reading: the argument, or its tokens as one hole. -/
def stripBraces (xa : AText) : List AText := [xa, AText.hull [xa]]

def stripAll (ps : List (AText × AText)) : List (AText × AText) :=
  ps.foldr (fun (p : AText × AText) (acc : List (AText × AText)) =>
    (stripBraces p.1).map (fun (x : AText) => (x, p.2)) ++ acc) []

def scanArgs : List TokList → AText → List (List AText × AText)
  | [], rest => [([], rest)]
  | δ :: ds, rest =>
    let firsts := if δ.isEmpty then scanUndelim rest else stripAll (scanDelim δ rest)
    firsts.foldr (fun (p : AText × AText) (acc : List (List AText × AText)) =>
      (scanArgs ds p.2).map (fun (q : List AText × AText) => (p.1 :: q.1, q.2)) ++ acc) []

/-- All the ways a macro's parameter text may match the abstract input. -/
def scanParams (ps : List ParamItem) (rest : AText) : List (List AText × AText) :=
  let sp := splitParams ps
  (dropAbs sp.1 rest).foldr (fun (r : AText) (acc : List (List AText × AText)) =>
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

Intuition. A loop runs `k` joining passes. A head still growing is widened,
and one still growing after that is summarized: every location the body may
write holds anything. This forces convergence in a bounded number of rounds
whatever the precision (`loopRounds_le`). Iterating to a precise fixpoint
would need a finite-height lattice, which token lists are not.

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
must cover every pass, not only the passes seen) holds anything, at any
level, and may have been saved in the current frame (`touched`). -/
def summaryHavoc (writes : List Loc) (a : AState) : AState :=
  { a with eqtb := fun l => if writes.contains l then .top else a.eqtb l,
           level := fun l => if writes.contains l then none else a.level l,
           save := writes.map ASave.touched ++ a.save }

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
