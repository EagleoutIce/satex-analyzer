/-
Satex.Concrete: the concrete semantics of the TeX SaTeX interprets.

Written with AI assistance from Claude Code (Claude Opus 5.5; build fixes with
Claude Sonnet 5.5), directed and reviewed by the author.

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
  /-- The design size a font file states (§ 568 `font_dsize`). -/
  designSize : String → Scaled := fun _ => 10 * 65536
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
  /-- One name per primitive identity (`cur_cmd`, `cur_chr`): engine aliases
  such as XeTeX's `\XeTeXmathcode` for `\Umathcode` map to one name. -/
  primId : String → String := id

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

/-- Inverse of `nameOf`'s escape: `U+FFFF` then a tag stands for `U+FFFF`
itself or for a surrogate code, which a `Char` cannot hold. -/
def decodeName : List Char → List CharCode
  | '\uFFFF' :: d :: r => (if d = '\uFFFF' then 0xFFFF else d.toNat - 0xE000 + 0xD800) :: decodeName r
  | c :: r => c.toNat :: decodeName r
  | [] => []

def strCodes (s : String) : List CharCode := decodeName s.toList

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
-- source: tex.web § 263 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L5618
-- source: SaTeX `meaning_text` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
def sprintCs (s : State) (r : CsRef) : List CharCode :=
  match r with
  | .active c => [c]
  | .name "" => escapeChars s ++ strCodes "csname" ++ escapeChars s ++ strCodes "endcsname"
  | .name n => escapeChars s ++ strCodes n

/-- `show_token_list` (§ 292–294): macro parameter characters print doubled,
`#n` as `match_chr` (`mc`, the last parameter character printed) and `n`. -/
def showToks (s : State) (ts : TokList) (mc : CharCode := 35) : List CharCode :=
  ts.foldr (fun t acc =>
    (match t with
     | .char c .param => [c, c]
     | .char c _ => [c]
     | .cs r => printCs s r
     | .noexp r => printCs s r
     | .outParam n => [mc] ++ digitsOf (Int.ofNat n)) ++ acc) []

/-- `str_toks` (§ 464): characters as category 12, the space as 10. -/
def strToks (cs : List CharCode) : TokList :=
  cs.map fun c => if c = 32 then Token.space else .char c .other

/-- `match_chr` after the parameter text (§ 294): its last parameter
character, `#` if none. -/
def matchChr : List ParamItem → CharCode → CharCode
  | [], c => c
  | .arg c :: ps, _ => matchChr ps c
  | .tok _ :: ps, c => matchChr ps c

/-- The parameter text (§ 294): `#n` with its own parameter character. -/
-- source: tex.web § 294 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L6289
-- source: SaTeX `meaning_text` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
def showParams (s : State) (m : Macro) : List CharCode :=
  let rec go (k : Nat) : List ParamItem → List CharCode
    | [] => []
    | .arg c :: rest => [c] ++ digitsOf (Int.ofNat k) ++ go (k + 1) rest
    | .tok t :: rest => showToks s [t] ++ go k rest
  go 1 m.params ++ (if m.braceDelim then showToks s [m.brace] else [])

def catWord : Cat → String
  | .bgroup => "begin-group character " | .egroup => "end-group character "
  | .mathShift => "math shift character " | .alignTab => "alignment tab character "
  | .param => "macro parameter character " | .sup => "superscript character "
  | .sub => "subscript character " | .space => "blank space "
  | .letter => "the letter " | .other => "the character "
  | _ => "the character "

def hexOf (n : Nat) : List CharCode :=
  [34] ++ (Nat.toDigits 16 n).map (fun c => (Char.toUpper c).toNat)

/-- A font's name, and ` at <size>pt` when that is not its design size
(§ 1261, § 472). -/
-- source: tex.web § 1261 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L23422
-- source: tex.web § 472 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L9276
-- source: SaTeX `meaning_text` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
def fontNameText (s : State) (f : Nat) : List CharCode :=
  match nth? s.fonts f with
  | some ft => strCodes ft.name ++
      (if ft.size ≠ ft.dsize then strCodes " at " ++ printScaled ft.size ++ strCodes "pt" else [])
  | none => strCodes "nullfont"

/-- `print_meaning` (§ 296) through `print_cmd_chr` (§ 227). -/
-- source: tex.web § 296 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L6320
-- source: etex.ch https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/etexdir/etex.ch#L3052
-- source: SaTeX `macro_head` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
def meaningText (s : State) (m : Meaning) : List CharCode :=
  match m with
  | .undefined => strCodes "undefined"
  | .prim "notexpanded:" _ => escapeChars s ++ strCodes "relax"   -- § 227: `relax` whatever `cur_chr`
  | .prim n _ => escapeChars s ++ strCodes n
  | .char c k => strCodes (catWord k) ++ [c]
  | .chardef n => escapeChars s ++ strCodes "char" ++ hexOf n
  | .mathchardef n => escapeChars s ++ strCodes "mathchar" ++ hexOf n
  | .reg k i =>
    let kn := match k with
      | .count => "count" | .dimen => "dimen" | .skip => "skip"
      | .muskip => "muskip" | .toks => "toks" | .box => "box"
    escapeChars s ++ strCodes kn ++ digitsOf (Int.ofNat i)
  | .font f => strCodes "select font " ++ fontNameText s f
  | .luaCall id _ => escapeChars s ++ strCodes "luacall" ++ digitsOf (Int.ofNat id)
  | .call mac =>
    -- etex.ch (§ 1295): the prefixes back to back, then one space
    (if mac.prot then escapeChars s ++ strCodes "protected" else []) ++
    (if mac.long then escapeChars s ++ strCodes "long" else []) ++
    (if mac.outer then escapeChars s ++ strCodes "outer" else []) ++
    (if mac.prot || mac.long || mac.outer then [32] else []) ++
    strCodes "macro:" ++ showParams s mac ++ strCodes "->" ++ showToks s mac.body (matchChr mac.params 35)

/-! ## Meanings of tokens -/

/-- The meaning `\relax` gets behind `\noexpand` (§ 358: `relax` with
`no_expand_flag`), a primitive identity of its own. -/
def notExpanded : Meaning := .prim "notexpanded:" .relax

/-- `frozen_relax` (§ 222, § 379). WellFormed states map it to `\relax`. -/
def frozenRelax : Token := .cs (.name "frozen\\relax")

/-- The `\par` token as the mouth inserts it (§ 333 `par_token`). -/
def parToken : Token := .cs (.name "par")

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

/-- § 358: the marker turns a token into `notExpanded` only if its meaning
is expandable when the marker is read. -/
-- source: tex.web § 358 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L7503
-- source: SaTeX `read_noexpanded` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/analysis/machine.rs
def meaningTok (s : State) : Token → Meaning
  | .char c k => .char c k
  | .cs r => s.meaningOf r
  | .noexp r => if (s.meaningOf r).expandable then notExpanded else s.meaningOf r
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

def expandableTok (s : State) (t : Token) : Bool := (meaningTok s t).expandable

/-- `\ifx` equality of meanings (§ 507–508). Primitives compare by
`cur_cmd` and `cur_chr`, so aliases (`primId`) are equal. -/
-- source: tex.web § 507 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L9840
-- source: SaTeX `Primitive` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs
def ifxEq (m n : Meaning) (primId : String → String := id) : Bool :=
  match m, n with
  | .undefined, .undefined => true
  | .prim a _, .prim b _ => primId a == primId b
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
-- source: tex.web § 506 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L9814
-- source: SaTeX `meaning_of` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
def charCat (s : State) (t : Token) : Nat × Nat :=
  match t, meaningTok s t with
  | .noexp (.active c), .prim "notexpanded:" _ => (c, 13)   -- § 506
  | _, .char c k => (c, k.toNat)
  | _, _ => (256, 16)

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

/-- `end_token_list` on every used-up token list on top, except a `v`
template (§ 390). -/
-- source: tex.web § 390 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L7978
-- source: SaTeX `expand_macro` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/analysis/machine.rs
def popDone : List InputLevel → List InputLevel
  | .toks k [] :: rest => if k = .vTemplate then .toks k [] :: rest else popDone rest
  | l => l

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
character `c`, so its head must be a second, identical one. -/
-- source: tex.web § 352 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L7371
-- source: SaTeX `sup_escape_at` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/tex.rs
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

/-- Letters of a control word, `^^` reduced inside (§ 355–356). A `^^` after
the letters is reduced in the buffer even if no letter comes of it. -/
-- source: tex.web § 356 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L7464
-- source: SaTeX `read_cs_name_inner` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/tex.rs
def scanLetters (cat : CharCode → Cat) : Nat → List CharCode → List CharCode × List CharCode
  | 0, cs => ([], cs)
  | fuel + 1, c :: rest =>
    if cat c = .letter then
      let (ls, r) := scanLetters cat fuel rest
      (c :: ls, r)
    else if cat c = .sup then
      match reduceHat c rest with
      | some (c', rest') =>
        scanLetters cat fuel (c' :: rest')   -- § 356: the buffer keeps the reduced code
      | none => ([], c :: rest)
    else ([], c :: rest)
  | _, [] => ([], [])

/-- A name's key (§ 372 `id_lookup` compares the codes as they are). Every
code up to `0x10FFFF` gets its own key: `U+FFFF` and the surrogates, which
`Char` cannot hold, are escaped (`decodeName` undoes it). -/
-- source: tex.web § 372 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L7745
-- source: SaTeX `intern` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/tex.rs
def nameOf (cs : List CharCode) : String :=
  String.mk (cs.flatMap fun c =>
    if c = 0xFFFF then ['\uFFFF', '\uFFFF']
    else if 0xD800 ≤ c ∧ c < 0xE000 then ['\uFFFF', Char.ofNat (c - 0xD800 + 0xE000)]
    else [Char.ofNat c])

/-- Reading one token from the rest of a line (§§ 343–356). `none` when the
line is used up. Catcodes are those in force when each character is read. -/
inductive LineTok (cat : CharCode → Cat) :
    MouthState → List CharCode → Option Token → MouthState → List CharCode → Prop
  | done {st : MouthState} : LineTok cat st [] none st []
  | hat {st st' : MouthState} {c c' : CharCode} {rest rest' rest'' : List CharCode} {r : Option Token} :
      cat c = .sup → reduceHat c rest = some (c', rest') →
      LineTok cat st (c' :: rest') r st' rest'' →
      LineTok cat st (c :: rest) r st' rest''
  | escWord {st : MouthState} {c c1 : CharCode} {rest : List CharCode} :
      cat c = .escape → cat c1 = .letter →
      LineTok cat st (c :: c1 :: rest)
        (some (.cs (.name (nameOf (scanLetters cat (rest.length + 1) (c1 :: rest)).1))))
        .skipBlanks (scanLetters cat (rest.length + 1) (c1 :: rest)).2
  | escSymbol {st : MouthState} {c c1 : CharCode} {rest : List CharCode} :
      cat c = .escape → cat c1 ≠ .letter →
      (cat c1 = .sup → reduceHat c1 rest = none) →
      LineTok cat st (c :: c1 :: rest) (some (.cs (.name (nameOf [c1]))))
        (if cat c1 = .space then .skipBlanks else .midLine) rest
  | escHat {st : MouthState} {c c1 c' : CharCode} {rest rest' : List CharCode} {r : Option Token}
      {st' : MouthState} {rest'' : List CharCode} :
      cat c = .escape → cat c1 = .sup → reduceHat c1 rest = some (c', rest') →
      LineTok cat st (c :: c' :: rest') r st' rest'' →
      LineTok cat st (c :: c1 :: rest) r st' rest''
  | escEmpty {st : MouthState} {c : CharCode} :
      cat c = .escape → LineTok cat st [c] (some (.cs (.name ""))) .skipBlanks []
  | active {st : MouthState} {c : CharCode} {rest : List CharCode} :
      cat c = .active → LineTok cat st (c :: rest) (some (.cs (.active c))) .midLine rest
  | plain {st : MouthState} {c : CharCode} {rest : List CharCode} :
      (cat c).tokenable = true → cat c ≠ .space → cat c ≠ .sup ∨ reduceHat c rest = none →
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

/-- A file level as `start_input` leaves it (§ 538): its first line is read
at once, with the `\endlinechar` in force now. -/
-- source: tex.web § 538 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L10400
-- source: SaTeX `locate_input` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/commands.rs
def openFile (s : State) (key : String) (lines : List (List CharCode)) : InputLevel :=
  .file { name := key, cur := prepareLine s (lines.headD []), lines := lines.tail, line := 1 }

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
      f.lines = line :: lines → s.forceEof = false →
      Next { s with input := .file { f with cur := prepareLine s line, lines := lines,
                                              state := .newLine, line := f.line + 1 } :: rest } t s' →
      Next s t s'
  /-- End of file (§ 362): the file level goes. -/
  -- source: tex.web § 362 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L7563
  -- source: SaTeX `end_input` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/tex.rs
  | endFile {s s' : State} {f : FileLevel} {rest : List InputLevel} {t : Token} {st : MouthState} :
      s.input = .file f :: rest →
      LineTok s.catOf f.state f.cur none st [] →
      (f.lines = [] ∧ (f.eofSeen = true ∨ f.text = true ∨ s.toksAt (.toksPar "everyeof") = []) ∨
       s.forceEof = true) →
      Next { s with input := rest, forceEof := false } t s' →
      Next s t s'
  /-- etex.ch: at the end of a file not ended by `\endinput` (nor a
  `\scantextokens` one), `\everyeof` comes first, the file still open with
  a fake empty line. -/
  -- source: etex.ch https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/etexdir/etex.ch#L933
  -- source: SaTeX `end_input_frame` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/analysis/machine.rs
  | everyEof {s s' : State} {f : FileLevel} {rest : List InputLevel} {t : Token} {st : MouthState} :
      s.input = .file f :: rest →
      LineTok s.catOf f.state f.cur none st [] →
      f.lines = [] → s.forceEof = false → f.eofSeen = false → f.text = false →
      s.toksAt (.toksPar "everyeof") ≠ [] →
      Next (insertEvery "everyeof"
              { s with input := .file { f with eofSeen := true, cur := [], state := .newLine, line := f.line + 1 }
                                :: rest }) t s' →
      Next s t s'

/-- The input yields no token any more (§ 360): every level is used up. -/
inductive Drained : State → Prop
  -- source: tex.web § 360 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L7522
  -- source: SaTeX `end_input` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/analysis/machine.rs
  | empty {s : State} : s.input = [] → Drained s
  | tokEnd {s : State} {k : TokListKind} {rest : List InputLevel} :
      s.input = .toks k [] :: rest → Drained { s with input := rest } → Drained s
  | nextLine {s : State} {f : FileLevel} {rest : List InputLevel} {st : MouthState}
      {line : List CharCode} {lines : List (List CharCode)} :
      s.input = .file f :: rest → LineTok s.catOf f.state f.cur none st [] →
      f.lines = line :: lines → s.forceEof = false →
      Drained { s with input := .file { f with cur := prepareLine s line, lines := lines,
                                               state := .newLine, line := f.line + 1 } :: rest } →
      Drained s
  | endFile {s : State} {f : FileLevel} {rest : List InputLevel} {st : MouthState} :
      s.input = .file f :: rest → LineTok s.catOf f.state f.cur none st [] →
      (f.lines = [] ∧ (f.eofSeen = true ∨ f.text = true ∨ s.toksAt (.toksPar "everyeof") = []) ∨
       s.forceEof = true) →
      Drained { s with input := rest, forceEof := false } → Drained s

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
  | undelimTok {s s0 s1 s2 : State} {sp : TokList} {t : Token} {c : CharCode} {ps : List ParamItem}
      {args : List TokList} :
      (∀ x ∈ sp, x = Token.space) → ReadToks s sp s0 →
      Next s0 t s1 → t.stored ≠ Token.space → t.isBgroup = false → t.isEgroup = false →
      (leadingToks ps).1 = [] →
      noParUnlessLong long [t.stored] →
      MatchParams long s1 ps args s2 →
      MatchParams long s (.arg c :: ps) ([t.stored] :: args) s2
  /-- Undelimited, a group: its braces go (§ 392, § 399). -/
  | undelimGroup {s s0 s1 s2 : State} {sp body : TokList} {b e : Token} {c : CharCode}
      {ps : List ParamItem} {args : List TokList} :
      (∀ x ∈ sp, x = Token.space) → ReadToks s sp s0 →
      b.isBgroup = true → e.isEgroup = true → balanced body = true →
      ReadToks s0 (b :: body ++ [e]) s1 → (leadingToks ps).1 = [] →
      noParUnlessLong long body →
      MatchParams long s1 ps args s2 →
      MatchParams long s (.arg c :: ps) (body :: args) s2
  /-- Delimited: the shortest balanced text before the delimiter (§ 397). -/
  | delimited {s s1 s2 : State} {ts : TokList} {c : CharCode} {ps : List ParamItem} {args : List TokList} :
      (leadingToks ps).1 ≠ [] →
      ReadToks s (ts ++ (leadingToks ps).1) s1 → firstDelim ts (leadingToks ps).1 →
      noParUnlessLong long ts →
      MatchParams long s1 (leadingToks ps).2 args s2 →
      MatchParams long s (.arg c :: ps) (stripBraces ts :: args) s2

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

-- source: pdftex.web https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/pdftexdir/pdftex.web#L38626
-- source: SaTeX `if_type` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
def _root_.Satex.IfOp.code : IfOp → Int
  | .charIf => 0 | .catIf => 1 | .num => 2 | .dim => 3 | .odd => 4 | .vmode => 5
  | .hmode => 6 | .mmode => 7 | .inner => 8 | .void => 9 | .hboxIf => 10
  | .vboxIf => 11 | .ifx => 12 | .eof => 13 | .iftrue => 14 | .iffalse => 15
  | .ifcase => 16 | .defined => 17 | .csname => 18 | .fontchar => 19
  | .incsname => 20 | .primitive => 21 | .absnum => 22 | .absdim => 23   -- pdftex.web
  | .condition => 24   -- LuaTeX; the number is not checked against luatex.web

/-- etex.ch `last_node_type`: -1 for an empty list, 0 for a character,
the node type plus one otherwise. -/
-- source: etex.ch https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/etexdir/etex.ch#L1262
-- source: SaTeX `last_item` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/commands.rs
def itemCode : Option Item → Int
  | none => -1
  | some .char => 0 | some .ligature => 7 | some .box => 1 | some .rule => 3
  | some (.glue _) => 11 | some (.kern _) => 12 | some (.penalty _) => 13
  | some .mark => 5 | some .whatsit => 9 | some .math => 10 | some .disc => 8
  | some .vbox => 2 | some .other => 15

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

/-- Lines of a `\scantokens` pseudo file (etex.ch `pseudo_start`): no line
for an empty text, none after a final `\newlinechar`. -/
-- source: etex.ch https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/etexdir/etex.ch#L4613
-- source: SaTeX `execute_command` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
def splitLines (nl : Int) (cs : List CharCode) : List (List CharCode) :=
  let rec go (cur : List CharCode) : List CharCode → List (List CharCode)
    | [] => if cur = [] then [] else [cur.reverse]
    | c :: rest => if Int.ofNat c = nl then cur.reverse :: go [] rest else go (c :: cur) rest
  go [] cs


def fontParamOf (s : State) (f : Nat) (n : Nat) : Scaled :=
  match nth? s.fonts f with
  | some p => p.params n
  | none => 0

def curFontParam (s : State) (n : Nat) : Scaled := fontParamOf s (s.intAt .curFont).toNat n

/-- `scan_four_bit_int` (§ 436): out of range is "Bad number" and 0. -/
-- source: tex.web § 436 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8653
-- source: SaTeX `scan_int` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
def fourBit (n : Int) : Nat := if 0 ≤ n ∧ n ≤ 15 then n.toNat else 0

def fourBitErr (n : Int) (s : State) : State := if 0 ≤ n ∧ n ≤ 15 then s else raise "Bad number" s

def isRelax : Option Cmd → Bool | some .relax => true | _ => false

/-- `cur_cmd = spacer`: explicit or implicit spaces of any code (§ 406). -/
def isSpacer : Meaning → Bool | .char _ .space => true | _ => false

-- source: tex.web § 406 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8220
-- source: SaTeX `next_non_blank_x_token` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
def spacerTok (s : State) (t : Token) : Bool := isSpacer (meaningTok s t)

/-- `\prevdepth`, `\pagegoal`, … are dimensions (§ 1243, § 1245). -/
def alterDimen (n : String) : Bool := n == "prevdepth" || n.startsWith "page"

/-- The mode `alter_aux` needs (§ 1243). -/
-- source: tex.web § 1243 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L23212
-- source: SaTeX `spacefactor (Owned::List)` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs
def alterModeOk (n : String) (s : State) : Bool :=
  if n == "spacefactor" then s.mode.kind == .horizontal
  else if n == "prevdepth" then s.mode.kind == .vertical else true

/-- `cur_cmd = left_brace`: `{` or an implicit `\bgroup` (§ 403). -/
-- source: tex.web § 403 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8189
-- source: SaTeX `scan_left_brace` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
def leftBraceTok (s : State) (t : Token) : Bool :=
  match meaningTok s t with | .char _ .bgroup => true | _ => false

/-- `prepare_mag` (§ 288): an invalid `\mag` is an error and becomes 1000,
globally. `mag_set` is not modelled. -/
-- source: tex.web § 288 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L6129
-- source: SaTeX `scan_unit_of` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
def prepMag (s : State) : State × Nat :=
  let m := s.intPar "mag"
  if 0 < m ∧ m ≤ 32768 then (s, m.toNat)
  else (geqDefine (raise "Illegal magnification has been changed to 1000" s) (.intPar "mag") (.int 1000), 1000)

/-- § 457: `true` divides by the magnification. -/
def magAdjust (m : Nat) (tr : Bool) (i f : Nat) : Nat × Nat :=
  if tr = true ∧ m ≠ 1000 then scaleUnit i f 1000 m else (i, f)

/-- `attach_sign` (§ 448, § 460): arith_error or `|v| ≥ 2^30` is "Dimension
too large", `max_dimen`; then the sign. -/
def dimOk : Option Int → Bool
  | some x => decide (x.natAbs < 1073741824)
  | none => false

-- source: tex.web § 448 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8829
-- source: SaTeX `attach_sign` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
def dimVal (neg : Bool) (o : Option Int) : Int :=
  let v := match o with | some x => if dimOk (some x) then x else maxDimen | none => maxDimen
  if neg then -v else v

def dimState (o : Option Int) (s : State) : State := if dimOk o then s else raise "Dimension too large" s

/-- How `scan_dimen` takes an internal quantity (§ 448–449): `some e` goes
straight to `attach_sign` (`e`: `mu_error`, "Incompatible glue units"). -/
def directDim (mu : Bool) : IVal → Option Bool
  | .dimen _ => if mu then none else some false
  | .glue _ => if mu then none else some false
  | .mu _ => some (!mu)
  | _ => none

/-- … `some e`: its value is a count of units. -/
def unitsDim (mu : Bool) : IVal → Option Bool
  | .int _ => some false
  | .dimen _ => if mu then some true else none
  | .glue _ => if mu then some true else none
  | _ => none

/-- An internal quantity used as unit (§ 455), with `mu_error`. -/
def internalUnitErr (mu : Bool) : IVal → Option Bool
  | .int _ | .dimen _ | .glue _ => some mu
  | .mu _ => some (!mu)
  | _ => none

def muError (e : Bool) (s : State) : State := if e then raise "Incompatible glue units" s else s

/-- `\read`'s line (§ 483): an unmatched `}` ends it, the rest of the line
is dropped. The tokens kept, and whether that happened. -/
-- source: tex.web § 483 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L9465
-- source: SaTeX `read_line_into` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
def cutUnmatched : Int → TokList → TokList × Bool
  | _, [] => ([], false)
  | d, t :: ts =>
    if t.isEgroup ∧ d = 0 then ([], true)
    else
      let r := cutUnmatched (if t.isBgroup then d + 1 else if t.isEgroup then d - 1 else d) ts
      (t :: r.1, r.2)

/-- Commands `scan_something_internal` accepts (§ 413). -/
def internalTok (s : State) (t : Token) : Bool :=
  match meaningTok s t with
  | .reg k _ => k != .box
  | .chardef _ | .mathchardef _ | .font _ => true
  | .prim _ c => match c with
    | .register k => k != .box
    | .intPar _ | .dimPar _ | .gluePar _ _ | .toksPar _ | .codeTable _ | .lastItem _
    | .expr _ | .fontParam _ | .special _ | .fontDef | .alter _ => true
    | _ => false
  | _ => false

/-- A token list or font identifier (§ 413 `toks_register`, `assign_toks`,
`set_font`, `def_font`; `def_family` is no `internalTok`): read only at
`tok_val`, elsewhere "Missing number" and the token comes back (§ 415). -/
-- source: tex.web § 415 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8391
-- source: SaTeX `scan_int_from` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
def tokLevelTok (s : State) (t : Token) : Bool :=
  match meaningTok s t with
  | .reg k _ => k == .toks
  | .font _ => true
  | .prim _ c => match c with
    | .register k => k == .toks
    | .toksPar _ | .fontDef => true
    | _ => false
  | _ => false

/-- An internal quantity read as a number (§ 413 at `int_val` … `mu_val`). -/
def numTok (s : State) (t : Token) : Bool := internalTok s t && !tokLevelTok s t

/-- "Missing number, treated as zero" (§ 415): `back_error`. -/
def missingNum (t : Token) (s : State) : State := raise "Missing number, treated as zero" (backInput t s)

def levelBound : Level → Int | .int => infinity | _ => maxDimen

def inBound (l : Level) (x : Int) : Option Int := if x.natAbs ≤ (levelBound l).natAbs then some x else none

/-- Adding glue components of possibly different orders (etex.ch
`add_or_sub` on glue): the higher order wins, unnegated (SEM § 11). -/
def addOrder (neg : Bool) (a : Int) (ao : Nat) (b : Int) (bo : Nat) : Int × Nat :=
  if ao = bo then (if neg then a - b else a + b, ao)
  else if ao < bo ∧ b ≠ 0 then (b, bo) else (a, ao)

/-- etex.ch `normalize_glue`: a zero component has order normal. -/
-- source: etex.ch https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/etexdir/etex.ch#L5169
-- source: SaTeX `scan_expr` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
def normGlue (g : Glue) : Glue :=
  { g with stretchOrder := if g.stretch = 0 then 0 else g.stretchOrder,
           shrinkOrder := if g.shrink = 0 then 0 else g.shrinkOrder }

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
      let b := normGlue w.toGlue
      let st := addOrder neg a.stretch a.stretchOrder b.stretch b.stretchOrder
      let sh := addOrder neg a.shrink a.shrinkOrder b.shrink b.shrinkOrder
      (glueOk (normGlue { width := if neg then a.width - b.width else a.width + b.width,
                          stretch := st.1, stretchOrder := st.2, shrink := sh.1, shrinkOrder := sh.2 })).map
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

def nameOfMeaning : Meaning → String
  | .prim n _ => n
  | _ => ""

/-- Math families (§ 1234 `scan_four_bit_int`; XeTeX and LuaTeX have 256). -/
def famCount (E : Engine) : Nat := if E.unicode then 256 else 16

/-- LuaTeX's math style numbers: display 0, then text, script,
scriptscript, each followed by its cramped form (LuaTeX manual, "Math
styles"). -/
def styleIndex : String → Nat
  | "displaystyle" => 0 | "crampeddisplaystyle" => 1
  | "textstyle" => 2 | "crampedtextstyle" => 3
  | "scriptstyle" => 4 | "crampedscriptstyle" => 5
  | "scriptscriptstyle" => 6 | _ => 7

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
  -- source: tex.web § 406 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8220
  -- source: SaTeX `next_non_blank_x_token` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
  | here {s s1 : State} {t : Token} : XNext E s t s1 → spacerTok s1 t = false → XNonBlank E s t s1
  | skip {s s1 s2 : State} {t u : Token} :
      XNext E s u s1 → spacerTok s1 u = true → XNonBlank E s1 t s2 → XNonBlank E s t s2

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
  -- source: tex.web § 407 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8224
  -- source: SaTeX `scan_keyword` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
  | letter {s s1 s2 : State} {t : Token} {k : Char} {ks : List Char} {acc : TokList} {b : Bool} :
      XNext E s t s1 → kwMatches k t = true → Keyword E s1 ks (acc ++ [t]) b s2 →
      Keyword E s (k :: ks) acc b s2
  | space {s s1 s2 : State} {u : Token} {k : Char} {ks : List Char} {b : Bool} :
      XNext E s u s1 → kwMatches k u = false → spacerTok s1 u = true →
      Keyword E s1 (k :: ks) [] b s2 → Keyword E s (k :: ks) [] b s2
  | fail {s s1 : State} {t : Token} {k : Char} {ks : List Char} {acc : TokList} :
      XNext E s t s1 → kwMatches k t = false → ¬ (spacerTok s1 t = true ∧ acc = []) →
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
  -- source: tex.web § 444 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8759
  -- source: SaTeX `scan_digits` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
  | more {s s1 s2 : State} {radix acc d : Nat} {t : Token} {ds : List Nat} :
      XNext E s t s1 → isDigitTok radix t = some d →
      Digits E radix s1 (acc * radix + d) ds s2 → Digits E radix s acc ds s2
  | endSpace {s s1 : State} {radix acc : Nat} {t : Token} :
      XNext E s t s1 → isDigitTok radix t = none → spacerTok s1 t = true → Digits E radix s acc [acc] s1
  | endOther {s s1 : State} {radix acc : Nat} {t : Token} :
      XNext E s t s1 → isDigitTok radix t = none → spacerTok s1 t = false →
      Digits E radix s acc [acc] (backInput t s1)

/-- `scan_int` (§ 440). -/
inductive ScanInt (E : Engine) : State → Int → State → Prop
  | internal {s s1 s2 : State} {neg : Bool} {t : Token} {v : IVal} :
      Signs E s neg t s1 → numTok s1 t = true → Internal E (backInput t s1) v s2 →
      ScanInt E s (if neg then -v.toInt else v.toInt) s2
  /-- `` `c ``: the character's code, then an optional space (§ 442). -/
  -- source: tex.web § 442 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8731
  -- source: SaTeX `scan_alphabetic_constant` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
  | alpha {s s1 s2 s3 : State} {neg : Bool} {u : Token} {c : Nat} :
      Signs E s neg (otherChar '`') s1 → Next s1 u s2 →
      (match u with
       | .char d _ => c = d
       | .cs (.name n) => strCodes n = [c]
       | .cs (.active d) => c = d
       | .noexp (.active d) => c = d
       | .noexp (.name n) => strCodes n = [c]
       | _ => False) →
      c ≤ charMax E → OptSpace E s2 s3 →
      ScanInt E s (if neg then -(Int.ofNat c) else Int.ofNat c) s3
  /-- "Improper alphabetic constant" (§ 442): a multi-letter name or a code
  out of range gives `"0"`, the token is read again. -/
  | alphaBad {s s1 s2 : State} {neg : Bool} {u : Token} :
      Signs E s neg (otherChar '`') s1 → Next s1 u s2 →
      (match u with
       | .char d _ => d > charMax E
       | .cs (.name n) => (strCodes n).length ≠ 1 ∨ (∀ c, strCodes n = [c] → c > charMax E)
       | .cs (.active d) => d > charMax E
       | .noexp (.name n) => (strCodes n).length ≠ 1 ∨ (∀ c, strCodes n = [c] → c > charMax E)
       | .noexp (.active d) => d > charMax E
       | _ => True) →
      ScanInt E s (if neg then -48 else 48) (raise "Improper alphabetic constant" (backInput u s2))
  | decimal {s s1 s2 : State} {neg : Bool} {t : Token} {d n : Nat} :
      Signs E s neg t s1 → isDigitTok 10 t = some d → Digits E 10 s1 d [n] s2 →
      ScanInt E s (if neg then -(min (Int.ofNat n) infinity) else min (Int.ofNat n) infinity)
        (if Int.ofNat n > infinity then raise "Number too big" s2 else s2)
  | octal {s s1 s2 s3 : State} {neg : Bool} {n d : Nat} {t : Token} :
      Signs E s neg (otherChar '\'') s1 → XNext E s1 t s2 → isDigitTok 8 t = some d → Digits E 8 s2 d [n] s3 →
      ScanInt E s (if neg then -(min (Int.ofNat n) infinity) else min (Int.ofNat n) infinity)
        (if Int.ofNat n > infinity then raise "Number too big" s3 else s3)
  | hex {s s1 s2 s3 : State} {neg : Bool} {n d : Nat} {t : Token} :
      Signs E s neg (otherChar '"') s1 → XNext E s1 t s2 → isDigitTok 16 t = some d → Digits E 16 s2 d [n] s3 →
      ScanInt E s (if neg then -(min (Int.ofNat n) infinity) else min (Int.ofNat n) infinity)
        (if Int.ofNat n > infinity then raise "Number too big" s3 else s3)
  /-- `'` or `"` without a digit (§ 444 `vacuous`): "Missing number", the
  token is read again. -/
  -- source: tex.web § 444 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8759
  -- source: SaTeX `scan_digits` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
  | vacuous {s s1 s2 : State} {neg : Bool} {q t : Token} {radix : Nat} :
      (q = otherChar '\'' ∧ radix = 8 ∨ q = otherChar '"' ∧ radix = 16) →
      Signs E s neg q s1 → XNext E s1 t s2 → isDigitTok radix t = none →
      ScanInt E s 0 (raise "Missing number, treated as zero" (backInput t s2))
  /-- "Missing number, treated as zero" (§ 446), also for a token list or
  font identifier (§ 415). -/
  -- source: tex.web § 415 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8391
  -- source: SaTeX `scan_int_from` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
  | missing {s s1 s2 : State} {neg : Bool} {t : Token} :
      Signs E s neg t s1 → isDigitTok 10 t = none →
      t ≠ otherChar '`' → t ≠ otherChar '\'' → t ≠ otherChar '"' →
      numTok s1 t = false →
      ScanInt E s 0 (raise "Missing number, treated as zero" (backInput t s1))

/-- One optional space (§ 443). -/
inductive OptSpace (E : Engine) : State → State → Prop
  -- source: tex.web § 443 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8755
  -- source: SaTeX `scan_optional_x_space` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
  | space {s s1 : State} {t : Token} : XNext E s t s1 → spacerTok s1 t = true → OptSpace E s s1
  | other {s s1 : State} {t : Token} : XNext E s t s1 → spacerTok s1 t = false → OptSpace E s (backInput t s1)

/-- The fraction digits after a decimal point (§ 452). -/
inductive Fraction (E : Engine) : State → List Nat → State → Prop
  -- source: tex.web § 452 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8902
  -- source: SaTeX `scan_dimen_parts` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
  | more {s s1 s2 : State} {d : Nat} {ds : List Nat} {t : Token} :
      XNext E s t s1 → isDigitTok 10 t = some d → Fraction E s1 ds s2 → Fraction E s (d :: ds) s2
  /-- § 452: a space after the digits goes. -/
  | stop {s s1 : State} {t : Token} :
      XNext E s t s1 → isDigitTok 10 t = none →
      Fraction E s [] (if spacerTok s1 t then s1 else backInput t s1)

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

/-- The unit after a number (§§ 453–459) for the magnitude `i + f/2^16`, in
tex.web's order: `fil` (if `inf`), an internal quantity, `em`/`ex` (not `mu`),
`mu` (only `mu`), `true` and a physical unit. The value is `none` on
arith_error (§ 453, § 105); `scan_dimen` reports it at `attach_sign`. -/
inductive ScanUnit (E : Engine) : Bool → Bool → State → Nat → Nat → Option Int × Nat → State → Prop
  /-- `fil`, `fill`, `filll` (§ 454). -/
  -- source: tex.web § 454 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8946
  -- source: SaTeX `scan_unit_of` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
  | fil {s s1 s2 s3 : State} {mu : Bool} {i f : Nat} {ls : Nat} :
      Keyword E s "fil".toList [] true s1 → MoreL E s1 0 ls s2 → OptSpace E s2 s3 →
      ScanUnit E true mu s i f (attachFraction i f, min (ls + 1) 3) s3
  /-- An internal quantity as unit (§ 455): `2\dimen0`, `2\count0` (sp),
  `2\muskip0` in math glue. No optional space. -/
  -- source: tex.web § 455 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8965
  -- source: SaTeX `scan_unit_of` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
  | internal {s s1 s2 s3 : State} {inf mu : Bool} {i f : Nat} {t : Token} {v : IVal} {e : Bool} :
      FilAbsent E inf s s1 → XNonBlank E s1 t s2 → internalTok s2 t = true →
      Internal E (backInput t s2) v s3 → internalUnitErr mu v = some e →
      ScanUnit E inf mu s i f
        (nxPlusY (Int.ofNat i) v.toInt (tdiv (v.toInt * Int.ofNat f) 65536), 0) (muError e s3)
  /-- A token list or font identifier as unit (§ 455, § 415): "Missing
  number", the unit is 0, in math glue also `mu_error`. -/
  -- source: tex.web § 415 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8391
  -- source: SaTeX `scan_unit_of` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
  | tokLevel {s s1 s2 : State} {inf mu : Bool} {i f : Nat} {t : Token} :
      FilAbsent E inf s s1 → XNonBlank E s1 t s2 → tokLevelTok s2 t = true →
      ScanUnit E inf mu s i f (nxPlusY (Int.ofNat i) 0 0, 0) (muError mu (missingNum t s2))
  /-- `em`, `ex` (§ 455): the current font's quad and x-height. -/
  -- source: tex.web § 455 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8965
  -- source: SaTeX `scan_unit_of` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
  | em {s s1 s2 s3 s4 : State} {inf : Bool} {i f : Nat} :
      FilAbsent E inf s s1 → NotInternal E s1 s2 → Keyword E s2 "em".toList [] true s3 →
      OptSpace E s3 s4 →
      ScanUnit E inf false s i f
        (nxPlusY (Int.ofNat i) (curFontParam s2 6) (tdiv (curFontParam s2 6 * Int.ofNat f) 65536), 0) s4
  | ex {s s1 s2 s3 s4 s5 : State} {inf : Bool} {i f : Nat} :
      FilAbsent E inf s s1 → NotInternal E s1 s2 → Keyword E s2 "em".toList [] false s3 →
      Keyword E s3 "ex".toList [] true s4 → OptSpace E s4 s5 →
      ScanUnit E inf false s i f
        (nxPlusY (Int.ofNat i) (curFontParam s3 5) (tdiv (curFontParam s3 5 * Int.ofNat f) 65536), 0) s5
  /-- `mu` (§ 456); without it "Illegal unit of measure (mu inserted)". -/
  -- source: tex.web § 456 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8987
  -- source: SaTeX `scan_unit_of` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
  | mu {s s1 s2 s3 s4 : State} {inf b : Bool} {i f : Nat} :
      FilAbsent E inf s s1 → NotInternal E s1 s2 → Keyword E s2 "mu".toList [] b s3 →
      OptSpace E (if b then s3 else raise "Illegal unit of measure (mu inserted)" s3) s4 →
      ScanUnit E inf true s i f (attachFraction i f, 0) s4
  /-- `true` (§ 457), then `pt`, a physical unit (§ 458), `sp` (no fraction)
  or none: "Illegal unit of measure (pt inserted)" (§ 459). -/
  -- source: tex.web § 457 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L9000
  -- source: tex.web § 458 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L9009
  -- source: SaTeX `scan_unit_of` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
  | physical {s s1 s2 s3 s4 s5 s6 : State} {inf tr : Bool} {i f : Nat} {u : Option String} :
      FilAbsent E inf s s1 → NotInternal E s1 s2 → Keyword E s2 "em".toList [] false s3 →
      Keyword E s3 "ex".toList [] false s4 → Keyword E s4 "true".toList [] tr s5 →
      UnitKw E (physicalUnits ++ ["sp"]) (if tr then (prepMag s5).1 else s5) u s6 →
      ScanUnit E inf false s i f
        (let m := (prepMag s5).2
         let p := magAdjust m tr i f
         match u with
         | some "sp" => (some (Int.ofNat p.1), 0)
         | some k => match unitRatio k with
           | some (num, den) => let q := scaleUnit p.1 p.2 num den; (attachFraction q.1 q.2, 0)
           | none => (attachFraction p.1 p.2, 0)
         | none => (attachFraction p.1 p.2, 0))
        (match u with | none => raise "Illegal unit of measure (pt inserted)" s6 | some _ => s6)

/-- `fil` was looked for and is absent, or not allowed here. -/
inductive FilAbsent (E : Engine) : Bool → State → State → Prop
  | notInf {s : State} : FilAbsent E false s s
  | absent {s s1 : State} : Keyword E s "fil".toList [] false s1 → FilAbsent E true s s1

/-- The next non-blank token is no internal quantity (§ 455); it goes back. -/
inductive NotInternal (E : Engine) : State → State → Prop
  | other {s s1 : State} {t : Token} :
      XNonBlank E s t s1 → internalTok s1 t = false → NotInternal E s (backInput t s1)

/-- The first of the units that comes (§ 458), then the optional space
(§ 448); `none` if none does. -/
inductive UnitKw (E : Engine) : List String → State → Option String → State → Prop
  | none_ {s s1 : State} : OptSpace E s s1 → UnitKw E [] s none s1
  | hit {s s1 s2 : State} {k : String} {ks : List String} :
      Keyword E s k.toList [] true s1 → OptSpace E s1 s2 → UnitKw E (k :: ks) s (some k) s2
  | miss {s s1 s2 : State} {k : String} {ks : List String} {r : Option String} :
      Keyword E s k.toList [] false s1 → UnitKw E ks s1 r s2 → UnitKw E (k :: ks) s r s2

/-- Further `l`s after `fil` (§ 454): past `filll` each is an error. -/
inductive MoreL (E : Engine) : State → Nat → Nat → State → Prop
  -- source: tex.web § 454 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8946
  -- source: SaTeX `scan_unit_of` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
  | stop {s s1 : State} {n : Nat} : Keyword E s ['l'] [] false s1 → MoreL E s n n s1
  | more {s s1 s2 : State} {n m : Nat} :
      Keyword E s ['l'] [] true s1 →
      MoreL E (if n ≥ 2 then raise "Illegal unit of measure (replaced by filll)" s1 else s1) (n + 1) m s2 →
      MoreL E s n m s2

/-- `scan_dimen` (§ 448): an internal dimension, or a number and a unit;
every value passes `attach_sign` (§ 460). Returns the value and its glue
order. -/
inductive ScanDimen (E : Engine) : Bool → Bool → State → Int × Nat → State → Prop
  -- source: tex.web § 448 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8829
  -- source: SaTeX `scan_dimen_parts` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
  | internal {s s1 s2 : State} {inf mu neg e : Bool} {t : Token} {v : IVal} :
      Signs E s neg t s1 → internalTok s1 t = true → Internal E (backInput t s1) v s2 →
      directDim mu v = some e →
      ScanDimen E inf mu s (dimVal neg (some v.toInt), 0) (dimState (some v.toInt) (muError e s2))
  /-- An internal integer (in math glue also a dimension, with `mu_error`),
  then a unit (§ 449). -/
  | intUnit {s s1 s2 s3 : State} {inf mu neg e : Bool} {t : Token} {v : IVal} {r : Option Int × Nat} :
      Signs E s neg t s1 → internalTok s1 t = true → Internal E (backInput t s1) v s2 →
      unitsDim mu v = some e →
      ScanUnit E inf mu (muError e s2) v.toInt.natAbs 0 r s3 →
      ScanDimen E inf mu s (dimVal (neg != decide (v.toInt < 0)) r.1, r.2) (dimState r.1 s3)
  /-- A token list or font identifier (§ 449, § 415): "Missing number", 0
  at `dimen_val`. Outside math glue that is the value; in it `mu_error`, and
  the 0 takes a unit. -/
  -- source: tex.web § 415 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8391
  -- source: tex.web § 449 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8875
  -- source: SaTeX `scan_dimen_parts` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
  | tokLevel {s s1 : State} {inf neg : Bool} {t : Token} :
      Signs E s neg t s1 → tokLevelTok s1 t = true →
      ScanDimen E inf false s (dimVal neg (some 0), 0) (dimState (some 0) (missingNum t s1))
  | tokLevelMu {s s1 s2 : State} {inf neg : Bool} {t : Token} {r : Option Int × Nat} :
      Signs E s neg t s1 → tokLevelTok s1 t = true →
      ScanUnit E inf true (muError true (missingNum t s1)) 0 0 r s2 →
      ScanDimen E inf true s (dimVal neg r.1, r.2) (dimState r.1 s2)
  | constant {s s1 s2 s3 : State} {inf mu neg : Bool} {t : Token} {i f : Nat} {r : Option Int × Nat} :
      Signs E s neg t s1 → Decimal E s1 t i f s2 → ScanUnit E inf mu s2 i f r s3 →
      ScanDimen E inf mu s (dimVal neg r.1, r.2) (dimState r.1 s3)
  | radixConst {s s1 s2 s3 : State} {inf mu neg : Bool} {t : Token} {n : Int} {r : Option Int × Nat} :
      Signs E s neg t s1 → (t = otherChar '\'' ∨ t = otherChar '"' ∨ t = otherChar '`') →
      ScanInt E (backInput t s1) n s2 → ScanUnit E inf mu s2 n.natAbs 0 r s3 →
      ScanDimen E inf mu s (dimVal (neg != decide (n < 0)) r.1, r.2) (dimState r.1 s3)

/-- `scan_glue` (§ 461). An internal glue is the whole value; an internal
dimension or a constant is the width, `plus` and `minus` may follow. -/
inductive ScanGlue (E : Engine) : Bool → State → Glue → State → Prop
  -- source: tex.web § 461 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L9064
  -- source: SaTeX `scan_glue_spec` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
  | internal {s s1 s2 : State} {mu neg : Bool} {t : Token} {v : IVal} :
      Signs E s neg t s1 → internalTok s1 t = true → Internal E (backInput t s1) v s2 →
      (match v with | .glue _ => True | .mu _ => True | _ => False) →
      ScanGlue E mu s
        (let g := v.toGlue
         if neg then { g with width := -g.width, stretch := -g.stretch, shrink := -g.shrink } else g)
        (muError (mu != (match v with | .mu _ => true | _ => false)) s2)
  | dimen {s s1 s2 s3 s4 : State} {mu neg : Bool} {t : Token} {d : Scaled} {st sh : Int × Nat} :
      Signs E s neg t s1 → internalTok s1 t = true → Internal E (backInput t s1) (.dimen d) s2 →
      Stretch E mu "plus" (muError mu s2) st s3 → Stretch E mu "minus" s3 sh s4 →
      ScanGlue E mu s { width := if neg then -d else d, stretch := st.1, stretchOrder := st.2,
                        shrink := sh.1, shrinkOrder := sh.2 } s4
  | intUnit {s s1 s2 s3 s4 s5 : State} {mu neg : Bool} {t : Token} {n : Int} {r : Option Int × Nat}
      {st sh : Int × Nat} :
      Signs E s neg t s1 → internalTok s1 t = true → Internal E (backInput t s1) (.int n) s2 →
      ScanUnit E false mu s2 n.natAbs 0 r s3 →
      Stretch E mu "plus" (dimState r.1 s3) st s4 → Stretch E mu "minus" s4 sh s5 →
      ScanGlue E mu s { width := dimVal (neg != decide (n < 0)) r.1, stretch := st.1, stretchOrder := st.2,
                        shrink := sh.1, shrinkOrder := sh.2 } s5
  /-- A token list or font identifier (§ 461, § 415): "Missing number", the
  width is 0, in math glue also `mu_error`. -/
  -- source: tex.web § 461 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L9064
  -- source: SaTeX `scan_glue_spec` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
  | tokLevel {s s1 s2 s3 : State} {mu neg : Bool} {t : Token} {st sh : Int × Nat} :
      Signs E s neg t s1 → tokLevelTok s1 t = true →
      Stretch E mu "plus" (muError mu (missingNum t s1)) st s2 → Stretch E mu "minus" s2 sh s3 →
      ScanGlue E mu s { width := 0, stretch := st.1, stretchOrder := st.2,
                        shrink := sh.1, shrinkOrder := sh.2 } s3
  | spec {s s1 s2 s3 s4 : State} {mu neg : Bool} {t : Token} {w : Int × Nat} {st sh : Int × Nat} :
      Signs E s neg t s1 → internalTok s1 t = false →
      ScanDimen E false mu (backInput t s1) w s2 →
      Stretch E mu "plus" s2 st s3 → Stretch E mu "minus" s3 sh s4 →
      ScanGlue E mu s { width := if neg then -w.1 else w.1, stretch := st.1, stretchOrder := st.2,
                        shrink := sh.1, shrinkOrder := sh.2 } s4

/-- `plus`/`minus` part of glue (§ 462). -/
inductive Stretch (E : Engine) : Bool → String → State → Int × Nat → State → Prop
  | absent {s s1 : State} {mu : Bool} {kw : String} :
      Keyword E s kw.toList [] false s1 → Stretch E mu kw s (0, 0) s1
  | present {s s1 s2 : State} {mu : Bool} {kw : String} {v : Int × Nat} :
      Keyword E s kw.toList [] true s1 → ScanDimen E true mu s1 v s2 → Stretch E mu kw s v s2

/-- LuaTeX `scan_math_style`: a style command, or its number. -/
inductive MathStyleArg (E : Engine) : State → Nat → State → Prop
  | cmd {s s1 : State} {t : Token} :
      XNonRelax E s t s1 → cmdOf s1 t = some (.mode "mathstyle") →
      MathStyleArg E s (styleIndex (nameOfMeaning (meaningTok s1 t))) s1
  | num {s s1 s2 : State} {t : Token} {i : Int} :
      XNonRelax E s t s1 → cmdOf s1 t ≠ some (.mode "mathstyle") → ScanInt E (backInput t s1) i s2 →
      0 ≤ i → i ≤ 7 → MathStyleArg E s i.toNat s2

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
      FontIdent E s2 f s3 →
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
  -- source: pdftex.web https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/pdftexdir/pdftex.web#L38626
  -- source: SaTeX `last_item` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/commands.rs
  | ifType {s s1 : State} {t : Token} :
      Next s t s1 → cmdOf s1 t = some (.lastItem .currentiftype) →
      Internal E s (.int (match s1.cond with
        | [] => 0
        | c :: _ => if c.negated then -(c.op.code + 1) else c.op.code + 1)) s1
  /-- etex.ch: 1 in a true branch or a chosen `\ifcase` case (`or_code`),
  -1 in a false branch, 0 while the test is read. -/
  -- source: etex.ch https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/etexdir/etex.ch#L3408
  -- source: SaTeX `last_item` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/commands.rs
  | ifBranch {s s1 : State} {t : Token} :
      Next s t s1 → cmdOf s1 t = some (.lastItem .currentifbranch) →
      Internal E s (.int (match s1.cond with
        | [] => 0
        | c :: _ => if c.limit = .ifCode then 0 else if c.limit = .fiCode then -1 else 1)) s1
  -- source: etex.ch https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/etexdir/etex.ch#L1262
  -- source: SaTeX `last_item` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/commands.rs
  | lastNodeType {s s1 : State} {t : Token} :
      Next s t s1 → cmdOf s1 t = some (.lastItem .lastnodetype) →
      Internal E s (.int (itemCode (tailItem s1))) s1
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
  /-- etex.ch `scan_normal_glue`: glue level. -/
  -- source: etex.ch https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/etexdir/etex.ch#L5169
  -- source: SaTeX `last_item` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/commands.rs
  | glueComp {s s1 s2 : State} {t : Token} {op : LastOp} {g : Glue} :
      Next s t s1 →
      cmdOf s1 t = some (.lastItem op) →
      (op = .gluestretch ∨ op = .glueshrink ∨ op = .gluestretchorder ∨ op = .glueshrinkorder) →
      ScanGlue E false s1 g s2 →
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
  /-- `\interactionmode` reads the interaction (etex.ch). -/
  -- source: etex.ch https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/etexdir/etex.ch#L3752
  -- source: SaTeX `last_item` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/commands.rs
  | interactionMode {s s1 : State} {t : Token} :
      Next s t s1 → cmdOf s1 t = some (.alter "interactionmode") →
      Internal E s (.int (Int.ofNat s1.interaction)) s1
  /-- **[over]** `\spacefactor`, `\prevdepth`, `\prevgraf`, `\deadcycles`,
  `\insertpenalties`, the page dimensions (§§ 418–422): values of lists and
  of the page builder SaTeX does not keep: any value. In the wrong mode
  "Improper \spacefactor" and 0 (§ 418). -/
  -- source: tex.web § 418 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8465
  -- source: SaTeX `Owned::List` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs
  | alter {s s1 : State} {t : Token} {n : String} {v : Int} :
      Next s t s1 → cmdOf s1 t = some (.alter n) → n ≠ "interactionmode" → alterModeOk n s1 = true →
      Internal E s (if alterDimen n then .dimen v else .int v) s1
  | alterImproper {s s1 : State} {t : Token} {n : String} :
      Next s t s1 → cmdOf s1 t = some (.alter n) → alterModeOk n s1 = false →
      Internal E s (if alterDimen n then .dimen 0 else .int 0) (raise "Improper" s1)
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
  /-- LuaTeX `\Umath…` parameters, per math style. -/
  | mathParam {s s1 s2 : State} {t : Token} {n : String} {k : RegKind} {st : Nat} :
      Next s t s1 → cmdOf s1 t = some (.mathParam n k) → MathStyleArg E s1 st s2 →
      Internal E s (valOfLoc k (s2.get (.special n st))) s2
  /-- `\textfont n` and relatives (§ 415): a font identifier. -/
  | famFont {s s1 s2 : State} {t : Token} {size : Nat} {fam : Int} :
      Next s t s1 → cmdOf s1 t = some (.famFont size) → ScanInt E s1 fam s2 → 0 ≤ fam →
      fam.toNat < famCount E → Internal E s (.font (s2.intAt (.famFont size fam.toNat)).toNat) s2
  /-- Other per-character or per-class tables (`\XeTeXcharclass`,
  `\Umathcode`, …): read back as stored at the operand's index. -/
  | specialTable {s s1 s2 : State} {t : Token} {n : String} {i : Int} :
      Next s t s1 → cmdOf s1 t = some (.special n) →
      n ∉ ["wd", "ht", "dp", "parshape", "interlinepenalties", "clubpenalties",
           "widowpenalties", "displaywidowpenalties"] →
      ScanInt E s1 i s2 →
      Internal E s (.int (s2.intAt (.special n i.toNat))) s2

/-- `scan_font_ident` (§ 577): after expansion and blanks, `\font`, a font
name, or `\textfont n` and its relatives; else "Missing font identifier". -/
inductive FontIdent (E : Engine) : State → Nat → State → Prop
  -- source: tex.web § 577 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L11227
  -- source: SaTeX `scan_font_ident` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/commands.rs
  | cur {s s1 : State} {t : Token} :
      XNonBlank E s t s1 → cmdOf s1 t = some .fontDef → FontIdent E s (s1.intAt .curFont).toNat s1
  | font {s s1 : State} {t : Token} {f : Nat} :
      XNonBlank E s t s1 → meaningTok s1 t = .font f → FontIdent E s f s1
  | fam {s s1 s2 : State} {t : Token} {size : Nat} {fam : Int} :
      XNonBlank E s t s1 → cmdOf s1 t = some (.famFont size) → ScanInt E s1 fam s2 → 0 ≤ fam →
      fam.toNat < famCount E → FontIdent E s (s2.intAt (.famFont size fam.toNat)).toNat s2
  | missing {s s1 : State} {t : Token} :
      XNonBlank E s t s1 → cmdOf s1 t ≠ some .fontDef → (∀ f, meaningTok s1 t ≠ .font f) →
      (∀ n, cmdOf s1 t ≠ some (.famFont n)) →
      FontIdent E s 0 (raise "Missing font identifier" (backInput t s1))

/-- etex.ch `scan_expr`: `+ - * /` and parentheses at one level. Each
operation is checked against the level's bound; an overflow makes the
whole expression 0 with "Arithmetic overflow". `\relax` may end it. -/
inductive ScanExpr (E : Engine) : Level → State → IVal → State → Prop
  -- source: etex.ch https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/etexdir/etex.ch#L5169
  -- source: SaTeX `scan_expr` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
  | ok {s s1 s2 : State} {l : Level} {v : IVal} {t : Token} :
      ExprSum E l s (some v) s1 → XNonBlank E s1 t s2 →
      ScanExpr E l s (match v with | .glue g => .glue (normGlue g) | .mu g => .mu (normGlue g) | w => w)
        (if isRelax (cmdOf s2 t) then s2 else backInput t s2)
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
      FontIdent E s f s1 → Operands E s1 ops s2 → Operands E s (.font :: ops) s2
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
  -- source: tex.web § 403 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8189
  -- source: tex.web § 473 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L9292
  -- source: SaTeX `read_general_text` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
  | plain {s s1 s2 : State} {bal : Bool} {b : Token} {ts : TokList} :
      XNonRelax E s b s1 → leftBraceTok s1 b = true → BodyRaw s1 0 ts s2 → GeneralText E bal s ts s2
  /-- "Missing { inserted" (§ 403): the token is read again inside. -/
  | missing {s s1 s2 : State} {bal : Bool} {b : Token} {ts : TokList} :
      XNonRelax E s b s1 → leftBraceTok s1 b = false →
      BodyRaw (raise "Missing { inserted" (backInput b s1)) 0 ts s2 → GeneralText E bal s ts s2

/-- The same, expanded: `\message`, `\write`, `\edef` body, `\expanded`.
Protected macros stay (etex.ch), `\the` and `\unexpanded` text stands. -/
inductive ExpandedText (E : Engine) : State → TokList → State → Prop
  | text {s s1 s2 : State} {b : Token} {ts : TokList} :
      XNonRelax E s b s1 → leftBraceTok s1 b = true → BodyX E s1 0 [] ts s2 → ExpandedText E s ts s2

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
(consumed) or a non-character (kept back). A character is any token whose
meaning is one of code ≤ 255, implicit ones too; a space is code 32 of any
category (§ 516 `more_name`). TeX Live: `"` toggles quoting, and a `{` makes
the name an expanded balanced text (`scan_file_name_braced`). -/
-- source: tex.web § 526 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L10201
-- source: tex.web § 516 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L10017
-- source: tex.ch (TeX Live) https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.ch#L1669
-- source: tex.ch (TeX Live) https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.ch#L4808
-- source: SaTeX `read_file_name_quoted` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
inductive FileName (E : Engine) : State → List CharCode → State → Prop
  | start {s s1 s2 : State} {t : Token} {n : List CharCode} :
      XNonBlank E s t s1 → leftBraceTok s1 t = false →
      FileNameRest E (backInput t s1) false [] n s2 → FileName E s n s2
  /-- `\input{…}`: the text printed, quotes dropped, spaces kept. -/
  | braced {s s1 s2 : State} {t : Token} {ts : TokList} :
      XNonBlank E s t s1 → leftBraceTok s1 t = true → BodyX E s1 0 [] ts s2 →
      FileName E s ((showToks s2 ts).filter (· ≠ 34)) s2

inductive FileNameRest (E : Engine) : State → Bool → List CharCode → List CharCode → State → Prop
  | quote {s s1 s2 : State} {q : Bool} {acc r : List CharCode} {t : Token} {k : Cat} :
      XNext E s t s1 → meaningTok s1 t = .char 34 k →
      FileNameRest E s1 (!q) acc r s2 → FileNameRest E s q acc r s2
  | char {s s1 s2 : State} {q : Bool} {acc r : List CharCode} {t : Token} {c : CharCode} {k : Cat} :
      XNext E s t s1 → meaningTok s1 t = .char c k → c ≤ 255 → c ≠ 34 → (c ≠ 32 ∨ q = true) →
      FileNameRest E s1 q (acc ++ [c]) r s2 → FileNameRest E s q acc r s2
  | endSpace {s s1 : State} {acc : List CharCode} {t : Token} {k : Cat} :
      XNext E s t s1 → meaningTok s1 t = .char 32 k → FileNameRest E s false acc acc s1
  | endOther {s s1 : State} {q : Bool} {acc : List CharCode} {t : Token} :
      XNext E s t s1 → (match meaningTok s1 t with | .char c _ => c > 255 | _ => True) →
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
      Next s t s1 → Next s1 u s2 → Test E .ifx s (ifxEq (meaningTok s2 t) (meaningTok s2 u) E.primId) s2
  -- source: tex.web § 506 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L9814
  -- source: SaTeX `scan_int` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
  | eof {s s1 : State} {n : Int} :
      ScanInt E s n s1 →
      Test E .eof s (match s1.readStreams (fourBit n) with | .closed => true | _ => false) (fourBitErr n s1)
  | defined {s s1 : State} {t : Token} :
      Next s t s1 →
      Test E .defined s (match meaningTok s1 t with | .undefined => false | _ => true) s1
  /-- etex.ch: the name is looked up, never entered, also after a missing
  `\endcsname`. -/
  -- source: etex.ch https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/etexdir/etex.ch#L4771
  -- source: SaTeX `scan_csname_text` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
  | csname {s s1 : State} {n : List CharCode} {ok : Bool} :
      CsName E { s with inCsname := s.inCsname + 1 } [] n ok s1 →
      Test E .csname s (match s1.meaningOf (.name (nameOf n)) with
                        | .undefined => false | _ => true)
        { s1 with inCsname := s.inCsname, lastCs := some (.name (nameOf n)) }
  | fontchar {s s1 s2 : State} {f : Nat} {c : Int} :
      FontIdent E s f s1 → ScanInt E s1 c s2 → Test E .fontchar s (E.hasChar f c.toNat) s2
  | incsname {s : State} : Test E .incsname s (decide (s.inCsname > 0)) s
  /-- **[over]** LuaTeX `\ifcondition`: the test expands what follows, any
  answer (LuaTeX manual, "Conditions"). -/
  | condition {s s1 : State} {b : Bool} : s1.vfs = s.vfs → Test E .condition s b s1
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
  /-- A macro call (§§ 389–390): match the parameter text, pop the used-up
  token lists (§ 390, "conserve stack space"), then push the replacement
  text with the arguments substituted. -/
  -- source: tex.web § 389 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L7938
  -- source: tex.web § 390 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L7978
  -- source: SaTeX `expand_macro` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/analysis/machine.rs
  | call {s s1 s2 : State} {t : Token} {m : Macro} {args : List TokList} :
      Next s t s1 → meaningTok s1 t = .call m →
      MatchParams m.long s1 (if m.braceDelim then m.params ++ [.tok m.brace] else m.params) args s2 →
      Expand E s (pushToks (.macroBody (match t with | .cs r => r | .noexp r => r | _ => .name ""))
                     (subst args m.body ++ (if m.braceDelim then [m.brace] else []))
                     { s2 with input := popDone s2.input })
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
  -- source: tex.web § 296 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L6320
  -- source: SaTeX `meaning_text` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
  | meaning {s s1 s2 : State} {t u : Token} :
      Next s t s1 → cmdOf s1 t = some (.convert .meaning) → Next s1 u s2 →
      Expand E s (pushToks .inserted (strToks (meaningText s2 (meaningTok s2 u))) s2)
  -- source: tex.web § 472 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L9276
  -- source: SaTeX `scan_font_ident` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/commands.rs
  | fontname {s s1 s2 : State} {t : Token} {f : Nat} :
      Next s t s1 → cmdOf s1 t = some (.convert .fontname) → FontIdent E s1 f s2 →
      Expand E s (pushToks .inserted (strToks (fontNameText s2 f)) s2)
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
  -- source: tex.web § 537 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L10365
  -- source: tex.web § 538 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L10400
  -- source: SaTeX `locate_input` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/commands.rs
  | input {s s1 s2 : State} {t : Token} {n : List CharCode} {key : String} {lines : List (List CharCode)} :
      Next s t s1 → cmdOf s1 t = some .input → FileName E s1 n s2 →
      E.findFile (nameOf n) = some key → s2.vfs key = some lines →
      Expand E s { s2 with input := openFile s2 key lines :: s2.input }
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
      Expand E s { s2 with input := openFile s2 key lines :: s2.input }
  /-- `\endinput` (§ 378) sets the global `force_eof`: the next file level
  that wants a line ends, even one `\input` opens later (§ 362). -/
  -- source: tex.web § 362 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L7563
  -- source: tex.web § 378 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L7807
  -- source: SaTeX `take_pending_eof` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/analysis/machine.rs
  | endinput {s s1 : State} {t : Token} :
      Next s t s1 → cmdOf s1 t = some .endinput →
      Expand E s { s1 with forceEof := true }
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
    | .fontParam _ | .readCmd _ | .special _ | .interaction _ | .alter _
    | .toksApp _ _ _ | .letCharcode | .fontCopy _ | .famFont _ => true
    | .register k => k != .box
    | .mode n => n == "setbox"
    | _ => false
  | .reg k _ => k != .box
  | .font _ => true
  | _ => false

/-- `\long`, `\outer` or `\protected` is pending before a command other than
`\def` and the prefixes (§ 1213, etex.ch). -/
-- source: tex.web § 1213 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L22759
-- source: etex.ch https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/etexdir/etex.ch#L2754
def longMisplaced (s : State) (m : Meaning) : Bool :=
  Nat.land s.pending.prefixes 11 != 0 && assignmentMeaning m &&
    (match m with | .prim _ (.defCmd _ _) | .prim _ (.pfx _) => false | _ => true)

/-- The next token, unexpandable (`main_control` after `get_x_token`). -/
def Head (s : State) (t : Token) (s1 : State) : Prop :=
  Next s t s1 ∧ expandableTok s1 t = false ∧ longMisplaced s (meaningTok s1 t) = false

/-- … for a command that takes no prefix: none is pending (§ 1211–1212). -/
def HeadNP (s : State) (t : Token) (s1 : State) : Prop := Head s t s1 ∧ s.pending.prefixes = 0

/-- `get_r_token` (§ 1215): the control sequence to define. -/
inductive RToken : State → CsRef → State → Prop
  | cs {s s1 : State} {r : CsRef} : Next s (.cs r) s1 → RToken s r s1
  | noexp {s s1 : State} {r : CsRef} : Next s (.noexp r) s1 → RToken s r s1
  | space {s s1 s2 : State} {r : CsRef} : Next s Token.space s1 → RToken s1 r s2 → RToken s r s2

/-- Parameter text (§ 474–476) up to the `{`: `n` arguments so far. -/
inductive ParamText : State → Nat → List ParamItem → Option Token → State → Prop
  -- source: tex.web § 474 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L9336
  -- source: tex.web § 476 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L9357
  -- source: SaTeX `make_macro` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
  | brace {s s1 : State} {n : Nat} {t : Token} :
      Next s t s1 → t.isBgroup = true → ParamText s n [] none s1
  | arg {s s1 s2 s3 : State} {n : Nat} {c : CharCode} {ps : List ParamItem} {b : Option Token} :
      Next s (.char c .param) s1 → Next s1 (.char (49 + n) .other) s2 → n < 9 →
      ParamText s2 (n + 1) ps b s3 → ParamText s n (.arg c :: ps) b s3
  /-- `#{` (§ 476): the last argument is delimited by the brace. -/
  | hashBrace {s s1 s2 : State} {n : Nat} {c : CharCode} {t : Token} :
      Next s (.char c .param) s1 → Next s1 t s2 → t.isBgroup = true → ParamText s n [] (some t) s2
  | tok {s s1 s2 : State} {n : Nat} {t : Token} {ps : List ParamItem} {b : Option Token} :
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
  | .arg _ :: ps => countArgs ps + 1
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
      XNonRelax E s t s1 → leftBraceTok s1 t = true → BodyRaw s1 0 ts s2 → Value E .toks s (.toks ts) s2

/-- `trap_zero_glue` (§ 1229). -/
def trapZero (g : Glue) : Glue := if g.width = 0 ∧ g.stretch = 0 ∧ g.shrink = 0 then {} else g

/-- `trap_zero_glue` on a glue value (§ 1228, § 1236). -/
def trapVal : Val → Val
  | .glue g => .glue (trapZero g)
  | w => w

/-- `\advance` (§§ 1238–1239): no overflow check, 32-bit wrap (SEM § 11).
Glue: the new spec `h` is normalized and kept; the old one's component wins
only with a higher order and nonzero. -/
-- source: tex.web § 1239 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L23143
-- source: SaTeX `arithmetic` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
def advanceVal (a b : Val) : Val :=
  match a, b with
  | .int x, .int y => .int (wrap32 (x + y))
  | .glue g, .glue h =>
    let h := normGlue h
    let st := addOrder false h.stretch h.stretchOrder g.stretch g.stretchOrder
    let sh := addOrder false h.shrink h.shrinkOrder g.shrink g.shrinkOrder
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

inductive ReadLines (s : State) : Int → List (List CharCode) → TokList → ReadStream → Bool → Prop
  -- source: tex.web § 483 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L9465
  -- source: tex.web § 486 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L9507
  -- source: SaTeX `read_line_into` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
  | line {d : Int} {l : List CharCode} {ls : List (List CharCode)} {ts : TokList} :
      TokLine s.catOf .newLine (prepareLine s l) ts →
      ((cutUnmatched d ts).2 = true ∨ depthAfter d ts = 0) →
      ReadLines s d (l :: ls) (cutUnmatched d ts).1 (.normal ls) false
  | more {d : Int} {l : List CharCode} {ls : List (List CharCode)} {ts rest : TokList} {r : ReadStream}
      {e : Bool} :
      TokLine s.catOf .newLine (prepareLine s l) ts → (cutUnmatched d ts).2 = false →
      depthAfter d ts ≠ 0 →
      ReadLines s (depthAfter d ts) ls rest r e → ReadLines s d (l :: ls) (ts ++ rest) r e
  /-- End of file: the stream closes, the empty line is read; inside a
  group that is "File ended within \read" (§ 486). -/
  | eof {d : Int} {ts : TokList} :
      TokLine s.catOf .newLine (prepareLine s []) ts → ReadLines s d [] ts .closed (decide (d ≠ 0))

def streamLines : ReadStream → Option (List (List CharCode))
  | .closed => none
  | .justOpen ls => some ls
  | .normal ls => some ls

/-- The token `write_out` puts after the text (§ 1371 `end_write_token`),
read back with `get_token`. -/
def endWriteToken : Token := .cs (.name "frozen\\endwrite")

/-- Stream numbers of `\write` (§ 1350): negative is 17 (log only), above
15 is 16 (terminal and log). -/
-- source: tex.web § 1350 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L24682
-- source: SaTeX `write_stream` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
def streamNo (n : Int) : Nat := if n < 0 then 17 else if n > 15 then 16 else n.toNat

/-- Where the text goes (§ 1370): an open stream, else the log (17) or
the terminal and log (16). -/
-- source: tex.web § 1370 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L24846
-- source: SaTeX `write_text` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
def writeStream (s : State) (n : Int) : Int :=
  let j := streamNo n
  if j < 16 ∧ (s.writeOpen j).isSome then j else if j = 17 then 17 else 16

-- source: tex.web § 1374 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L24914
-- source: SaTeX `write_open` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/commands.rs
def openStream (s : State) (n : Int) (f : List CharCode) : State :=
  { s with writeOpen := upd s.writeOpen (streamNo n) (some (if 46 ∈ f then nameOf f else nameOf f ++ ".tex")) }

/-- `\write`, `\openout`, `\closeout`, now (`imm`) or as a whatsit (§§ 1350–1356, 1370–1374). -/
inductive WriteCmd (E : Engine) : Bool → State → Token → State → Prop
  /-- `\immediate\write` (§ 1352, §§ 1370–1372): the text is read
  unexpanded, then expanded inside an extra `{…}` that `end_write` must
  follow. Mode 0 during the expansion and an unbalanced text are OPEN. -/
  -- source: tex.web § 1352 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L24702
  -- source: tex.web § 1370 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L24846
  -- source: SaTeX `write_text` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
  | writeNow {s s1 s2 s3 s4 : State} {t : Token} {n : Int} {raw ts : TokList} :
      cmdOf s t = some .write → ScanInt E s n s1 → GeneralText E false s1 raw s2 →
      ExpandedText E (pushToks .write ([.char 123 .bgroup] ++ raw ++ [.char 125 .egroup, endWriteToken]) s2) ts s3 →
      Next s3 endWriteToken s4 →
      WriteCmd E true s t (logOut (if n = 18 then .shell (showToks s4 ts)
                                   else .write (writeStream s4 n) (showToks s4 ts)) s4)
  /-- A deferred `\write` is a whatsit; its text is expanded at `\shipout`
  (§ 1370). SaTeX keeps it unexpanded till then (`Pending`). -/
  | writeLater {s s1 s2 : State} {t : Token} {n : Int} {ts : TokList} :
      cmdOf s t = some .write → ScanInt E s n s1 → GeneralText E false s1 ts s2 →
      WriteCmd E false s t (appendItem .whatsit s2)
  /-- § 1374: a name without extension gets `.tex`. -/
  -- source: tex.web § 1374 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L24914
  -- source: SaTeX `write_open` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/commands.rs
  | openNow {s s1 s2 s3 : State} {t : Token} {n : Int} {f : List CharCode} :
      cmdOf s t = some .openout → ScanInt E s n s1 → OptEquals E s1 s2 → FileName E s2 f s3 →
      WriteCmd E true s t (openStream s3 n f)
  | openLater {s s1 s2 s3 : State} {t : Token} {n : Int} {f : List CharCode} :
      cmdOf s t = some .openout → ScanInt E s n s1 → OptEquals E s1 s2 → FileName E s2 f s3 →
      WriteCmd E false s t (appendItem .whatsit s3)
  | closeNow {s s1 : State} {t : Token} {n : Int} :
      cmdOf s t = some .closeout → ScanInt E s n s1 →
      WriteCmd E true s t { s1 with writeOpen := upd s1.writeOpen (streamNo n) none }
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

/-- `end_graf` (§ 1096): in a paragraph, the empty list goes, a nonempty
one becomes the lines `is`, appended to the enclosing list. -/
-- source: tex.web § 1096 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L21177
-- source: SaTeX `end_graf` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs
def endGraf (is : List Item) (s : State) : State :=
  if s.mode = { kind := .horizontal, inner := false } then
    let s1 := match s.nest with
      | l :: _ => if l.items = [] then popNest s else is.foldr appendItem (popNest s)
      | [] => s
    normalParagraph { s1 with errorCount := 0 }
  else s

/-- `\fam := -1` on entering math (§ 1139, § 1145). -/
-- source: tex.web § 1139 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L21720
-- source: SaTeX `math_shift_token` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs
def famReset (s : State) : State := eqDefine s (.intPar "fam") (.int (-1))

/-- The mode of the list below the current one. -/
def outerMode (s : State) : Mode :=
  match s.nest with
  | _ :: l :: _ => l.mode
  | _ => {}

/-- "Check that another `$` follows" (§ 1197). -/
-- source: tex.web § 1197 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L22470
-- source: SaTeX `close_math` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs
def dollarCheck (s : State) (u : Token) : State :=
  match meaningTok s u with
  | .char _ .mathShift => s
  | _ => raise "Display math should end with $$" (backInput u s)

/-- The display goes into the enclosing list (§ 1199), then
`resume_after_display` (§ 1200) without its optional space. -/
-- source: tex.web § 1200 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L22526
-- source: SaTeX `close_math` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs
def resumeAfterDisplay (s : State) : State :=
  pushNest { kind := .horizontal, inner := false } (unsave (appendItem .box s)).1

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
  -- source: tex.web § 1075 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L20873
  -- source: SaTeX `box_end` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs
  | shift {s : State} {d : Scaled} {b : BoxVal} :
      BoxEnd E (.shift d) b s (match b with
        | .void => s | .hbox _ _ _ => appendItem .box s | .vbox _ _ _ => appendItem .vbox s)
  | setBox {s : State} {glob : Bool} {n : Nat} {b : BoxVal} :
      BoxEnd E (.setBox glob n) b s (define s glob (.reg .box n) (.box b))
  /-- **[over]** Leaders take the glue command that must follow (§ 1078);
  the glue is scanned by its grammar. -/
  | leaders {s s1 s2 : State} {b : BoxVal} {t : Token} {n k : String} {h : Bool} :
      XNonRelax E s t s1 → meaningTok s1 t = .prim n (.material h k) →
      Operands E s1 (E.opsOf n) s2 → BoxEnd E .leaders b s (appendItem (.glue {}) s2)
  /-- **[over]** `\shipout`: the box goes; deferred `\write`s it carries
  run then (§ 1374). The model adds any output; SaTeX expands each in the
  environment at shipout (`ship_pending`). -/
  | shipout {s : State} {b : BoxVal} {o : List Output} :
      BoxEnd E .shipout b s { s with out := o ++ s.out }

/-- The group a box command opens. LuaTeX's `\hpack`, `\vpack`, `\tpack`
are `\hbox`, `\vbox`, `\vtop` without callbacks (LuaTeX manual, "Boxes"). -/
def boxKind : String → Option GroupCode
  | "hbox" | "hpack" => some .hbox
  | "vbox" | "vpack" => some .vbox
  | "vtop" | "tpack" => some .vtop
  | _ => none

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
  -- source: tex.web § 1080 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L20962
  -- source: SaTeX `begin_box` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs
  | lastbox {s s1 : State} {ctx : BoxContext} {t : Token} {b : BoxVal} :
      cmdOf s t = some (.boxCmd "lastbox") → s.mode.kind ≠ .math →
      (tailItem s = some .box → ∃ w h d, b = .hbox w h d) →
      (tailItem s = some .vbox → ∃ w h d, b = .vbox w h d) →
      BoxEnd E ctx (if tailItem s = some .box ∨ tailItem s = some .vbox then b else .void)
        (match s.nest with
         | l :: rest => if tailItem s = some .box ∨ tailItem s = some .vbox
                        then { s with nest := { l with items := l.items.drop 1 } :: rest } else s
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
  /-- `\hbox`, `\vbox`, `\vtop` (§ 1083): spec, `{`, a group of its own;
  an `\hbox` in a list of vertical mode is an `adjusted_hbox_group`; a
  vertical box starts with `normal_paragraph`. -/
  -- source: tex.web § 1083 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L21009
  -- source: SaTeX `begin_box` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs
  | open_ {s s1 s2 s3 : State} {ctx : BoxContext} {t b : Token} {n : String} {g : GroupCode} :
      cmdOf s t = some (.boxCmd n) → boxKind n = some g →
      BoxSpec E s s1 → XNonRelax E s1 b s2 → leftBraceTok s2 b = true →
      s3 = (let g' := if g = .hbox ∧ (match ctx with | .shift _ => true | _ => false) = true ∧ s.mode.kind = .vertical then .adjustedHbox else g
            let s4 := newSaveLevel g' (some ctx) s2
            let s5 := if g = .hbox then s4 else normalParagraph s4
            insertEvery (if g = .hbox then "everyhbox" else "everyvbox")
              (pushNest { kind := if g = .hbox then .horizontal else .vertical, inner := true } s5)) →
      BeginBox E ctx s t s3

-- source: tex.web § 1084 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L21034
-- source: SaTeX `scan_box` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs
def isRuleTok (s : State) (t : Token) : Bool :=
  match meaningTok s t with
  | .prim n (.material _ _) => n == "hrule" || n == "vrule"
  | _ => false

def isBoxCmd : Option Cmd → Bool
  | some (.boxCmd n) => n == "copy" || n == "lastbox" || n == "vsplit" || (boxKind n).isSome
  | some (.register .box) => true
  | _ => false

/-- `scan_box` (§ 1084): the next non-blank non-relax token must begin a box. -/
inductive ScanBox (E : Engine) : BoxContext → State → State → Prop
  | box {s s1 s2 : State} {ctx : BoxContext} {t : Token} :
      XNonRelax E s t s1 → isBoxCmd (cmdOf s1 t) = true → BeginBox E ctx s1 t s2 → ScanBox E ctx s s2
  /-- A rule in a leaders context (§ 1084): `\leaders\hrule`. -/
  -- source: tex.web § 1084 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L21034
  -- source: SaTeX `scan_box` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs
  | rule {s s1 s2 s3 : State} {t : Token} :
      XNonRelax E s t s1 → isRuleTok s1 t = true → RuleSpec E s1 s2 →
      BoxEnd E .leaders .void s2 s3 → ScanBox E .leaders s s3
  | missing {s s1 : State} {ctx : BoxContext} {t : Token} :
      XNonRelax E s t s1 → isBoxCmd (cmdOf s1 t) = false → ¬ (ctx = .leaders ∧ isRuleTok s1 t = true) →
      ScanBox E ctx s (raise "A <box> was supposed to be here" (backInput t s1))

/-- A math delimiter (§ 1160 `scan_delimiter`), by grammar. -/
inductive Delim (E : Engine) : State → State → Prop
  | char {s s1 : State} {c : CharCode} {k : Cat} :
      XNonRelax E s (.char c k) s1 → (k = .letter ∨ k = .other) → Delim E s s1
  | cmd {s s1 s2 : State} {t : Token} {n : String} {c : Cmd} :
      XNonRelax E s t s1 → meaningTok s1 t = .prim n c → Operands E s1 (E.opsOf n) s2 → Delim E s s2

def charOfMeaning : Meaning → Option CharCode
  | .char c .letter => some c
  | .char c .other => some c
  | .chardef n => some n
  | _ => none

/-- `\let`'s right side (§ 1221): spaces, an optional `=`, one optional space. -/
inductive LetRhs : State → Token → State → Prop
  -- source: tex.web § 1221 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L22838
  -- source: SaTeX `primitive_let` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
  | plain {s s1 : State} {t : Token} :
      Next s t s1 → spacerTok s1 t = false → t ≠ .char 61 .other → LetRhs s t s1
  | eq {s s1 s2 : State} {t : Token} :
      Next s (.char 61 .other) s1 → Next s1 t s2 → spacerTok s2 t = false → LetRhs s t s2
  | eqSpace {s s1 s2 s3 : State} {t u : Token} :
      Next s (.char 61 .other) s1 → Next s1 u s2 → spacerTok s2 u = true → Next s2 t s3 → LetRhs s t s3
  | space {s s1 s2 : State} {t u : Token} :
      Next s u s1 → spacerTok s1 u = true → LetRhs s1 t s2 → LetRhs s t s2

/-- `n` dimensions (`\parshape`). -/
inductive Dimens (E : Engine) : Nat → State → List Int → State → Prop
  | nil {s : State} : Dimens E 0 s [] s
  | cons {s s1 s2 : State} {n : Nat} {d : Int × Nat} {ds : List Int} :
      ScanDimen E false false s d s1 → Dimens E n s1 ds s2 → Dimens E (n + 1) s (d.1 :: ds) s2

/-- LuaTeX's toks register operand: a `\toksdef` name or a number. -/
inductive ToksReg (E : Engine) : State → Nat → State → Prop
  | name {s s1 : State} {t : Token} {i : Nat} :
      XNonRelax E s t s1 → meaningTok s1 t = .reg .toks i → ToksReg E s i s1
  | num {s s1 s2 : State} {t : Token} {n : Int} :
      XNonRelax E s t s1 → (∀ i, meaningTok s1 t ≠ .reg .toks i) → ScanInt E (backInput t s1) n s2 →
      0 ≤ n → n.toNat ≤ E.maxReg → ToksReg E s n.toNat s2

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
  /-- The run read past its last file (§ 360): in batch and nonstop mode
  "*** (job aborted, no legal \end found)". -/
  -- source: tex.web § 360 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L7522
  -- source: SaTeX `end_input` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/analysis/machine.rs
  | noInput {s : State} : Drained s → s.interaction ≤ 1 →
      Exec E s (fatalError "*** (job aborted, no legal \\end found)" s)
  /-- **[over]** Otherwise TeX prompts with `*`: any line comes. -/
  | noInputPrompt {s : State} {l : List CharCode} : Drained s → s.interaction ≥ 2 →
      Exec E s { s with input := [.file { name := "", cur := prepareLine s l, line := 1 }] }
  /-- Prefixes (§ 1211): spaces and `\relax` between them are skipped. -/
  | prefix_ {s s1 : State} {t : Token} {p : Nat} :
      Head s t s1 → cmdOf s1 t = some (.pfx p) →
      Exec E s { s1 with pending := { s1.pending with prefixes := Nat.lor s1.pending.prefixes p } }
  | prefixSkip {s s1 : State} {t : Token} :
      Head s t s1 → s.pending.prefixes ≠ 0 → (isSpacer (meaningTok s1 t) ∨ isRelax (cmdOf s1 t)) → Exec E s s1
  /-- "You can't use a prefix with …" (§ 1212). -/
  -- source: tex.web § 1212 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L22751
  -- source: SaTeX `drop_prefixes` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
  | prefixBad {s s1 : State} {t : Token} :
      Head s t s1 → s.pending.prefixes ≠ 0 → assignmentMeaning (meaningTok s1 t) = false →
      ¬ (isSpacer (meaningTok s1 t) ∨ isRelax (cmdOf s1 t)) →
      Exec E s (raise "You can't use a prefix with this" (backInput t
        { s1 with pending := { s1.pending with prefixes := 0 } }))
  /-- "You can't use `\long' or `\outer' with" (§ 1213, etex.ch adds
  `\protected`): an error, then the command runs; only `\global` counts. -/
  -- source: tex.web § 1213 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L22759
  -- source: etex.ch https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/etexdir/etex.ch#L2754
  | prefixLong {s s1 : State} {t : Token} :
      Next s t s1 → expandableTok s1 t = false → longMisplaced s (meaningTok s1 t) = true →
      Exec E s (raise "You can't use `\\long' or `\\outer' or `\\protected' with this" (backInput t
        { s1 with pending := { s1.pending with prefixes := Nat.land s1.pending.prefixes 4 } }))
  | relax {s s1 : State} {t : Token} :
      HeadNP s t s1 → s.pending.prefixes = 0 → isRelax (cmdOf s1 t) = true → Exec E s s1
  /-- `\ignorespaces` (§ 1045): the next non-blank token runs next
  (`goto reswitch`), a `\noexpand`ed one as `\relax`, so it keeps its marker. -/
  -- source: tex.web § 1045 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L20377
  -- source: SaTeX `execute_command` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
  | ignorespaces {s s1 s2 : State} {t u : Token} :
      HeadNP s t s1 → cmdOf s1 t = some .ignorespaces → XNonBlank E s1 u s2 →
      Exec E s (pushToks .backedUp [u] s2)
  | endcsname {s s1 : State} {t : Token} :
      HeadNP s t s1 → cmdOf s1 t = some .endcsname → Exec E s (raise "Extra \\endcsname" s1)
  /-- `\def`, `\gdef` (§ 1218). -/
  | def_ {s s1 s2 s3 s4 : State} {t : Token} {glob : Bool} {r : CsRef} {ps : List ParamItem}
      {b : Option Token} {body : TokList} :
      Head s t s1 → cmdOf s1 t = some (.defCmd glob false) → RToken s1 r s2 →
      ParamText s2 0 ps b s3 → DefBody (countArgs ps) s3 0 body s4 →
      Exec E s (finishAssign (define s4 (glob || pfxGlobal s4) (.meaning r) (.meaning (.call
        { long := Nat.testBit s4.pending.prefixes 0, outer := Nat.testBit s4.pending.prefixes 1,
          prot := Nat.testBit s4.pending.prefixes 3, params := ps, braceDelim := b.isSome,
          brace := b.getD (.char 123 .bgroup), body := body }))))
  /-- `\edef`, `\xdef` (§ 1218, § 477). -/
  | edef {s s1 s2 s3 s4 : State} {t : Token} {glob : Bool} {r : CsRef} {ps : List ParamItem}
      {b : Option Token} {body : TokList} :
      Head s t s1 → cmdOf s1 t = some (.defCmd glob true) → RToken s1 r s2 →
      ParamText s2 0 ps b s3 → DefBodyX E (countArgs ps) s3 0 body s4 →
      Exec E s (finishAssign (define s4 (glob || pfxGlobal s4) (.meaning r) (.meaning (.call
        { long := Nat.testBit s4.pending.prefixes 0, outer := Nat.testBit s4.pending.prefixes 1,
          prot := Nat.testBit s4.pending.prefixes 3, params := ps, braceDelim := b.isSome,
          brace := b.getD (.char 123 .bgroup), body := body }))))
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
  -- source: tex.web § 1224 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L22902
  -- source: tex.web § 436 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L8653
  -- source: SaTeX `font_def` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
  | shorthandDef {s s1 s2 s3 s4 : State} {t : Token} {k : RegKind} {cl : Nat} {r : CsRef} {n : Int} :
      Head s t s1 → cmdOf s1 t = some (.shorthandDef k cl) → RToken s1 r s2 →
      OptEquals E (define s2 (pfxGlobal s2) (.meaning r) (.meaning (.prim "relax" .relax))) s3 →
      ScanInt E s3 n s4 →
      0 ≤ n → n.toNat ≤ (if cl = 1 then charMax E else if cl = 2 then 32767 else E.maxReg) →   -- § 436
      Exec E s (assignTo s4 (.meaning r) (.meaning
        (if cl = 1 then .chardef n.toNat else if cl = 2 then .mathchardef n.toNat else .reg k n.toNat)))
  /-- `\count n = v` and relatives (§ 1224); glue passes `trap_zero_glue` (§ 1236). -/
  -- source: tex.web § 1236 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L23078
  -- source: SaTeX `scan_register_value` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
  | register {s s1 s2 s3 s4 : State} {t : Token} {k : RegKind} {n : Int} {v : Val} :
      Head s t s1 → cmdOf s1 t = some (.register k) → k ≠ .box →
      ScanInt E s1 n s2 → 0 ≤ n → n.toNat ≤ E.maxReg → OptEquals E s2 s3 → Value E k s3 v s4 →
      Exec E s (assignTo s4 (.reg k n.toNat) (trapVal v))
  -- source: tex.web § 1236 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L23078
  -- source: SaTeX `scan_register_value` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
  | regMeaning {s s1 s2 s3 : State} {t : Token} {k : RegKind} {i : Nat} {v : Val} :
      Head s t s1 → meaningTok s1 t = .reg k i → k ≠ .box →
      OptEquals E s1 s2 → Value E k s2 v s3 → Exec E s (assignTo s3 (.reg k i) (trapVal v))
  | intPar {s s1 s2 s3 : State} {t : Token} {n : String} {v : Val} :
      Head s t s1 → cmdOf s1 t = some (.intPar n) → OptEquals E s1 s2 → Value E .count s2 v s3 →
      Exec E s (assignTo s3 (.intPar n) v)
  | dimPar {s s1 s2 s3 : State} {t : Token} {n : String} {v : Val} :
      Head s t s1 → cmdOf s1 t = some (.dimPar n) → OptEquals E s1 s2 → Value E .dimen s2 v s3 →
      Exec E s (assignTo s3 (.dimPar n) v)
  | gluePar {s s1 s2 s3 : State} {t : Token} {n : String} {mu : Bool} {v : Val} :
      Head s t s1 → cmdOf s1 t = some (.gluePar n mu) → OptEquals E s1 s2 →
      Value E (if mu then .muskip else .skip) s2 v s3 →
      Exec E s (assignTo s3 (.gluePar n) (trapVal v))
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
  /-- `\catcode`, `\lccode`, … (§ 1232): an invalid code is an error and 0;
  only `\delcode` takes negative values. -/
  -- source: tex.web § 1232 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L23040
  -- source: SaTeX `assign_char_code` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
  | code {s s1 s2 s3 s4 : State} {t : Token} {tab : CodeTable} {c v : Int} :
      Head s t s1 → cmdOf s1 t = some (.codeTable tab) → ScanInt E s1 c s2 →
      0 ≤ c → c.toNat ≤ charMax E → OptEquals E s2 s3 → ScanInt E s3 v s4 →
      Exec E s (if (0 ≤ v ∨ tab = .del) ∧ v ≤ codeMax E tab then assignTo s4 (.code tab c.toNat) (.int v)
                else assignTo (raise "Invalid code" s4) (.code tab c.toNat) (.int 0))
  /-- `\uppercase`, `\lowercase` (§§ 1285–1289): characters and active
  characters (`t < cs_token_flag + single_base`). -/
  -- source: tex.web § 1289 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L23622
  -- source: SaTeX `case_shift` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
  | caseShift {s s1 s2 : State} {t : Token} {up : Bool} {ts : TokList} :
      HeadNP s t s1 → cmdOf s1 t = some (.caseShift up) → GeneralText E false s1 ts s2 →
      Exec E s (backList (ts.map fun u => match u with
        | .char c k =>
          let m := s2.intAt (.code (if up then .uc else .lc) c)
          if m ≠ 0 then .char m.toNat k else u
        | .cs (.active c) =>
          let m := s2.intAt (.code (if up then .uc else .lc) c)
          if m ≠ 0 then .cs (.active m.toNat) else u
        | v => v) s2)
  /-- `\font\cs=name at d` (§§ 1257–1260); fonts with the same name and
  actual size are shared (§ 1260); a font not found is an error (§ 561). -/
  -- source: tex.web § 1257 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L23346
  -- source: tex.web § 1260 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L23404
  -- source: SaTeX `font_def` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
  | fontDef {s s1 s2 s3 s4 s5 : State} {t : Token} {r : CsRef} {n : List CharCode} {size : Scaled} :
      Head s t s1 → cmdOf s1 t = some .fontDef → RToken s1 r s2 →
      OptEquals E (define s2 (pfxGlobal s2) (.meaning r) (.meaning (.font 0))) s3 →
      FileName E s3 n s4 → FontSize E s4 size s5 →
      Exec E s (let ds := E.designSize (nameOf n)
        let sz := if size > 0 then size else tdiv (ds * -size) 1000   -- `xn_over_d`
        let key := nameOf n ++ "@" ++ toString sz
        match indexOf? key (s5.fonts.map fun ft => ft.name ++ "@" ++ toString ft.size) with
        | some i => assignTo s5 (.meaning r) (.meaning (.font i))
        | none => match E.loadFont (nameOf n) sz with
          | some ps => assignTo { s5 with fonts := s5.fonts ++ [{ name := nameOf n, size := sz, dsize := ds, params := ps }] }
                         (.meaning r) (.meaning (.font s5.fonts.length))
          | none => assignTo (raise "Font not loadable: Metric (TFM) file not found" s5) (.meaning r)
                      (.meaning (.font 0)))
  /-- Selecting a font is a local assignment (§ 1217). -/
  | selectFont {s s1 : State} {t : Token} {f : Nat} :
      Head s t s1 → meaningTok s1 t = .font f → Exec E s (assignTo s1 .curFont (.int (Int.ofNat f)))
  /-- `\fontdimen n f = d` is global (§ 1253). -/
  -- source: tex.web § 1253 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L23319
  -- source: SaTeX `scan_font_ident` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/commands.rs
  | fontdimen {s s1 s2 s3 s4 s5 : State} {t : Token} {n : Int} {f : Nat} {d : Int × Nat} :
      Head s t s1 → cmdOf s1 t = some (.fontParam "fontdimen") → ScanInt E s1 n s2 →
      FontIdent E s2 f s3 → OptEquals E s3 s4 → ScanDimen E false false s4 d s5 →
      Exec E s (finishAssign { s5 with fonts := updAt s5.fonts f (fun p => { p with params := upd p.params n.toNat d.1 }) })
  /-- `\hyphenchar`, `\skewchar` and per-font codes: global (§ 1253).
  **[over]** the operands are read by grammar, the entry written is any. -/
  -- source: tex.web § 1253 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L23319
  -- source: SaTeX `scan_font_ident` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/commands.rs
  | fontInt {s s1 s2 s3 s4 : State} {t : Token} {n : String} {f : Nat} {v : Int} :
      Head s t s1 → cmdOf s1 t = some (.fontParam n) → n ≠ "fontdimen" →
      Operands E s1 (E.opsOf n) s2 → OptEquals E s2 s3 → ScanInt E s3 v s4 →
      Exec E s (finishAssign (geqDefine s4 (.special n f) (.int v)))
  /-- `\openin n = name` (§ 1275). -/
  -- source: tex.web § 1275 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L23509
  -- source: SaTeX `scan_int` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/scan.rs
  | openin {s s1 s2 s3 s4 : State} {t : Token} {n : Int} {f : List CharCode} :
      HeadNP s t s1 → cmdOf s1 t = some .openin → ScanInt E s1 n s2 → OptEquals E s2 s3 →
      FileName E s3 f s4 →
      Exec E s { (fourBitErr n s4) with readStreams := upd s4.readStreams (fourBit n) (openedStream E s4 f) }
  | closein {s s1 s2 : State} {t : Token} {n : Int} :
      HeadNP s t s1 → cmdOf s1 t = some .closein → ScanInt E s1 n s2 →
      Exec E s { (fourBitErr n s2) with readStreams := upd s2.readStreams (fourBit n) .closed }
  /-- `\read n to \cs` (§ 1225, §§ 482–486). -/
  -- source: tex.web § 1225 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L22927
  -- source: tex.web § 482 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L9444
  -- source: SaTeX `read_line_into` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
  | read {s s1 s2 s3 s4 : State} {t : Token} {n : Int} {b : Bool} {r : CsRef} {ls : List (List CharCode)}
      {ts : TokList} {st : ReadStream} {e : Bool} :
      Head s t s1 → cmdOf s1 t = some (.readCmd false) → ScanInt E s1 n s2 →
      Keyword E s2 "to".toList [] b s3 → RToken s3 r s4 →
      0 ≤ n → n < 16 → streamLines (s4.readStreams n.toNat) = some ls →
      ReadLines s4 0 ls ts st e →
      Exec E s (assignTo { (let s5 := if b then s4 else raise "Missing `to' inserted" s4
                            if e then raise "File ended within \\read" s5 else s5) with
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
  /-- **[over]** `\readline` from the terminal in scroll and errorstop mode:
  any line. -/
  -- source: tex.web § 484 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L9485 (terminal), etex_man § 3.2
  -- source: SaTeX `read_line_verbatim` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/commands.rs
  | readlineTerminal {s s1 s2 s3 s4 : State} {t : Token} {n : Int} {b : Bool} {r : CsRef} {l : List CharCode} :
      Head s t s1 → cmdOf s1 t = some (.readCmd true) → ScanInt E s1 n s2 →
      Keyword E s2 "to".toList [] b s3 → RToken s3 r s4 →
      (n < 0 ∨ n ≥ 16 ∨ streamLines (s4.readStreams n.toNat) = none) → s4.interaction ≥ 2 →
      Exec E s (assignTo s4 (.meaning r) (.meaning (.call { body := strToks (prepareLine s4 l) })))
  | write {s s1 s2 : State} {t : Token} :
      HeadNP s t s1 → isExtension (cmdOf s1 t) = true → WriteCmd E false s1 t s2 → Exec E s s2
  /-- `\immediate` (§ 1375): an extension after it runs now; anything
  else is read again. -/
  | immediate {s s1 s2 s3 : State} {t u : Token} :
      HeadNP s t s1 → cmdOf s1 t = some .immediate → XNext E s1 u s2 → isExtension (cmdOf s2 u) = true →
      WriteCmd E true s2 u s3 → Exec E s s3
  | immediateOther {s s1 s2 : State} {t u : Token} :
      HeadNP s t s1 → cmdOf s1 t = some .immediate → XNext E s1 u s2 → isExtension (cmdOf s2 u) = false →
      Exec E s (backInput u s2)
  /-- `\message`, `\errmessage` (§ 1279). -/
  | message {s s1 s2 : State} {t : Token} {err : Bool} {ts : TokList} :
      HeadNP s t s1 → cmdOf s1 t = some (.message err) → ExpandedText E s1 ts s2 →
      Exec E s (if err then raise (nameOf (showToks s2 ts)) s2 else logOut (.message (showToks s2 ts)) s2)
  /-- `\end` (§ 1054 `its_all_over`): the job ends when the lists are
  empty and `\deadcycles` is 0. **[over]** The page and `\deadcycles` are
  not modelled, so with an empty list either outcome may come. -/
  -- source: tex.web § 1054 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L20477
  -- source: SaTeX `par` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs
  | end_ {s s1 : State} {t : Token} :
      HeadNP s t s1 → cmdOf s1 t = some .endCmd → s1.mode = { kind := .vertical, inner := false } →
      (s1.nest.head?.map ListState.items) = some [] →
      Exec E s { s1 with halted := true }
  /-- **[over]** Otherwise `\end` goes back, an empty box, `\vfill` and a
  penalty come, and the page builder may run the output routine: any state
  that keeps the files, with `\end` read again. -/
  | endEject {s s1 s2 : State} {t : Token} :
      HeadNP s t s1 → cmdOf s1 t = some .endCmd → s1.mode = { kind := .vertical, inner := false } →
      s2.vfs = s1.vfs → Exec E s (backInput t s2)
  | endInHorizontal {s s1 : State} {t : Token} :
      HeadNP s t s1 → cmdOf s1 t = some .endCmd → s1.mode.kind = .horizontal → s1.mode.inner = false →
      Exec E s (pushToks .inserted [parToken] (backInput t s1))
  /-- `\show…` (§§ 1291–1298): the answer goes to the log. **[over]** its
  text is any text here. -/
  | showCmd {s s1 s2 : State} {t : Token} {n : String} {o : List CharCode} :
      HeadNP s t s1 → cmdOf s1 t = some (.showCmd n) → Operands E s1 (E.opsOf n) s2 →
      Exec E s (logOut (.message o) s2)
  /-- `\batchmode` … `\errorstopmode` (§ 1265): not saved by groups. -/
  | interaction {s s1 : State} {t : Token} {n : Nat} :
      Head s t s1 → cmdOf s1 t = some (.interaction n) → Exec E s (finishAssign { s1 with interaction := n })
  /-- `alter_aux`, `alter_prev_graf`, `alter_page_so_far`, `alter_integer`
  (§§ 1243–1246): global, never saved; `\spacefactor` only in horizontal,
  `\prevdepth` only in vertical mode, else "You can't use" (§ 1243). The
  value is checked: space factor 1–32767, `\prevgraf` ≥ 0, interaction 0–3
  (etex.ch). The others are values of the page builder SaTeX does not
  model; reading them is **[over]**, see `Internal.alter`. -/
  -- source: tex.web § 1243 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L23212
  -- source: tex.web § 1244 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L23232
  -- source: tex.web § 1245 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L23248
  -- source: tex.web § 1246 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L23255
  -- source: SaTeX `Owned::List` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs
  | alter {s s1 s2 s3 : State} {t : Token} {n : String} {v : Val} :
      Head s t s1 → cmdOf s1 t = some (.alter n) → alterModeOk n s1 = true →
      OptEquals E s1 s2 → Value E (if alterDimen n then .dimen else .count) s2 v s3 →
      Exec E s (finishAssign (match n, v with
        | "interactionmode", .int i => if 0 ≤ i ∧ i ≤ 3 then { s3 with interaction := i.toNat }
                                       else raise "Bad interaction mode" s3
        | "spacefactor", .int i => if 0 < i ∧ i ≤ 32767 then s3 else raise "Bad space factor" s3
        | "prevgraf", .int i => if 0 ≤ i then s3 else raise "Bad \\prevgraf" s3
        | _, _ => s3))
  | alterIllegal {s s1 : State} {t : Token} {n : String} :
      Head s t s1 → cmdOf s1 t = some (.alter n) → alterModeOk n s1 = false →
      Exec E s (raise "You can't use this here" s1)
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
  /-- LuaTeX `\Umath…` parameters: per math style a dimension, a math glue
(the spacings) or an integer (LuaTeX manual, "Math parameter settings"). -/
  | mathParam {s s1 s2 s3 s4 : State} {t : Token} {n : String} {k : RegKind} {st : Nat} {v : Val} :
      Head s t s1 → cmdOf s1 t = some (.mathParam n k) → MathStyleArg E s1 st s2 →
      OptEquals E s2 s3 → Value E k s3 v s4 → Exec E s (assignTo s4 (.special n st) v)
  /-- LuaTeX `\toksapp`, `\tokspre` and their expanding (`e`, `x`) and
  global (`g`, `x`) forms: a toks register, then a text put at its end or
  front (LuaTeX manual, "Token lists"). -/
  | toksApp {s s1 s2 s3 : State} {t : Token} {ex gl front : Bool} {i : Nat} {ts : TokList} :
      Head s t s1 → cmdOf s1 t = some (.toksApp ex gl front) → ToksReg E s1 i s2 →
      (if ex then ExpandedText E s2 ts s3 else GeneralText E false s2 ts s3) →
      Exec E s (finishAssign (define s3 (gl || pfxGlobal s3) (.reg .toks i)
        (.toks (if front then ts ++ s3.toksAt (.reg .toks i) else s3.toksAt (.reg .toks i) ++ ts))))
  /-- `\textfont n = f` and relatives (§ 1234). -/
  -- source: tex.web § 1234 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L23068
  -- source: tex.web § 577 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L11227
  -- source: SaTeX `scan_font_ident` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/commands.rs
  | famFont {s s1 s2 s3 s4 : State} {t : Token} {size : Nat} {fam : Int} {f : Nat} :
      Head s t s1 → cmdOf s1 t = some (.famFont size) → ScanInt E s1 fam s2 → 0 ≤ fam →
      fam.toNat < famCount E → OptEquals E s2 s3 → FontIdent E s3 f s4 →
      Exec E s (assignTo s4 (.famFont size fam.toNat) (.int (Int.ofNat f)))
  /-- LuaTeX `\letcharcode c = t`: the active character `c` means what `t`
  means (LuaTeX manual, "Macros"). -/
  | letCharcode {s s1 s2 s3 s4 : State} {t u : Token} {c : Int} :
      Head s t s1 → cmdOf s1 t = some .letCharcode → ScanInt E s1 c s2 → 0 ≤ c →
      c.toNat ≤ charMax E → OptEquals E s2 s3 → Next s3 u s4 →
      Exec E s (assignTo s4 (.meaning (.active c.toNat)) (.meaning (meaningTok s4 u)))
  /-- **[over]** `\copyfont`, `\pdfcopyfont`, `\letterspacefont` (pdfTeX and
  LuaTeX manuals): a name for a copy of a font, letter-spaced by an integer
  for the last. The copy's number is any. -/
  | fontCopy {s s1 s2 s3 s4 : State} {t : Token} {sp : Bool} {r : CsRef} {f f' : Nat} {k : Int} :
      Head s t s1 → cmdOf s1 t = some (.fontCopy sp) → RToken s1 r s2 → FontIdent E s2 f s3 →
      (if sp then ScanInt E s3 k s4 else s4 = s3) →
      Exec E s (assignTo s4 (.meaning r) (.meaning (.font f')))
  /-- **[over]** `\XeTeXinterchartoks a b = {…}`: stored where SaTeX reads
  it, which this formalisation leaves open: any value. -/
  | interchartoks {s s1 s2 s3 s4 : State} {t : Token} {i j : Int} {v : Val} :
      Head s t s1 → cmdOf s1 t = some (.special "XeTeXinterchartoks") →
      ScanInt E s1 i s2 → ScanInt E s2 j s3 → Value E .toks s3 v s4 →
      Exec E s (finishAssign s4)
  /-- `\setbox n = box` (§ 1241): a prefixed command. `scan_box` runs first;
  `\afterassignment`'s token comes at `done:` (§ 1269), so inside a box
  that `\hbox{` opened, before its `\everyhbox`. -/
  -- source: tex.web § 1241 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L23181
  -- source: tex.web § 1269 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L23473
  -- source: SaTeX `scan_box` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs
  | setbox {s s1 s2 s3 s4 : State} {t : Token} {n : Int} :
      Head s t s1 → cmdOf s1 t = some (.mode "setbox") → ScanInt E s1 n s2 → 0 ≤ n → n.toNat ≤ E.maxReg →
      OptEquals E s2 s3 →
      ScanBox E (.setBox (isGlobal s3 (pfxGlobal s3)) n.toNat) s3 s4 →
      Exec E s (finishAssign { s4 with pending := { s4.pending with
        prefixes := s3.pending.prefixes, afterAssignment := s3.pending.afterAssignment } })
  /-- A box in a list (§ 1073 `box_context` 0). -/
  | box {s s1 s2 : State} {t : Token} :
      HeadNP s t s1 → isBoxCmd (cmdOf s1 t) = true → BeginBox E (.shift 0) s1 t s2 → Exec E s s2
  /-- `\raise`, `\lower` (not in vertical mode), `\moveleft`, `\moveright`
  (only there) (§ 1073). -/
  -- source: tex.web § 1073 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L20853
  -- source: SaTeX `box_end` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs
  | shift {s s1 s2 s3 : State} {t : Token} {n : String} {d : Int × Nat} :
      HeadNP s t s1 → cmdOf s1 t = some (.mode n) →
      ((n = "raise" ∨ n = "lower") ∧ s1.mode.kind ≠ .vertical ∨
       (n = "moveleft" ∨ n = "moveright") ∧ s1.mode.kind = .vertical) →
      ScanDimen E false false s1 d s2 →
      ScanBox E (.shift (if n = "raise" ∨ n = "moveleft" then -d.1 else d.1)) s2 s3 → Exec E s s3   -- § 1073
  | leaders {s s1 s2 : State} {t : Token} :
      HeadNP s t s1 → cmdOf s1 t = some (.mode "leaders") → ScanBox E .leaders s1 s2 → Exec E s s2
  | shipout {s s1 s2 : State} {t : Token} :
      HeadNP s t s1 → cmdOf s1 t = some (.mode "shipout") → ScanBox E .shipout s1 s2 → Exec E s s2
  /-- `\vcenter` in math (§ 1167), with `normal_paragraph`. -/
  -- source: tex.web § 1167 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L22111
  -- source: SaTeX `begin_box` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs
  | vcenter {s s1 s2 s3 : State} {t b : Token} :
      HeadNP s t s1 → cmdOf s1 t = some (.boxCmd "vcenter") → s1.mode.kind = .math →
      BoxSpec E s1 s2 → XNonRelax E (newSaveLevel .vcenter none s2) b s3 → leftBraceTok s3 b = true →
      Exec E s (insertEvery "everyvbox" (pushNest { kind := .vertical, inner := true } (normalParagraph s3)))
  /-- `\insert n {`, `\vadjust {` (§ 1099): the group opens, then the
  brace, then `normal_paragraph`. -/
  -- source: tex.web § 1099 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L21197
  -- source: SaTeX `group_closed` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs
  | insert {s s1 s2 s3 : State} {t b : Token} {n : Int} :
      HeadNP s t s1 → cmdOf s1 t = some (.mode "insert") → ScanInt E s1 n s2 →
      XNonRelax E (newSaveLevel .insert none s2) b s3 → leftBraceTok s3 b = true →
      Exec E s (pushNest { kind := .vertical, inner := true } (normalParagraph s3))
  | vadjust {s s1 s2 : State} {t b : Token} :
      HeadNP s t s1 → cmdOf s1 t = some (.mode "vadjust") → s1.mode.kind ≠ .vertical →
      XNonRelax E (newSaveLevel .insert none s1) b s2 → leftBraceTok s2 b = true →
      Exec E s (pushNest { kind := .vertical, inner := true } (normalParagraph s2))
  /-- `\par` (§ 1094, § 1096): in a paragraph it ends it (`end_graf`).
  **[over]** The lines it breaks into are any items. -/
  -- source: tex.web § 1094 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L21147
  -- source: tex.web § 1096 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L21177
  -- source: SaTeX `par` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs
  | parEnd {s s1 : State} {t : Token} {is : List Item} :
      HeadNP s t s1 → cmdOf s1 t = some (.mode "par") → s1.mode = { kind := .horizontal, inner := false } →
      Exec E s (endGraf is s1)
  /-- In a vertical mode `normal_paragraph`; in restricted horizontal
  mode nothing (§ 1094). -/
  | parOther {s s1 : State} {t : Token} :
      HeadNP s t s1 → cmdOf s1 t = some (.mode "par") → s1.mode.kind ≠ .math →
      s1.mode ≠ { kind := .horizontal, inner := false } →
      Exec E s (if s1.mode.kind = .vertical then normalParagraph s1 else s1)
  /-- `\indent`, `\noindent`, `\quitvmode` (§§ 1088–1093). -/
  | startPar {s s1 : State} {t : Token} {n : String} :
      HeadNP s t s1 → cmdOf s1 t = some (.mode n) → (n = "indent" ∨ n = "noindent" ∨ n = "quitvmode") →
      Exec E s (if s1.mode.kind = .vertical then newGraf (n != "noindent") s1
                else if n = "indent" then appendItem .box s1 else s1)
  /-- Horizontal material in a vertical mode starts a paragraph and is read
  again (§ 1090). -/
  | hmaterialInV {s s1 : State} {t : Token} :
      HeadNP s t s1 → (∃ h, cmdOf s1 t = some (.material true h)) → s1.mode.kind = .vertical →
      Exec E s (newGraf true (backInput t s1))
  /-- Vertical material in a paragraph ends it first (§ 1094 `head_for_vmode`). -/
  | vmaterialInH {s s1 : State} {t : Token} :
      HeadNP s t s1 → (∃ h, cmdOf s1 t = some (.material false h)) →
      s1.mode = { kind := .horizontal, inner := false } →
      Exec E s (pushToks .inserted [parToken] (backInput t s1))
  /-- … and in restricted horizontal mode closes the open group (§ 1064). -/
  | vmaterialInRestricted {s s1 : State} {t : Token} :
      HeadNP s t s1 → (∃ h, cmdOf s1 t = some (.material false h)) →
      s1.mode = { kind := .horizontal, inner := true } →
      Exec E s (if topGroup s1 = .bottom then raise "Extra" s1
                else raise "Missing } inserted" (pushToks .inserted (closerFor (topGroup s1)) (backInput t s1)))
  /-- Material in its own mode: operands by grammar. **[over]** The item it
  makes has any size (`\unhbox`, `\discretionary`, rules, glue). -/
  | material {s s1 s2 : State} {t : Token} {n k : String} {h : Bool} {i : Item} :
      HeadNP s t s1 → meaningTok s1 t = .prim n (.material h k) →
      (if h then s1.mode.kind ≠ .vertical else s1.mode.kind = .vertical) →
      (s1.mode.kind = .math → n ∉ nonMath) →
      Operands E s1 (E.opsOf n) s2 → Exec E s (appendItem i s2)
  /-- Math-only commands outside math, non-math ones in it (§ 1046). -/
  | dollarMissing {s s1 : State} {t : Token} :
      HeadNP s t s1 →
      (s1.mode.kind ≠ .math ∧ nameOfMeaning (meaningTok s1 t) ∈ mathOnly ∨
       s1.mode.kind = .math ∧ nameOfMeaning (meaningTok s1 t) ∈ nonMath) →
      Exec E s (insertDollar t s1)
  /-- A character, in a vertical mode: a paragraph begins (§ 1090). -/
  | charV {s s1 : State} {t : Token} {c : CharCode} :
      HeadNP s t s1 → charOfMeaning (meaningTok s1 t) = some c → s1.mode.kind = .vertical →
      Exec E s (newGraf true (backInput t s1))
  /-- … in a paragraph it is appended (§ 1034 `main_loop`). -/
  | charH {s s1 : State} {t : Token} {c : CharCode} :
      HeadNP s t s1 → charOfMeaning (meaningTok s1 t) = some c → s1.mode.kind = .horizontal →
      Exec E s (appendItem .char s1)
  /-- … in math, a `"8000` math code makes it active (§ 1151). -/
  | charMathActive {s s1 : State} {t : Token} {c : CharCode} :
      HeadNP s t s1 → charOfMeaning (meaningTok s1 t) = some c → s1.mode.kind = .math →
      s1.intAt (.code .math c) = 32768 → Exec E s (backInput (.cs (.active c)) s1)
  | charMath {s s1 : State} {t : Token} {c : CharCode} :
      HeadNP s t s1 → charOfMeaning (meaningTok s1 t) = some c → s1.mode.kind = .math →
      s1.intAt (.code .math c) ≠ 32768 → Exec E s (appendItem .other s1)
  /-- `\mathchardef`'d characters in math, `^`, `_` (§§ 1151–1176). -/
  | mathItem {s s1 : State} {t : Token} :
      HeadNP s t s1 → s1.mode.kind = .math →
      ((∃ n, meaningTok s1 t = .mathchardef n) ∨ (∃ c, meaningTok s1 t = .char c .sup) ∨
       (∃ c, meaningTok s1 t = .char c .sub)) →
      Exec E s (appendItem .other s1)
  | mathItemOutside {s s1 : State} {t : Token} :
      HeadNP s t s1 → s1.mode.kind ≠ .math →
      ((∃ n, meaningTok s1 t = .mathchardef n) ∨ (∃ c, meaningTok s1 t = .char c .sup) ∨
       (∃ c, meaningTok s1 t = .char c .sub)) →
      Exec E s (insertDollar t s1)
  /-- A space: glue in a paragraph, nothing elsewhere (§ 1045). **[over]**
  the glue's size. -/
  | spaceH {s s1 : State} {t : Token} {g : Glue} :
      HeadNP s t s1 → isSpacer (meaningTok s1 t) = true → s1.mode.kind = .horizontal →
      s.pending.prefixes = 0 → Exec E s (appendItem (.glue g) s1)
  | spaceOther {s s1 : State} {t : Token} :
      HeadNP s t s1 → isSpacer (meaningTok s1 t) = true → s1.mode.kind ≠ .horizontal →
      s.pending.prefixes = 0 → Exec E s s1
  /-- `{` (§ 1063); in math a math group (§ 1154). -/
  | bgroup {s s1 : State} {t : Token} {c : CharCode} :
      HeadNP s t s1 → meaningTok s1 t = .char c .bgroup →
      Exec E s (if s1.mode.kind = .math then pushNest { kind := .math, inner := true } (newSaveLevel .math none s1)
                else newSaveLevel .simple none s1)
  /-- `}` (§§ 1068–1069, 1085–1086), by the group it closes. -/
  | egroupSimple {s s1 : State} {t : Token} {c : CharCode} :
      HeadNP s t s1 → meaningTok s1 t = .char c .egroup → topGroup s1 = .simple →
      Exec E s (unsave s1).1
  | egroupBottom {s s1 : State} {t : Token} {c : CharCode} :
      HeadNP s t s1 → meaningTok s1 t = .char c .egroup → topGroup s1 = .bottom →
      Exec E s (raise "Too many }'s" s1)
  | egroupExtra {s s1 : State} {t : Token} {c : CharCode} :
      HeadNP s t s1 → meaningTok s1 t = .char c .egroup →
      (topGroup s1 = .semiSimple ∨ topGroup s1 = .mathShift ∨ topGroup s1 = .mathLeft) →
      Exec E s (raise "Extra }, or forgotten \\endgroup" s1)
  /-- A box group ends: the box goes where its context says (§ 1086); a
  vertical box first ends its paragraph (§ 1085 `end_graf`). **[over]** its
  size and the lines. -/
  -- source: tex.web § 1085 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L21051
  -- source: tex.web § 1086 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L21064
  -- source: SaTeX `group_closed` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs
  | egroupBox {s s1 s2 : State} {t : Token} {c : CharCode} {ctx : BoxContext} {g : GroupCode}
      {b : BoxVal} {is : List Item} :
      HeadNP s t s1 → meaningTok s1 t = .char c .egroup →
      (unsave s1).2 = some (g, some ctx) → (g = .hbox ∨ g = .adjustedHbox ∨ g = .vbox ∨ g = .vtop) →
      (match g, b with
       | .hbox, .hbox _ _ _ => True | .adjustedHbox, .hbox _ _ _ => True
       | .vbox, .vbox _ _ _ => True | .vtop, .vbox _ _ _ => True | _, _ => False) →
      BoxEnd E ctx b (popNest (unsave (if g = .vbox ∨ g = .vtop then endGraf is s1 else s1)).1) s2 →
      Exec E s s2
  /-- Other list groups end: math, insert, vcenter (§§ 1100, 1168, 1186);
  insert and vcenter first end their paragraph (`end_graf`). -/
  -- source: tex.web § 1100 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L21213
  -- source: tex.web § 1168 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L22117
  -- source: SaTeX `group_closed` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs
  | egroupList {s s1 : State} {t : Token} {c : CharCode} {is : List Item} :
      HeadNP s t s1 → meaningTok s1 t = .char c .egroup →
      (topGroup s1 = .math ∨ topGroup s1 = .insert ∨ topGroup s1 = .vcenter ∨
       topGroup s1 = .mathChoice ∨ topGroup s1 = .disc) →
      Exec E s (appendItem .other (popNest (unsave
        (if topGroup s1 = .insert ∨ topGroup s1 = .vcenter then endGraf is s1 else s1)).1))
  /-- **[over]** `\mathchoice` and `\discretionary` take four and three
  groups (§§ 1117, 1172); the count is not kept: another may follow. -/
  | egroupPart {s s1 s2 : State} {t b : Token} {c : CharCode} {g : GroupCode} {m : Mode} :
      HeadNP s t s1 → meaningTok s1 t = .char c .egroup → topGroup s1 = g →
      (g = .mathChoice ∨ g = .disc) → m = s1.mode →
      XNonRelax E (popNest (unsave s1).1) b s2 → leftBraceTok s2 b = true →
      Exec E s (pushNest m (newSaveLevel g none s2))
  | mathChoice {s s1 s2 : State} {t b : Token} :
      HeadNP s t s1 → cmdOf s1 t = some (.mode "mathchoice") → s1.mode.kind = .math →
      XNonRelax E s1 b s2 → leftBraceTok s2 b = true →
      Exec E s (pushNest { kind := .math, inner := true } (newSaveLevel .mathChoice none s2))
  | mathStyle {s s1 : State} {t : Token} :
      HeadNP s t s1 → cmdOf s1 t = some (.mode "mathstyle") → s1.mode.kind = .math →
      Exec E s (appendItem .other s1)
  /-- `\begingroup`, `\endgroup` (§§ 1063, 1069). -/
  | begingroup {s s1 : State} {t : Token} :
      HeadNP s t s1 → cmdOf s1 t = some .beginGroup → Exec E s (newSaveLevel .semiSimple none s1)
  | endgroup {s s1 : State} {t : Token} :
      HeadNP s t s1 → cmdOf s1 t = some .endGroup → topGroup s1 = .semiSimple → Exec E s (unsave s1).1
  | endgroupBottom {s s1 : State} {t : Token} :
      HeadNP s t s1 → cmdOf s1 t = some .endGroup → topGroup s1 = .bottom →
      Exec E s (raise "Extra \\endgroup" s1)
  | endgroupWrong {s s1 : State} {t : Token} :
      HeadNP s t s1 → cmdOf s1 t = some .endGroup → topGroup s1 ≠ .semiSimple → topGroup s1 ≠ .bottom →
      Exec E s (raise "Missing } inserted" (pushToks .inserted (closerFor (topGroup s1)) (backInput t s1)))
  /-- `\aftergroup` (§ 1271): kept on the save stack. -/
  | aftergroup {s s1 s2 : State} {t u : Token} :
      HeadNP s t s1 → cmdOf s1 t = some .aftergroup → Next s1 u s2 →
      Exec E s (if s2.curLevel > 1 then { s2 with save := .insertToken u.stored :: s2.save } else s2)
  /-- `\afterassignment` (§ 1269): replaces the token kept. -/
  | afterassignment {s s1 s2 : State} {t u : Token} :
      HeadNP s t s1 → cmdOf s1 t = some .afterassignment → Next s1 u s2 →
      Exec E s { s2 with pending := { s2.pending with afterAssignment := some u.stored } }
  /-- `$` (§§ 1137–1140): math begins; `$$` in a paragraph is display
  math. -/
  | mathShiftV {s s1 : State} {t : Token} {c : CharCode} :
      HeadNP s t s1 → meaningTok s1 t = .char c .mathShift → s1.mode.kind = .vertical →
      Exec E s (newGraf true (backInput t s1))
  -- source: tex.web § 1138 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L21702
  -- source: tex.web § 1139 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L21720
  -- source: SaTeX `math_shift_token` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs
  | mathInline {s s1 s2 : State} {t u : Token} {c : CharCode} :
      HeadNP s t s1 → meaningTok s1 t = .char c .mathShift → s1.mode.kind = .horizontal →
      Next s1 u s2 → (s1.mode.inner = true ∨ (match meaningTok s2 u with | .char _ .mathShift => False | _ => True)) →
      Exec E s (insertEvery "everymath" (pushNest { kind := .math, inner := true }
        (famReset (newSaveLevel .mathShift none (backInput u s2)))))
  /-- **[over]** Display math ends the paragraph so far into any lines
  (§ 1145); `\predisplaysize`, `\displaywidth`, `\displayindent` are set
  at the new level to values the lines decide, `\fam` to -1. -/
  -- source: tex.web § 1145 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L21755
  -- source: SaTeX `math_shift_token` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs
  | mathDisplay {s s1 s2 : State} {t u : Token} {c d : CharCode} {is : List Item} {w l i : Scaled} :
      HeadNP s t s1 → meaningTok s1 t = .char c .mathShift → s1.mode = { kind := .horizontal, inner := false } →
      Next s1 u s2 → meaningTok s2 u = .char d .mathShift →
      Exec E s (insertEvery "everydisplay" (pushNest { kind := .math, inner := false }
        (let s3 := famReset (newSaveLevel .mathShift none
                    (match s2.nest with
                     | p :: _ => if p.items = [] then popNest s2 else is.foldr appendItem (popNest s2)
                     | [] => s2))
         let s3 := eqDefine s3 (.dimPar "predisplaysize") (.int w)
         let s3 := eqDefine s3 (.dimPar "displaywidth") (.int l)
         eqDefine s3 (.dimPar "displayindent") (.int i))))
  -- source: tex.web § 1194 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L22404
  -- source: SaTeX `close_math` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs
  | mathEnd {s s1 : State} {t : Token} {c : CharCode} :
      HeadNP s t s1 → meaningTok s1 t = .char c .mathShift → s1.mode = { kind := .math, inner := true } →
      topGroup s1 = .mathShift → (outerMode s1 ≠ { kind := .math, inner := false }) →
      Exec E s (appendItem .math (popNest (unsave s1).1))
  /-- The end of display math (§§ 1194–1200): the nest level goes, a second
  `$` is required (§ 1197), then `resume_after_display` (§ 1200) unsaves,
  resumes the paragraph and scans an optional space. -/
  -- source: tex.web § 1194 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L22404
  -- source: tex.web § 1197 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L22470
  -- source: tex.web § 1200 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L22526
  -- source: SaTeX `close_math` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs
  | displayEnd {s s1 s2 s3 : State} {t u : Token} {c : CharCode} :
      HeadNP s t s1 → meaningTok s1 t = .char c .mathShift → s1.mode = { kind := .math, inner := false } →
      topGroup s1 = .mathShift → XNext E (popNest s1) u s2 →
      OptSpace E (resumeAfterDisplay (dollarCheck s2 u)) s3 → Exec E s s3
  /-- … an `\eqno` part ends (§ 1194, `mode = -m`): another `$` is
  required, the number's group goes, then the display ends in the same
  step. -/
  -- source: tex.web § 1194 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L22404
  -- source: SaTeX `close_math` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs
  | eqnoEnd {s s1 s2 s3 : State} {t u : Token} {c : CharCode} :
      HeadNP s t s1 → meaningTok s1 t = .char c .mathShift → s1.mode = { kind := .math, inner := true } →
      topGroup s1 = .mathShift → outerMode s1 = { kind := .math, inner := false } →
      XNext E (popNest s1) u s2 →
      OptSpace E (resumeAfterDisplay (popNest (unsave (dollarCheck s2 u)).1)) s3 → Exec E s s3
  | mathShiftWrong {s s1 : State} {t : Token} {c : CharCode} :
      HeadNP s t s1 → meaningTok s1 t = .char c .mathShift → s1.mode.kind = .math →
      topGroup s1 ≠ .mathShift →
      Exec E s (raise "Missing } inserted" (pushToks .inserted (closerFor (topGroup s1)) (backInput t s1)))
  /-- `\eqno`, `\leqno` in display math (§ 1140). **[over]** placement. -/
  -- source: tex.web § 1140 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L21725
  -- source: tex.web § 1142 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L21740
  -- source: SaTeX `math_shift_token` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs
  | eqno {s s1 : State} {t : Token} :
      HeadNP s t s1 → cmdOf s1 t = some (.mode "eqno") → s1.mode = { kind := .math, inner := false } →
      topGroup s1 = .mathShift →
      Exec E s (insertEvery "everymath" (pushNest { kind := .math, inner := true }
        (famReset (newSaveLevel .mathShift none s1))))
  /-- `\left` (§ 1191): a delimiter, then a group of its own. -/
  | left {s s1 s2 : State} {t : Token} :
      HeadNP s t s1 → cmdOf s1 t = some (.mode "left") → s1.mode.kind = .math → Delim E s1 s2 →
      Exec E s (pushNest { kind := .math, inner := true } (newSaveLevel .mathLeft none s2))
  | right {s s1 s2 : State} {t : Token} :
      HeadNP s t s1 → cmdOf s1 t = some (.mode "right") → s1.mode.kind = .math →
      topGroup s1 = .mathLeft → Delim E s1 s2 →
      Exec E s (appendItem .other (popNest (unsave s2).1))
  /-- e-TeX `\middle`: closes and reopens (etex.ch). -/
  | middle {s s1 s2 : State} {t : Token} :
      HeadNP s t s1 → cmdOf s1 t = some (.mode "middle") → s1.mode.kind = .math →
      topGroup s1 = .mathLeft → Delim E s1 s2 →
      Exec E s (pushNest { kind := .math, inner := true } (newSaveLevel .mathLeft none (popNest (unsave s2).1)))
  | rightExtra {s s1 s2 : State} {t : Token} {n : String} :
      HeadNP s t s1 → cmdOf s1 t = some (.mode n) → (n = "right" ∨ n = "middle") → s1.mode.kind = .math →
      topGroup s1 = .mathShift → Delim E s1 s2 → Exec E s (raise "Extra \\right" s2)
  /-- **[over]** Alignments (§§ 768–812: `\halign`, `\valign`, `\noalign`,
  `\cr`, `\span`, `\omit`, `&`): templates are not modelled. The
  alignment may do anything but change the file system. -/
  | alignment {s s1 s2 : State} {t : Token} {n : String} :
      HeadNP s t s1 → (n ∈ ["halign", "valign", "noalign", "cr", "crcr", "span", "omit"]) →
      nameOfMeaning (meaningTok s1 t) = n → s2.vfs = s1.vfs → Exec E s s2
  /-- `&` outside an alignment (§ 1128). -/
  | alignTab {s s1 : State} {t : Token} :
      HeadNP s t s1 → ((∃ c, meaningTok s1 t = .char c .alignTab) ∨ cmdOf s1 t = some .alignTab) →
      Exec E s (raise "Misplaced alignment tab character &" s1)
  /-- `#` in the stomach (§ 1045 `report_illegal_case`), and internal
  quantities used as commands. -/
  | illegal {s s1 : State} {t : Token} :
      HeadNP s t s1 →
      ((∃ c, meaningTok s1 t = .char c .param) ∨ cmdOf s1 t = some .alignMark ∨
       (∃ op, cmdOf s1 t = some (.lastItem op)) ∨ (∃ l, cmdOf s1 t = some (.expr l))) →
      Exec E s (raise "You can't use this here" s1)
  /-- Commands SaTeX scans but whose effect is typesetting, PDF output or a
  whatsit (§§ 1341–1380, pdfTeX/XeTeX manuals): operands by grammar.
  **[over]** an item of any kind may be appended. -/
  | operands {s s1 s2 : State} {t : Token} {ops : List Operand} {i : Option Item} :
      HeadNP s t s1 → cmdOf s1 t = some (.operands ops) →
      (s1.mode.kind ≠ .math → nameOfMeaning (meaningTok s1 t) ∉ mathOnly) →
      Operands E s1 ops s2 →
      Exec E s (match i with | some it => appendItem it s2 | none => s2)
  /-- **[over]** Lua (`\latelua`, catcode tables, Lua-defined protected
  commands): SaTeX runs real Lua. Any assignment, any text read next. -/
  | lua {s s1 s2 s3 : State} {t : Token} {ts : TokList} :
      HeadNP s t s1 →
      ((∃ n, cmdOf s1 t = some (.lua n)) ∨ (∃ id, meaningTok s1 t = .luaCall id true)) →
      Operands E s1 (E.opsOf (nameOfMeaning (meaningTok s1 t))) s2 →
      s3.vfs = s2.vfs → s3.input = s2.input → Exec E s (pushToks .inserted ts s3)
  | setFontId {s s1 s2 : State} {t : Token} {f : Int} :
      HeadNP s t s1 → cmdOf s1 t = some .setFontId → ScanInt E s1 f s2 →
      Exec E s (assignTo s2 .curFont (.int f))

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
