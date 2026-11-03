import Lake
open Lake DSL

-- Shared by the three semantics jobs: Core, Concrete, and the abstract
-- domains built on them. Plain core Lean 4, no mathlib.
package Satex

@[default_target]
lean_lib Satex
