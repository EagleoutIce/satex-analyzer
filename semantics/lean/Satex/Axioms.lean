/-
Satex.Axioms: the main theorems use only Lean's standard axioms (`propext`,
`Classical.choice`, `Quot.sound`): no gap and no axiom of our own. The
build fails if that changes.

Written with AI assistance from Claude Code (Claude Opus 5.5; build fixes with
Claude Sonnet 5.5), directed and reviewed by the author.
-/
import Satex.Soundness

open Satex.Soundness

/-- info: 'Satex.Soundness.satex_sound_and_terminating' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms satex_sound_and_terminating

/-- info: 'Satex.Soundness.run_covered_families' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms run_covered_families

/-- info: 'Satex.Soundness.endGroup_sound' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms endGroup_sound

/-- info: 'Satex.Soundness.assign_local_sound' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms assign_local_sound

/-- info: 'Satex.Soundness.weakAssign_sound' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms weakAssign_sound

/-- info: 'Satex.Soundness.summaryHavoc_sound' depends on axioms: [propext] -/
#guard_msgs in #print axioms summaryHavoc_sound

/-- info: 'Satex.Soundness.scanDelim_sound' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms scanDelim_sound

/-- info: 'Satex.Soundness.stripBraces_sound' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms stripBraces_sound

/-- info: 'Satex.Soundness.aval_widen_sound' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms aval_widen_sound

/-- info: 'Satex.Soundness.astate_join_sound' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs in #print axioms astate_join_sound

/-- info: 'Satex.Soundness.loopHead_sound' depends on axioms: [propext] -/
#guard_msgs in #print axioms loopHead_sound

/-- info: 'Satex.Soundness.branch_sound' depends on axioms: [propext] -/
#guard_msgs in #print axioms branch_sound

/-- info: 'Satex.Soundness.ifNum_branch_sound' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in #print axioms ifNum_branch_sound
