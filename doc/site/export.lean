import Lean
import Satex
open Lean Meta Elab

def kindOf : ConstantInfo → String
  | .defnInfo _ => "def" | .thmInfo _ => "theorem" | .inductInfo _ => "inductive"
  | .ctorInfo _ => "constructor" | .opaqueInfo _ => "opaque" | .axiomInfo _ => "axiom"
  | .recInfo _ => "recursor" | .quotInfo _ => "quot"

def entries : CoreM (Array Json) := do
  let env ← getEnv
  let mut out : Array Json := #[]
  for (n, ci) in env.constants.map₁.toList do
    let some mod := env.getModuleIdxFor? n | continue
    let modName := env.header.moduleNames[mod.toNat]!
    unless modName.getRoot == `Satex do continue
    if n.isInternal || n.isAnonymous then continue
    let ranges ← findDeclarationRanges? n
    let ty ← try (do let f ← (ppExpr ci.type : MetaM Format).run'; pure (toString f) : CoreM String) catch _ => pure ""
    let doc ← findDocString? env n
    let (l1, l2) := match ranges with
      | some r => (r.range.pos.line, r.range.endPos.line) | none => (0, 0)
    out := out.push <| Json.mkObj [("name", n.toString), ("kind", kindOf ci), ("type", ty),
      ("doc", doc.getD ""), ("module", modName.toString), ("line", l1), ("end", l2)]
  return out

def main : IO Unit := do
  initSearchPath (← findSysroot)
  let env ← importModules #[{ module := `Satex }] {}
  let ctx : Core.Context := { fileName := "", fileMap := default }
  let (out, _) ← entries.toIO ctx { env := env }
  IO.println (Json.arr out).compress
