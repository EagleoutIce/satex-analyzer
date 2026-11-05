# The formal model

The [concrete rules](Semantics) and the [abstract domains](Domains) as a Lean 4 model, with the soundness theorem that ties them together.
The model is core Lean, no mathlib, and `lake build` passes with no `sorry`.
It states what TeX does and what satex computes in place of it. It does not check the Rust. The [mapping](#lean-and-rust) says where the two differ.

> **AI assistance.** The Lean model (`Core`, `Concrete`, `Abstract`, `Soundness`), its mapping to the Rust code and the primitive table were written with AI assistance from Claude Code (Claude Opus 5.5, the build fixes with Claude Sonnet 5.5), directed and reviewed by the author. The global theorem proves the effects of assignment and grouping and names what is left: the abstract reading of commands and the families without a transfer. See [`PROOF-STATUS.md`](../semantics/PROOF-STATUS.md).

Every Lean file is shown as code on this site. Each name links to its definition. Hovering a name shows its type and doc comment. Section numbers (`§ 207`) link to the line of `tex.web` that defines them.

## The files

- [`Core.lean`](../semantics/lean/Satex/Core.lean): characters, category codes, tokens, meanings, the state TeX computes with. Shared by the rest.
- [`Concrete.lean`](../semantics/lean/Satex/Concrete.lean): TeX itself as inductive rules, one `Exec` rule per primitive. This is the [concrete semantics](Semantics) in code.
- [`Abstract.lean`](../semantics/lean/Satex/Abstract.lean): the abstract values, joins and widenings of the [domains](Domains), and the abstract state.
- [`Soundness.lean`](../semantics/lean/Satex/Soundness.lean): the theorems. Every concrete trace of a document is covered by an abstract path, and a run ends within its budget.
- `Axioms.lean`: a check that the main theorems use only Lean's standard axioms.

The theorems that are proven and the ones that are not yet are listed in [PROOF-STATUS](../semantics/PROOF-STATUS.md). Findings of the last review are in [REVIEW-2](../semantics/REVIEW-2.md).

## Lean and Rust

<!--satex-site:mapping-->

## Primitives

One row per primitive the engines define. The rule is the `Exec` rule of `Concrete.lean` that gives it meaning. A primitive is **exact** when the rule is TeX's own, **over-approximated** when the rule states a superset of what TeX does, and has **no rule yet** when satex follows the manual but the model is silent. The Rust column is where `builtins.rs` names it.
<!--satex-site:primitives-->

Next: [Primitive coverage](Primitive-coverage), what each primitive does in the interpreter.
