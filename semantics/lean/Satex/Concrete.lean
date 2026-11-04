/-
Satex.Concrete: the concrete semantics of the TeX SaTeX interprets.

`Step E s s'` is one step of TeX's main loop: one expansion in the gullet
(§ 366 `expand`), or one command of the stomach (§ 1030 `main_control`),
including the operands that command scans. Scanners are big-step relations
over the token stream (§§ 402–486); the main loop is small-step.

Rules are exact unless their comment says **[over]**: then the rule admits
every outcome the engine may produce where SaTeX models only an effect
(typesetting, PDF, Lua, the clock), so it over-approximates. A situation
with no rule (marked OPEN) is a gap of this formalisation, never a claim.

`E : Engine` holds the facts that come from outside the token stream: file
resolution, font metrics, the operand grammar of scan-only primitives.
Every primitive name of src/engine/builtins.rs is mapped to its rule in
semantics/primitives.tsv.
-/
import Satex.Core

namespace Satex
namespace Concrete

/-! ## Engine facts -/

structure Engine where
  /-- XeTeX and LuaTeX read characters, the 8-bit engines bytes (SEM § 3). -/
  unicode : Bool := false
  /-- kpathsea's lookup for `\input`, `\openin`: the key of the found file
  in `State.vfs` (§ 537, TeX Live tries `name.tex` first). -/
  findFile : String → Option String
  /-- TFM or native metrics of a font file at a size: its parameters. -/
  loadFont : String → Scaled → Option (Nat → Scaled)
  /-- Whether a loaded font has a character (`\iffontchar`, etex_man § 3.10). -/
  hasChar : Nat → CharCode → Bool
  /-- Operand grammar of a primitive SaTeX scans but does not run. -/
  opsOf : String → List Operand
  /-- Constants the engine reports (`\pdftexversion`, `\shellescape`, …). -/
  constant : String → Int
  /-- The primitive a name had in INITEX (`\pdfprimitive`, `\ifpdfprimitive`). -/
  primitive : String → Option Cmd
  /-- The largest register number (255 TeX, 32767 eTeX, 65535 LuaTeX). -/
  maxReg : Nat := 32767

def nth? {α : Type} : List α → Nat → Option α
  | [], _ => none
  | x :: _, 0 => some x
  | _ :: xs, n + 1 => nth? xs n

def indexOf? (k : String) : List String → Option Nat
  | [] => none
  | x :: xs => if x = k then some 0 else (indexOf? k xs).map (· + 1)

def updAt {α : Type} : List α → Nat → (α → α) → List α
  | [], _, _ => []
  | x :: xs, 0, f => f x :: xs
  | x :: xs, n + 1, f => x :: updAt xs n f

/-! ## Arithmetic (§§ 100–108, SEM § 11) -/

/-- Division truncating toward zero (§ 106 `x_over_n`); `b = 0` gives 0. -/
def tdiv (a b : Int) : Int :=
  let q : Int := Int.ofNat (a.natAbs / b.natAbs)
  if b = 0 then 0 else if (a < 0 ∧ b < 0) ∨ (0 ≤ a ∧ 0 < b) then q else -q

/-- Division rounding to nearest, ties away from zero (etex.ch `quotient`). -/
def qdiv (a b : Int) : Int :=
  let n := a.natAbs
  let d := b.natAbs
  let q : Int := Int.ofNat ((2 * n + d) / (2 * d))
  if b = 0 then 0 else if (a < 0 ∧ b < 0) ∨ (0 ≤ a ∧ 0 < b) then q else -q

/-- 32-bit wrap-around of `\advance` (§ 1238, SEM § 11: no overflow check). -/
def wrap32 (x : Int) : Int :=
  let m : Int := 4294967296
  let r := x % m
  let r := if r < 0 then r + m else r
  if r ≥ 2147483648 then r - m else r

/-- `round_decimals` (§ 102): at most 17 digits of a fraction to `sp`. -/
def roundDecimals (ds : List Nat) : Nat :=
  let a := (ds.take 17).foldr (fun d a => (a + d * 131072) / 10) 0
  (a + 1) / 2

/-- `xn_over_d` with the fraction carried (§ 458): `(i + f/2^16)·num/den` as
integer and fraction parts. -/
def scaleUnit (i f num den : Nat) : Nat × Nat :=
  let v := i * num / den
  let r := i * num % den
  let f' := (num * f + 65536 * r) / den
  (v + f' / 65536, f' % 65536)

/-- `attach_fraction` (§ 453): `none` is "Dimension too large". -/
def attachFraction (i f : Nat) : Option Int :=
  if i ≥ 16384 then none else some (Int.ofNat (i * 65536 + f))

/-- Physical units, as `num/den` of a point (§ 458, SEM § 11). `sp` is
handled apart (§ 459). -/
def unitRatio : String → Option (Nat × Nat)
  | "pt" => some (1, 1)     | "in" => some (7227, 100) | "pc" => some (12, 1)
  | "cm" => some (7227, 254) | "mm" => some (7227, 2540) | "bp" => some (7227, 7200)
  | "dd" => some (1238, 1157) | "cc" => some (14856, 1157)
  | "nd" => some (685, 642) | "nc" => some (1370, 107)
  | _ => none

def physicalUnits : List String := ["pt", "in", "pc", "cm", "mm", "bp", "dd", "cc", "nd", "nc"]

/-- `nx_plus_y` (§ 105) with bound `maxDimen`. -/
def nxPlusY (n x y : Int) : Option Int :=
  let r := n * x + y
  if r.natAbs ≤ maxDimen.natAbs then some r else none

/-! ## Printing (§§ 59–70, 103, 178, 262, 294–298) -/

def digitsOf (n : Int) : List CharCode :=
  (toString n).toList.map Char.toNat

/-- `print_scaled` (§ 103). -/
def printScaled (x : Int) : List CharCode :=
  let sign : List CharCode := if x < 0 then [45] else []
  let a := x.natAbs
  let ip := digitsOf (Int.ofNat (a / 65536))
  let rec frac (fuel s delta : Nat) : List CharCode :=
    match fuel with
    | 0 => []
    | fuel + 1 =>
      let s := if delta > 65536 then s + 32768 - 50000 else s
      let d := 48 + s / 65536
      let s' := 10 * (s % 65536)
      let delta' := delta * 10
      if s' ≤ delta' then [d] else d :: frac fuel s' delta'
  sign ++ ip ++ [46] ++ frac 12 (10 * (a % 65536) + 5) 10

def strCodes (s : String) : List CharCode := s.toList.map Char.toNat

/-- `print_glue` and `print_spec` (§§ 177–178). -/
def printGlue (v : Int) (order : Nat) (unit : String) : List CharCode :=
  printScaled v ++
    (match order with
     | 0 => strCodes unit
     | 1 => strCodes "fil"
     | 2 => strCodes "fill"
     | _ => strCodes "filll")

def printSpec (g : Glue) (unit : String) : List CharCode :=
  printScaled g.width ++ strCodes unit ++
    (if g.stretch ≠ 0 then strCodes " plus " ++ printGlue g.stretch g.stretchOrder unit else []) ++
    (if g.shrink ≠ 0 then strCodes " minus " ++ printGlue g.shrink g.shrinkOrder unit else [])

/-- `print_roman_int` (§ 69). -/
def romanOf (n : Int) : List CharCode :=
  let table : List (Nat × String) :=
    [(1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"),
     (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")]
  let rec go (fuel n : Nat) (t : List (Nat × String)) : List CharCode :=
    match fuel, t with
    | 0, _ => []
    | _, [] => []
    | fuel + 1, (v, r) :: rest =>
      if n ≥ v then strCodes r ++ go fuel (n - v) ((v, r) :: rest) else go fuel n rest
  if n ≤ 0 then [] else go (n.toNat + 13) n.toNat table

/-- The escape character as printed (§ 63 `print_esc`): none outside 0–255. -/
def escapeChars (s : State) : List CharCode :=
  let e := s.intPar "escapechar"
  if 0 ≤ e ∧ e < 256 then [e.toNat] else []

def nameCodes (r : CsRef) : List CharCode :=
  match r with
  | .name n => strCodes n
  | .active c => [c]

/-- `print_cs` (§ 262): a space after a control word or a one-letter name
whose character is a letter now (SEM § 8). -/
def printCs (s : State) (r : CsRef) : List CharCode :=
  match r with
  | .active c => [c]
  | .name n =>
    let cs := strCodes n
    match cs with
    | [] => escapeChars s ++ strCodes "csname" ++ escapeChars s ++ strCodes "endcsname "
    | [c] => escapeChars s ++ [c] ++ (if s.catOf c = .letter then [32] else [])
    | _ => escapeChars s ++ cs ++ [32]

/-- `sprint_cs` (§ 263): no trailing space (`\string`). -/
def sprintCs (s : State) (r : CsRef) : List CharCode :=
  match r with
  | .active c => [c]
  | .name n => escapeChars s ++ strCodes n

/-- `show_token_list` (§ 292–294): macro parameter characters print doubled,
`#n` as the parameter character and `n`. -/
def showToks (s : State) (ts : TokList) : List CharCode :=
  ts.foldr (fun t acc =>
    (match t with
     | .char c .param => [c, c]
     | .char c _ => [c]
     | .cs r => printCs s r
     | .noexp r => printCs s r
     | .outParam n => [35] ++ digitsOf (Int.ofNat n)) ++ acc) []

/-- `str_toks` (§ 464): characters as category 12, the space as 10. -/
def strToks (cs : List CharCode) : TokList :=
  cs.map fun c => if c = 32 then Token.space else .char c .other

def showParams (s : State) (m : Macro) : List CharCode :=
  let rec go (k : Nat) : List ParamItem → List CharCode
    | [] => []
    | .arg :: rest => [35] ++ digitsOf (Int.ofNat k) ++ go (k + 1) rest
    | .tok t :: rest => showToks s [t] ++ go k rest
  go 1 m.params ++ (if m.braceDelim then [123] else [])

def catWord : Cat → String
  | .bgroup => "begin-group character " | .egroup => "end-group character "
  | .mathShift => "math shift character " | .alignTab => "alignment tab character "
  | .param => "macro parameter character " | .sup => "superscript character "
  | .sub => "subscript character " | .space => "blank space "
  | .letter => "the letter " | .other => "the character "
  | _ => "the character "

def hexOf (n : Nat) : List CharCode :=
  [34] ++ (Nat.toDigits 16 n).map (fun c => (Char.toUpper c).toNat)

/-- `print_meaning` (§ 296) through `print_cmd_chr` (§ 227). -/
def meaningText (s : State) (m : Meaning) : List CharCode :=
  match m with
  | .undefined => strCodes "undefined"
  | .prim n _ => escapeChars s ++ strCodes n
  | .char c k => strCodes (catWord k) ++ [c]
  | .chardef n => escapeChars s ++ strCodes "char" ++ hexOf n
  | .mathchardef n => escapeChars s ++ strCodes "mathchar" ++ hexOf n
  | .reg k i =>
    let kn := match k with
      | .count => "count" | .dimen => "dimen" | .skip => "skip"
      | .muskip => "muskip" | .toks => "toks" | .box => "box"
    escapeChars s ++ strCodes kn ++ digitsOf (Int.ofNat i)
  | .font f => strCodes "select font " ++
      strCodes ((nth? s.fonts f).map Prod.fst |>.getD "nullfont")
  | .luaCall id _ => escapeChars s ++ strCodes "luacall" ++ digitsOf (Int.ofNat id)
  | .call mac =>
    (if mac.prot then escapeChars s ++ strCodes "protected " else []) ++
    (if mac.long then escapeChars s ++ strCodes "long " else []) ++
    (if mac.outer then escapeChars s ++ strCodes "outer " else []) ++
    strCodes "macro:" ++ showParams s mac ++ strCodes "->" ++ showToks s mac.body

/-! ## Meanings of tokens -/

/-- The meaning `\relax` gets behind `\noexpand` (§ 358: `relax` with
`no_expand_flag`), a primitive identity of its own. -/
def notExpanded : Meaning := .prim "notexpanded:" .relax

/-- `frozen_relax` (§ 222, § 379). WellFormed states map it to `\relax`. -/
def frozenRelax : Token := .cs (.name "frozen\\relax")

/-- The `\par` token as the mouth inserts it (§ 333 `par_token`). -/
def parToken : Token := .cs (.name "par")

def meaningTok (s : State) : Token → Meaning
  | .char c k => .char c k
  | .cs r => s.meaningOf r
  | .noexp _ => notExpanded
  | .outParam _ => .undefined

/-- A token as stored: `\noexpand`'s marker is gone once the token is read
(§ 358: `cur_tok` is the plain token). -/
def _root_.Satex.Token.stored : Token → Token
  | .noexp r => .cs r
  | t => t

def cmdOf (s : State) (t : Token) : Option Cmd :=
  match meaningTok s t with
  | .prim _ c => some c
  | _ => none

/-- Gullet commands (§ 366: `cur_cmd > max_command`). -/
def _root_.Satex.Cmd.expandable : Cmd → Bool
  | .expandafter | .noexpand | .csname | .begincsname | .lastnamedcs
  | .convert _ | .the | .unexpanded | .expanded | .scantokens _
  | .ifTest _ | .fiOrElse _ | .unlessCmd | .input | .endinput
  | .topBotMark _ _ | .primitiveCmd => true
  | .lua n => n == "directlua" || n == "luaescapestring"   -- LuaTeX manual, "Lua related primitives"
  | _ => false

def _root_.Satex.Meaning.expandable : Meaning → Bool
  | .undefined => true          -- `undefined_cs` > max_command (§ 210, § 370)
  | .call _ => true
  | .prim _ c => c.expandable
  | .luaCall _ p => !p
  | _ => false

def expandableTok (s : State) (t : Token) : Bool := (meaningTok s t).expandable

/-- `\ifx` equality of meanings (§ 507–508). -/
def ifxEq : Meaning → Meaning → Bool
  | .undefined, .undefined => true
  | .prim a _, .prim b _ => a == b
  | .call m, .call n => decide (m = n)
  | .char c k, .char d l => decide (c = d ∧ k = l)
  | .chardef a, .chardef b => a == b
  | .mathchardef a, .mathchardef b => a == b
  | .reg k i, .reg l j => decide (k = l ∧ i = j)
  | .font a, .font b => a == b
  | .luaCall a p, .luaCall b q => decide (a = b ∧ p = q)
  | _, _ => false

/-- `\if`/`\ifcat` operands (§ 506): character code and category, with every
control sequence that is no character as code 256 and category 16. A
noexpanded active character counts as itself. -/
def charCat (s : State) : Token → Nat × Nat
  | .noexp (.active c) => (c, 13)
  | t => match meaningTok s t with
    | .char c k => (c, k.toNat)
    | _ => (256, 16)

/-! ## Eqtb assignment and grouping (§§ 274–284, SEM § 10) -/

/-- `eq_define`/`eq_word_define` (§§ 277–279): save the old entry when it
was set at another level and a group is open. -/
def eqDefine (s : State) (l : Loc) (v : Val) : State :=
  let e := s.eqtb l
  let save := if e.level ≠ s.curLevel ∧ s.curLevel > 1 then SaveEntry.restore l e :: s.save else s.save
  { s with eqtb := upd s.eqtb l { val := v, level := s.curLevel }, save := save }

/-- `geq_define` (§ 279): level one, no save entry. -/
def geqDefine (s : State) (l : Loc) (v : Val) : State :=
  { s with eqtb := upd s.eqtb l { val := v, level := 1 } }

/-- `\globaldefs` decides (§ 1214). -/
def isGlobal (s : State) (glob : Bool) : Bool :=
  let g := s.intPar "globaldefs"
  if g > 0 then true else if g < 0 then false else glob

def define (s : State) (glob : Bool) (l : Loc) (v : Val) : State :=
  if isGlobal s glob then geqDefine s l v else eqDefine s l v

/-- `unsave` (§§ 281–283): restore entries down to the group boundary; an
entry set globally since (level one) is retained. `\aftergroup` tokens come
back in the order they were given. -/
def unsaveAux (eqtb : Loc → Entry) (ins : TokList) :
    List SaveEntry → (Loc → Entry) × TokList × Option (GroupCode × Option BoxContext) × List SaveEntry
  | [] => (eqtb, ins, none, [])
  | .group c ctx _ :: rest => (eqtb, ins, some (c, ctx), rest)
  | .restore l old :: rest =>
    unsaveAux (if (eqtb l).level = 1 then eqtb else upd eqtb l old) ins rest
  | .retain _ :: rest => unsaveAux eqtb ins rest
  | .insertToken t :: rest => unsaveAux eqtb (t :: ins) rest

def pushToks (k : TokListKind) (ts : TokList) (s : State) : State :=
  { s with input := .toks k ts :: s.input }

/-- `back_input` (§ 325). -/
def backInput (t : Token) (s : State) : State := pushToks .backedUp [t.stored] s

def backList (ts : TokList) (s : State) : State := pushToks .backedUp (ts.map Token.stored) s

/-- `new_save_level` (§ 274). -/
def newSaveLevel (c : GroupCode) (ctx : Option BoxContext) (s : State) : State :=
  { s with save := .group c ctx 0 :: s.save, curLevel := s.curLevel + 1 }

/-- `unsave` as a state change; the group code and context it closed. -/
def unsave (s : State) : State × Option (GroupCode × Option BoxContext) :=
  let (eqtb, ins, g, rest) := unsaveAux s.eqtb [] s.save
  let s' := { s with eqtb := eqtb, save := rest, curLevel := s.curLevel - 1 }
  (if ins = [] then s' else pushToks .backedUp ins s', g)

def topGroup (s : State) : GroupCode :=
  let rec go : List SaveEntry → GroupCode
    | [] => .bottom
    | .group c _ _ :: _ => c
    | _ :: rest => go rest
  go s.save

def logOut (o : Output) (s : State) : State := { s with out := o :: s.out }

/-- `error` (§ 82): the hundredth in one paragraph ends the run. -/
def raise (msg : String) (s : State) : State :=
  let s := logOut (.error (strCodes msg)) s
  if s.errorCount + 1 ≥ 100 then { s with fatal := true } else { s with errorCount := s.errorCount + 1 }

def fatalError (msg : String) (s : State) : State :=
  { logOut (.error (strCodes msg)) s with fatal := true }

def pushNest (m : Mode) (s : State) : State := { s with nest := { mode := m } :: s.nest }

def popNest (s : State) : State := { s with nest := s.nest.drop 1 }

def appendItem (i : Item) (s : State) : State :=
  match s.nest with
  | l :: rest => { s with nest := { l with items := i :: l.items } :: rest }
  | [] => s

def tailItem (s : State) : Option Item :=
  match s.nest with
  | l :: _ => l.items.head?
  | [] => none

def setMode (m : Mode) (s : State) : State :=
  match s.nest with
  | l :: rest => { s with nest := { l with mode := m } :: rest }
  | [] => s

/-- `begin_token_list` for an `\every…` parameter (§ 323). -/
def insertEvery (n : String) (s : State) : State :=
  let ts := s.toksAt (.toksPar n)
  if ts = [] then s else pushToks (.every n) ts s

/-! ## The mouth (§§ 343–365) -/

def isHexDigit (c : CharCode) : Bool := (48 ≤ c ∧ c ≤ 57) || (97 ≤ c ∧ c ≤ 102)
def hexVal (c : CharCode) : Nat := if c ≤ 57 then c - 48 else c - 87

/-- `^^` reduction (§ 352): `cs` starts after the first superscript
character `c`. -/
def reduceHat (c : CharCode) : List CharCode → Option (CharCode × List CharCode)
  | c2 :: c3 :: rest =>
    if c2 = c ∧ c3 < 128 then
      match rest with
      | c4 :: rest' =>
        if isHexDigit c3 ∧ isHexDigit c4 then some (16 * hexVal c3 + hexVal c4, rest')
        else some (if c3 < 64 then c3 + 64 else c3 - 64, rest)
      | [] => some (if c3 < 64 then c3 + 64 else c3 - 64, rest)
    else none
  | _ => none

/-- Letters of a control word, `^^` reduced inside (§ 355–356). -/
def scanLetters (cat : CharCode → Cat) : Nat → List CharCode → List CharCode × List CharCode
  | 0, cs => ([], cs)
  | fuel + 1, c :: rest =>
    if cat c = .letter then
      let (ls, r) := scanLetters cat fuel rest
      (c :: ls, r)
    else if cat c = .sup then
      match reduceHat c (c :: rest) with
      | some (c', rest') =>
        if cat c' = .letter then scanLetters cat fuel (c' :: rest') else ([], c :: rest)
      | none => ([], c :: rest)
    else ([], c :: rest)
  | _, [] => ([], [])

def nameOf (cs : List CharCode) : String := String.mk (cs.map Char.ofNat)

/-- Reading one token from the rest of a line (§§ 343–356). `none` when the
line is used up. Catcodes are those in force when each character is read. -/
inductive LineTok (cat : CharCode → Cat) :
    MouthState → List CharCode → Option Token → MouthState → List CharCode → Prop
  | done {st : MouthState} : LineTok cat st [] none st []
  | hat {st st' : MouthState} {c c' : CharCode} {rest rest' rest'' : List CharCode} {r : Option Token} :
      cat c = .sup → reduceHat c (c :: rest) = some (c', rest') →
      LineTok cat st (c' :: rest') r st' rest'' →
      LineTok cat st (c :: rest) r st' rest''
  | escWord {st : MouthState} {c c1 : CharCode} {rest : List CharCode} :
      cat c = .escape → cat c1 = .letter →
      LineTok cat st (c :: c1 :: rest)
        (some (.cs (.name (nameOf (scanLetters cat (rest.length + 1) (c1 :: rest)).1))))
        .skipBlanks (scanLetters cat (rest.length + 1) (c1 :: rest)).2
  | escSymbol {st : MouthState} {c c1 : CharCode} {rest : List CharCode} :
      cat c = .escape → cat c1 ≠ .letter →
      (cat c1 = .sup → reduceHat c1 (c1 :: rest) = none) →
      LineTok cat st (c :: c1 :: rest) (some (.cs (.name (nameOf [c1]))))
        (if cat c1 = .space then .skipBlanks else .midLine) rest
  | escHat {st : MouthState} {c c1 c' : CharCode} {rest rest' : List CharCode} {r : Option Token}
      {st' : MouthState} {rest'' : List CharCode} :
      cat c = .escape → cat c1 = .sup → reduceHat c1 (c1 :: rest) = some (c', rest') →
      LineTok cat st (c :: c' :: rest') r st' rest'' →
      LineTok cat st (c :: c1 :: rest) r st' rest''
  | escEmpty {st : MouthState} {c : CharCode} :
      cat c = .escape → LineTok cat st [c] (some (.cs (.name ""))) .skipBlanks []
  | active {st : MouthState} {c : CharCode} {rest : List CharCode} :
      cat c = .active → LineTok cat st (c :: rest) (some (.cs (.active c))) .midLine rest
  | plain {st : MouthState} {c : CharCode} {rest : List CharCode} :
      (cat c).tokenable = true → cat c ≠ .space → cat c ≠ .sup ∨ reduceHat c (c :: rest) = none →
      LineTok cat st (c :: rest) (some (.char c (cat c))) .midLine rest
  | spaceMid {c : CharCode} {rest : List CharCode} :
      cat c = .space → LineTok cat .midLine (c :: rest) (some Token.space) .skipBlanks rest
  | spaceSkip {st st' : MouthState} {c : CharCode} {rest rest' : List CharCode} {r : Option Token} :
      cat c = .space → st ≠ .midLine →
      LineTok cat st rest r st' rest' → LineTok cat st (c :: rest) r st' rest'
  | eolPar {c : CharCode} {rest : List CharCode} :
      cat c = .eol → LineTok cat .newLine (c :: rest) (some parToken) .newLine []
  | eolSpace {c : CharCode} {rest : List CharCode} :
      cat c = .eol → LineTok cat .midLine (c :: rest) (some Token.space) .newLine []
  | eolSkip {c : CharCode} {rest : List CharCode} :
      cat c = .eol → LineTok cat .skipBlanks (c :: rest) none .newLine []
  | ignored {st st' : MouthState} {c : CharCode} {rest rest' : List CharCode} {r : Option Token} :
      cat c = .ignored → LineTok cat st rest r st' rest' → LineTok cat st (c :: rest) r st' rest'
  | comment {st : MouthState} {c : CharCode} {rest : List CharCode} :
      cat c = .comment → LineTok cat st (c :: rest) none .newLine []
  /-- "Text line contains an invalid character" (§ 346): dropped. The
  error is not logged here. -/
  | invalid {st st' : MouthState} {c : CharCode} {rest rest' : List CharCode} {r : Option Token} :
      cat c = .invalid → LineTok cat st rest r st' rest' → LineTok cat st (c :: rest) r st' rest'

/-- A line as TeX stores it (§ 31, § 362): trailing spaces dropped, the
`\endlinechar` in force now appended when it is in 0–255. -/
def prepareLine (s : State) (line : List CharCode) : List CharCode :=
  let trimmed := (line.reverse.dropWhile (· == 32)).reverse
  let e := s.intPar "endlinechar"
  if 0 ≤ e ∧ e < 256 then trimmed ++ [e.toNat] else trimmed

def replaceTop (lv : InputLevel) (s : State) : State :=
  { s with input := lv :: s.input.drop 1 }

/-- `get_next` (§§ 341–365): the next token, unexpanded. -/
inductive Next : State → Token → State → Prop
  /-- From a token list (§ 357). -/
  | tok {s : State} {k : TokListKind} {t : Token} {ts : TokList} {rest : List InputLevel} :
      s.input = .toks k (t :: ts) :: rest →
      Next s t { s with input := .toks k ts :: rest }
  /-- An exhausted token list is popped (§ 324 `end_token_list`). -/
  | tokEnd {s s' : State} {k : TokListKind} {rest : List InputLevel} {t : Token} :
      s.input = .toks k [] :: rest → Next { s with input := rest } t s' → Next s t s'
  /-- A token from the current line of a file (§ 343). -/
  | fileTok {s : State} {f : FileLevel} {rest : List InputLevel} {t : Token}
      {st : MouthState} {cur : List CharCode} :
      s.input = .file f :: rest →
      LineTok s.catOf f.state f.cur (some t) st cur →
      Next s t { s with input := .file { f with state := st, cur := cur } :: rest }
  /-- The line is used up: read the next one (§ 360–362). -/
  | nextLine {s s' : State} {f : FileLevel} {rest : List InputLevel} {t : Token}
      {st : MouthState} {line : List CharCode} {lines : List (List CharCode)} :
      s.input = .file f :: rest →
      LineTok s.catOf f.state f.cur none st [] →
      f.lines = line :: lines → f.forceEof = false →
      Next { s with input := .file { f with cur := prepareLine s line, lines := lines,
                                              state := .newLine, line := f.line + 1 } :: rest } t s' →
      Next s t s'
  /-- End of file (§ 362): the file level goes, `\everyeof` comes (etex.ch),
  except after `\scantextokens`. -/
  | endFile {s s' : State} {f : FileLevel} {rest : List InputLevel} {t : Token} {st : MouthState} :
      s.input = .file f :: rest →
      LineTok s.catOf f.state f.cur none st [] →
      (f.lines = [] ∨ f.forceEof = true) →
      Next (let s1 := { s with input := rest }
            if f.text then s1 else insertEvery "everyeof" s1) t s' →
      Next s t s'

/-- Reading a given token sequence with `get_token` (§ 365). -/
inductive ReadToks : State → TokList → State → Prop
  | nil {s : State} : ReadToks s [] s
  | cons {s s1 s2 : State} {t : Token} {ts : TokList} :
      Next s t s1 → ReadToks s1 ts s2 → ReadToks s (t.stored :: ts) s2

/-! ## Token-list shapes for argument matching (§§ 389–400) -/

/-- Brace depth never below zero and zero at the end. -/
def balancedFrom : Nat → TokList → Bool
  | d, [] => d == 0
  | d, t :: ts =>
    if t.isBgroup then balancedFrom (d + 1) ts
    else if t.isEgroup then (if d = 0 then false else balancedFrom (d - 1) ts)
    else balancedFrom d ts

def balanced (ts : TokList) : Bool := balancedFrom 0 ts

/-- The argument is the shortest balanced text after which the delimiter
follows at brace level zero (§ 397). -/
def firstDelim (ts delim : TokList) : Prop :=
  balanced ts = true ∧
  ∀ k, k < ts.length → ¬ (balanced (ts.take k) = true ∧ delim.isPrefixOf (ts.drop k ++ delim) = true)

/-- § 400: one pair of braces around the whole argument goes. -/
def stripBraces (ts : TokList) : TokList :=
  match ts with
  | b :: rest =>
    match rest.getLast? with
    | some e => if b.isBgroup && e.isEgroup && balanced rest.dropLast then rest.dropLast else ts
    | none => ts
  | [] => ts

/-- The parameter text splits into leading delimiters, then per argument the
delimiter tokens that follow it. -/
def leadingToks : List ParamItem → TokList × List ParamItem
  | .tok t :: rest => let (ts, r) := leadingToks rest; (t :: ts, r)
  | ps => ([], ps)

/-- `\par` in an argument of a non-long macro (§ 396 compares `par_token`). -/
def noParUnlessLong (long : Bool) (ts : TokList) : Prop := long = true ∨ parToken ∉ ts

/-- Substitution of arguments into the replacement text (§ 390). -/
def subst (args : List TokList) : TokList → TokList
  | [] => []
  | .outParam n :: rest => (args.getD (n - 1) []) ++ subst args rest
  | t :: rest => t :: subst args rest

/-- Matching the parameter text (§§ 391–399). Argument reading uses
`get_token`, never expansion. -/
inductive MatchParams (long : Bool) : State → List ParamItem → List TokList → State → Prop
  | nil {s : State} : MatchParams long s [] [] s
  /-- A delimiter token of the parameter text must come literally (§ 397). -/
  | literal {s s1 s2 : State} {t : Token} {ps : List ParamItem} {args : List TokList} :
      Next s t s1 → MatchParams long s1 ps args s2 →
      MatchParams long s (.tok t.stored :: ps) args s2
  /-- Undelimited, one token: spaces are skipped first (§ 393). -/
  | undelimTok {s s0 s1 s2 : State} {sp : TokList} {t : Token} {ps : List ParamItem} {args : List TokList} :
      (∀ x ∈ sp, x = Token.space) → ReadToks s sp s0 →
      Next s0 t s1 → t.stored ≠ Token.space → t.isBgroup = false → t.isEgroup = false →
      (leadingToks ps).1 = [] →
      noParUnlessLong long [t.stored] →
      MatchParams long s1 ps args s2 →
      MatchParams long s (.arg :: ps) ([t.stored] :: args) s2
  /-- Undelimited, a group: its braces go (§ 392, § 399). -/
  | undelimGroup {s s0 s1 s2 : State} {sp body : TokList} {b e : Token}
      {ps : List ParamItem} {args : List TokList} :
      (∀ x ∈ sp, x = Token.space) → ReadToks s sp s0 →
      b.isBgroup = true → e.isEgroup = true → balanced body = true →
      ReadToks s0 (b :: body ++ [e]) s1 → (leadingToks ps).1 = [] →
      noParUnlessLong long body →
      MatchParams long s1 ps args s2 →
      MatchParams long s (.arg :: ps) (body :: args) s2
  /-- Delimited: the shortest balanced text before the delimiter (§ 397). -/
  | delimited {s s1 s2 : State} {ts : TokList} {ps : List ParamItem} {args : List TokList} :
      (leadingToks ps).1 ≠ [] →
      ReadToks s (ts ++ (leadingToks ps).1) s1 → firstDelim ts (leadingToks ps).1 →
      noParUnlessLong long ts →
      MatchParams long s1 (leadingToks ps).2 args s2 →
      MatchParams long s (.arg :: ps) (stripBraces ts :: args) s2

/-! ## Conditional skipping (§§ 494–500) -/

def _root_.Satex.FiCode.toNat : FiCode → Nat | .fi => 2 | .elseCode => 3 | .orCode => 4
def _root_.Satex.IfLimit.toNat : IfLimit → Nat | .ifCode => 1 | .fiCode => 2 | .elseCode => 3 | .orCode => 4

def isIfTest : Option Cmd → Bool | some (.ifTest _) => true | _ => false
def fiCodeOf : Option Cmd → Option FiCode | some (.fiOrElse c) => some c | _ => none

/-- `pass_text` (§ 494): skip without expansion to the `\fi`, `\else` or
`\or` of nesting level zero, nesting by meaning (SEM § 9). -/
inductive PassText : State → Nat → FiCode → State → Prop
  | stop {s s1 : State} {t : Token} {c : FiCode} :
      Next s t s1 → fiCodeOf (cmdOf s1 t) = some c → PassText s 0 c s1
  | close {s s1 s2 : State} {t : Token} {l : Nat} {c : FiCode} :
      Next s t s1 → fiCodeOf (cmdOf s1 t) = some .fi → PassText s1 l c s2 → PassText s (l + 1) c s2
  | innerElse {s s1 s2 : State} {t : Token} {l : Nat} {c c' : FiCode} :
      Next s t s1 → fiCodeOf (cmdOf s1 t) = some c' → c' ≠ .fi →
      PassText s1 (l + 1) c s2 → PassText s (l + 1) c s2
  | openIf {s s1 s2 : State} {t : Token} {l : Nat} {c : FiCode} :
      Next s t s1 → isIfTest (cmdOf s1 t) = true → PassText s1 (l + 1) c s2 → PassText s l c s2
  | other {s s1 s2 : State} {t : Token} {l : Nat} {c : FiCode} :
      Next s t s1 → isIfTest (cmdOf s1 t) = false → fiCodeOf (cmdOf s1 t) = none →
      PassText s1 l c s2 → PassText s l c s2

/-- Set the limit of the conditional `depth` levels above the bottom. -/
def setLimitAt (cs : List CondLevel) (depth : Nat) (l : IfLimit) : List CondLevel :=
  let n := cs.length
  let rec go : Nat → List CondLevel → List CondLevel
    | _, [] => []
    | i, c :: rest => (if n - 1 - i = depth then { c with limit := l } else c) :: go (i + 1) rest
  go 0 cs

/-- After a false test (§ 500): skip to our `\else` or `\fi`. Conditionals
the test opened and left above ours are ended by the `\fi`s met. -/
inductive SkipFalse : State → Nat → State → Prop
  | toElse {s s1 : State} {d : Nat} :
      PassText s 0 .elseCode s1 → s1.cond.length = d + 1 →
      SkipFalse s d { s1 with cond := setLimitAt s1.cond d .fiCode }
  | toFi {s s1 : State} {d : Nat} :
      PassText s 0 .fi s1 → s1.cond.length = d + 1 →
      SkipFalse s d { s1 with cond := s1.cond.drop 1 }
  | extraOr {s s1 s2 : State} {d : Nat} :
      PassText s 0 .orCode s1 → s1.cond.length = d + 1 →
      SkipFalse (raise "Extra \\or" s1) d s2 → SkipFalse s d s2
  | innerFi {s s1 s2 : State} {d : Nat} :
      PassText s 0 .fi s1 → s1.cond.length > d + 1 →
      SkipFalse { s1 with cond := s1.cond.drop 1 } d s2 → SkipFalse s d s2
  | innerOther {s s1 s2 : State} {d : Nat} {c : FiCode} :
      PassText s 0 c s1 → c ≠ .fi → s1.cond.length > d + 1 →
      SkipFalse s1 d s2 → SkipFalse s d s2

/-- `\ifcase` (§ 509): pass `n` `\or`s of our level. -/
inductive SkipCase : State → Nat → Int → State → Prop
  | found {s : State} {d : Nat} : SkipCase s d 0 s
  | nextOr {s s1 s2 : State} {d : Nat} {n : Int} :
      n ≠ 0 → PassText s 0 .orCode s1 → s1.cond.length = d + 1 →
      SkipCase s1 d (n - 1) s2 → SkipCase s d n s2
  | toElse {s s1 : State} {d : Nat} {n : Int} :
      n ≠ 0 → PassText s 0 .elseCode s1 → s1.cond.length = d + 1 →
      SkipCase s d n { s1 with cond := setLimitAt s1.cond d .fiCode }
  | toFi {s s1 : State} {d : Nat} {n : Int} :
      n ≠ 0 → PassText s 0 .fi s1 → s1.cond.length = d + 1 →
      SkipCase s d n { s1 with cond := s1.cond.drop 1 }
  | innerFi {s s1 s2 : State} {d : Nat} {n : Int} :
      n ≠ 0 → PassText s 0 .fi s1 → s1.cond.length > d + 1 →
      SkipCase { s1 with cond := s1.cond.drop 1 } d n s2 → SkipCase s d n s2
  | innerOther {s s1 s2 : State} {d : Nat} {n : Int} {c : FiCode} :
      n ≠ 0 → PassText s 0 c s1 → c ≠ .fi → s1.cond.length > d + 1 →
      SkipCase s1 d n s2 → SkipCase s d n s2

/-- `\fi`, `\else`, `\or` met while their text is read (§ 510): skip to
`\fi` at level zero. -/
inductive SkipToFi : State → State → Prop
  | fi {s s1 : State} : PassText s 0 .fi s1 → SkipToFi s s1
  | more {s s1 s2 : State} {c : FiCode} : PassText s 0 c s1 → c ≠ .fi → SkipToFi s1 s2 → SkipToFi s s2

/-! ## Internal values -/

inductive IVal where
  | int (i : Int) | dimen (d : Scaled) | glue (g : Glue) | mu (g : Glue)
  | toks (t : TokList) | font (f : Nat)
  deriving Inhabited

def IVal.toInt : IVal → Int
  | .int i => i | .dimen d => d | .glue g => g.width | .mu g => g.width
  | _ => 0

def IVal.toGlue : IVal → Glue
  | .int i => { width := i } | .dimen d => { width := d }
  | .glue g => g | .mu g => g | _ => {}

def valOfLoc (k : RegKind) (v : Val) : IVal :=
  match k, v with
  | .count, .int i => .int i
  | .dimen, .int i => .dimen i
  | .skip, .glue g => .glue g
  | .muskip, .glue g => .mu g
  | .toks, .toks t => .toks t
  | .count, _ => .int 0
  | .dimen, _ => .dimen 0
  | .skip, _ => .glue {}
  | .muskip, _ => .mu {}
  | _, _ => .toks []

/-- The text `\the` gives (§ 465). Token lists stand as they are. -/
def theText (s : State) : IVal → TokList
  | .int i => strToks (digitsOf i)
  | .dimen d => strToks (printScaled d ++ strCodes "pt")
  | .glue g => strToks (printSpec g "pt")
  | .mu g => strToks (printSpec g "mu")
  | .toks t => t
  | .font f => [.cs (.name ("font" ++ toString f))]   -- `font_id_text`, see OPEN

/-- Codes the tables accept (§ 1232). -/
def codeMax (E : Engine) : CodeTable → Int
  | .cat => 15
  | .lc | .uc => if E.unicode then 1114111 else 255
  | .sf => 32767
  | .math => 32768
  | .del => 16777215

def charMax (E : Engine) : Nat := if E.unicode then 1114111 else 255

def boxDims : BoxVal → Scaled × Scaled × Scaled
  | .void => (0, 0, 0)
  | .hbox w h d => (w, h, d)
  | .vbox w h d => (w, h, d)

def setBoxDim (which : Nat) (v : Scaled) : BoxVal → BoxVal
  | .void => .void
  | .hbox w h d => if which = 0 then .hbox v h d else if which = 1 then .hbox w v d else .hbox w h v
  | .vbox w h d => if which = 0 then .vbox v h d else if which = 1 then .vbox w v d else .vbox w h v

def boxAt (s : State) (n : Nat) : BoxVal :=
  match s.get (.reg .box n) with
  | .box b => b
  | _ => .void

def lineNo (s : State) : Nat :=
  let rec go : List InputLevel → Nat
    | [] => 0
    | .file f :: _ => f.line
    | _ :: rest => go rest
  go s.input

/-- The innermost file level gets `\endinput`'s `force_eof` (§ 362). -/
def forceEofTop : List InputLevel → List InputLevel
  | [] => []
  | .file f :: rest => .file { f with forceEof := true } :: rest
  | l :: rest => l :: forceEofTop rest

def _root_.Satex.IfOp.code : IfOp → Int
  | .charIf => 0 | .catIf => 1 | .num => 2 | .dim => 3 | .odd => 4 | .vmode => 5
  | .hmode => 6 | .mmode => 7 | .inner => 8 | .void => 9 | .hboxIf => 10
  | .vboxIf => 11 | .ifx => 12 | .eof => 13 | .iftrue => 14 | .iffalse => 15
  | .ifcase => 16 | .defined => 17 | .csname => 18 | .fontchar => 19
  | .absnum => 20 | .absdim => 21 | .incsname => 22 | .primitive => 23

def itemCode : Option Item → Int
  | none => -1
  | some .char => 0 | some .ligature => 7 | some .box => 1 | some .rule => 3
  | some (.glue _) => 11 | some (.kern _) => 12 | some (.penalty _) => 13
  | some .mark => 5 | some .whatsit => 9 | some .math => 10 | some .disc => 8
  | some .other => 15

def relHolds (rel : Nat) (a b : Int) : Bool :=
  if rel = 60 then decide (a < b) else if rel = 62 then decide (a > b) else a == b

/-- Pieces of an expanded body (§ 477–478): `\the` and `\unexpanded` give
text that is stored as it stands. -/
inductive Piece where
  | tok (t : Token)
  | verbatim (ts : TokList)

def isDigitTok (radix : Nat) : Token → Option Nat
  | .char c .other =>
    if 48 ≤ c ∧ c ≤ 57 ∧ c - 48 < radix then some (c - 48)
    else if radix = 16 ∧ 65 ≤ c ∧ c ≤ 70 then some (c - 55) else none
  | .char c .letter => if radix = 16 ∧ 65 ≤ c ∧ c ≤ 70 then some (c - 55) else none
  | _ => none

def kwMatches (k : Char) : Token → Bool
  | .char c _ => c == k.toNat || c == (Char.toUpper k).toNat
  | _ => false

def otherChar (c : Char) : Token := .char c.toNat .other

/-- Lines of a `\scantokens` pseudo file (etex.ch `pseudo_start`). -/
def splitLines (nl : Int) (cs : List CharCode) : List (List CharCode) :=
  let rec go (cur : List CharCode) : List CharCode → List (List CharCode)
    | [] => [cur.reverse]
    | c :: rest => if Int.ofNat c = nl then cur.reverse :: go [] rest else go (c :: cur) rest
  go [] cs


def fontParamOf (s : State) (f : Nat) (n : Nat) : Scaled :=
  match nth? s.fonts f with
  | some p => p.2 n
  | none => 0

def curFontParam (s : State) (n : Nat) : Scaled := fontParamOf s (s.intAt .curFont).toNat n

def isRelax : Option Cmd → Bool | some .relax => true | _ => false

/-- Commands `scan_something_internal` accepts (§ 413). -/
def internalTok (s : State) (t : Token) : Bool :=
  match meaningTok s t with
  | .reg k _ => k != .box
  | .chardef _ | .mathchardef _ | .font _ => true
  | .prim _ c => match c with
    | .register k => k != .box
    | .intPar _ | .dimPar _ | .gluePar _ _ | .toksPar _ | .codeTable _ | .lastItem _
    | .expr _ | .fontParam _ | .special _ | .fontDef => true
    | _ => false
  | _ => false

def levelBound : Level → Int | .int => infinity | _ => maxDimen

def inBound (l : Level) (x : Int) : Option Int := if x.natAbs ≤ (levelBound l).natAbs then some x else none

/-- Adding glue components of possibly different orders (etex.ch
`add_or_sub` on glue): the higher order wins, unnegated (SEM § 11). -/
def addOrder (neg : Bool) (a : Int) (ao : Nat) (b : Int) (bo : Nat) : Int × Nat :=
  if ao = bo then (if neg then a - b else a + b, ao)
  else if ao < bo ∧ b ≠ 0 then (b, bo) else (a, ao)

def glueOk (g : Glue) : Option Glue :=
  match inBound .glue g.width, inBound .glue g.stretch, inBound .glue g.shrink with
  | some _, some _, some _ => some g
  | _, _, _ => none

def ivalOf (l : Level) (x : Int) (g : Glue) : IVal :=
  match l with
  | .int => .int x | .dimen => .dimen x | .mu => .mu g | _ => .glue g

/-- `+`/`-` of an expression (etex.ch `add_or_sub`); `none` is overflow. -/
def exprAdd (l : Level) (neg : Bool) : Option IVal → Option IVal → Option IVal
  | some v, some w =>
    match l with
    | .int | .dimen =>
      (inBound l (if neg then v.toInt - w.toInt else v.toInt + w.toInt)).map (fun x => ivalOf l x {})
    | _ =>
      let a := v.toGlue
      let b := w.toGlue
      let st := addOrder neg a.stretch a.stretchOrder b.stretch b.stretchOrder
      let sh := addOrder neg a.shrink a.shrinkOrder b.shrink b.shrinkOrder
      (glueOk { width := if neg then a.width - b.width else a.width + b.width,
                stretch := st.1, stretchOrder := st.2, shrink := sh.1, shrinkOrder := sh.2 }).map
        (fun g => ivalOf l 0 g)
  | _, _ => none

/-- `x·n/d` rounded, ties away from zero (etex.ch `fract`, `quotient`,
`nx_plus_y`); `none` is overflow or a zero divisor. -/
def scaleBy (l : Level) (x n d : Int) : Option Int := if d = 0 then none else inBound l (qdiv (x * n) d)

def exprScale (l : Level) (n d : Int) : Option IVal → Option IVal
  | some v =>
    match l with
    | .int | .dimen => (scaleBy l v.toInt n d).map (fun x => ivalOf l x {})
    | _ =>
      let g := v.toGlue
      match scaleBy l g.width n d, scaleBy l g.stretch n d, scaleBy l g.shrink n d with
      | some w, some st, some sh =>
        some (ivalOf l 0 { g with width := w, stretch := st, shrink := sh })
      | _, _, _ => none
  | none => none

/-- A physical unit applied to `i + f/2^16` (§§ 457–458); `none` is
"Dimension too large". -/
def physValue (mag : Int) (tr : Bool) (i f num den : Nat) : Option Int :=
  let m := mag.toNat
  let m := if 0 < m ∧ m ≤ 32768 then m else 1000
  let p1 := if tr = true ∧ m ≠ 1000 then scaleUnit i f 1000 m else (i, f)
  let p2 := scaleUnit p1.1 p1.2 num den
  attachFraction p2.1 p2.2

/-- The head of a conditional, with `\unless` (etex.ch). -/
inductive CondHead : State → Token → IfOp → Bool → State → Prop
  | plain {s s1 : State} {t : Token} {op : IfOp} :
      Next s t s1 → cmdOf s1 t = some (.ifTest op) → CondHead s t op false s1
  | unless_ {s s1 s2 : State} {t u : Token} {op : IfOp} :
      Next s t s1 → cmdOf s1 t = some .unlessCmd → Next s1 u s2 → cmdOf s2 u = some (.ifTest op) →
      op ≠ .ifcase → CondHead s u op true s2

/-- A raw balanced text up to the `}` that balances (§ 473 without expansion). -/
inductive BodyRaw : State → Nat → TokList → State → Prop
  | close {s s1 : State} {t : Token} : Next s t s1 → t.isEgroup = true → BodyRaw s 0 [] s1
  | closeInner {s s1 s2 : State} {d : Nat} {ts : TokList} {t : Token} :
      Next s t s1 → t.isEgroup = true → BodyRaw s1 d ts s2 → BodyRaw s (d + 1) (t :: ts) s2
  | openGroup {s s1 s2 : State} {d : Nat} {ts : TokList} {t : Token} :
      Next s t s1 → t.isBgroup = true → BodyRaw s1 (d + 1) ts s2 → BodyRaw s d (t :: ts) s2
  | tok {s s1 s2 : State} {d : Nat} {ts : TokList} {t : Token} :
      Next s t s1 → t.isBgroup = false → t.isEgroup = false → BodyRaw s1 d ts s2 →
      BodyRaw s d (t.stored :: ts) s2

/-! ## The gullet and the scanners (mutual: expansion happens inside scans) -/

mutual

/-- `get_x_token` (§ 380): expand until an unexpandable token comes. -/
inductive XNext (E : Engine) : State → Token → State → Prop
  | plain {s s1 : State} {t : Token} :
      Next s t s1 → expandableTok s1 t = false → XNext E s t s1
  | expand {s s1 s2 : State} {t : Token} :
      Expand E s s1 → XNext E s1 t s2 → XNext E s t s2

/-- The next non-blank token after expansion (§ 406). -/
inductive XNonBlank (E : Engine) : State → Token → State → Prop
  | here {s s1 : State} {t : Token} : XNext E s t s1 → t ≠ Token.space → XNonBlank E s t s1
  | skip {s s1 s2 : State} {t : Token} :
      XNext E s Token.space s1 → XNonBlank E s1 t s2 → XNonBlank E s t s2

/-- Non-blank, non-`\relax` (§ 404). -/
inductive XNonRelax (E : Engine) : State → Token → State → Prop
  | here {s s1 : State} {t : Token} :
      XNonBlank E s t s1 → cmdOf s1 t ≠ some .relax → XNonRelax E s t s1
  | skip {s s1 s2 : State} {t u : Token} :
      XNonBlank E s u s1 → cmdOf s1 u = some .relax → XNonRelax E s1 t s2 → XNonRelax E s t s2

/-- `scan_keyword` (§ 407): letters of either case, spaces only before the
first; on failure the tokens go back. -/
inductive Keyword (E : Engine) : State → List Char → TokList → Bool → State → Prop
  | done {s : State} {acc : TokList} : Keyword E s [] acc true s
  | letter {s s1 s2 : State} {t : Token} {k : Char} {ks : List Char} {acc : TokList} {b : Bool} :
      XNext E s t s1 → kwMatches k t = true → Keyword E s1 ks (acc ++ [t]) b s2 →
      Keyword E s (k :: ks) acc b s2
  | space {s s1 s2 : State} {k : Char} {ks : List Char} {b : Bool} :
      XNext E s Token.space s1 → Keyword E s1 (k :: ks) [] b s2 → Keyword E s (k :: ks) [] b s2
  | fail {s s1 : State} {t : Token} {k : Char} {ks : List Char} {acc : TokList} :
      XNext E s t s1 → kwMatches k t = false → ¬ (t = Token.space ∧ acc = []) →
      Keyword E s (k :: ks) acc false (backList (acc ++ [t]) s1)

/-- `scan_optional_equals` (§ 405). -/
inductive OptEquals (E : Engine) : State → State → Prop
  | eq {s s1 : State} : XNonBlank E s (otherChar '=') s1 → OptEquals E s s1
  | absent {s s1 : State} {t : Token} : XNonBlank E s t s1 → t ≠ otherChar '=' → OptEquals E s (backInput t s1)

/-- Signs before a number (§ 441), returning the first other token. -/
inductive Signs (E : Engine) : State → Bool → Token → State → Prop
  | done {s s1 : State} {t : Token} :
      XNonBlank E s t s1 → t ≠ otherChar '-' → t ≠ otherChar '+' → Signs E s false t s1
  | plus {s s1 s2 : State} {neg : Bool} {t : Token} :
      XNonBlank E s (otherChar '+') s1 → Signs E s1 neg t s2 → Signs E s neg t s2
  | minus {s s1 s2 : State} {neg : Bool} {t : Token} :
      XNonBlank E s (otherChar '-') s1 → Signs E s1 neg t s2 → Signs E s (!neg) t s2

/-- Digits (§§ 444–445): accumulate, then one optional space (§ 443). The
value is unbounded here; the caller checks it. -/
inductive Digits (E : Engine) : Nat → State → Nat → List Nat → State → Prop
  | more {s s1 s2 : State} {radix acc d : Nat} {t : Token} {ds : List Nat} :
      XNext E s t s1 → isDigitTok radix t = some d →
      Digits E radix s1 (acc * radix + d) ds s2 → Digits E radix s acc ds s2
  | endSpace {s s1 : State} {radix acc : Nat} :
      XNext E s Token.space s1 → Digits E radix s acc [acc] s1
  | endOther {s s1 : State} {radix acc : Nat} {t : Token} :
      XNext E s t s1 → isDigitTok radix t = none → t ≠ Token.space →
      Digits E radix s acc [acc] (backInput t s1)

/-- `scan_int` (§ 440). -/
inductive ScanInt (E : Engine) : State → Int → State → Prop
  | internal {s s1 s2 : State} {neg : Bool} {t : Token} {v : IVal} :
      Signs E s neg t s1 → internalTok s1 t = true → Internal E (backInput t s1) v s2 →
      ScanInt E s (if neg then -v.toInt else v.toInt) s2
  /-- `` `c ``: the character's code, then an optional space (§ 442). -/
  | alpha {s s1 s2 s3 : State} {neg : Bool} {u : Token} {c : Nat} :
      Signs E s neg (otherChar '`') s1 → Next s1 u s2 →
      (match u with
       | .char d _ => c = d
       | .cs (.name n) => strCodes n = [c]
       | .cs (.active d) => c = d
       | .noexp (.active d) => c = d
       | .noexp (.name n) => strCodes n = [c]
       | _ => False) →
      OptSpace E s2 s3 →
      ScanInt E s (if neg then -(Int.ofNat c) else Int.ofNat c) s3
  | decimal {s s1 s2 : State} {neg : Bool} {t : Token} {d n : Nat} :
      Signs E s neg t s1 → isDigitTok 10 t = some d → Digits E 10 s1 d [n] s2 →
      ScanInt E s (if neg then -(min (Int.ofNat n) infinity) else min (Int.ofNat n) infinity)
        (if Int.ofNat n > infinity then raise "Number too big" s2 else s2)
  | octal {s s1 s2 : State} {neg : Bool} {n : Nat} :
      Signs E s neg (otherChar '\'') s1 → Digits E 8 s1 0 [n] s2 →
      ScanInt E s (if neg then -(min (Int.ofNat n) infinity) else min (Int.ofNat n) infinity)
        (if Int.ofNat n > infinity then raise "Number too big" s2 else s2)
  | hex {s s1 s2 : State} {neg : Bool} {n : Nat} :
      Signs E s neg (otherChar '"') s1 → Digits E 16 s1 0 [n] s2 →
      ScanInt E s (if neg then -(min (Int.ofNat n) infinity) else min (Int.ofNat n) infinity)
        (if Int.ofNat n > infinity then raise "Number too big" s2 else s2)
  /-- "Missing number, treated as zero" (§ 446). -/
  | missing {s s1 s2 : State} {neg : Bool} {t : Token} :
      Signs E s neg t s1 → isDigitTok 10 t = none →
      t ≠ otherChar '`' → t ≠ otherChar '\'' → t ≠ otherChar '"' →
      internalTok s1 t = false →
      ScanInt E s 0 (raise "Missing number, treated as zero" (backInput t s1))

/-- One optional space (§ 443). -/
inductive OptSpace (E : Engine) : State → State → Prop
  | space {s s1 : State} : XNext E s Token.space s1 → OptSpace E s s1
  | other {s s1 : State} {t : Token} : XNext E s t s1 → t ≠ Token.space → OptSpace E s (backInput t s1)

/-- The fraction digits after a decimal point (§ 452). -/
inductive Fraction (E : Engine) : State → List Nat → State → Prop
  | more {s s1 s2 : State} {d : Nat} {ds : List Nat} {t : Token} :
      XNext E s t s1 → isDigitTok 10 t = some d → Fraction E s1 ds s2 → Fraction E s (d :: ds) s2
  | stop {s s1 : State} {t : Token} :
      XNext E s t s1 → isDigitTok 10 t = none → Fraction E s [] (backInput t s1)

/-- Integer and fraction part of a decimal constant (§ 448, § 452). -/
inductive Decimal (E : Engine) : State → Token → Nat → Nat → State → Prop
  | digits {s s1 s2 s3 : State} {t : Token} {d n : Nat} {ds : List Nat} :
      isDigitTok 10 t = some d → DigitsNoSpace E s d n s1 →
      PointFraction E s1 ds s2 → s3 = s2 → Decimal E s t n (roundDecimals ds) s3
  | point {s s1 : State} {t : Token} {ds : List Nat} :
      (t = otherChar '.' ∨ t = otherChar ',') → Fraction E s ds s1 →
      Decimal E s t 0 (roundDecimals ds) s1

/-- Decimal digits that stop at any non-digit, kept back (§ 448). -/
inductive DigitsNoSpace (E : Engine) : State → Nat → Nat → State → Prop
  | more {s s1 s2 : State} {acc d n : Nat} {t : Token} :
      XNext E s t s1 → isDigitTok 10 t = some d → DigitsNoSpace E s1 (acc * 10 + d) n s2 →
      DigitsNoSpace E s acc n s2
  | stop {s s1 : State} {acc : Nat} {t : Token} :
      XNext E s t s1 → isDigitTok 10 t = none → DigitsNoSpace E s acc acc (backInput t s1)

/-- An optional `.`/`,` fraction after the integer part (§ 448). -/
inductive PointFraction (E : Engine) : State → List Nat → State → Prop
  | absent {s s1 : State} {t : Token} :
      XNext E s t s1 → t ≠ otherChar '.' → t ≠ otherChar ',' → PointFraction E s [] (backInput t s1)
  | frac {s s1 s2 : State} {t : Token} {ds : List Nat} :
      XNext E s t s1 → (t = otherChar '.' ∨ t = otherChar ',') → Fraction E s1 ds s2 →
      PointFraction E s ds s2

/-- The unit after a number (§§ 453–459), for a magnitude `i + f/2^16`;
`inf` allows `fil` (glue components), `mu` math units only. -/
inductive ScanUnit (E : Engine) : Bool → Bool → State → Nat → Nat → Int × Nat → State → Prop
  /-- `em`, `ex` (§ 455): the current font's quad and x-height. -/
  | fontUnit {s s1 s2 : State} {inf : Bool} {i f : Nat} {kw : String} {r : Int} {v : Scaled} :
      (kw = "em" ∧ v = curFontParam s 6 ∨ kw = "ex" ∧ v = curFontParam s 5) →
      Keyword E s kw.toList [] true s1 → OptSpace E s1 s2 →
      nxPlusY (Int.ofNat i) v (tdiv (v * Int.ofNat f) 65536) = some r →
      ScanUnit E inf false s i f (r, 0) s2
  /-- An internal dimension as unit (§ 455): `2\dimen0`. -/
  | internalUnit {s s1 s2 : State} {inf : Bool} {i f : Nat} {t : Token} {v : IVal} {r : Int} :
      XNonBlank E s t s1 → internalTok s1 t = true → Internal E (backInput t s1) v s2 →
      (match v with | .dimen _ => True | .glue _ => True | _ => False) →
      nxPlusY (Int.ofNat i) v.toInt (tdiv (v.toInt * Int.ofNat f) 65536) = some r →
      ScanUnit E inf false s i f (r, 0) s2
  /-- `true` then a physical unit (§§ 457–458). -/
  | physical {s s1 s2 s3 : State} {inf : Bool} {i f : Nat} {tr : Bool} {u : String}
      {num den : Nat} :
      Keyword E s "true".toList [] tr s1 →
      u ∈ physicalUnits → unitRatio u = some (num, den) →
      Keyword E s1 u.toList [] true s2 → OptSpace E s2 s3 →
      ScanUnit E inf false s i f ((physValue (s.intPar "mag") tr i f num den).getD maxDimen, 0)
        (if (physValue (s.intPar "mag") tr i f num den).isNone then raise "Dimension too large" s3 else s3)
  /-- `sp` (§ 459): the integer part only. -/
  | sp {s s1 s2 : State} {inf : Bool} {i f : Nat} {tr : Bool} :
      Keyword E s "true".toList [] tr s1 → Keyword E s1 "sp".toList [] true s2 →
      ScanUnit E inf false s i f (Int.ofNat i, 0) s2
  /-- `fil`, `fill`, `filll` (§ 454): the order, more `l`s stay `filll`. -/
  | fil {s s1 s2 s3 : State} {i f : Nat} {ls : Nat} :
      Keyword E s "fil".toList [] true s1 → MoreL E s1 0 ls s2 → OptSpace E s2 s3 →
      ScanUnit E true false s i f (Int.ofNat (i * 65536 + f), min (ls + 1) 3) s3
  /-- `mu` (§ 456). -/
  | mu {s s1 s2 : State} {inf : Bool} {i f : Nat} :
      Keyword E s "mu".toList [] true s1 → OptSpace E s1 s2 →
      ScanUnit E inf true s i f (Int.ofNat (i * 65536 + f), 0) s2
  /-- "Illegal unit of measure (pt inserted)" (§ 459). -/
  | missing {s s1 : State} {inf mu : Bool} {i f : Nat} :
      NoKeyword E (if mu then ["mu"] else ["em", "ex", "true"] ++ physicalUnits ++ ["sp"]) s s1 →
      ScanUnit E inf mu s i f ((attachFraction i f).getD maxDimen, 0)
        (raise "Illegal unit of measure (pt inserted)" s1)

/-- Each keyword in turn is absent (§§ 453–459). -/
inductive NoKeyword (E : Engine) : List String → State → State → Prop
  | nil {s : State} : NoKeyword E [] s s
  | cons {s s1 s2 : State} {k : String} {ks : List String} :
      Keyword E s k.toList [] false s1 → NoKeyword E ks s1 s2 → NoKeyword E (k :: ks) s s2

/-- Further `l`s after `fil` (§ 454). -/
inductive MoreL (E : Engine) : State → Nat → Nat → State → Prop
  | stop {s s1 : State} {n : Nat} : Keyword E s ['l'] [] false s1 → MoreL E s n n s1
  | more {s s1 s2 : State} {n m : Nat} : Keyword E s ['l'] [] true s1 → MoreL E s1 (n + 1) m s2 → MoreL E s n m s2

/-- `scan_dimen` (§ 448): an internal dimension, or a number and a unit.
Returns the value and its glue order. -/
inductive ScanDimen (E : Engine) : Bool → Bool → State → Int × Nat → State → Prop
  | internal {s s1 s2 : State} {inf mu neg : Bool} {t : Token} {v : IVal} :
      Signs E s neg t s1 → internalTok s1 t = true → Internal E (backInput t s1) v s2 →
      (match v with | .int _ => False | _ => True) →
      ScanDimen E inf mu s ((if neg then -v.toInt else v.toInt), 0) s2
  /-- An internal integer, then a unit (§ 449). -/
  | intUnit {s s1 s2 s3 : State} {inf mu neg : Bool} {t : Token} {n : Int} {r : Int × Nat} :
      Signs E s neg t s1 → Internal E (backInput t s1) (.int n) s2 →
      ScanUnit E inf mu s2 n.natAbs 0 r s3 →
      ScanDimen E inf mu s
        ((if neg != decide (n < 0) then -r.1 else r.1), r.2) s3
  | constant {s s1 s2 s3 : State} {inf mu neg : Bool} {t : Token} {i f : Nat} {r : Int × Nat} :
      Signs E s neg t s1 → Decimal E s1 t i f s2 → ScanUnit E inf mu s2 i f r s3 →
      ScanDimen E inf mu s ((if neg then -r.1 else r.1), r.2) s3
  | radixConst {s s1 s2 s3 : State} {inf mu neg : Bool} {t : Token} {n : Int} {r : Int × Nat} :
      Signs E s neg t s1 → (t = otherChar '\'' ∨ t = otherChar '"' ∨ t = otherChar '`') →
      ScanInt E (backInput t s1) n s2 → ScanUnit E inf mu s2 n.natAbs 0 r s3 →
      ScanDimen E inf mu s ((if neg then -r.1 else r.1), r.2) s3

/-- `scan_glue` (§ 461). -/
inductive ScanGlue (E : Engine) : Bool → State → Glue → State → Prop
  | internal {s s1 s2 : State} {mu neg : Bool} {t : Token} {g : Glue} :
      Signs E s neg t s1 → internalTok s1 t = true →
      Internal E (backInput t s1) (if mu then .mu g else .glue g) s2 →
      ScanGlue E mu s
        (if neg then { g with width := -g.width, stretch := -g.stretch, shrink := -g.shrink } else g) s2
  | spec {s s1 s2 s3 : State} {mu : Bool} {w : Int × Nat} {st sh : Int × Nat} :
      ScanDimen E false mu s w s1 →
      Stretch E mu "plus" s1 st s2 → Stretch E mu "minus" s2 sh s3 →
      ScanGlue E mu s { width := w.1, stretch := st.1, stretchOrder := st.2,
                        shrink := sh.1, shrinkOrder := sh.2 } s3

/-- `plus`/`minus` part of glue (§ 462). -/
inductive Stretch (E : Engine) : Bool → String → State → Int × Nat → State → Prop
  | absent {s s1 : State} {mu : Bool} {kw : String} :
      Keyword E s kw.toList [] false s1 → Stretch E mu kw s (0, 0) s1
  | present {s s1 s2 : State} {mu : Bool} {kw : String} {v : Int × Nat} :
      Keyword E s kw.toList [] true s1 → ScanDimen E true mu s1 v s2 → Stretch E mu kw s v s2

/-- `scan_something_internal` (§ 413): the value of an internal quantity
whose token comes next. -/
inductive Internal (E : Engine) : State → IVal → State → Prop
  | register {s s1 s2 : State} {t : Token} {k : RegKind} {n : Int} :
      Next s t s1 → cmdOf s1 t = some (.register k) → k ≠ .box →
      ScanInt E s1 n s2 → 0 ≤ n → n.toNat ≤ E.maxReg →
      Internal E s (valOfLoc k (s2.get (.reg k n.toNat))) s2
  | regMeaning {s s1 : State} {t : Token} {k : RegKind} {i : Nat} :
      Next s t s1 → meaningTok s1 t = .reg k i → k ≠ .box →
      Internal E s (valOfLoc k (s1.get (.reg k i))) s1
  | intPar {s s1 : State} {t : Token} {n : String} :
      Next s t s1 → cmdOf s1 t = some (.intPar n) → Internal E s (.int (s1.intAt (.intPar n))) s1
  | dimPar {s s1 : State} {t : Token} {n : String} :
      Next s t s1 → cmdOf s1 t = some (.dimPar n) → Internal E s (.dimen (s1.intAt (.dimPar n))) s1
  | gluePar {s s1 : State} {t : Token} {n : String} {mu : Bool} :
      Next s t s1 → cmdOf s1 t = some (.gluePar n mu) →
      Internal E s (let g := (match s1.get (.gluePar n) with | .glue g => g | _ => {})
                    if mu then .mu g else .glue g) s1
  | toksPar {s s1 : State} {t : Token} {n : String} :
      Next s t s1 → cmdOf s1 t = some (.toksPar n) → Internal E s (.toks (s1.toksAt (.toksPar n))) s1
  | code {s s1 s2 : State} {t : Token} {tab : CodeTable} {c : Int} :
      Next s t s1 → cmdOf s1 t = some (.codeTable tab) → ScanInt E s1 c s2 →
      0 ≤ c → c.toNat ≤ charMax E →
      Internal E s (.int (s2.intAt (.code tab c.toNat))) s2
  | chardef {s s1 : State} {t : Token} {n : Nat} :
      Next s t s1 → (meaningTok s1 t = .chardef n ∨ meaningTok s1 t = .mathchardef n) →
      Internal E s (.int (Int.ofNat n)) s1
  | font {s s1 : State} {t : Token} {f : Nat} :
      Next s t s1 → meaningTok s1 t = .font f → Internal E s (.font f) s1
  | curFont {s s1 : State} {t : Token} :
      Next s t s1 → cmdOf s1 t = some .fontDef → Internal E s (.font (s1.intAt .curFont).toNat) s1
  /-- `\fontdimen n f` (§ 425, § 578). -/
  | fontdimen {s s1 s2 s3 : State} {t : Token} {n : Int} {f : Nat} :
      Next s t s1 → cmdOf s1 t = some (.fontParam "fontdimen") → ScanInt E s1 n s2 →
      Internal E s2 (.font f) s3 →
      Internal E s (.dimen (fontParamOf s3 f n.toNat)) s3
  /-- **[over]** `\hyphenchar`, `\skewchar` and the pdfTeX/LuaTeX per-font
  codes: an integer SaTeX keeps per font, any value here. -/
  | fontInt {s s1 s2 : State} {t : Token} {n : String} {v : Int} :
      Next s t s1 → cmdOf s1 t = some (.fontParam n) → n ≠ "fontdimen" →
      Operands E s1 (E.opsOf n) s2 →
      Internal E s (.int v) s2
  | lastPenalty {s s1 : State} {t : Token} :
      Next s t s1 → cmdOf s1 t = some (.lastItem .lastpenalty) →
      Internal E s (.int (match tailItem s1 with | some (.penalty n) => n | _ => 0)) s1
  | lastKern {s s1 : State} {t : Token} :
      Next s t s1 → cmdOf s1 t = some (.lastItem .lastkern) →
      Internal E s (.dimen (match tailItem s1 with | some (.kern d) => d | _ => 0)) s1
  | lastSkip {s s1 : State} {t : Token} :
      Next s t s1 → cmdOf s1 t = some (.lastItem .lastskip) →
      Internal E s (.glue (match tailItem s1 with | some (.glue g) => g | _ => {})) s1
  | inputLineNo {s s1 : State} {t : Token} :
      Next s t s1 → cmdOf s1 t = some (.lastItem .inputlineno) →
      Internal E s (.int (Int.ofNat (lineNo s1))) s1
  | etexVersion {s s1 : State} {t : Token} :
      Next s t s1 → cmdOf s1 t = some (.lastItem .eTeXversion) → Internal E s (.int 2) s1
  | groupLevel {s s1 : State} {t : Token} :
      Next s t s1 → cmdOf s1 t = some (.lastItem .currentgrouplevel) →
      Internal E s (.int (Int.ofNat (s1.curLevel - 1))) s1
  | groupType {s s1 : State} {t : Token} :
      Next s t s1 → cmdOf s1 t = some (.lastItem .currentgrouptype) →
      Internal E s (.int (Int.ofNat (topGroup s1).toNat)) s1
  | ifLevel {s s1 : State} {t : Token} :
      Next s t s1 → cmdOf s1 t = some (.lastItem .currentiflevel) →
      Internal E s (.int (Int.ofNat s1.cond.length)) s1
  /-- etex.ch: the type, negative after `\unless`, 0 outside conditionals. -/
  | ifType {s s1 : State} {t : Token} :
      Next s t s1 → cmdOf s1 t = some (.lastItem .currentiftype) →
      Internal E s (.int (match s1.cond with
        | [] => 0
        | c :: _ => if c.negated then -(c.op.code + 1) else c.op.code + 1)) s1
  /-- etex.ch: 1 true branch, -1 false, 0 while the test is read. -/
  | ifBranch {s s1 : State} {t : Token} :
      Next s t s1 → cmdOf s1 t = some (.lastItem .currentifbranch) →
      Internal E s (.int (match s1.cond with
        | [] => 0
        | c :: _ => if c.limit = .ifCode then 0 else if c.limit = .elseCode then 1 else -1)) s1
  | lastNodeType {s s1 : State} {t : Token} :
      Next s t s1 → cmdOf s1 t = some (.lastItem .lastnodetype) →
      Internal E s (.int (match s1.nest with
        | [l] => if l.items = [] then -1 else itemCode l.items.head? + 1
        | _ => itemCode (tailItem s1) + 1)) s1
  | parshapeLength {s s1 s2 : State} {t : Token} {n : Int} {which : Nat} :
      Next s t s1 →
      (cmdOf s1 t = some (.lastItem .parshapelength) ∧ which = 1 ∨
       cmdOf s1 t = some (.lastItem .parshapeindent) ∧ which = 0) →
      ScanInt E s1 n s2 →
      Internal E s (.dimen (let m := s2.intAt (.special "parshape" 0)
        if n ≤ 0 ∨ m = 0 then 0
        else let k := if n > m then m else n
             s2.intAt (.special "parshape" (2 * k.toNat - 1 + which)))) s2
  | parshapeDimen {s s1 s2 : State} {t : Token} {n : Int} :
      Next s t s1 → cmdOf s1 t = some (.lastItem .parshapedimen) → ScanInt E s1 n s2 →
      Internal E s (.dimen (let m := s2.intAt (.special "parshape" 0)
        if n ≤ 0 ∨ m = 0 then 0
        else let k := if n > 2 * m then 2 * m - (n % 2) else n
             s2.intAt (.special "parshape" k.toNat))) s2
  | glueComp {s s1 s2 : State} {t : Token} {op : LastOp} {g : Glue} {mu : Bool} :
      Next s t s1 →
      cmdOf s1 t = some (.lastItem op) →
      (op = .gluestretch ∨ op = .glueshrink ∨ op = .gluestretchorder ∨ op = .glueshrinkorder) →
      ScanGlue E mu s1 g s2 →
      Internal E s (match op with
        | .gluestretch => .dimen g.stretch
        | .glueshrink => .dimen g.shrink
        | .gluestretchorder => .int (Int.ofNat g.stretchOrder)
        | _ => .int (Int.ofNat g.shrinkOrder)) s2
  | muToGlue {s s1 s2 : State} {t : Token} {g : Glue} :
      Next s t s1 → cmdOf s1 t = some (.lastItem .mutoglue) → ScanGlue E true s1 g s2 →
      Internal E s (.glue g) s2
  | glueToMu {s s1 s2 : State} {t : Token} {g : Glue} :
      Next s t s1 → cmdOf s1 t = some (.lastItem .gluetomu) → ScanGlue E false s1 g s2 →
      Internal E s (.mu g) s2
  | constant {s s1 : State} {t : Token} {n : String} :
      Next s t s1 → cmdOf s1 t = some (.lastItem (.constant n)) → Internal E s (.int (E.constant n)) s1
  /-- **[over]** Badness, font character dimensions, the clock, the random
  generator, page positions, XeTeX font tables: any value, after their
  operands. -/
  | unknowable {s s1 s2 : State} {t : Token} {op : LastOp} {n m : String} {v : IVal} :
      Next s t s1 → meaningTok s1 t = .prim n (.lastItem op) →
      (op = .badness ∨ op = .fontcharwd ∨ op = .fontcharht ∨ op = .fontchardp ∨
       op = .fontcharic ∨ op = .unknowable m) →
      Operands E s1 (E.opsOf n) s2 →
      Internal E s v s2
  | expr {s s1 s2 : State} {t : Token} {l : Level} {v : IVal} :
      Next s t s1 → cmdOf s1 t = some (.expr l) → ScanExpr E l s1 v s2 → Internal E s v s2
  /-- `\wd`, `\ht`, `\dp` (§ 420). -/
  | boxDim {s s1 s2 : State} {t : Token} {n : Int} {which : Nat} :
      Next s t s1 →
      (cmdOf s1 t = some (.special "wd") ∧ which = 0 ∨ cmdOf s1 t = some (.special "ht") ∧ which = 1 ∨
       cmdOf s1 t = some (.special "dp") ∧ which = 2) →
      ScanInt E s1 n s2 → 0 ≤ n →
      Internal E s (.dimen (let (w, h, d) := boxDims (boxAt s2 n.toNat)
                            if which = 0 then w else if which = 1 then h else d)) s2
  | parshape {s s1 : State} {t : Token} :
      Next s t s1 → cmdOf s1 t = some (.special "parshape") →
      Internal E s (.int (s1.intAt (.special "parshape" 0))) s1
  /-- `\interlinepenalties n` and its relatives (etex_man § 3.13). -/
  | penalties {s s1 s2 : State} {t : Token} {n : String} {i : Int} :
      Next s t s1 → cmdOf s1 t = some (.special n) →
      n ∈ ["interlinepenalties", "clubpenalties", "widowpenalties", "displaywidowpenalties"] →
      ScanInt E s1 i s2 →
      Internal E s (.int (let m := s2.intAt (.special n 0)
                          if i < 0 then 0 else if i = 0 then m else
                          s2.intAt (.special n (if i > m then m.toNat else i.toNat)))) s2
  /-- Other per-character or per-class tables (`\XeTeXcharclass`,
  `\Umathcode`, …): read back as stored at the operand's index. -/
  | specialTable {s s1 s2 : State} {t : Token} {n : String} {i : Int} :
      Next s t s1 → cmdOf s1 t = some (.special n) →
      n ∉ ["wd", "ht", "dp", "parshape", "interlinepenalties", "clubpenalties",
           "widowpenalties", "displaywidowpenalties"] →
      ScanInt E s1 i s2 →
      Internal E s (.int (s2.intAt (.special n i.toNat))) s2

/-- etex.ch `scan_expr`: `+ - * /` and parentheses at one level. Each
operation is checked against the level's bound; an overflow makes the
whole expression 0 with "Arithmetic overflow". `\relax` may end it. -/
inductive ScanExpr (E : Engine) : Level → State → IVal → State → Prop
  | ok {s s1 s2 : State} {l : Level} {v : IVal} {t : Token} :
      ExprSum E l s (some v) s1 → XNonBlank E s1 t s2 →
      ScanExpr E l s v (if isRelax (cmdOf s2 t) then s2 else backInput t s2)
  | overflow {s s1 s2 : State} {l : Level} {t : Token} :
      ExprSum E l s none s1 → XNonBlank E s1 t s2 →
      ScanExpr E l s (ivalOf l 0 {})
        (raise "Arithmetic overflow" (if isRelax (cmdOf s2 t) then s2 else backInput t s2))

/-- Terms joined by `+`/`-`. -/
inductive ExprSum (E : Engine) : Level → State → Option IVal → State → Prop
  | sum {s s1 s2 : State} {l : Level} {v w : Option IVal} :
      ExprTerm E l s v s1 → ExprSumRest E l s1 v w s2 → ExprSum E l s w s2

inductive ExprSumRest (E : Engine) : Level → State → Option IVal → Option IVal → State → Prop
  | stop {s s1 : State} {l : Level} {v : Option IVal} {t : Token} :
      XNonBlank E s t s1 → t ≠ otherChar '+' → t ≠ otherChar '-' →
      ExprSumRest E l s v v (backInput t s1)
  | add {s s1 s2 s3 : State} {l : Level} {v w r : Option IVal} {t : Token} :
      XNonBlank E s t s1 → (t = otherChar '+' ∨ t = otherChar '-') →
      ExprTerm E l s1 w s2 →
      ExprSumRest E l s2 (exprAdd l (decide (t = otherChar '-')) v w) r s3 →
      ExprSumRest E l s v r s3

/-- A factor, then `*` or `/` by integers; `*` then `/` is one scaling. -/
inductive ExprTerm (E : Engine) : Level → State → Option IVal → State → Prop
  | term {s s1 s2 : State} {l : Level} {v r : Option IVal} :
      ExprFactor E l s v s1 → ExprTermRest E l s1 v r s2 → ExprTerm E l s r s2

inductive ExprTermRest (E : Engine) : Level → State → Option IVal → Option IVal → State → Prop
  | stop {s s1 : State} {l : Level} {v : Option IVal} {t : Token} :
      XNonBlank E s t s1 → t ≠ otherChar '*' → t ≠ otherChar '/' →
      ExprTermRest E l s v v (backInput t s1)
  | mulDiv {s s1 s2 s3 s4 s5 : State} {l : Level} {v r : Option IVal} {n d : IVal} :
      XNonBlank E s (otherChar '*') s1 → ExprFactor E .int s1 (some n) s2 →
      XNonBlank E s2 (otherChar '/') s3 → ExprFactor E .int s3 (some d) s4 →
      ExprTermRest E l s4 (exprScale l n.toInt d.toInt v) r s5 →
      ExprTermRest E l s v r s5
  | mul {s s1 s2 s3 s4 : State} {l : Level} {v r : Option IVal} {n : IVal} {t : Token} :
      XNonBlank E s (otherChar '*') s1 → ExprFactor E .int s1 (some n) s2 →
      XNonBlank E s2 t s3 → t ≠ otherChar '/' →
      ExprTermRest E l (backInput t s3) (exprScale l n.toInt 1 v) r s4 →
      ExprTermRest E l s v r s4
  | div {s s1 s2 s3 : State} {l : Level} {v r : Option IVal} {d : IVal} :
      XNonBlank E s (otherChar '/') s1 → ExprFactor E .int s1 (some d) s2 →
      ExprTermRest E l s2 (exprScale l 1 d.toInt v) r s3 →
      ExprTermRest E l s v r s3

inductive ExprFactor (E : Engine) : Level → State → Option IVal → State → Prop
  | paren {s s1 s2 s3 : State} {l : Level} {v : Option IVal} :
      XNonBlank E s (otherChar '(') s1 → ExprSum E l s1 v s2 →
      XNonBlank E s2 (otherChar ')') s3 → ExprFactor E l s v s3
  | int {s s1 s2 : State} {t : Token} {n : Int} :
      XNonBlank E s t s1 → t ≠ otherChar '(' → ScanInt E (backInput t s1) n s2 →
      ExprFactor E .int s (some (.int n)) s2
  | dimen {s s1 s2 : State} {t : Token} {d : Int × Nat} :
      XNonBlank E s t s1 → t ≠ otherChar '(' → ScanDimen E false false (backInput t s1) d s2 →
      ExprFactor E .dimen s (some (.dimen d.1)) s2
  | glue {s s1 s2 : State} {t : Token} {g : Glue} :
      XNonBlank E s t s1 → t ≠ otherChar '(' → ScanGlue E false (backInput t s1) g s2 →
      ExprFactor E .glue s (some (.glue g)) s2
  | mu {s s1 s2 : State} {t : Token} {g : Glue} :
      XNonBlank E s t s1 → t ≠ otherChar '(' → ScanGlue E true (backInput t s1) g s2 →
      ExprFactor E .mu s (some (.mu g)) s2

/-- Operands of a scan-only command, by its grammar. -/
inductive Operands (E : Engine) : State → List Operand → State → Prop
  | nil {s : State} : Operands E s [] s
  | int {s s1 s2 : State} {ops : List Operand} {n : Int} :
      ScanInt E s n s1 → Operands E s1 ops s2 → Operands E s (.int :: ops) s2
  | dimen {s s1 s2 : State} {ops : List Operand} {d : Int × Nat} :
      ScanDimen E false false s d s1 → Operands E s1 ops s2 → Operands E s (.dimen :: ops) s2
  | glue {s s1 s2 : State} {ops : List Operand} {g : Glue} :
      ScanGlue E false s g s1 → Operands E s1 ops s2 → Operands E s (.glue :: ops) s2
  | mu {s s1 s2 : State} {ops : List Operand} {g : Glue} :
      ScanGlue E true s g s1 → Operands E s1 ops s2 → Operands E s (.mu :: ops) s2
  | optEquals {s s1 s2 : State} {ops : List Operand} :
      OptEquals E s s1 → Operands E s1 ops s2 → Operands E s (.optEquals :: ops) s2
  | generalText {s s1 s2 : State} {ops : List Operand} {ts : TokList} :
      GeneralText E false s ts s1 → Operands E s1 ops s2 → Operands E s (.generalText :: ops) s2
  | balancedText {s s1 s2 : State} {ops : List Operand} {ts : TokList} :
      GeneralText E true s ts s1 → Operands E s1 ops s2 → Operands E s (.balancedText :: ops) s2
  | fileName {s s1 s2 : State} {ops : List Operand} {n : List CharCode} :
      FileName E s n s1 → Operands E s1 ops s2 → Operands E s (.fileName :: ops) s2
  | font {s s1 s2 : State} {ops : List Operand} {f : Nat} :
      Internal E s (.font f) s1 → Operands E s1 ops s2 → Operands E s (.font :: ops) s2
  | anyToken {s s1 s2 : State} {ops : List Operand} {t : Token} :
      Next s t s1 → Operands E s1 ops s2 → Operands E s (.anyToken :: ops) s2
  | keyword {s s1 s2 : State} {ops : List Operand} {k : String} :
      Keyword E s k.toList [] true s1 → Operands E s1 ops s2 → Operands E s (.keyword k :: ops) s2
  | optKeywordYes {s s1 s2 s3 : State} {ops more : List Operand} {k : String} :
      Keyword E s k.toList [] true s1 → Operands E s1 more s2 → Operands E s2 ops s3 →
      Operands E s (.optKeyword k more :: ops) s3
  | optKeywordNo {s s1 s2 : State} {ops more : List Operand} {k : String} :
      Keyword E s k.toList [] false s1 → Operands E s1 ops s2 →
      Operands E s (.optKeyword k more :: ops) s2
  /-- **[over]** A box or rule operand: its content is typesetting. Any
  state that keeps the input below and the definitions made globally. -/
  | box {s s1 s2 : State} {ops : List Operand} :
      s1.vfs = s.vfs → Operands E s1 ops s2 → Operands E s (.box :: ops) s2
  | ruleSpec {s s1 s2 : State} {ops : List Operand} :
      RuleSpec E s s1 → Operands E s1 ops s2 → Operands E s (.ruleSpec :: ops) s2

/-- `scan_rule_spec` (§ 463): `width`, `height`, `depth` in any order. -/
inductive RuleSpec (E : Engine) : State → State → Prop
  | done {s s1 s2 s3 : State} :
      Keyword E s "width".toList [] false s1 → Keyword E s1 "height".toList [] false s2 →
      Keyword E s2 "depth".toList [] false s3 → RuleSpec E s s3
  | dim {s s1 s2 s3 : State} {k : String} {d : Int × Nat} :
      k ∈ ["width", "height", "depth"] → Keyword E s k.toList [] true s1 →
      ScanDimen E false false s1 d s2 → RuleSpec E s2 s3 → RuleSpec E s s3

/-- `scan_left_brace` (§ 403), then a balanced text read with `get_token`
(`xpand = false`) or with expansion (§ 473–477). `balanced = true` takes the
group as a `\pdfstrcmp`-style balanced text: no `\relax` skipping. -/
inductive GeneralText (E : Engine) : Bool → State → TokList → State → Prop
  | plain {s s1 s2 : State} {bal : Bool} {b : Token} {ts : TokList} :
      XNonRelax E s b s1 → b.isBgroup = true → BodyRaw s1 0 ts s2 → GeneralText E bal s ts s2
  /-- "Missing { inserted" (§ 403): the token is read again inside. -/
  | missing {s s1 s2 : State} {bal : Bool} {b : Token} {ts : TokList} :
      XNonRelax E s b s1 → b.isBgroup = false →
      BodyRaw (raise "Missing { inserted" (backInput b s1)) 0 ts s2 → GeneralText E bal s ts s2

/-- The same, expanded: `\message`, `\write`, `\edef` body, `\expanded`.
Protected macros stay (etex.ch), `\the` and `\unexpanded` text stands. -/
inductive ExpandedText (E : Engine) : State → TokList → State → Prop
  | text {s s1 s2 : State} {b : Token} {ts : TokList} :
      XNonRelax E s b s1 → b.isBgroup = true → BodyX E s1 0 [] ts s2 → ExpandedText E s ts s2

/-- Collect an expanded body up to its balancing `}` (§ 477). -/
inductive BodyX (E : Engine) : State → Nat → TokList → TokList → State → Prop
  | close {s s1 : State} {acc : TokList} {t : Token} :
      XPiece E s (.tok t) s1 → t.isEgroup = true → BodyX E s 0 acc acc s1
  | closeInner {s s1 s2 : State} {d : Nat} {acc r : TokList} {t : Token} :
      XPiece E s (.tok t) s1 → t.isEgroup = true → BodyX E s1 d (acc ++ [t]) r s2 →
      BodyX E s (d + 1) acc r s2
  | openGroup {s s1 s2 : State} {d : Nat} {acc r : TokList} {t : Token} :
      XPiece E s (.tok t) s1 → t.isBgroup = true → BodyX E s1 (d + 1) (acc ++ [t]) r s2 →
      BodyX E s d acc r s2
  | tok {s s1 s2 : State} {d : Nat} {acc r : TokList} {t : Token} :
      XPiece E s (.tok t) s1 → t.isBgroup = false → t.isEgroup = false →
      BodyX E s1 d (acc ++ [t.stored]) r s2 → BodyX E s d acc r s2
  | verbatim {s s1 s2 : State} {d : Nat} {acc r ts : TokList} :
      XPiece E s (.verbatim ts) s1 → BodyX E s1 d (acc ++ ts) r s2 → BodyX E s d acc r s2

/-- One piece of an expanded body (§ 477–478). -/
inductive XPiece (E : Engine) : State → Piece → State → Prop
  | unexpandable {s s1 : State} {t : Token} :
      Next s t s1 → expandableTok s1 t = false → XPiece E s (.tok t) s1
  | prot {s s1 : State} {t : Token} {m : Macro} :
      Next s t s1 → meaningTok s1 t = .call m → m.prot = true → XPiece E s (.tok t) s1
  | the {s s1 s2 : State} {t : Token} {ts : TokList} :
      Next s t s1 → (cmdOf s1 t = some .the ∨ cmdOf s1 t = some .unexpanded) →
      TheToks E (backInput t s1) ts s2 → XPiece E s (.verbatim ts) s2
  | expand {s s1 s2 s3 : State} {t : Token} {p : Piece} :
      Next s t s1 → expandableTok s1 t = true →
      (∀ m, meaningTok s1 t = .call m → m.prot = false) →
      cmdOf s1 t ≠ some .the → cmdOf s1 t ≠ some .unexpanded →
      Expand E s s2 → XPiece E s2 p s3 → XPiece E s p s3

/-- `\the` and `\unexpanded` (§ 465, etex.ch `the_toks`). -/
inductive TheToks (E : Engine) : State → TokList → State → Prop
  | the {s s1 s2 s3 : State} {t u : Token} {v : IVal} :
      Next s t s1 → cmdOf s1 t = some .the → XNext E s1 u s2 →
      Internal E (backInput u s2) v s3 → TheToks E s (theText s3 v) s3
  | unexpanded {s s1 s2 : State} {t : Token} {ts : TokList} :
      Next s t s1 → cmdOf s1 t = some .unexpanded → GeneralText E false s1 ts s2 →
      TheToks E s ts s2

/-- `scan_file_name` (§ 526): characters after expansion up to a space
(consumed) or a non-character (kept back). TeX Live allows quotes. -/
inductive FileName (E : Engine) : State → List CharCode → State → Prop
  | start {s s1 s2 : State} {t : Token} {n : List CharCode} :
      XNonBlank E s t s1 → FileNameRest E (backInput t s1) false [] n s2 → FileName E s n s2

inductive FileNameRest (E : Engine) : State → Bool → List CharCode → List CharCode → State → Prop
  | quote {s s1 s2 : State} {q : Bool} {acc r : List CharCode} {k : Cat} :
      XNext E s (.char 34 k) s1 → FileNameRest E s1 (!q) acc r s2 → FileNameRest E s q acc r s2
  | char {s s1 s2 : State} {q : Bool} {acc r : List CharCode} {c : CharCode} {k : Cat} :
      XNext E s (.char c k) s1 → c ≠ 34 → (k ≠ .space ∨ q = true) →
      FileNameRest E s1 q (acc ++ [c]) r s2 → FileNameRest E s q acc r s2
  | endSpace {s s1 : State} {acc : List CharCode} :
      XNext E s Token.space s1 → FileNameRest E s false acc acc s1
  | endOther {s s1 : State} {q : Bool} {acc : List CharCode} {t : Token} :
      XNext E s t s1 → (match t with | .char _ _ => False | _ => True) →
      FileNameRest E s q acc acc (backInput t s1)

/-- `\csname` name characters after expansion up to `\endcsname` (§ 372).
`ok = false`: a non-character came, `\endcsname` is inserted (§ 373). -/
inductive CsName (E : Engine) : State → List CharCode → List CharCode → Bool → State → Prop
  | char {s s1 s2 : State} {acc r : List CharCode} {c : CharCode} {k : Cat} {ok : Bool} :
      XNext E s (.char c k) s1 → CsName E s1 (acc ++ [c]) r ok s2 → CsName E s acc r ok s2
  | finish {s s1 : State} {acc : List CharCode} {t : Token} :
      XNext E s t s1 → cmdOf s1 t = some .endcsname → CsName E s acc acc true s1
  | missing {s s1 : State} {acc : List CharCode} {t : Token} :
      XNext E s t s1 → cmdOf s1 t ≠ some .endcsname → (match t with | .char _ _ => False | _ => True) →
      CsName E s acc acc false (raise "Missing \\endcsname inserted" (backInput t s1))

/-- Test of a conditional (§§ 501–508, etex.ch, pdftex.web). -/
inductive Test (E : Engine) : IfOp → State → Bool → State → Prop
  | iftrue {s : State} : Test E .iftrue s true s
  | iffalse {s : State} : Test E .iffalse s false s
  | chars {s s1 s2 : State} {t u : Token} :
      XNext E s t s1 → XNext E s1 u s2 →
      Test E .charIf s ((charCat s2 t).1 == (charCat s2 u).1) s2
  | cats {s s1 s2 : State} {t u : Token} :
      XNext E s t s1 → XNext E s1 u s2 →
      Test E .catIf s ((charCat s2 t).2 == (charCat s2 u).2) s2
  | num {s s1 s2 s3 s4 : State} {a b : Int} {rel : Nat} {abs : Bool} {op : IfOp} :
      (op = .num ∧ abs = false ∨ op = .absnum ∧ abs = true) →
      ScanInt E s a s1 → Relation E s1 rel s2 → ScanInt E s2 b s3 → s4 = s3 →
      Test E op s (if abs then relHolds rel a.natAbs b.natAbs else relHolds rel a b) s4
  | dim {s s1 s2 s3 : State} {a b : Int × Nat} {rel : Nat} {abs : Bool} {op : IfOp} :
      (op = .dim ∧ abs = false ∨ op = .absdim ∧ abs = true) →
      ScanDimen E false false s a s1 → Relation E s1 rel s2 → ScanDimen E false false s2 b s3 →
      Test E op s (if abs then relHolds rel a.1.natAbs b.1.natAbs else relHolds rel a.1 b.1) s3
  | odd {s s1 : State} {n : Int} : ScanInt E s n s1 → Test E .odd s (n % 2 != 0) s1
  | vmode {s : State} : Test E .vmode s (decide (s.mode.kind = .vertical)) s
  | hmode {s : State} : Test E .hmode s (decide (s.mode.kind = .horizontal)) s
  | mmode {s : State} : Test E .mmode s (decide (s.mode.kind = .math)) s
  | inner {s : State} : Test E .inner s s.mode.inner s
  | box {s s1 : State} {n : Int} {op : IfOp} :
      (op = .void ∨ op = .hboxIf ∨ op = .vboxIf) → ScanInt E s n s1 → 0 ≤ n →
      Test E op s (match op, boxAt s1 n.toNat with
        | .void, .void => true
        | .hboxIf, .hbox _ _ _ => true
        | .vboxIf, .vbox _ _ _ => true
        | _, _ => false) s1
  /-- `\ifx` reads both tokens with `get_next` (§ 507). -/
  | ifx {s s1 s2 : State} {t u : Token} :
      Next s t s1 → Next s1 u s2 → Test E .ifx s (ifxEq (meaningTok s2 t) (meaningTok s2 u)) s2
  | eof {s s1 : State} {n : Int} :
      ScanInt E s n s1 →
      Test E .eof s (match s1.readStreams n.toNat with | .closed => true | _ => false) s1
  | defined {s s1 : State} {t : Token} :
      Next s t s1 →
      Test E .defined s (match meaningTok s1 t with | .undefined => false | _ => true) s1
  /-- etex.ch: the name is looked up, never entered. -/
  | csname {s s1 : State} {n : List CharCode} {ok : Bool} :
      CsName E { s with inCsname := s.inCsname + 1 } [] n ok s1 →
      Test E .csname s (ok && (match s1.meaningOf (.name (nameOf n)) with
                               | .undefined => false | _ => true))
        { s1 with inCsname := s.inCsname, lastCs := some (.name (nameOf n)) }
  | fontchar {s s1 s2 : State} {f : Nat} {c : Int} :
      Internal E s (.font f) s1 → ScanInt E s1 c s2 → Test E .fontchar s (E.hasChar f c.toNat) s2
  | incsname {s : State} : Test E .incsname s (decide (s.inCsname > 0)) s
  | primitive {s s1 : State} {t : Token} :
      Next s t s1 →
      Test E .primitive s (match t with
        | .cs (.name n) => (match s1.meaningOf (.name n), E.primitive n with
            | .prim m _, some _ => m == n
            | _, _ => false)
        | _ => false) s1

/-- The relation of `\ifnum`/`\ifdim` (§ 503): `<`, `=`, `>` of category
12; otherwise "Missing = inserted". -/
inductive Relation (E : Engine) : State → Nat → State → Prop
  | rel {s s1 : State} {c : Nat} :
      XNonBlank E s (.char c .other) s1 → (c = 60 ∨ c = 61 ∨ c = 62) → Relation E s c s1
  | missing {s s1 : State} {t : Token} :
      XNonBlank E s t s1 → t ≠ .char 60 .other → t ≠ .char 61 .other → t ≠ .char 62 .other →
      Relation E s 61 (raise "Missing = inserted for \\ifnum" (backInput t s1))

/-- `expand` (§ 366): one expansion of the next token. -/
inductive Expand (E : Engine) : State → State → Prop
  /-- A macro call (§§ 389–390): match the parameter text, then push the
  replacement text with the arguments substituted. -/
  | call {s s1 s2 : State} {t : Token} {m : Macro} {args : List TokList} :
      Next s t s1 → meaningTok s1 t = .call m →
      MatchParams m.long s1 (if m.braceDelim then m.params ++ [.tok (.char 123 .bgroup)] else m.params) args s2 →
      Expand E s (pushToks (.macroBody (match t with | .cs r => r | .noexp r => r | _ => .name ""))
                     (subst args m.body ++ (if m.braceDelim then [.char 123 .bgroup] else [])) s2)
  /-- "Undefined control sequence" (§ 370). -/
  | undefined {s s1 : State} {t : Token} :
      Next s t s1 → meaningTok s1 t = .undefined →
      Expand E s (raise "Undefined control sequence" s1)
  /-- `\expandafter` (§ 368). -/
  | expandafterX {s s1 s2 s3 s4 : State} {t t1 t2 : Token} :
      Next s t s1 → cmdOf s1 t = some .expandafter → Next s1 t1 s2 → Next s2 t2 s3 →
      expandableTok s3 t2 = true → Expand E (backInput t2 s3) s4 →
      Expand E s (backInput t1 s4)
  | expandafterN {s s1 s2 s3 : State} {t t1 t2 : Token} :
      Next s t s1 → cmdOf s1 t = some .expandafter → Next s1 t1 s2 → Next s2 t2 s3 →
      expandableTok s3 t2 = false →
      Expand E s (backInput t1 (backInput t2 s3))
  /-- `\noexpand` (§ 367): an expandable token comes back marked. -/
  | noexpandX {s s1 s2 : State} {t u : Token} :
      Next s t s1 → cmdOf s1 t = some .noexpand → Next s1 u s2 → expandableTok s2 u = true →
      Expand E s (pushToks .backedUp [match u with | .cs r => .noexp r | v => v] s2)
  | noexpandN {s s1 s2 : State} {t u : Token} :
      Next s t s1 → cmdOf s1 t = some .noexpand → Next s1 u s2 → expandableTok s2 u = false →
      Expand E s (backInput u s2)
  /-- `\csname` (§ 372): an undefined name becomes `\relax`, locally. -/
  | csname {s s1 s2 : State} {t : Token} {n : List CharCode} {ok : Bool} :
      Next s t s1 → cmdOf s1 t = some .csname →
      CsName E { s1 with inCsname := s1.inCsname + 1 } [] n ok s2 →
      Expand E s
        (let s3 := { s2 with inCsname := s1.inCsname, lastCs := some (.name (nameOf n)) }
         let r : CsRef := .name (nameOf n)
         let s4 := match s3.meaningOf r with
           | .undefined => eqDefine s3 (.meaning r) (.meaning (.prim "relax" .relax))
           | _ => s3
         pushToks .backedUp [.cs r] s4)
  /-- LuaTeX `\begincsname`: an undefined name gives nothing. -/
  | begincsname {s s1 s2 : State} {t : Token} {n : List CharCode} {ok : Bool} :
      Next s t s1 → cmdOf s1 t = some .begincsname →
      CsName E { s1 with inCsname := s1.inCsname + 1 } [] n ok s2 →
      Expand E s
        (let s3 := { s2 with inCsname := s1.inCsname, lastCs := some (.name (nameOf n)) }
         match s3.meaningOf (.name (nameOf n)) with
         | .undefined => s3
         | _ => pushToks .backedUp [.cs (.name (nameOf n))] s3)
  | lastnamedcs {s s1 : State} {t : Token} :
      Next s t s1 → cmdOf s1 t = some .lastnamedcs →
      Expand E s (match s1.lastCs with
        | some r => pushToks .backedUp [.cs r] s1
        | none => s1)
  /-- `\number` (§ 470). -/
  | number {s s1 s2 : State} {t : Token} {n : Int} :
      Next s t s1 → cmdOf s1 t = some (.convert .number) → ScanInt E s1 n s2 →
      Expand E s (pushToks .inserted (strToks (digitsOf n)) s2)
  | romannumeral {s s1 s2 : State} {t : Token} {n : Int} :
      Next s t s1 → cmdOf s1 t = some (.convert .romannumeral) → ScanInt E s1 n s2 →
      Expand E s (pushToks .inserted (strToks (romanOf n)) s2)
  /-- `\string` reads with `get_token` (§ 470). -/
  | string {s s1 s2 : State} {t u : Token} :
      Next s t s1 → cmdOf s1 t = some (.convert .string) → Next s1 u s2 →
      Expand E s (pushToks .inserted (strToks (match u with
        | .cs r => sprintCs s2 r
        | .noexp r => sprintCs s2 r
        | .char c _ => [c]
        | .outParam _ => [])) s2)
  | meaning {s s1 s2 : State} {t u : Token} :
      Next s t s1 → cmdOf s1 t = some (.convert .meaning) → Next s1 u s2 →
      Expand E s (pushToks .inserted (strToks (meaningText s2 (meaningTok s2 u))) s2)
  | fontname {s s1 s2 : State} {t : Token} {f : Nat} :
      Next s t s1 → cmdOf s1 t = some (.convert .fontname) → Internal E s1 (.font f) s2 →
      Expand E s (pushToks .inserted
        (strToks (strCodes ((nth? s2.fonts f).map Prod.fst |>.getD "nullfont"))) s2)
  | jobname {s s1 : State} {t : Token} :
      Next s t s1 → cmdOf s1 t = some (.convert .jobname) →
      Expand E s (pushToks .inserted (strToks (strCodes s1.jobname)) s1)
  | detokenize {s s1 s2 : State} {t : Token} {ts : TokList} :
      Next s t s1 → cmdOf s1 t = some (.convert .detokenize) → GeneralText E false s1 ts s2 →
      Expand E s (pushToks .inserted (strToks (showToks s2 ts)) s2)
  | etexRevision {s s1 : State} {t : Token} :
      Next s t s1 → cmdOf s1 t = some (.convert .etexRevision) →
      Expand E s (pushToks .inserted (strToks (strCodes ".6")) s1)
  /-- **[over]** pdfTeX, XeTeX and LuaTeX converters (`\pdfstrcmp`,
  `\pdfmdfivesum`, `\pdffilemoddate`, `\Uchar`, …): their operands are read
  by grammar; the text is any string of characters (§ 464 `str_toks`).
  SaTeX computes most exactly (see primitives.tsv). -/
  | convertOther {s s1 s2 : State} {t : Token} {n : String} {cs : List CharCode} :
      Next s t s1 → cmdOf s1 t = some (.convert (.other n)) → Operands E s1 (E.opsOf n) s2 →
      Expand E s (pushToks .inserted (strToks cs) s2)
  /-- `\the`, `\unexpanded` outside an expanded body (§ 367 `ins_the_toks`). -/
  | the {s s1 : State} {ts : TokList} :
      TheToks E s ts s1 → Expand E s (pushToks .inserted ts s1)
  /-- pdfTeX `\expanded`. -/
  | expanded {s s1 s2 : State} {t : Token} {ts : TokList} :
      Next s t s1 → cmdOf s1 t = some .expanded → ExpandedText E s1 ts s2 →
      Expand E s (pushToks .inserted ts s2)
  /-- `\scantokens` (etex.ch `pseudo_start`): the text printed, split at
  `\newlinechar`, read back as a file. -/
  | scantokens {s s1 s2 : State} {t : Token} {ts : TokList} {text : Bool} :
      Next s t s1 → cmdOf s1 t = some (.scantokens text) → GeneralText E false s1 ts s2 →
      Expand E s { s2 with input := .file { name := "", pseudo := true, text := text, lines := splitLines (s2.intPar "newlinechar") (showToks s2 ts) } :: s2.input }
  /-- A conditional (§ 498): push it, run the test, keep or skip. -/
  | ifTrue {s s1 s2 : State} {t : Token} {op : IfOp} {b neg : Bool} :
      CondHead s t op neg s1 → op ≠ .ifcase →
      Test E op { s1 with cond := { limit := .ifCode, op := op, negated := neg, line := lineNo s1 } :: s1.cond }
        b s2 →
      (b != neg) = true →
      Expand E s { s2 with cond := setLimitAt s2.cond s1.cond.length .elseCode }
  | ifFalse {s s1 s2 s3 : State} {t : Token} {op : IfOp} {b neg : Bool} :
      CondHead s t op neg s1 → op ≠ .ifcase →
      Test E op { s1 with cond := { limit := .ifCode, op := op, negated := neg, line := lineNo s1 } :: s1.cond }
        b s2 →
      (b != neg) = false →
      SkipFalse s2 s1.cond.length s3 →
      Expand E s s3
  | ifcase {s s1 s2 s3 : State} {t : Token} {n : Int} :
      Next s t s1 → cmdOf s1 t = some (.ifTest .ifcase) →
      ScanInt E { s1 with cond := { limit := .ifCode, op := .ifcase, line := lineNo s1 } :: s1.cond } n s2 →
      SkipCase { s2 with cond := setLimitAt s2.cond s1.cond.length .orCode } s1.cond.length n s3 →
      Expand E s s3
  /-- "Extra \or", "Extra \else", "Extra \fi" (§ 510), or `\relax`
  inserted while the test is read. -/
  | fiExtra {s s1 : State} {t : Token} {c : FiCode} :
      Next s t s1 → cmdOf s1 t = some (.fiOrElse c) →
      (match s1.cond with | [] => True | k :: _ => c.toNat > k.limit.toNat ∧ k.limit ≠ .ifCode) →
      Expand E s (raise "Extra \\fi, \\else or \\or" s1)
  | fiInsertRelax {s s1 : State} {t : Token} {c : FiCode} {k : CondLevel} {ks : List CondLevel} :
      Next s t s1 → cmdOf s1 t = some (.fiOrElse c) → s1.cond = k :: ks → k.limit = .ifCode →
      Expand E s (pushToks .inserted [frozenRelax, t.stored] s1)
  | fi {s s1 : State} {t : Token} {k : CondLevel} {ks : List CondLevel} :
      Next s t s1 → cmdOf s1 t = some (.fiOrElse .fi) → s1.cond = k :: ks → k.limit ≠ .ifCode →
      Expand E s { s1 with cond := ks }
  | elseOr {s s1 s2 : State} {t : Token} {c : FiCode} {k : CondLevel} {ks : List CondLevel} :
      Next s t s1 → cmdOf s1 t = some (.fiOrElse c) → c ≠ .fi → s1.cond = k :: ks →
      k.limit ≠ .ifCode → c.toNat ≤ k.limit.toNat →
      SkipToFi s1 s2 → Expand E s { s2 with cond := s2.cond.drop 1 }
  /-- `\input` (§ 537): push the file found. -/
  | input {s s1 s2 : State} {t : Token} {n : List CharCode} {key : String} {lines : List (List CharCode)} :
      Next s t s1 → cmdOf s1 t = some .input → FileName E s1 n s2 →
      E.findFile (nameOf n) = some key → s2.vfs key = some lines →
      Expand E s { s2 with input := .file { name := key, lines := lines } :: s2.input }
  /-- A missing file in batch or nonstop mode is fatal (§ 530). -/
  | inputMissing {s s1 s2 : State} {t : Token} {n : List CharCode} :
      Next s t s1 → cmdOf s1 t = some .input → FileName E s1 n s2 →
      (E.findFile (nameOf n) = none ∨ ∃ k, E.findFile (nameOf n) = some k ∧ s2.vfs k = none) →
      s2.interaction < 2 →
      Expand E s (fatalError "*** (job aborted, file error in nonstop mode)" s2)
  /-- **[over]** In scroll and errorstop mode TeX asks the terminal for a
  name: any file may be read. -/
  | inputPrompt {s s1 s2 : State} {t : Token} {n : List CharCode} {key : String}
      {lines : List (List CharCode)} :
      Next s t s1 → cmdOf s1 t = some .input → FileName E s1 n s2 →
      E.findFile (nameOf n) = none → s2.interaction ≥ 2 → s2.vfs key = some lines →
      Expand E s { s2 with input := .file { name := key, lines := lines } :: s2.input }
  | endinput {s s1 : State} {t : Token} :
      Next s t s1 → cmdOf s1 t = some .endinput →
      Expand E s { s1 with input := forceEofTop s1.input }
  /-- **[over]** `\topmark` and relatives (§ 386): marks of pages SaTeX
  never builds; any token list. The e-TeX classes read a number first. -/
  | marks {s s1 : State} {t : Token} {code : Nat} {ts : TokList} :
      Next s t s1 → cmdOf s1 t = some (.topBotMark code false) →
      Expand E s (pushToks .mark ts s1)
  | marksClass {s s1 s2 : State} {t : Token} {code : Nat} {ts : TokList} {n : Int} :
      Next s t s1 → cmdOf s1 t = some (.topBotMark code true) → ScanInt E s1 n s2 →
      Expand E s (pushToks .mark ts s2)
  /-- pdfTeX `\pdfprimitive` / `\primitive`: the name's INITEX meaning, read
  through a reserved frozen name (WellFormed maps it). -/
  | primitive {s s1 s2 : State} {t : Token} {n : String} {c : Cmd} :
      Next s t s1 → cmdOf s1 t = some .primitiveCmd → Next s1 (.cs (.name n)) s2 →
      E.primitive n = some c →
      Expand E s (pushToks .backedUp [.cs (.name ("primitive\\" ++ n))] s2)
  /-- **[over]** `\directlua`, `\luaescapestring` (expandable in LuaTeX):
  the text is read expanded; SaTeX runs the chunk in real Lua (SEM § 8).
  Any text comes back, any assignment may happen. -/
  | lua {s s1 s2 s3 : State} {t : Token} {n : String} {ts res : TokList} :
      Next s t s1 → cmdOf s1 t = some (.lua n) → (n == "directlua" || n == "luaescapestring") = true →
      ExpandedText E s1 ts s2 → s3.vfs = s2.vfs → s3.input = s2.input →
      Expand E s (pushToks .inserted res s3)
  /-- **[over]** A Lua function called through `token.set_lua` that is
  expandable: SaTeX runs it in real Lua. Any text, any assignments. -/
  | luaCall {s s1 s2 : State} {t : Token} {id : Nat} {ts : TokList} :
      Next s t s1 → meaningTok s1 t = .luaCall id false →
      s2.vfs = s1.vfs → s2.input = s1.input →
      Expand E s (pushToks .inserted ts s2)


end


/-! ## The stomach (§§ 1029–1380) -/

/-- The next token, unexpandable (`main_control` after `get_x_token`). -/
def Head (s : State) (t : Token) (s1 : State) : Prop := Next s t s1 ∧ expandableTok s1 t = false

/-- Prefix bits: 1 long, 2 outer, 4 global, 8 protected (§ 1211, etex.ch). -/
def pfxGlobal (s : State) : Bool := Nat.testBit s.pending.prefixes 2

/-- The end of every assignment (§ 1269): prefixes go, the
`\afterassignment` token comes. -/
def finishAssign (s : State) : State :=
  let s' := { s with pending := { s.pending with prefixes := 0, afterAssignment := none } }
  match s.pending.afterAssignment with
  | some t => backInput t s'
  | none => s'

def assignTo (s : State) (l : Loc) (v : Val) : State := finishAssign (define s (pfxGlobal s) l v)

/-- Commands of `prefixed_command` (§ 1210), which take prefixes. -/
def assignmentMeaning : Meaning → Bool
  | .prim _ c => match c with
    | .defCmd _ _ | .letCmd _ | .gletCmd | .pfx _ | .shorthandDef _ _ | .advance | .multiply | .divide
    | .intPar _ | .dimPar _ | .gluePar _ _ | .toksPar _ | .codeTable _ | .fontDef
    | .fontParam _ | .readCmd _ | .special _ | .interaction _ => true
    | .register k => k != .box
    | .mode n => n == "setbox"
    | _ => false
  | .reg k _ => k != .box
  | .font _ => true
  | _ => false

def isSpacer : Meaning → Bool | .char _ .space => true | _ => false

/-- `get_r_token` (§ 1215): the control sequence to define. -/
inductive RToken : State → CsRef → State → Prop
  | cs {s s1 : State} {r : CsRef} : Next s (.cs r) s1 → RToken s r s1
  | noexp {s s1 : State} {r : CsRef} : Next s (.noexp r) s1 → RToken s r s1
  | space {s s1 s2 : State} {r : CsRef} : Next s Token.space s1 → RToken s1 r s2 → RToken s r s2

/-- Parameter text (§ 474–476) up to the `{`: `n` arguments so far. -/
inductive ParamText : State → Nat → List ParamItem → Bool → State → Prop
  | brace {s s1 : State} {n : Nat} {t : Token} :
      Next s t s1 → t.isBgroup = true → ParamText s n [] false s1
  | arg {s s1 s2 s3 : State} {n : Nat} {c : CharCode} {ps : List ParamItem} {b : Bool} :
      Next s (.char c .param) s1 → Next s1 (.char (49 + n) .other) s2 → n < 9 →
      ParamText s2 (n + 1) ps b s3 → ParamText s n (.arg :: ps) b s3
  /-- `#{` (§ 476): the last argument is delimited by the brace. -/
  | hashBrace {s s1 s2 : State} {n : Nat} {c : CharCode} {t : Token} :
      Next s (.char c .param) s1 → Next s1 t s2 → t.isBgroup = true → ParamText s n [] true s2
  | tok {s s1 s2 : State} {n : Nat} {t : Token} {ps : List ParamItem} {b : Bool} :
      Next s t s1 → t.isBgroup = false → t.isEgroup = false →
      (match t with | .char _ .param => False | _ => True) →
      ParamText s1 n ps b s2 → ParamText s n (.tok t.stored :: ps) b s2

/-- Replacement text, unexpanded (§ 477, § 479): `##` is one `#`, `#k` an
argument. -/
inductive DefBody : Nat → State → Nat → TokList → State → Prop
  | close {np : Nat} {s s1 : State} {t : Token} : Next s t s1 → t.isEgroup = true → DefBody np s 0 [] s1
  | closeInner {np : Nat} {s s1 s2 : State} {d : Nat} {ts : TokList} {t : Token} :
      Next s t s1 → t.isEgroup = true → DefBody np s1 d ts s2 → DefBody np s (d + 1) (t :: ts) s2
  | openGroup {np : Nat} {s s1 s2 : State} {d : Nat} {ts : TokList} {t : Token} :
      Next s t s1 → t.isBgroup = true → DefBody np s1 (d + 1) ts s2 → DefBody np s d (t :: ts) s2
  | hashHash {np : Nat} {s s1 s2 s3 : State} {d : Nat} {ts : TokList} {c c' : CharCode} :
      Next s (.char c .param) s1 → Next s1 (.char c' .param) s2 → DefBody np s2 d ts s3 →
      DefBody np s d (.char c' .param :: ts) s3
  | argRef {np : Nat} {s s1 s2 s3 : State} {d k : Nat} {ts : TokList} {c : CharCode} :
      Next s (.char c .param) s1 → Next s1 (.char (48 + k) .other) s2 → 1 ≤ k → k ≤ np →
      DefBody np s2 d ts s3 → DefBody np s d (.outParam k :: ts) s3
  | tok {np : Nat} {s s1 s2 : State} {d : Nat} {ts : TokList} {t : Token} :
      Next s t s1 → t.isBgroup = false → t.isEgroup = false →
      (match t with | .char _ .param => False | _ => True) →
      DefBody np s1 d ts s2 → DefBody np s d (t.stored :: ts) s2

/-- Replacement text of `\edef`/`\xdef` (§ 477–479), with the pieces of
`XPiece`: `\the`/`\unexpanded` text is stored as it stands. -/
inductive DefBodyX (E : Engine) : Nat → State → Nat → TokList → State → Prop
  | close {np : Nat} {s s1 : State} {t : Token} :
      XPiece E s (.tok t) s1 → t.isEgroup = true → DefBodyX E np s 0 [] s1
  | closeInner {np : Nat} {s s1 s2 : State} {d : Nat} {ts : TokList} {t : Token} :
      XPiece E s (.tok t) s1 → t.isEgroup = true → DefBodyX E np s1 d ts s2 →
      DefBodyX E np s (d + 1) (t :: ts) s2
  | openGroup {np : Nat} {s s1 s2 : State} {d : Nat} {ts : TokList} {t : Token} :
      XPiece E s (.tok t) s1 → t.isBgroup = true → DefBodyX E np s1 (d + 1) ts s2 →
      DefBodyX E np s d (t :: ts) s2
  | hashHash {np : Nat} {s s1 s2 s3 : State} {d : Nat} {ts : TokList} {c c' : CharCode} :
      XPiece E s (.tok (.char c .param)) s1 → XPiece E s1 (.tok (.char c' .param)) s2 →
      DefBodyX E np s2 d ts s3 → DefBodyX E np s d (.char c' .param :: ts) s3
  | argRef {np : Nat} {s s1 s2 s3 : State} {d k : Nat} {ts : TokList} {c : CharCode} :
      XPiece E s (.tok (.char c .param)) s1 → XPiece E s1 (.tok (.char (48 + k) .other)) s2 →
      1 ≤ k → k ≤ np → DefBodyX E np s2 d ts s3 → DefBodyX E np s d (.outParam k :: ts) s3
  | tok {np : Nat} {s s1 s2 : State} {d : Nat} {ts : TokList} {t : Token} :
      XPiece E s (.tok t) s1 → t.isBgroup = false → t.isEgroup = false →
      (match t with | .char _ .param => False | _ => True) →
      DefBodyX E np s1 d ts s2 → DefBodyX E np s d (t.stored :: ts) s2
  | verbatim {np : Nat} {s s1 s2 : State} {d : Nat} {ts vs : TokList} :
      XPiece E s (.verbatim vs) s1 → DefBodyX E np s1 d ts s2 → DefBodyX E np s d (vs ++ ts) s2

def countArgs : List ParamItem → Nat
  | [] => 0
  | .arg :: ps => countArgs ps + 1
  | _ :: ps => countArgs ps

/-- The value an assignment to a location of this kind scans (§ 1228). -/
inductive Value (E : Engine) : RegKind → State → Val → State → Prop
  | int {s s1 : State} {n : Int} : ScanInt E s n s1 → Value E .count s (.int n) s1
  | dimen {s s1 : State} {d : Int × Nat} : ScanDimen E false false s d s1 → Value E .dimen s (.int d.1) s1
  | glue {s s1 : State} {g : Glue} : ScanGlue E false s g s1 → Value E .skip s (.glue g) s1
  | mu {s s1 : State} {g : Glue} : ScanGlue E true s g s1 → Value E .muskip s (.glue g) s1
  /-- A token register copies another (§ 1227) … -/
  | toksCopy {s s1 s2 : State} {t : Token} {ts : TokList} :
      XNonRelax E s t s1 → internalTok s1 t = true →
      Internal E (backInput t s1) (.toks ts) s2 → Value E .toks s (.toks ts) s2
  /-- … or takes a balanced text, unexpanded (§ 1226). -/
  | toksText {s s1 s2 : State} {t : Token} {ts : TokList} :
      XNonRelax E s t s1 → t.isBgroup = true → BodyRaw s1 0 ts s2 → Value E .toks s (.toks ts) s2

/-- `trap_zero_glue` (§ 1229). -/
def trapZero (g : Glue) : Glue := if g.width = 0 ∧ g.stretch = 0 ∧ g.shrink = 0 then {} else g

/-- `\advance` (§ 1238): no overflow check, 32-bit wrap (SEM § 11). -/
def advanceVal (a b : Val) : Val :=
  match a, b with
  | .int x, .int y => .int (wrap32 (x + y))
  | .glue g, .glue h =>
    let st := addOrder false g.stretch g.stretchOrder h.stretch h.stretchOrder
    let sh := addOrder false g.shrink g.shrinkOrder h.shrink h.shrinkOrder
    .glue (trapZero { width := wrap32 (g.width + h.width), stretch := wrap32 st.1, stretchOrder := st.2,
                      shrink := wrap32 sh.1, shrinkOrder := sh.2 })
  | v, _ => v

/-- `\multiply` and `\divide` (§ 1240): `none` is "Arithmetic overflow",
which leaves the register unchanged. -/
def scaleVal (k : RegKind) (mul : Bool) (a : Val) (n : Int) : Option Val :=
  let bound : Int := if k = .count then infinity else maxDimen
  let f : Int → Option Int := fun x =>
    if mul then (if (x * n).natAbs ≤ bound.natAbs then some (x * n) else none)
    else if n = 0 then none else some (tdiv x n)
  match a with
  | .int x => (f x).map .int
  | .glue g =>
    match f g.width, f g.stretch, f g.shrink with
    | some w, some st, some sh => some (.glue (trapZero { g with width := w, stretch := st, shrink := sh }))
    | _, _, _ => none
  | v => some v

/-- The variable after `\advance`, `\multiply`, `\divide` (§ 1237). -/
inductive ArithTarget (E : Engine) : State → Loc → RegKind → State → Prop
  | register {s s1 s2 : State} {t : Token} {k : RegKind} {n : Int} :
      XNext E s t s1 → cmdOf s1 t = some (.register k) → k ≠ .box → k ≠ .toks →
      ScanInt E s1 n s2 → 0 ≤ n → n.toNat ≤ E.maxReg → ArithTarget E s (.reg k n.toNat) k s2
  | meaning {s s1 : State} {t : Token} {k : RegKind} {i : Nat} :
      XNext E s t s1 → meaningTok s1 t = .reg k i → k ≠ .box → k ≠ .toks →
      ArithTarget E s (.reg k i) k s1
  | intPar {s s1 : State} {t : Token} {n : String} :
      XNext E s t s1 → cmdOf s1 t = some (.intPar n) → ArithTarget E s (.intPar n) .count s1
  | dimPar {s s1 : State} {t : Token} {n : String} :
      XNext E s t s1 → cmdOf s1 t = some (.dimPar n) → ArithTarget E s (.dimPar n) .dimen s1
  | gluePar {s s1 : State} {t : Token} {n : String} {mu : Bool} :
      XNext E s t s1 → cmdOf s1 t = some (.gluePar n mu) →
      ArithTarget E s (.gluePar n) (if mu then .muskip else .skip) s1

/-- `\read` (§§ 482–486): lines until braces balance. -/
inductive TokLine (cat : CharCode → Cat) : MouthState → List CharCode → TokList → Prop
  | done {st st' : MouthState} {cs : List CharCode} : LineTok cat st cs none st' [] → TokLine cat st cs []
  | more {st st' : MouthState} {cs cs' : List CharCode} {t : Token} {ts : TokList} :
      LineTok cat st cs (some t) st' cs' → TokLine cat st' cs' ts → TokLine cat st cs (t :: ts)

def depthAfter : Int → TokList → Int
  | d, [] => d
  | d, t :: ts => depthAfter (if t.isBgroup then d + 1 else if t.isEgroup then d - 1 else d) ts

inductive ReadLines (s : State) : Int → List (List CharCode) → TokList → ReadStream → Prop
  | line {d : Int} {l : List CharCode} {ls : List (List CharCode)} {ts : TokList} :
      TokLine s.catOf .newLine (prepareLine s l) ts → depthAfter d ts = 0 →
      ReadLines s d (l :: ls) ts (.normal ls)
  | more {d : Int} {l : List CharCode} {ls : List (List CharCode)} {ts rest : TokList} {r : ReadStream} :
      TokLine s.catOf .newLine (prepareLine s l) ts → depthAfter d ts ≠ 0 →
      ReadLines s (depthAfter d ts) ls rest r → ReadLines s d (l :: ls) (ts ++ rest) r
  /-- End of file: the stream closes, the empty line is read (§ 486). -/
  | eof {d : Int} {ts : TokList} :
      TokLine s.catOf .newLine (prepareLine s []) ts → ReadLines s d [] ts .closed

def streamLines : ReadStream → Option (List (List CharCode))
  | .closed => none
  | .justOpen ls => some ls
  | .normal ls => some ls

/-- `\write`, `\openout`, `\closeout`, now (`imm`) or as a whatsit (§§ 1350–1356, 1370–1374). -/
inductive WriteCmd (E : Engine) : Bool → State → Token → State → Prop
  | writeNow {s s1 s2 : State} {t : Token} {n : Int} {ts : TokList} :
      cmdOf s t = some .write → ScanInt E s n s1 → ExpandedText E s1 ts s2 →
      WriteCmd E true s t (logOut (if n = 18 then .shell (showToks s2 ts) else .write n (showToks s2 ts)) s2)
  /-- **[over]** A deferred `\write` is a whatsit; its text is expanded at
  `\shipout`, which SaTeX never reaches (SEM § 9). -/
  | writeLater {s s1 s2 : State} {t : Token} {n : Int} {ts : TokList} :
      cmdOf s t = some .write → ScanInt E s n s1 → GeneralText E false s1 ts s2 →
      WriteCmd E false s t (appendItem .whatsit s2)
  | openNow {s s1 s2 s3 : State} {t : Token} {n : Int} {f : List CharCode} :
      cmdOf s t = some .openout → ScanInt E s n s1 → OptEquals E s1 s2 → FileName E s2 f s3 →
      WriteCmd E true s t { s3 with writeOpen := upd s3.writeOpen n.toNat (some (nameOf f)) }
  | openLater {s s1 s2 s3 : State} {t : Token} {n : Int} {f : List CharCode} :
      cmdOf s t = some .openout → ScanInt E s n s1 → OptEquals E s1 s2 → FileName E s2 f s3 →
      WriteCmd E false s t (appendItem .whatsit s3)
  | closeNow {s s1 : State} {t : Token} {n : Int} :
      cmdOf s t = some .closeout → ScanInt E s n s1 →
      WriteCmd E true s t { s1 with writeOpen := upd s1.writeOpen n.toNat none }
  | closeLater {s s1 : State} {t : Token} {n : Int} :
      cmdOf s t = some .closeout → ScanInt E s n s1 → WriteCmd E false s t (appendItem .whatsit s1)

def isExtension : Option Cmd → Bool
  | some .write | some .openout | some .closeout => true
  | _ => false

/-- What closes a group when the wrong closer comes (§ 1065 `off_save`). -/
def closerFor : GroupCode → TokList
  | .semiSimple => [.cs (.name "endgroup")]
  | .mathShift => [.char 36 .mathShift]
  | .mathLeft => [.cs (.name "right"), .char 46 .other]
  | _ => [.char 125 .egroup]

/-- `insert_dollar_sign` (§ 1047). -/
def insertDollar (t : Token) (s : State) : State :=
  raise "Missing $ inserted" (pushToks .inserted [.char 36 .mathShift] (backInput t s))

/-- `new_graf` (§ 1091): a paragraph begins, `\everypar` comes. -/
def newGraf (indent : Bool) (s : State) : State :=
  let s1 := pushNest { kind := .horizontal, inner := false } s
  let s2 := if indent then appendItem .box s1 else s1
  insertEvery "everypar" s2

/-- `normal_paragraph` (§ 1070). -/
def normalParagraph (s : State) : State :=
  let s := eqDefine s (.intPar "looseness") (.int 0)
  let s := eqDefine s (.dimPar "hangindent") (.int 0)
  let s := eqDefine s (.intPar "hangafter") (.int 1)
  eqDefine s (.special "parshape" 0) (.int 0)

/-- Names whose command is math-only, or not allowed in math (§ 1046),
as `math_material` in src/engine/builtins.rs. -/
def mathOnly : List String :=
  ["mathchar", "delimiter", "radical", "mathaccent", "mathord", "mathop", "mathbin", "mathrel",
   "mathopen", "mathclose", "mathpunct", "mathinner", "underline", "overline", "over", "atop",
   "above", "overwithdelims", "atopwithdelims", "abovewithdelims", "displaystyle", "textstyle",
   "scriptstyle", "scriptscriptstyle", "mathchoice", "vcenter", "nonscript", "mkern", "mskip",
   "limits", "nolimits", "displaylimits", "left", "right", "middle", "Umathchar", "Umathcharnum",
   "Udelimiter", "Uradical", "Umathaccent"]

def nonMath : List String :=
  ["par", "end", "dump", "vskip", "vfil", "vfill", "vss", "vfilneg", "unvbox", "unvcopy",
   "pagediscards", "splitdiscards", "valign", "hrule"]

/-- `scan_spec` (§ 645): `to d`, `spread d`, or nothing. -/
inductive BoxSpec (E : Engine) : State → State → Prop
  | none_ {s s1 s2 : State} :
      Keyword E s "to".toList [] false s1 → Keyword E s1 "spread".toList [] false s2 → BoxSpec E s s2
  | to {s s1 s2 : State} {d : Int × Nat} :
      Keyword E s "to".toList [] true s1 → ScanDimen E false false s1 d s2 → BoxSpec E s s2
  | spread {s s1 s2 s3 : State} {d : Int × Nat} :
      Keyword E s "to".toList [] false s1 → Keyword E s1 "spread".toList [] true s2 →
      ScanDimen E false false s2 d s3 → BoxSpec E s s3

/-- Where a finished box goes (§ 1075 `box_end`). -/
inductive BoxEnd (E : Engine) : BoxContext → BoxVal → State → State → Prop
  | shift {s : State} {d : Scaled} {b : BoxVal} :
      BoxEnd E (.shift d) b s (if b = .void then s else appendItem .box s)
  | setBox {s : State} {glob : Bool} {n : Nat} {b : BoxVal} :
      BoxEnd E (.setBox glob n) b s (define s glob (.reg .box n) (.box b))
  /-- **[over]** Leaders take the glue command that must follow (§ 1078);
  the glue is scanned by its grammar. -/
  | leaders {s s1 s2 : State} {b : BoxVal} {t : Token} {n k : String} {h : Bool} :
      XNonRelax E s t s1 → meaningTok s1 t = .prim n (.material h k) →
      Operands E s1 (E.opsOf n) s2 → BoxEnd E .leaders b s (appendItem (.glue {}) s2)
  /-- **[over]** `\shipout`: the box goes; deferred `\write`s it carries
  run then, which SaTeX does not model: any output may be added. -/
  | shipout {s : State} {b : BoxVal} {o : List Output} :
      BoxEnd E .shipout b s { s with out := o ++ s.out }

/-- `begin_box` (§§ 1079–1084) in a context. -/
inductive BeginBox (E : Engine) : BoxContext → State → Token → State → Prop
  /-- `\box n`: the register is emptied in place, no save entry (§ 1079). -/
  | box {s s1 s2 : State} {ctx : BoxContext} {t : Token} {n : Int} :
      cmdOf s t = some (.register .box) → ScanInt E s n s1 → 0 ≤ n →
      BoxEnd E ctx (boxAt s1 n.toNat)
        { s1 with eqtb := upd s1.eqtb (.reg .box n.toNat) { s1.eqtb (.reg .box n.toNat) with val := .box .void } } s2 →
      BeginBox E ctx s t s2
  | copy {s s1 s2 : State} {ctx : BoxContext} {t : Token} {n : Int} :
      cmdOf s t = some (.boxCmd "copy") → ScanInt E s n s1 → 0 ≤ n →
      BoxEnd E ctx (boxAt s1 n.toNat) s1 s2 → BeginBox E ctx s t s2
  /-- **[over]** `\lastbox` (§ 1080): the last box of the list, any size. -/
  | lastbox {s s1 : State} {ctx : BoxContext} {t : Token} {b : BoxVal} :
      cmdOf s t = some (.boxCmd "lastbox") → s.mode.kind ≠ .math →
      BoxEnd E ctx (if tailItem s = some .box then b else .void)
        (match s.nest with
         | l :: rest => if tailItem s = some .box then { s with nest := { l with items := l.items.drop 1 } :: rest } else s
         | [] => s) s1 →
      BeginBox E ctx s t s1
  /-- **[over]** `\vsplit n to d` (§ 1082): any box, any remainder. -/
  | vsplit {s s1 s2 s3 s4 : State} {ctx : BoxContext} {t : Token} {n : Int} {d : Int × Nat}
      {b r : BoxVal} :
      cmdOf s t = some (.boxCmd "vsplit") → ScanInt E s n s1 → Keyword E s1 "to".toList [] true s2 →
      ScanDimen E false false s2 d s3 → 0 ≤ n →
      BoxEnd E ctx b
        { s3 with eqtb := upd s3.eqtb (.reg .box n.toNat) { s3.eqtb (.reg .box n.toNat) with val := .box r } } s4 →
      BeginBox E ctx s t s4
  /-- `\hbox`, `\vbox`, `\vtop` (§ 1083): spec, `{`, a group of its own. -/
  | open_ {s s1 s2 s3 : State} {ctx : BoxContext} {t b : Token} {n : String} :
      cmdOf s t = some (.boxCmd n) → (n = "hbox" ∨ n = "vbox" ∨ n = "vtop") →
      BoxSpec E s s1 → XNonRelax E s1 b s2 → b.isBgroup = true →
      s3 = (let g : GroupCode := if n = "hbox" then .hbox else if n = "vbox" then .vbox else .vtop
            let s4 := pushNest { kind := if n = "hbox" then .horizontal else .vertical, inner := true }
                        (newSaveLevel g (some ctx) s2)
            insertEvery (if n = "hbox" then "everyhbox" else "everyvbox") s4) →
      BeginBox E ctx s t s3

def isBoxCmd : Option Cmd → Bool
  | some (.boxCmd n) => n == "copy" || n == "lastbox" || n == "vsplit" || n == "hbox" || n == "vbox" || n == "vtop"
  | some (.register .box) => true
  | _ => false

/-- `scan_box` (§ 1084): the next non-blank non-relax token must begin a box. -/
inductive ScanBox (E : Engine) : BoxContext → State → State → Prop
  | box {s s1 s2 : State} {ctx : BoxContext} {t : Token} :
      XNonRelax E s t s1 → isBoxCmd (cmdOf s1 t) = true → BeginBox E ctx s1 t s2 → ScanBox E ctx s s2
  | missing {s s1 : State} {ctx : BoxContext} {t : Token} :
      XNonRelax E s t s1 → isBoxCmd (cmdOf s1 t) = false →
      ScanBox E ctx s (raise "A <box> was supposed to be here" (backInput t s1))

/-- A math delimiter (§ 1160 `scan_delimiter`), by grammar. -/
inductive Delim (E : Engine) : State → State → Prop
  | char {s s1 : State} {c : CharCode} {k : Cat} :
      XNonRelax E s (.char c k) s1 → (k = .letter ∨ k = .other) → Delim E s s1
  | cmd {s s1 s2 : State} {t : Token} {n : String} {c : Cmd} :
      XNonRelax E s t s1 → meaningTok s1 t = .prim n c → Operands E s1 (E.opsOf n) s2 → Delim E s s2

def nameOfMeaning : Meaning → String
  | .prim n _ => n
  | _ => ""

def charOfMeaning : Meaning → Option CharCode
  | .char c .letter => some c
  | .char c .other => some c
  | .chardef n => some n
  | _ => none

/-- `\let`'s right side (§ 1221): spaces, an optional `=`, one optional space. -/
inductive LetRhs : State → Token → State → Prop
  | plain {s s1 : State} {t : Token} :
      Next s t s1 → t ≠ Token.space → t ≠ .char 61 .other → LetRhs s t s1
  | eq {s s1 s2 : State} {t : Token} :
      Next s (.char 61 .other) s1 → Next s1 t s2 → t ≠ Token.space → LetRhs s t s2
  | eqSpace {s s1 s2 s3 : State} {t : Token} :
      Next s (.char 61 .other) s1 → Next s1 Token.space s2 → Next s2 t s3 → LetRhs s t s3
  | space {s s1 s2 : State} {t : Token} :
      Next s Token.space s1 → LetRhs s1 t s2 → LetRhs s t s2

/-- `n` dimensions (`\parshape`). -/
inductive Dimens (E : Engine) : Nat → State → List Int → State → Prop
  | nil {s : State} : Dimens E 0 s [] s
  | cons {s s1 s2 : State} {n : Nat} {d : Int × Nat} {ds : List Int} :
      ScanDimen E false false s d s1 → Dimens E n s1 ds s2 → Dimens E (n + 1) s (d.1 :: ds) s2

/-- `n` integers (penalty arrays). -/
inductive Ints (E : Engine) : Nat → State → List Int → State → Prop
  | nil {s : State} : Ints E 0 s [] s
  | cons {s s1 s2 : State} {n : Nat} {i : Int} {is : List Int} :
      ScanInt E s i s1 → Ints E n s1 is s2 → Ints E (n + 1) s (i :: is) s2

/-- `\font`'s `at d` or `scaled n` (§ 1258); out-of-range values are errors
and give the design size. -/
inductive FontSize (E : Engine) : State → Scaled → State → Prop
  | at_ {s s1 s2 : State} {d : Int × Nat} :
      Keyword E s "at".toList [] true s1 → ScanDimen E false false s1 d s2 →
      FontSize E s (if 0 < d.1 ∧ d.1 < 2048 * 65536 then d.1 else 10 * 65536)
        (if 0 < d.1 ∧ d.1 < 2048 * 65536 then s2 else raise "Improper `at' size" s2)
  | scaled {s s1 s2 s3 : State} {n : Int} :
      Keyword E s "at".toList [] false s1 → Keyword E s1 "scaled".toList [] true s2 → ScanInt E s2 n s3 →
      FontSize E s (if 0 < n ∧ n ≤ 32768 then -n else -1000)
        (if 0 < n ∧ n ≤ 32768 then s3 else raise "Illegal magnification" s3)
  | design {s s1 s2 : State} :
      Keyword E s "at".toList [] false s1 → Keyword E s1 "scaled".toList [] false s2 → FontSize E s (-1000) s2

/-- The stream state after `\openin` (§ 1275): the file's lines, or closed. -/
def openedStream (E : Engine) (s : State) (f : List CharCode) : ReadStream :=
  match E.findFile (nameOf f) with
  | some k => match s.vfs k with | some ls => .justOpen ls | none => .closed
  | none => .closed

/-- The box register entry after `\wd`, `\ht`, `\dp` (§ 1247). -/
def boxDimEntry (s : State) (which : Nat) (d : Int) (n : Nat) : Entry :=
  { s.eqtb (.reg .box n) with val := .box (setBoxDim which d (boxAt s n)) }

/-- One step of the stomach (§ 1030 `main_control`, § 1211 `prefixed_command`). -/
inductive Exec (E : Engine) : State → State → Prop
  | expand {s s1 : State} : Expand E s s1 → Exec E s s1
  /-- The run read past its last file (§ 360): "*** (job aborted, no legal \end found)". -/
  | noInput {s : State} : s.input = [] → Exec E s (fatalError "*** (job aborted, no legal \\end found)" s)
  /-- Prefixes (§ 1211): spaces and `\relax` between them are skipped. -/
  | prefix_ {s s1 : State} {t : Token} {p : Nat} :
      Head s t s1 → cmdOf s1 t = some (.pfx p) →
      Exec E s { s1 with pending := { s1.pending with prefixes := Nat.lor s1.pending.prefixes p } }
  | prefixSkip {s s1 : State} {t : Token} :
      Head s t s1 → s.pending.prefixes ≠ 0 → (isSpacer (meaningTok s1 t) ∨ isRelax (cmdOf s1 t)) → Exec E s s1
  /-- "You can't use a prefix with …" (§ 1212). -/
  | prefixBad {s s1 : State} {t : Token} :
      Head s t s1 → s.pending.prefixes ≠ 0 → assignmentMeaning (meaningTok s1 t) = false →
      ¬ (isSpacer (meaningTok s1 t) ∨ isRelax (cmdOf s1 t)) →
      Exec E s (raise "You can't use a prefix with this" (backInput t
        { s1 with pending := { s1.pending with prefixes := 0 } }))
  | relax {s s1 : State} {t : Token} :
      Head s t s1 → s.pending.prefixes = 0 → isRelax (cmdOf s1 t) = true → Exec E s s1
  | ignorespaces {s s1 s2 : State} {t u : Token} :
      Head s t s1 → cmdOf s1 t = some .ignorespaces → XNonBlank E s1 u s2 → Exec E s (backInput u s2)
  | endcsname {s s1 : State} {t : Token} :
      Head s t s1 → cmdOf s1 t = some .endcsname → Exec E s (raise "Extra \\endcsname" s1)
  /-- `\def`, `\gdef` (§ 1218). -/
  | def_ {s s1 s2 s3 s4 : State} {t : Token} {glob : Bool} {r : CsRef} {ps : List ParamItem}
      {b : Bool} {body : TokList} :
      Head s t s1 → cmdOf s1 t = some (.defCmd glob false) → RToken s1 r s2 →
      ParamText s2 0 ps b s3 → DefBody (countArgs ps) s3 0 body s4 →
      Exec E s (finishAssign (define s4 (glob || pfxGlobal s4) (.meaning r) (.meaning (.call
        { long := Nat.testBit s4.pending.prefixes 0, outer := Nat.testBit s4.pending.prefixes 1,
          prot := Nat.testBit s4.pending.prefixes 3, params := ps, braceDelim := b, body := body }))))
  /-- `\edef`, `\xdef` (§ 1218, § 477). -/
  | edef {s s1 s2 s3 s4 : State} {t : Token} {glob : Bool} {r : CsRef} {ps : List ParamItem}
      {b : Bool} {body : TokList} :
      Head s t s1 → cmdOf s1 t = some (.defCmd glob true) → RToken s1 r s2 →
      ParamText s2 0 ps b s3 → DefBodyX E (countArgs ps) s3 0 body s4 →
      Exec E s (finishAssign (define s4 (glob || pfxGlobal s4) (.meaning r) (.meaning (.call
        { long := Nat.testBit s4.pending.prefixes 0, outer := Nat.testBit s4.pending.prefixes 1,
          prot := Nat.testBit s4.pending.prefixes 3, params := ps, braceDelim := b, body := body }))))
  /-- `\let` (§ 1221): one optional space after `=`. -/
  | let_ {s s1 s2 s3 s4 : State} {t u : Token} {r : CsRef} :
      Head s t s1 → cmdOf s1 t = some (.letCmd false) → RToken s1 r s2 →
      LetRhs s2 u s3 → s4 = s3 →
      Exec E s (assignTo s4 (.meaning r) (.meaning (meaningTok s3 u)))
  /-- LuaTeX `\glet`: `\global\let`. -/
  | glet {s s1 s2 s3 : State} {t u : Token} {r : CsRef} :
      Head s t s1 → cmdOf s1 t = some .gletCmd → RToken s1 r s2 → LetRhs s2 u s3 →
      Exec E s (finishAssign (define s3 true (.meaning r) (.meaning (meaningTok s3 u))))
  /-- `\futurelet` (§ 1221): both tokens stay in the input. -/
  | futurelet {s s1 s2 s3 s4 : State} {t t1 t2 : Token} {r : CsRef} :
      Head s t s1 → cmdOf s1 t = some (.letCmd true) → RToken s1 r s2 →
      Next s2 t1 s3 → Next s3 t2 s4 →
      Exec E s (assignTo (backInput t1 (backInput t2 s4)) (.meaning r) (.meaning (meaningTok s4 t2)))
  /-- `\chardef`, `\mathchardef`, `\countdef`, … (§ 1224): the name is
  `\relax` while the number is read. -/
  | shorthandDef {s s1 s2 s3 s4 : State} {t : Token} {k : RegKind} {cl : Nat} {r : CsRef} {n : Int} :
      Head s t s1 → cmdOf s1 t = some (.shorthandDef k cl) → RToken s1 r s2 →
      OptEquals E (define s2 (pfxGlobal s2) (.meaning r) (.meaning (.prim "relax" .relax))) s3 →
      ScanInt E s3 n s4 →
      0 ≤ n → n.toNat ≤ (if cl = 1 then charMax E else if cl = 2 then 32768 else E.maxReg) →
      Exec E s (assignTo s4 (.meaning r) (.meaning
        (if cl = 1 then .chardef n.toNat else if cl = 2 then .mathchardef n.toNat else .reg k n.toNat)))
  /-- `\count n = v` and relatives (§ 1224). -/
  | register {s s1 s2 s3 s4 : State} {t : Token} {k : RegKind} {n : Int} {v : Val} :
      Head s t s1 → cmdOf s1 t = some (.register k) → k ≠ .box →
      ScanInt E s1 n s2 → 0 ≤ n → n.toNat ≤ E.maxReg → OptEquals E s2 s3 → Value E k s3 v s4 →
      Exec E s (assignTo s4 (.reg k n.toNat) v)
  | regMeaning {s s1 s2 s3 : State} {t : Token} {k : RegKind} {i : Nat} {v : Val} :
      Head s t s1 → meaningTok s1 t = .reg k i → k ≠ .box →
      OptEquals E s1 s2 → Value E k s2 v s3 → Exec E s (assignTo s3 (.reg k i) v)
  | intPar {s s1 s2 s3 : State} {t : Token} {n : String} {v : Val} :
      Head s t s1 → cmdOf s1 t = some (.intPar n) → OptEquals E s1 s2 → Value E .count s2 v s3 →
      Exec E s (if n = "interactionmode"
        then finishAssign { s3 with interaction := (match v with | .int i => i.toNat | _ => s3.interaction) }
        else assignTo s3 (.intPar n) v)
  | dimPar {s s1 s2 s3 : State} {t : Token} {n : String} {v : Val} :
      Head s t s1 → cmdOf s1 t = some (.dimPar n) → OptEquals E s1 s2 → Value E .dimen s2 v s3 →
      Exec E s (assignTo s3 (.dimPar n) v)
  | gluePar {s s1 s2 s3 : State} {t : Token} {n : String} {mu : Bool} {v : Val} :
      Head s t s1 → cmdOf s1 t = some (.gluePar n mu) → OptEquals E s1 s2 →
      Value E (if mu then .muskip else .skip) s2 v s3 →
      Exec E s (assignTo s3 (.gluePar n) (match v with | .glue g => .glue (trapZero g) | w => w))
  | toksPar {s s1 s2 s3 : State} {t : Token} {n : String} {v : Val} :
      Head s t s1 → cmdOf s1 t = some (.toksPar n) → OptEquals E s1 s2 → Value E .toks s2 v s3 →
      Exec E s (assignTo s3 (.toksPar n) v)
  /-- `\advance` (§ 1238). -/
  | advance {s s1 s2 s3 s4 : State} {t : Token} {l : Loc} {k : RegKind} {b : Bool} {v : Val} :
      Head s t s1 → cmdOf s1 t = some .advance → ArithTarget E s1 l k s2 →
      Keyword E s2 "by".toList [] b s3 → Value E k s3 v s4 →
      Exec E s (assignTo s4 l (advanceVal (s4.get l) v))
  /-- `\multiply`, `\divide` (§ 1240). -/
  | scale {s s1 s2 s3 s4 : State} {t : Token} {l : Loc} {k : RegKind} {b mul : Bool} {n : Int} :
      Head s t s1 → (cmdOf s1 t = some .multiply ∧ mul = true ∨ cmdOf s1 t = some .divide ∧ mul = false) →
      ArithTarget E s1 l k s2 → Keyword E s2 "by".toList [] b s3 → ScanInt E s3 n s4 →
      Exec E s (match scaleVal k mul (s4.get l) n with
        | some v => assignTo s4 l v
        | none => finishAssign (raise "Arithmetic overflow" s4))
  /-- `\catcode`, `\lccode`, … (§ 1232): an invalid code is an error and 0. -/
  | code {s s1 s2 s3 s4 : State} {t : Token} {tab : CodeTable} {c v : Int} :
      Head s t s1 → cmdOf s1 t = some (.codeTable tab) → ScanInt E s1 c s2 →
      0 ≤ c → c.toNat ≤ charMax E → OptEquals E s2 s3 → ScanInt E s3 v s4 →
      Exec E s (if 0 ≤ v ∧ v ≤ codeMax E tab then assignTo s4 (.code tab c.toNat) (.int v)
                else assignTo (raise "Invalid code" s4) (.code tab c.toNat) (.int 0))
  /-- `\uppercase`, `\lowercase` (§§ 1285–1288). -/
  | caseShift {s s1 s2 : State} {t : Token} {up : Bool} {ts : TokList} :
      Head s t s1 → cmdOf s1 t = some (.caseShift up) → GeneralText E false s1 ts s2 →
      Exec E s (backList (ts.map fun u => match u with
        | .char c k =>
          let m := s2.intAt (.code (if up then .uc else .lc) c)
          if m ≠ 0 then .char m.toNat k else u
        | v => v) s2)
  /-- `\font\cs=name at d` (§§ 1257–1260); fonts with the same name and size
  are shared. -/
  | fontDef {s s1 s2 s3 s4 s5 : State} {t : Token} {r : CsRef} {n : List CharCode} {size : Scaled} :
      Head s t s1 → cmdOf s1 t = some .fontDef → RToken s1 r s2 →
      OptEquals E (define s2 (pfxGlobal s2) (.meaning r) (.meaning (.font 0))) s3 →
      FileName E s3 n s4 → FontSize E s4 size s5 →
      Exec E s (let key := nameOf n ++ "@" ++ toString size
        match indexOf? key (s5.fonts.map Prod.fst) with
        | some i => assignTo s5 (.meaning r) (.meaning (.font i))
        | none => match E.loadFont (nameOf n) size with
          | some ps => assignTo { s5 with fonts := s5.fonts ++ [(key, ps)] } (.meaning r)
                         (.meaning (.font s5.fonts.length))
          | none => assignTo (logOut (.message (strCodes "font-not-loadable")) s5) (.meaning r)
                      (.meaning (.font 0)))
  /-- Selecting a font is a local assignment (§ 1217). -/
  | selectFont {s s1 : State} {t : Token} {f : Nat} :
      Head s t s1 → meaningTok s1 t = .font f → Exec E s (assignTo s1 .curFont (.int (Int.ofNat f)))
  /-- `\fontdimen n f = d` is global (§ 1253). -/
  | fontdimen {s s1 s2 s3 s4 s5 : State} {t : Token} {n : Int} {f : Nat} {d : Int × Nat} :
      Head s t s1 → cmdOf s1 t = some (.fontParam "fontdimen") → ScanInt E s1 n s2 →
      Internal E s2 (.font f) s3 → OptEquals E s3 s4 → ScanDimen E false false s4 d s5 →
      Exec E s (finishAssign { s5 with fonts := updAt s5.fonts f (fun p => (p.1, upd p.2 n.toNat d.1)) })
  /-- `\hyphenchar`, `\skewchar` and per-font codes: global (§ 1253). -/
  | fontInt {s s1 s2 s3 s4 : State} {t : Token} {n : String} {f : Nat} {v : Int} :
      Head s t s1 → cmdOf s1 t = some (.fontParam n) → n ≠ "fontdimen" →
      Operands E s1 (E.opsOf n) s2 → OptEquals E s2 s3 → ScanInt E s3 v s4 →
      Exec E s (finishAssign (geqDefine s4 (.special n f) (.int v)))
  /-- `\openin n = name` (§ 1275). -/
  | openin {s s1 s2 s3 s4 : State} {t : Token} {n : Int} {f : List CharCode} :
      Head s t s1 → cmdOf s1 t = some .openin → ScanInt E s1 n s2 → OptEquals E s2 s3 →
      FileName E s3 f s4 →
      Exec E s { s4 with readStreams := upd s4.readStreams n.toNat (openedStream E s4 f) }
  | closein {s s1 s2 : State} {t : Token} {n : Int} :
      Head s t s1 → cmdOf s1 t = some .closein → ScanInt E s1 n s2 →
      Exec E s { s2 with readStreams := upd s2.readStreams n.toNat .closed }
  /-- `\read n to \cs` (§ 1225, §§ 482–486). -/
  | read {s s1 s2 s3 s4 : State} {t : Token} {n : Int} {b : Bool} {r : CsRef} {ls : List (List CharCode)}
      {ts : TokList} {st : ReadStream} :
      Head s t s1 → cmdOf s1 t = some (.readCmd false) → ScanInt E s1 n s2 →
      Keyword E s2 "to".toList [] b s3 → RToken s3 r s4 →
      0 ≤ n → n < 16 → streamLines (s4.readStreams n.toNat) = some ls →
      ReadLines s4 0 ls ts st →
      Exec E s (assignTo { (if b then s4 else raise "Missing `to' inserted" s4) with
          readStreams := upd s4.readStreams n.toNat st } (.meaning r) (.meaning (.call { body := ts })))
  /-- `\read` from the terminal in batch or nonstop mode is fatal (§ 484). -/
  | readTerminalFatal {s s1 s2 s3 s4 : State} {t : Token} {n : Int} {b : Bool} {r : CsRef} :
      Head s t s1 → cmdOf s1 t = some (.readCmd false) → ScanInt E s1 n s2 →
      Keyword E s2 "to".toList [] b s3 → RToken s3 r s4 →
      (n < 0 ∨ n ≥ 16 ∨ streamLines (s4.readStreams n.toNat) = none) → s4.interaction ≤ 1 →
      Exec E s (fatalError "*** (cannot \\read from terminal in nonstop modes)" s4)
  /-- **[over]** From the terminal otherwise: any line. -/
  | readTerminal {s s1 s2 s3 s4 : State} {t : Token} {n : Int} {b : Bool} {r : CsRef} {ts : TokList} :
      Head s t s1 → cmdOf s1 t = some (.readCmd false) → ScanInt E s1 n s2 →
      Keyword E s2 "to".toList [] b s3 → RToken s3 r s4 →
      (n < 0 ∨ n ≥ 16 ∨ streamLines (s4.readStreams n.toNat) = none) → s4.interaction ≥ 2 →
      Exec E s (assignTo s4 (.meaning r) (.meaning (.call { body := ts })))
  /-- `\readline` (etex_man § 3.2): one line, characters of category 12,
  spaces 10. -/
  | readline {s s1 s2 s3 s4 : State} {t : Token} {n : Int} {b : Bool} {r : CsRef}
      {l : List CharCode} {ls : List (List CharCode)} :
      Head s t s1 → cmdOf s1 t = some (.readCmd true) → ScanInt E s1 n s2 →
      Keyword E s2 "to".toList [] b s3 → RToken s3 r s4 →
      0 ≤ n → n < 16 → streamLines (s4.readStreams n.toNat) = some (l :: ls) →
      Exec E s (assignTo { s4 with readStreams := upd s4.readStreams n.toNat (.normal ls) } (.meaning r)
        (.meaning (.call { body := strToks (prepareLine s4 l) })))
  | readlineEof {s s1 s2 s3 s4 : State} {t : Token} {n : Int} {b : Bool} {r : CsRef} :
      Head s t s1 → cmdOf s1 t = some (.readCmd true) → ScanInt E s1 n s2 →
      Keyword E s2 "to".toList [] b s3 → RToken s3 r s4 →
      0 ≤ n → n < 16 → streamLines (s4.readStreams n.toNat) = some [] →
      Exec E s (assignTo { s4 with readStreams := upd s4.readStreams n.toNat .closed } (.meaning r)
        (.meaning (.call { body := strToks (prepareLine s4 []) })))
  | readlineTerminalFatal {s s1 s2 s3 s4 : State} {t : Token} {n : Int} {b : Bool} {r : CsRef} :
      Head s t s1 → cmdOf s1 t = some (.readCmd true) → ScanInt E s1 n s2 →
      Keyword E s2 "to".toList [] b s3 → RToken s3 r s4 →
      (n < 0 ∨ n ≥ 16 ∨ streamLines (s4.readStreams n.toNat) = none) → s4.interaction ≤ 1 →
      Exec E s (fatalError "*** (cannot \\read from terminal in nonstop modes)" s4)
  | write {s s1 s2 : State} {t : Token} :
      Head s t s1 → isExtension (cmdOf s1 t) = true → WriteCmd E false s1 t s2 → Exec E s s2
  /-- `\immediate` (§ 1375): an extension after it runs now; anything
  else is read again. -/
  | immediate {s s1 s2 s3 : State} {t u : Token} :
      Head s t s1 → cmdOf s1 t = some .immediate → XNext E s1 u s2 → isExtension (cmdOf s2 u) = true →
      WriteCmd E true s2 u s3 → Exec E s s3
  | immediateOther {s s1 s2 : State} {t u : Token} :
      Head s t s1 → cmdOf s1 t = some .immediate → XNext E s1 u s2 → isExtension (cmdOf s2 u) = false →
      Exec E s (backInput u s2)
  /-- `\message`, `\errmessage` (§ 1279). -/
  | message {s s1 s2 : State} {t : Token} {err : Bool} {ts : TokList} :
      Head s t s1 → cmdOf s1 t = some (.message err) → ExpandedText E s1 ts s2 →
      Exec E s (if err then raise (nameOf (showToks s2 ts)) s2 else logOut (.message (showToks s2 ts)) s2)
  /-- `\end` (§ 1054): in vertical mode the job ends. **[over]** TeX may
  first run the output routine for pending material, which SaTeX never does. -/
  | end_ {s s1 : State} {t : Token} :
      Head s t s1 → cmdOf s1 t = some .endCmd → s1.mode = { kind := .vertical, inner := false } →
      Exec E s { s1 with halted := true }
  | endInHorizontal {s s1 : State} {t : Token} :
      Head s t s1 → cmdOf s1 t = some .endCmd → s1.mode.kind = .horizontal → s1.mode.inner = false →
      Exec E s (pushToks .inserted [parToken] (backInput t s1))
  /-- `\show…` (§§ 1291–1298): the answer goes to the log. **[over]** its
  text is any text here. -/
  | showCmd {s s1 s2 : State} {t : Token} {n : String} {o : List CharCode} :
      Head s t s1 → cmdOf s1 t = some (.showCmd n) → Operands E s1 (E.opsOf n) s2 →
      Exec E s (logOut (.message o) s2)
  /-- `\batchmode` … `\errorstopmode` (§ 1265): not saved by groups. -/
  | interaction {s s1 : State} {t : Token} {n : Nat} :
      Head s t s1 → cmdOf s1 t = some (.interaction n) → Exec E s (finishAssign { s1 with interaction := n })
  /-- `\parshape n d₁ l₁ …` (§ 1248). -/
  | parshape {s s1 s2 s3 s4 : State} {t : Token} {n : Int} {ds : List Int} :
      Head s t s1 → cmdOf s1 t = some (.special "parshape") → OptEquals E s1 s2 → ScanInt E s2 n s3 →
      Dimens E (2 * n.toNat) s3 ds s4 →
      Exec E s (finishAssign ((List.range (2 * n.toNat + 1)).foldl (fun st i =>
        define st (pfxGlobal s4) (.special "parshape" i)
          (.int (if i = 0 then (if n ≤ 0 then 0 else n) else (ds.getD (i - 1) 0)))) s4))
  /-- `\interlinepenalties n p₁ …` (etex_man § 3.13). -/
  | penalties {s s1 s2 s3 s4 : State} {t : Token} {nm : String} {n : Int} {ps : List Int} :
      Head s t s1 → cmdOf s1 t = some (.special nm) →
      nm ∈ ["interlinepenalties", "clubpenalties", "widowpenalties", "displaywidowpenalties"] →
      OptEquals E s1 s2 → ScanInt E s2 n s3 → Ints E n.toNat s3 ps s4 →
      Exec E s (finishAssign ((List.range (n.toNat + 1)).foldl (fun st i =>
        define st (pfxGlobal s4) (.special nm i)
          (.int (if i = 0 then (if n ≤ 0 then 0 else n) else (ps.getD (i - 1) 0)))) s4))
  /-- `\wd n = d` (§ 1247): changes the box in place, no save entry. -/
  | boxDim {s s1 s2 s3 s4 : State} {t : Token} {n : Int} {which : Nat} {d : Int × Nat} {nm : String} :
      Head s t s1 → cmdOf s1 t = some (.special nm) →
      (nm = "wd" ∧ which = 0 ∨ nm = "ht" ∧ which = 1 ∨ nm = "dp" ∧ which = 2) →
      ScanInt E s1 n s2 → 0 ≤ n → OptEquals E s2 s3 → ScanDimen E false false s3 d s4 →
      Exec E s (finishAssign { s4 with eqtb := upd s4.eqtb (.reg .box n.toNat) (boxDimEntry s4 which d.1 n.toNat) })
  /-- Per-character tables of the extended engines (`\XeTeXcharclass`,
  `\Umathcode`, …): index, optional `=`, value. -/
  | specialTable {s s1 s2 s3 s4 : State} {t : Token} {nm : String} {i v : Int} :
      Head s t s1 → cmdOf s1 t = some (.special nm) →
      nm ∉ ["wd", "ht", "dp", "parshape", "interlinepenalties", "clubpenalties", "widowpenalties",
            "displaywidowpenalties", "XeTeXinterchartoks"] →
      ScanInt E s1 i s2 → OptEquals E s2 s3 → ScanInt E s3 v s4 →
      Exec E s (assignTo s4 (.special nm i.toNat) (.int v))
  /-- **[over]** `\XeTeXinterchartoks a b = {…}`: stored where SaTeX reads
  it, which this formalisation leaves open: any value. -/
  | interchartoks {s s1 s2 s3 s4 : State} {t : Token} {i j : Int} {v : Val} :
      Head s t s1 → cmdOf s1 t = some (.special "XeTeXinterchartoks") →
      ScanInt E s1 i s2 → ScanInt E s2 j s3 → Value E .toks s3 v s4 →
      Exec E s (finishAssign s4)
  /-- `\setbox n = box` (§ 1241): a prefixed command; `\afterassignment`'s
  token comes at once, before the box's text (§ 1269). -/
  | setbox {s s1 s2 s3 s4 : State} {t : Token} {n : Int} :
      Head s t s1 → cmdOf s1 t = some (.mode "setbox") → ScanInt E s1 n s2 → 0 ≤ n → n.toNat ≤ E.maxReg →
      OptEquals E s2 s3 →
      ScanBox E (.setBox (isGlobal s3 (pfxGlobal s3)) n.toNat) (finishAssign s3) s4 →
      Exec E s s4
  /-- A box in a list (§ 1073 `box_context` 0). -/
  | box {s s1 s2 : State} {t : Token} :
      Head s t s1 → isBoxCmd (cmdOf s1 t) = true → BeginBox E (.shift 0) s1 t s2 → Exec E s s2
  /-- `\raise`, `\lower` (not in vertical mode), `\moveleft`, `\moveright`
  (only there) (§ 1073). -/
  | shift {s s1 s2 s3 : State} {t : Token} {n : String} {d : Int × Nat} :
      Head s t s1 → cmdOf s1 t = some (.mode n) →
      ((n = "raise" ∨ n = "lower") ∧ s1.mode.kind ≠ .vertical ∨
       (n = "moveleft" ∨ n = "moveright") ∧ s1.mode.kind = .vertical) →
      ScanDimen E false false s1 d s2 →
      ScanBox E (.shift (if n = "lower" ∨ n = "moveright" then -d.1 else d.1)) s2 s3 → Exec E s s3
  | leaders {s s1 s2 : State} {t : Token} :
      Head s t s1 → cmdOf s1 t = some (.mode "leaders") → ScanBox E .leaders s1 s2 → Exec E s s2
  | shipout {s s1 s2 : State} {t : Token} :
      Head s t s1 → cmdOf s1 t = some (.mode "shipout") → ScanBox E .shipout s1 s2 → Exec E s s2
  /-- `\vcenter` in math (§ 1167). -/
  | vcenter {s s1 s2 s3 : State} {t b : Token} :
      Head s t s1 → cmdOf s1 t = some (.boxCmd "vcenter") → s1.mode.kind = .math →
      BoxSpec E s1 s2 → XNonRelax E s2 b s3 → b.isBgroup = true →
      Exec E s (insertEvery "everyvbox" (pushNest { kind := .vertical, inner := true } (newSaveLevel .vcenter none s3)))
  /-- `\insert n {`, `\vadjust {` (§ 1097). -/
  | insert {s s1 s2 s3 : State} {t b : Token} {n : Int} :
      Head s t s1 → cmdOf s1 t = some (.mode "insert") → ScanInt E s1 n s2 → XNonRelax E s2 b s3 →
      b.isBgroup = true →
      Exec E s (pushNest { kind := .vertical, inner := true } (newSaveLevel .insert none s3))
  | vadjust {s s1 s2 : State} {t b : Token} :
      Head s t s1 → cmdOf s1 t = some (.mode "vadjust") → s1.mode.kind ≠ .vertical →
      XNonRelax E s1 b s2 → b.isBgroup = true →
      Exec E s (pushNest { kind := .vertical, inner := true } (newSaveLevel .insert none s2))
  /-- `\par` (§ 1094, § 1096): in a paragraph it ends it. **[over]** The
  lines it breaks into are any boxes. -/
  | parEnd {s s1 : State} {t : Token} :
      Head s t s1 → cmdOf s1 t = some (.mode "par") → s1.mode = { kind := .horizontal, inner := false } →
      Exec E s (normalParagraph (appendItem .box (popNest { s1 with errorCount := 0 })))
  | parOther {s s1 : State} {t : Token} :
      Head s t s1 → cmdOf s1 t = some (.mode "par") → s1.mode.kind ≠ .math →
      s1.mode ≠ { kind := .horizontal, inner := false } →
      Exec E s (normalParagraph s1)
  /-- `\indent`, `\noindent`, `\quitvmode` (§§ 1088–1093). -/
  | startPar {s s1 : State} {t : Token} {n : String} :
      Head s t s1 → cmdOf s1 t = some (.mode n) → (n = "indent" ∨ n = "noindent" ∨ n = "quitvmode") →
      Exec E s (if s1.mode.kind = .vertical then newGraf (n != "noindent") s1
                else if n = "indent" then appendItem .box s1 else s1)
  /-- Horizontal material in a vertical mode starts a paragraph and is read
  again (§ 1090). -/
  | hmaterialInV {s s1 : State} {t : Token} :
      Head s t s1 → (∃ h, cmdOf s1 t = some (.material true h)) → s1.mode.kind = .vertical →
      Exec E s (newGraf true (backInput t s1))
  /-- Vertical material in a paragraph ends it first (§ 1094 `head_for_vmode`). -/
  | vmaterialInH {s s1 : State} {t : Token} :
      Head s t s1 → (∃ h, cmdOf s1 t = some (.material false h)) →
      s1.mode = { kind := .horizontal, inner := false } →
      Exec E s (pushToks .inserted [parToken] (backInput t s1))
  /-- … and in restricted horizontal mode closes the open group (§ 1064). -/
  | vmaterialInRestricted {s s1 : State} {t : Token} :
      Head s t s1 → (∃ h, cmdOf s1 t = some (.material false h)) →
      s1.mode = { kind := .horizontal, inner := true } →
      Exec E s (if topGroup s1 = .bottom then raise "Extra" s1
                else raise "Missing } inserted" (pushToks .inserted (closerFor (topGroup s1)) (backInput t s1)))
  /-- Material in its own mode: operands by grammar. **[over]** The item it
  makes has any size (`\unhbox`, `\discretionary`, rules, glue). -/
  | material {s s1 s2 : State} {t : Token} {n k : String} {h : Bool} {i : Item} :
      Head s t s1 → meaningTok s1 t = .prim n (.material h k) →
      (if h then s1.mode.kind ≠ .vertical else s1.mode.kind = .vertical) →
      (s1.mode.kind = .math → n ∉ nonMath) →
      Operands E s1 (E.opsOf n) s2 → Exec E s (appendItem i s2)
  /-- Math-only commands outside math, non-math ones in it (§ 1046). -/
  | dollarMissing {s s1 : State} {t : Token} :
      Head s t s1 →
      (s1.mode.kind ≠ .math ∧ nameOfMeaning (meaningTok s1 t) ∈ mathOnly ∨
       s1.mode.kind = .math ∧ nameOfMeaning (meaningTok s1 t) ∈ nonMath) →
      Exec E s (insertDollar t s1)
  /-- A character, in a vertical mode: a paragraph begins (§ 1090). -/
  | charV {s s1 : State} {t : Token} {c : CharCode} :
      Head s t s1 → charOfMeaning (meaningTok s1 t) = some c → s1.mode.kind = .vertical →
      Exec E s (newGraf true (backInput t s1))
  /-- … in a paragraph it is appended (§ 1034 `main_loop`). -/
  | charH {s s1 : State} {t : Token} {c : CharCode} :
      Head s t s1 → charOfMeaning (meaningTok s1 t) = some c → s1.mode.kind = .horizontal →
      Exec E s (appendItem .char s1)
  /-- … in math, a `"8000` math code makes it active (§ 1151). -/
  | charMathActive {s s1 : State} {t : Token} {c : CharCode} :
      Head s t s1 → charOfMeaning (meaningTok s1 t) = some c → s1.mode.kind = .math →
      s1.intAt (.code .math c) = 32768 → Exec E s (backInput (.cs (.active c)) s1)
  | charMath {s s1 : State} {t : Token} {c : CharCode} :
      Head s t s1 → charOfMeaning (meaningTok s1 t) = some c → s1.mode.kind = .math →
      s1.intAt (.code .math c) ≠ 32768 → Exec E s (appendItem .other s1)
  /-- `\mathchardef`'d characters in math, `^`, `_` (§§ 1151–1176). -/
  | mathItem {s s1 : State} {t : Token} :
      Head s t s1 → s1.mode.kind = .math →
      ((∃ n, meaningTok s1 t = .mathchardef n) ∨ (∃ c, meaningTok s1 t = .char c .sup) ∨
       (∃ c, meaningTok s1 t = .char c .sub)) →
      Exec E s (appendItem .other s1)
  | mathItemOutside {s s1 : State} {t : Token} :
      Head s t s1 → s1.mode.kind ≠ .math →
      ((∃ n, meaningTok s1 t = .mathchardef n) ∨ (∃ c, meaningTok s1 t = .char c .sup) ∨
       (∃ c, meaningTok s1 t = .char c .sub)) →
      Exec E s (insertDollar t s1)
  /-- A space: glue in a paragraph, nothing elsewhere (§ 1045). **[over]**
  the glue's size. -/
  | spaceH {s s1 : State} {t : Token} {g : Glue} :
      Head s t s1 → isSpacer (meaningTok s1 t) = true → s1.mode.kind = .horizontal →
      s.pending.prefixes = 0 → Exec E s (appendItem (.glue g) s1)
  | spaceOther {s s1 : State} {t : Token} :
      Head s t s1 → isSpacer (meaningTok s1 t) = true → s1.mode.kind ≠ .horizontal →
      s.pending.prefixes = 0 → Exec E s s1
  /-- `{` (§ 1063); in math a math group (§ 1154). -/
  | bgroup {s s1 : State} {t : Token} {c : CharCode} :
      Head s t s1 → meaningTok s1 t = .char c .bgroup →
      Exec E s (if s1.mode.kind = .math then pushNest { kind := .math, inner := true } (newSaveLevel .math none s1)
                else newSaveLevel .simple none s1)
  /-- `}` (§§ 1068–1069, 1085–1086), by the group it closes. -/
  | egroupSimple {s s1 : State} {t : Token} {c : CharCode} :
      Head s t s1 → meaningTok s1 t = .char c .egroup → topGroup s1 = .simple →
      Exec E s (unsave s1).1
  | egroupBottom {s s1 : State} {t : Token} {c : CharCode} :
      Head s t s1 → meaningTok s1 t = .char c .egroup → topGroup s1 = .bottom →
      Exec E s (raise "Too many }'s" s1)
  | egroupExtra {s s1 : State} {t : Token} {c : CharCode} :
      Head s t s1 → meaningTok s1 t = .char c .egroup →
      (topGroup s1 = .semiSimple ∨ topGroup s1 = .mathShift ∨ topGroup s1 = .mathLeft) →
      Exec E s (raise "Extra }, or forgotten \\endgroup" s1)
  /-- A box group ends: the box goes where its context says (§ 1086).
  **[over]** its size. -/
  | egroupBox {s s1 s2 : State} {t : Token} {c : CharCode} {ctx : BoxContext} {g : GroupCode}
      {b : BoxVal} :
      Head s t s1 → meaningTok s1 t = .char c .egroup →
      (unsave s1).2 = some (g, some ctx) → (g = .hbox ∨ g = .adjustedHbox ∨ g = .vbox ∨ g = .vtop) →
      (match g, b with
       | .hbox, .hbox _ _ _ => True | .adjustedHbox, .hbox _ _ _ => True
       | .vbox, .vbox _ _ _ => True | .vtop, .vbox _ _ _ => True | _, _ => False) →
      BoxEnd E ctx b (popNest (unsave s1).1) s2 → Exec E s s2
  /-- Other list groups end: math, insert, vcenter (§§ 1100, 1168, 1186). -/
  | egroupList {s s1 : State} {t : Token} {c : CharCode} :
      Head s t s1 → meaningTok s1 t = .char c .egroup →
      (topGroup s1 = .math ∨ topGroup s1 = .insert ∨ topGroup s1 = .vcenter ∨
       topGroup s1 = .mathChoice ∨ topGroup s1 = .disc) →
      Exec E s (appendItem .other (popNest (unsave s1).1))
  /-- **[over]** `\mathchoice` and `\discretionary` take four and three
  groups (§§ 1117, 1172); the count is not kept: another may follow. -/
  | egroupPart {s s1 s2 : State} {t b : Token} {c : CharCode} {g : GroupCode} {m : Mode} :
      Head s t s1 → meaningTok s1 t = .char c .egroup → topGroup s1 = g →
      (g = .mathChoice ∨ g = .disc) → m = s1.mode →
      XNonRelax E (popNest (unsave s1).1) b s2 → b.isBgroup = true →
      Exec E s (pushNest m (newSaveLevel g none s2))
  | mathChoice {s s1 s2 : State} {t b : Token} :
      Head s t s1 → cmdOf s1 t = some (.mode "mathchoice") → s1.mode.kind = .math →
      XNonRelax E s1 b s2 → b.isBgroup = true →
      Exec E s (pushNest { kind := .math, inner := true } (newSaveLevel .mathChoice none s2))
  | mathStyle {s s1 : State} {t : Token} :
      Head s t s1 → cmdOf s1 t = some (.mode "mathstyle") → s1.mode.kind = .math →
      Exec E s (appendItem .other s1)
  /-- `\begingroup`, `\endgroup` (§§ 1063, 1069). -/
  | begingroup {s s1 : State} {t : Token} :
      Head s t s1 → cmdOf s1 t = some .beginGroup → Exec E s (newSaveLevel .semiSimple none s1)
  | endgroup {s s1 : State} {t : Token} :
      Head s t s1 → cmdOf s1 t = some .endGroup → topGroup s1 = .semiSimple → Exec E s (unsave s1).1
  | endgroupBottom {s s1 : State} {t : Token} :
      Head s t s1 → cmdOf s1 t = some .endGroup → topGroup s1 = .bottom →
      Exec E s (raise "Extra \\endgroup" s1)
  | endgroupWrong {s s1 : State} {t : Token} :
      Head s t s1 → cmdOf s1 t = some .endGroup → topGroup s1 ≠ .semiSimple → topGroup s1 ≠ .bottom →
      Exec E s (raise "Missing } inserted" (pushToks .inserted (closerFor (topGroup s1)) (backInput t s1)))
  /-- `\aftergroup` (§ 1271): kept on the save stack. -/
  | aftergroup {s s1 s2 : State} {t u : Token} :
      Head s t s1 → cmdOf s1 t = some .aftergroup → Next s1 u s2 →
      Exec E s (if s2.curLevel > 1 then { s2 with save := .insertToken u.stored :: s2.save } else s2)
  /-- `\afterassignment` (§ 1269): replaces the token kept. -/
  | afterassignment {s s1 s2 : State} {t u : Token} :
      Head s t s1 → cmdOf s1 t = some .afterassignment → Next s1 u s2 →
      Exec E s { s2 with pending := { s2.pending with afterAssignment := some u.stored } }
  /-- `$` (§§ 1137–1140): math begins; `$$` in a paragraph is display
  math. -/
  | mathShiftV {s s1 : State} {t : Token} {c : CharCode} :
      Head s t s1 → meaningTok s1 t = .char c .mathShift → s1.mode.kind = .vertical →
      Exec E s (newGraf true (backInput t s1))
  | mathInline {s s1 s2 : State} {t u : Token} {c : CharCode} :
      Head s t s1 → meaningTok s1 t = .char c .mathShift → s1.mode.kind = .horizontal →
      Next s1 u s2 → (s1.mode.inner = true ∨ (match meaningTok s2 u with | .char _ .mathShift => False | _ => True)) →
      Exec E s (insertEvery "everymath" (pushNest { kind := .math, inner := true }
        (newSaveLevel .mathShift none (backInput u s2))))
  /-- **[over]** Display math ends the paragraph so far into any lines. -/
  | mathDisplay {s s1 s2 : State} {t u : Token} {c d : CharCode} :
      Head s t s1 → meaningTok s1 t = .char c .mathShift → s1.mode = { kind := .horizontal, inner := false } →
      Next s1 u s2 → meaningTok s2 u = .char d .mathShift →
      Exec E s (insertEvery "everydisplay" (pushNest { kind := .math, inner := false }
        (newSaveLevel .mathShift none (appendItem .box (popNest s2)))))
  | mathEnd {s s1 : State} {t : Token} {c : CharCode} :
      Head s t s1 → meaningTok s1 t = .char c .mathShift → s1.mode = { kind := .math, inner := true } →
      topGroup s1 = .mathShift → Exec E s (appendItem .math (popNest (unsave s1).1))
  /-- The end of display math (§§ 1194–1200): a second `$` is required, then
  the paragraph resumes after an optional space. -/
  | displayEnd {s s1 s2 s3 : State} {t u : Token} {c : CharCode} :
      Head s t s1 → meaningTok s1 t = .char c .mathShift → s1.mode = { kind := .math, inner := false } →
      topGroup s1 = .mathShift → XNext E (unsave s1).1 u s2 →
      s3 = (match meaningTok s2 u with
            | .char _ .mathShift => s2
            | _ => raise "Display math should end with $$" (backInput u s2)) →
      Exec E s (pushNest { kind := .horizontal, inner := false } (appendItem .box (popNest s3)))
  | mathShiftWrong {s s1 : State} {t : Token} {c : CharCode} :
      Head s t s1 → meaningTok s1 t = .char c .mathShift → s1.mode.kind = .math →
      topGroup s1 ≠ .mathShift →
      Exec E s (raise "Missing } inserted" (pushToks .inserted (closerFor (topGroup s1)) (backInput t s1)))
  /-- `\eqno`, `\leqno` in display math (§ 1140). **[over]** placement. -/
  | eqno {s s1 : State} {t : Token} :
      Head s t s1 → cmdOf s1 t = some (.mode "eqno") → s1.mode = { kind := .math, inner := false } →
      topGroup s1 = .mathShift →
      Exec E s (insertEvery "everymath" (pushNest { kind := .math, inner := true } (newSaveLevel .mathShift none s1)))
  /-- `\left` (§ 1191): a delimiter, then a group of its own. -/
  | left {s s1 s2 : State} {t : Token} :
      Head s t s1 → cmdOf s1 t = some (.mode "left") → s1.mode.kind = .math → Delim E s1 s2 →
      Exec E s (pushNest { kind := .math, inner := true } (newSaveLevel .mathLeft none s2))
  | right {s s1 s2 : State} {t : Token} :
      Head s t s1 → cmdOf s1 t = some (.mode "right") → s1.mode.kind = .math →
      topGroup s1 = .mathLeft → Delim E s1 s2 →
      Exec E s (appendItem .other (popNest (unsave s2).1))
  /-- e-TeX `\middle`: closes and reopens (etex.ch). -/
  | middle {s s1 s2 : State} {t : Token} :
      Head s t s1 → cmdOf s1 t = some (.mode "middle") → s1.mode.kind = .math →
      topGroup s1 = .mathLeft → Delim E s1 s2 →
      Exec E s (pushNest { kind := .math, inner := true } (newSaveLevel .mathLeft none (popNest (unsave s2).1)))
  | rightExtra {s s1 s2 : State} {t : Token} {n : String} :
      Head s t s1 → cmdOf s1 t = some (.mode n) → (n = "right" ∨ n = "middle") → s1.mode.kind = .math →
      topGroup s1 = .mathShift → Delim E s1 s2 → Exec E s (raise "Extra \\right" s2)
  /-- **[over]** Alignments (§§ 768–812: `\halign`, `\valign`, `\noalign`,
  `\cr`, `\span`, `\omit`, `&`): templates are not modelled. The
  alignment may do anything but change the file system. -/
  | alignment {s s1 s2 : State} {t : Token} {n : String} :
      Head s t s1 → (n ∈ ["halign", "valign", "noalign", "cr", "crcr", "span", "omit"]) →
      nameOfMeaning (meaningTok s1 t) = n → s2.vfs = s1.vfs → Exec E s s2
  /-- `&` outside an alignment (§ 1128). -/
  | alignTab {s s1 : State} {t : Token} :
      Head s t s1 → ((∃ c, meaningTok s1 t = .char c .alignTab) ∨ cmdOf s1 t = some .alignTab) →
      Exec E s (raise "Misplaced alignment tab character &" s1)
  /-- `#` in the stomach (§ 1045 `report_illegal_case`), and internal
  quantities used as commands. -/
  | illegal {s s1 : State} {t : Token} :
      Head s t s1 →
      ((∃ c, meaningTok s1 t = .char c .param) ∨ cmdOf s1 t = some .alignMark ∨
       (∃ op, cmdOf s1 t = some (.lastItem op)) ∨ (∃ l, cmdOf s1 t = some (.expr l))) →
      Exec E s (raise "You can't use this here" s1)
  /-- Commands SaTeX scans but whose effect is typesetting, PDF output or a
  whatsit (§§ 1341–1380, pdfTeX/XeTeX manuals): operands by grammar.
  **[over]** an item of any kind may be appended. -/
  | operands {s s1 s2 : State} {t : Token} {ops : List Operand} {i : Option Item} :
      Head s t s1 → cmdOf s1 t = some (.operands ops) →
      (s1.mode.kind ≠ .math → nameOfMeaning (meaningTok s1 t) ∉ mathOnly) →
      Operands E s1 ops s2 →
      Exec E s (match i with | some it => appendItem it s2 | none => s2)
  /-- **[over]** Lua (`\latelua`, catcode tables, Lua-defined protected
  commands): SaTeX runs real Lua. Any assignment, any text read next. -/
  | lua {s s1 s2 s3 : State} {t : Token} {ts : TokList} :
      Head s t s1 →
      ((∃ n, cmdOf s1 t = some (.lua n)) ∨ (∃ id, meaningTok s1 t = .luaCall id true)) →
      Operands E s1 (E.opsOf (nameOfMeaning (meaningTok s1 t))) s2 →
      s3.vfs = s2.vfs → s3.input = s2.input → Exec E s (pushToks .inserted ts s3)
  | setFontId {s s1 s2 : State} {t : Token} {f : Int} :
      Head s t s1 → cmdOf s1 t = some .setFontId → ScanInt E s1 f s2 →
      Exec E s (assignTo s2 .curFont (.int f))
  /-- **[over]** A primitive SaTeX does not model (`Unmodeled`): it reads
  no operands there (an assumption SaTeX reports as unsupported) and may
  change the mode and the list. -/
  | unmodeled {s s1 : State} {t : Token} {m : Mode} {i : List Item} :
      Head s t s1 → cmdOf s1 t = some .unmodeled →
      Exec E s (match s1.nest with
        | l :: rest => { s1 with nest := { l with mode := m, items := i } :: rest }
        | [] => s1)

/-! ## The step relation -/

/-- One step of a live run: `\end` or a fatal error stops it. -/
def Step (E : Engine) (s s' : State) : Prop := s.halted = false ∧ s.fatal = false ∧ Exec E s s'

/-- Reflexive-transitive closure: runs. -/
inductive Steps (E : Engine) : State → State → Prop
  | refl {s : State} : Steps E s s
  | step {s s1 s2 : State} : Step E s s1 → Steps E s1 s2 → Steps E s s2

/-- States the rules assume (§ 222): the frozen names mean their primitives,
and `\pdfprimitive`'s reserved names mean what INITEX gave. -/
def WellFormed (E : Engine) (s : State) : Prop :=
  (match s.meaningOf (.name "frozen\\relax") with | .prim "relax" .relax => True | _ => False) ∧
  ∀ n c, E.primitive n = some c →
    ∃ m, s.meaningOf (.name ("primitive\\" ++ n)) = .prim m c

end Concrete
end Satex
