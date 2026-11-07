/-
Satex.Core: the objects TeX computes with, as SaTeX reads them.

Written with AI assistance from Claude Code (Claude Opus 5.5; build fixes with
Claude Sonnet 5.5), directed and reviewed by the author.

Shared by all semantics jobs. Names here are stable once published (see
semantics/CHANGES.md). Citations: tex.web module numbers (§), the eTeX
manual (etex_man), the pdfTeX, XeTeX and LuaTeX manuals, SEMANTICS.md (SEM).

Plain core Lean 4, no mathlib. Partial maps are total functions with a
default, so the state needs no finite-map library.
-/

namespace Satex

/-! ## Characters and category codes (§ 207, SEM § 2) -/

/-- A character code. 0–255 under an 8-bit engine, 0–0x10FFFF under XeTeX
and LuaTeX (SEM § 3). The engine's decoding happens before the mouth. -/
abbrev CharCode := Nat

/-- The 16 category codes, in tex.web's order (§ 207). -/
inductive Cat where
  | escape | bgroup | egroup | mathShift | alignTab | eol | param | sup
  | sub | ignored | space | letter | other | active | comment | invalid
  deriving DecidableEq, Repr, Inhabited

namespace Cat

def toNat : Cat → Nat
  | .escape => 0 | .bgroup => 1 | .egroup => 2 | .mathShift => 3 | .alignTab => 4
  | .eol => 5 | .param => 6 | .sup => 7 | .sub => 8 | .ignored => 9 | .space => 10
  | .letter => 11 | .other => 12 | .active => 13 | .comment => 14 | .invalid => 15

/-- Out-of-range values are rejected by `\catcode` (§ 1232); `other` is a
total default only. -/
def ofCode : Nat → Cat
  | 0 => .escape | 1 => .bgroup | 2 => .egroup | 3 => .mathShift | 4 => .alignTab
  | 5 => .eol | 6 => .param | 7 => .sup | 8 => .sub | 9 => .ignored | 10 => .space
  | 11 => .letter | 12 => .other | 13 => .active | 14 => .comment | 15 => .invalid
  | _ => .other

/-- Categories a character token may carry (§ 289): the others never leave
the mouth. -/
def tokenable : Cat → Bool
  | .escape | .eol | .ignored | .comment | .invalid | .active => false
  | _ => true

end Cat

/-! ## Tokens (§ 289, SEM § 4) -/

/-- A control sequence: a name, or an active character (§ 222 `active_base`). -/
inductive CsRef where
  | name (n : String)
  | active (c : CharCode)
  deriving DecidableEq, Repr, Inhabited

/-- A token. `noexp r` is `r` behind `\noexpand`'s `dont_expand` marker
(§ 358, § 369): it reads as `\relax` with `no_expand_flag` and is `\ifx`-equal
to nothing else (SEM § 8). `outParam n` and `macParam` occur only in a stored
replacement text (§ 289 `out_param`, `mac_param`). -/
inductive Token where
  | char (c : CharCode) (k : Cat)
  | cs (r : CsRef)
  | noexp (r : CsRef)
  | outParam (n : Nat)
  deriving DecidableEq, Repr, Inhabited

namespace Token
def isBgroup : Token → Bool | char _ .bgroup => true | _ => false
def isEgroup : Token → Bool | char _ .egroup => true | _ => false
def isSpace  : Token → Bool | char _ .space => true | _ => false
/-- The space token every category-10 character becomes (§§ 348–349). -/
def space : Token := char 32 .space
def ofName (n : String) : Token := cs (.name n)
end Token

abbrev TokList := List Token

/-! ## Numbers, dimensions, glue (§§ 101–108, 150, SEM § 11) -/

/-- Scaled points. `1pt = 65536sp`. -/
abbrev Scaled := Int

def unity : Int := 65536
def maxDimen : Int := 1073741823        -- 2^30 - 1 (§ 421)
def infinity : Int := 2147483647        -- 2^31 - 1 (§ 445)

/-- Glue orders: normal, fil, fill, filll (§ 150). -/
abbrev GlueOrder := Nat

/-- A glue specification (§ 150). -/
structure Glue where
  width : Scaled := 0
  stretch : Scaled := 0
  stretchOrder : GlueOrder := 0
  shrink : Scaled := 0
  shrinkOrder : GlueOrder := 0
  deriving DecidableEq, Repr, Inhabited

def Glue.zero : Glue := {}

/-- The levels of internal quantities (§ 410): int, dimen, glue, mu_glue,
ident (font), tok. -/
inductive Level where
  | int | dimen | glue | mu | ident | tok
  deriving DecidableEq, Repr, Inhabited

/-! ## Boxes and lists, at the depth SaTeX models them (SEM § 9) -/

/-- A box register's content. Material is not kept: a box SaTeX cannot size
carries the dimensions the run decided (any, in the concrete semantics). -/
inductive BoxVal where
  | void
  | hbox (w h d : Scaled)
  | vbox (w h d : Scaled)
  deriving DecidableEq, Repr, Inhabited

/-- The last item of a list, which `\lastpenalty`, `\lastkern`,
`\lastskip`, `\lastnodetype`, `\unskip` read (§ 424, etex.ch). -/
inductive Item where
  | char | ligature | box | rule | glue (g : Glue) | kern (d : Scaled)
  | penalty (n : Int) | mark | whatsit | math | disc | other
  | vbox                            -- a vlist node; `box` is an hlist node
  -- source: tex.web § 137 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L2937; SaTeX `append_box` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/mode.rs
  deriving DecidableEq, Repr, Inhabited

/-- Modes (§ 211). `inner` is internal vertical, restricted horizontal or
non-display math. -/
inductive ModeKind where
  | vertical | horizontal | math
  deriving DecidableEq, Repr, Inhabited

structure Mode where
  kind : ModeKind := .vertical
  inner : Bool := false
  deriving DecidableEq, Repr, Inhabited

/-- One level of the semantic nest (§ 211–213). -/
structure ListState where
  mode : Mode := {}
  items : List Item := []      -- newest first
  deriving Repr, Inhabited

/-! ## Groups (§ 269) -/

inductive GroupCode where
  | bottom | simple | hbox | adjustedHbox | vbox | vtop | align | noAlign
  | output | math | disc | insert | vcenter | mathChoice | semiSimple
  | mathShift | mathLeft
  deriving DecidableEq, Repr, Inhabited

def GroupCode.toNat : GroupCode → Nat
  | .bottom => 0 | .simple => 1 | .hbox => 2 | .adjustedHbox => 3 | .vbox => 4
  | .vtop => 5 | .align => 6 | .noAlign => 7 | .output => 8 | .math => 9
  | .disc => 10 | .insert => 11 | .vcenter => 12 | .mathChoice => 13
  | .semiSimple => 14 | .mathShift => 15 | .mathLeft => 16

/-- What a box group does with its box when it ends (§ 1075 `box_context`). -/
inductive BoxContext where
  | shift (amount : Scaled)          -- append, shifted
  | setBox (glob : Bool) (n : Nat)   -- \setbox
  | leaders                          -- \leaders family
  | shipout
  deriving DecidableEq, Repr, Inhabited

/-! ## Commands: what a primitive meaning does

A primitive is identified by the name it was born as and its command class.
`\let` copies both, so `\ifx` tells copies of two primitives apart (§ 1221,
§ 507, SEM § 5). The classes follow the `Primitive` enum of
src/engine/builtins.rs; semantics/primitives.tsv maps every name. -/

inductive RegKind where
  | count | dimen | skip | muskip | toks | box
  deriving DecidableEq, Repr, Inhabited

/-- The per-character code tables (§ 230, § 1230). -/
inductive CodeTable where
  | cat | lc | uc | sf | math | del
  deriving DecidableEq, Repr, Inhabited

/-- One operand a command scans. Commands whose effect SaTeX models only up
to reading their operands are given as such a list (SEM § 14). -/
inductive Operand where
  | int | dimen | glue | mu | optEquals | generalText | balancedText
  | fileName | font | box | ruleSpec | anyToken
  | keyword (k : String)
  | optKeyword (k : String) (thenOps : List Operand)
  deriving Inhabited

/-- Expandable converters (§ 468 `convert`, etex.ch, pdftex.web). -/
inductive ConvOp where
  | number | romannumeral | string | meaning | fontname | jobname
  | detokenize | etexRevision
  | other (name : String)     -- pdfTeX/XeTeX/LuaTeX converters, see Concrete
  deriving DecidableEq, Repr, Inhabited

/-- Conditionals (§ 487 `if_test` chr codes, etex.ch, pdftex.web). -/
inductive IfOp where
  | charIf | catIf | num | dim | odd | vmode | hmode | mmode | inner | void
  | hboxIf | vboxIf | ifx | eof | iftrue | iffalse | ifcase
  | defined | csname | fontchar | absnum | absdim | incsname | primitive
  | condition                       -- LuaTeX \ifcondition
  deriving DecidableEq, Repr, Inhabited

/-- `\fi`, `\else`, `\or` (§ 489). -/
inductive FiCode where
  | fi | elseCode | orCode
  deriving DecidableEq, Repr, Inhabited

/-- Internal quantities computed rather than stored (§ 416 `last_item`,
etex.ch, pdftex.web). `name` keeps the primitive. -/
inductive LastOp where
  | lastpenalty | lastkern | lastskip | inputlineno | badness
  | eTeXversion | currentgrouplevel | currentgrouptype
  | currentiflevel | currentiftype | currentifbranch | lastnodetype
  | fontcharwd | fontcharht | fontchardp | fontcharic
  | parshapelength | parshapeindent | parshapedimen
  | gluestretch | glueshrink | gluestretchorder | glueshrinkorder
  | mutoglue | gluetomu
  | constant (name : String)      -- version numbers, \shellescape
  | unknowable (name : String)    -- clock, RNG, positions, XeTeX font tables
  deriving DecidableEq, Repr, Inhabited

/-- The command classes. -/
inductive Cmd where
  -- gullet (§ 366)
  | expandafter | noexpand | csname | begincsname | lastnamedcs
  | convert (op : ConvOp)
  | the | unexpanded | expanded
  | scantokens (text : Bool)
  | ifTest (op : IfOp)
  | fiOrElse (code : FiCode)
  | unlessCmd
  | input | endinput
  | topBotMark (code : Nat) (multi : Bool)
  | primitiveCmd                    -- \pdfprimitive
  -- stomach (§ 1045, § 1210)
  | relax | endcsname | ignorespaces
  | defCmd (glob expand : Bool)
  | letCmd (future : Bool)
  | gletCmd                        -- LuaTeX \glet
  | pfx (p : Nat)                   -- 1 long, 2 outer, 4 global, 8 protected (§ 1211, etex.ch)
  | immediate
  | beginGroup | endGroup | aftergroup | afterassignment
  | register (k : RegKind)
  | shorthandDef (k : RegKind) (charLike : Nat)  -- \countdef…; \chardef 1, \mathchardef 2
  | advance | multiply | divide
  | intPar (n : String) | dimPar (n : String) | gluePar (n : String) (mu : Bool)
  | toksPar (n : String)
  | codeTable (t : CodeTable)
  | caseShift (upper : Bool)
  | expr (l : Level)                -- \numexpr … (etex_man § 3.5)
  | lastItem (op : LastOp)
  | fontDef | fontParam (n : String) | setFontId
  | openin | closein | readCmd (line : Bool) | openout | closeout | write
  | message (err : Bool)
  | endCmd
  | showCmd (n : String)
  | interaction (n : Nat)
  | special (n : String)            -- \parshape, \wd, penalties arrays, …
  | mode (n : String)               -- mode-governed commands (§ 1045)
  | material (horizontal : Bool) (n : String)
  | boxCmd (n : String)             -- \hbox \vbox \vtop \vcenter \copy \box \lastbox \vsplit
  | operands (ops : List Operand)   -- scanned, effect not modelled (SEM § 14)
  | lua (n : String)
  | mathParam (n : String) (k : RegKind)       -- LuaTeX `\Umath…` per style
  | toksApp (expand glob front : Bool)       -- LuaTeX `\toksapp` family
  | famFont (size : Nat)                     -- \textfont 0, \scriptfont 1, \scriptscriptfont 2
  | letCharcode                              -- LuaTeX `\letcharcode`
  | fontCopy (spacing : Bool)                -- \copyfont, \pdfcopyfont; \letterspacefont
  | alignMark | alignTab
  /-- `\spacefactor`, `\prevdepth`, `\prevgraf`, `\deadcycles`,
  `\insertpenalties`, the page dimensions, `\interactionmode`: list, page or
  global values set by `alter_aux` and its relatives (§§ 1242–1246), never
  saved by groups. -/
  -- source: tex.web § 1242 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L23197; SaTeX `Owned::List` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/builtins.rs
  | alter (n : String)
  deriving Inhabited

/-! ## Meanings (§ 210, SEM § 5) -/

/-- An item of a macro's parameter text: a delimiter token, or `#n` with
its parameter character (§ 476 `match_token+cur_chr`). -/
-- source: tex.web § 476 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L9357; SaTeX `ParameterText` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/tex.rs
inductive ParamItem where
  | tok (t : Token)
  | arg (c : CharCode)
  deriving DecidableEq, Repr, Inhabited

structure Macro where
  long : Bool := false
  outer : Bool := false
  prot : Bool := false
  params : List ParamItem := []
  /-- The parameter text ended in `#{` (§ 476). -/
  braceDelim : Bool := false
  /-- The brace token that ended it (§ 476 `hash_brace`). -/
  -- source: tex.web § 476 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L9357; SaTeX `make_macro` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
  brace : Token := .char 123 .bgroup
  body : TokList := []          -- `outParam n` marks `#n`
  deriving DecidableEq, Repr, Inhabited

inductive Meaning where
  | undefined
  | prim (name : String) (c : Cmd)
  | call (m : Macro)
  | char (c : CharCode) (k : Cat)          -- \let\a=b
  | chardef (n : Nat) | mathchardef (n : Nat)
  | reg (k : RegKind) (i : Nat)            -- \countdef\a=3
  | font (f : Nat)
  | luaCall (id : Nat) (prot : Bool)
  deriving Inhabited

/-! ## The equivalents table with grouping (§§ 220–255, 268–284) -/

/-- A location of eqtb. -/
inductive Loc where
  | meaning (r : CsRef)
  | code (t : CodeTable) (c : CharCode)
  | reg (k : RegKind) (i : Nat)
  | intPar (n : String) | dimPar (n : String) | gluePar (n : String)
  | toksPar (n : String)
  | curFont
  | famFont (size fam : Nat)
  | special (n : String) (i : Nat)  -- parshape and penalties arrays, \uccode-like extras
  deriving DecidableEq, Repr, Inhabited

inductive Val where
  | meaning (m : Meaning)
  | int (i : Int)
  | glue (g : Glue)
  | toks (t : TokList)
  | box (b : BoxVal)
  deriving Inhabited

/-- An eqtb entry: value and the level it was set at (§ 221 `eq_level`). -/
structure Entry where
  val : Val
  level : Nat := 1
  deriving Inhabited

/-- Save-stack entries (§ 268). -/
inductive SaveEntry where
  | restore (l : Loc) (old : Entry)   -- restore_old_value
  | retain (l : Loc)                 -- restore_zero / global marker
  | insertToken (t : Token)          -- insert_token (\aftergroup)
  | group (c : GroupCode) (ctx : Option BoxContext) (line : Nat)
  deriving Inhabited

/-! ## Input (§§ 300–331) -/

/-- The scanner state of a file level (§ 303). -/
inductive MouthState where
  | newLine | skipBlanks | midLine
  deriving DecidableEq, Repr, Inhabited

/-- What a token-list level is (§ 307 `token_type`). -/
inductive TokListKind where
  | backedUp | inserted | macroBody (n : CsRef) | every (n : String)
  | mark | write | output | uTemplate | vTemplate
  deriving DecidableEq, Repr, Inhabited

/-- A file level. Lines are decoded character codes without the line
terminator (SEM § 3); `cur` is the rest of the current line, with its
`\endlinechar` already appended. -/
structure FileLevel where
  name : String
  cur : List CharCode := []
  lines : List (List CharCode) := []
  state : MouthState := .newLine
  line : Nat := 0
  /-- `\scantokens` pseudo file (etex.ch `pseudo_input`); `text` for
  `\scantextokens` (no `\endlinechar` on the last line, no `\everyeof`). -/
  pseudo : Bool := false
  text : Bool := false
  /-- `\everyeof` was inserted at the end of this file (etex.ch `eof_seen`). -/
  -- source: https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/etexdir/etex.ch#L933; SaTeX `end_input_frame` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/analysis/machine.rs
  eofSeen : Bool := false
  deriving Repr, Inhabited

inductive InputLevel where
  | toks (kind : TokListKind) (ts : TokList)
  | file (f : FileLevel)
  deriving Repr, Inhabited

/-! ## Conditionals (§ 489) -/

inductive IfLimit where
  | ifCode | fiCode | elseCode | orCode
  deriving DecidableEq, Repr, Inhabited

structure CondLevel where
  limit : IfLimit
  op : IfOp
  negated : Bool := false              -- under `\unless` (etex.ch)
  line : Nat := 0
  deriving Repr, Inhabited

/-! ## Streams and files -/

/-- A `\read` stream (§ 480 `read_open`). -/
inductive ReadStream where
  | closed
  | justOpen (lines : List (List CharCode))
  | normal (lines : List (List CharCode))
  deriving Repr, Inhabited

/-- Something the run emits: the log and terminal, written files. -/
inductive Output where
  | message (text : List CharCode)
  | error (text : List CharCode)
  | write (stream : Int) (text : List CharCode)
  | shell (text : List CharCode)
  | whatsit (what : String)
  deriving Repr, Inhabited

/-- What the stomach is in the middle of, when one command spans steps. -/
structure Pending where
  /-- Prefix bits read so far (§ 1211): 1 long, 2 outer, 4 global, 8 protected. -/
  prefixes : Nat := 0
  immediate : Bool := false
  afterAssignment : Option Token := none   -- § 1269
  deriving Repr, Inhabited

/-- A loaded font (§ 549): its name, size, design size, parameters. -/
-- source: tex.web § 549 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L10711; SaTeX `font_def` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/engine/exec.rs
structure Font where
  name : String
  size : Scaled := 0
  dsize : Scaled := 0
  params : Nat → Scaled := fun _ => 0

/-! ## The state -/

structure State where
  eqtb : Loc → Entry
  curLevel : Nat := 1                     -- § 221 `level_one`
  save : List SaveEntry := []
  input : List InputLevel := []           -- head is the current level
  cond : List CondLevel := []             -- head is `cond_ptr`
  nest : List ListState := [{}]           -- head is `cur_list`
  /-- A read-only virtual file system: what the resolver finds (SEM § 15). -/
  vfs : String → Option (List (List CharCode))
  readStreams : Nat → ReadStream := fun _ => .closed
  writeOpen : Nat → Option String := fun _ => none
  out : List Output := []                  -- newest first
  pending : Pending := {}
  jobname : String := "texput"
  interaction : Nat := 1                    -- nonstopmode (§ 73: batch 0, nonstop 1, scroll 2, errorstop 3), SEM § 1
  errorCount : Nat := 0                     -- § 76
  /-- Fonts (§ 549); index 0 is `\nullfont`. -/
  fonts : List Font := [{ name := "nullfont" }]
  inCsname : Nat := 0                       -- pdfTeX \ifincsname
  lastCs : Option CsRef := none             -- LuaTeX \lastnamedcs
  /-- Set by `\end` once the job is over (§ 1054). -/
  halted : Bool := false
  /-- Fatal error ended the job (§ 93). -/
  fatal : Bool := false
  /-- `force_eof` (§ 361): `\endinput` was executed; the next file level
  that wants a line ends instead, and clears it (§ 362). One global flag. -/
  -- source: tex.web § 362 https://github.com/TeX-Live/texlive-source/blob/a553f86ad9f31c0a70b94c0245ab8247ff8b8c56/texk/web2c/tex.web#L7563; SaTeX `take_pending_eof` https://github.com/EagleoutIce/satex-analyzer/blob/main/src/analysis/machine.rs
  forceEof : Bool := false

namespace State

def get (s : State) (l : Loc) : Val := (s.eqtb l).val

def meaningOf (s : State) (r : CsRef) : Meaning :=
  match s.get (.meaning r) with
  | .meaning m => m
  | _ => .undefined

def intAt (s : State) (l : Loc) : Int :=
  match s.get l with
  | .int i => i
  | _ => 0

def catOf (s : State) (c : CharCode) : Cat := Cat.ofCode (s.intAt (.code .cat c)).toNat

def intPar (s : State) (n : String) : Int := s.intAt (.intPar n)

def toksAt (s : State) (l : Loc) : TokList :=
  match s.get l with
  | .toks t => t
  | _ => []

def mode (s : State) : Mode :=
  match s.nest with
  | l :: _ => l.mode
  | [] => {}

end State

/-- Point update of a total function. -/
def upd {α β : Type} [DecidableEq α] (f : α → β) (a : α) (b : β) : α → β :=
  fun x => if x = a then b else f x

end Satex
