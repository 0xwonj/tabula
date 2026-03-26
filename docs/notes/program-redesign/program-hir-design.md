# Program HIR Design

> **Status**: Implemented V3 structured-control architecture note
> **Date**: 2026-03-25
> **Scope**: Defines the intended HIR role and the MLIR-inspired design choices
> that now guide the new `tabula-lang::program` frontend.
> **Related**: [program-hir-contract-and-data-model.md](program-hir-contract-and-data-model.md),
> [program-mir-design.md](program-mir-design.md),
> [program-typing-and-effect-system.md](program-typing-and-effect-system.md)

---

## 1. Position in the Stack

The active compiler stack is:

1. AST
2. HIR
3. MIR
4. canonical IR
5. `RuntimeProgram { execution, proof }`
6. executor/runtime

HIR is the first semantic layer and the last strongly source-shaped layer.

---

## 2. Core Design Thesis

The ideal Tabula HIR is:

- symbol-table oriented
- region-based
- non-SSA
- declaration-category preserving
- source-semantic
- compiler-friendly without being proof-shaped

The most important negative constraint is:

> **HIR must not try to become MIR early.**

That means HIR should not absorb:

- ANF normalization
- SSA locals
- effect summaries
- canonical guards
- runtime/executor identities

---

## 3. What We Borrow from MLIR

The useful MLIR pattern is not “copy MLIR syntax”. It is a set of structural
habits.

### 3.1 Separate structural IR from analyses

Raw HIR is structure plus resolved source semantics.

It is not:

- effect analysis
- call graph analysis
- normalization state

That separation matches the MIR refactor and keeps pass ownership clean.

### 3.2 Make regions explicit early

HIR now uses:

- `Body`
- `Region`
- `Terminator`

This now directly supports exact V3 structured control: root regions end in
`Return`, nested control regions end in `Yield`, and `if` / `match` stay
structured in HIR rather than being flattened early.

### 3.3 Keep symbol tables and lexical scopes explicit

HIR is the correct layer to preserve:

- top-level declaration categories
- lexical bindings
- source names
- source-oriented diagnostics

This is closer to MLIR symbol-based frontends than to low-level CFG IRs.

### 3.4 Use passes, not omniscient nodes

The frontend is intentionally split into:

- collection
- building
- verification

The public API stays free-function based. The implementation uses context
objects such as `CollectCx`, `BuildCx`, `BodyBuildCx`, and `VerifyCx`.

That is the same architectural instinct as MLIR verification, analysis, and
conversion passes.

### 3.5 Let verification depend on semantic interfaces, not builder history

The current HIR verifier takes the same immutable `FrontendPrelude` semantic
context as the builder.

That is deliberate:

- builder-time checking is allowed for early diagnostics,
- but verifier-time checking must still independently reject invalid raw HIR,
- and the shared semantic interface is the frontend equivalent of MLIR
  verifier hooks plus dialect/type interfaces.

So the authoritative contract is not “builder already resolved it”, but rather
“raw HIR plus semantic context verifies”.

---

## 4. What HIR Preserves

HIR preserves source-semantic categories that MIR intentionally erases later.

In current exact code this means preserving:

- one program root
- capability imports
- context declarations
- state declarations
- const declarations
- relation declarations
- event declarations
- callable category: `Function` vs `Query` vs `Tx`
- lexical bindings
- source-shaped expressions

This is why HIR has separate declaration structs instead of pushing everything
through a generic node soup.

---

## 5. What HIR Does Not Preserve

HIR deliberately does not preserve:

- unresolved generic calls
- parser-only trivia
- runtime identity
- canonical op taxonomy
- proof visibility filtering
- effect summaries

Those belong either earlier in AST or later in MIR/canonical/runtime.

---

## 6. Why HIR Is Not SSA

HIR is not the place for SSA because the source language still wants:

- names
- lexical scopes
- direct source-shaped expression trees
- declaration-level body policies

SSA becomes useful only once the compiler starts normalizing computation order
and structured control for lowering. That is exactly why MIR exists.

So the division is:

- HIR: lexical bindings and source semantics
- MIR: ANF + explicit regions + analysis

---

## 7. Why HIR Is Not CFG

General CFG is a poor default for the source-semantic layer.

The important invariant is:

> **Structured source remains structured in HIR.**

In current exact code, that means one root region ending in `Return`.

Later, when `if` and `match` are enabled, the intended extension is still
structured nested regions, not arbitrary basic blocks.

---

## 8. Symbol Policy

The current rewritten symbol policy is intentionally strict.

- one flat top-level namespace
- table fields are table-local
- local bindings and params may not shadow top-level symbols or imported
  capability aliases
- bare call heads must resolve to function, capability, or blessed hash import

This reduces ambiguity during bring-up and keeps builder diagnostics sharp.

---

## 9. HIR and the Type/Effect Story

Typed effect reasoning is centered in MIR, not HIR.

HIR still participates in the static story by preserving callable categories and
 source distinctions such as:

- state access versus const use
- function call versus capability call
- relation assertion versus relation evaluation
- tx bodies versus helper functions

But HIR does not carry derived effect summaries.

This is deliberate and mirrors the MLIR-style separation between structural IR
and analyses.

One important corollary is that source-selected field scheme metadata stays in
frontend/compiler-owned structures. It is verified in HIR, preserved by the
next compiler artifact as sidecar metadata, and intentionally not pushed into
canonical IR.

---

## 10. HIR -> MIR Ownership

The ownership boundary is now fixed.

- `tabula-lang` stops at `VerifiedProgram`
- `tabula-compiler` owns `VerifiedProgram -> mir::Program`

This is the correct split because:

- HIR lowering is compiler conversion logic, not frontend semantic resolution
- MIR verification, analysis, inlining, and canonical lowering should remain
  MIR-owned

So the sequence is:

```text
parse -> build_hir -> verify_hir -> lower_hir_to_mir(program_id)
       -> verify MIR -> analyze MIR -> inline -> canonicalize
       -> analyze MIR -> lower to canonical
```

`ProgramId` is compiler-owned identity, not frontend-owned source semantics, so
it is minted by the compiler pipeline and injected at the HIR -> MIR boundary.

---

## 11. Current Scope Discipline

The current exact HIR code intentionally does not implement:

- `for`
- `requires` (intentionally deferred to a later phase)
- `ensures`
- `predicate`
- `invariant`

That means the rewritten frontend now owns:

- `context`
- `query`
- `event`
- `emit`
- statement-level `if`
- statement-level `match`

while still leaving later spec and sugar forms closed.

The important discipline is that exact code only opens the surface that the
rest of the stack can execute end-to-end today.

---

## 12. End-State Judgment

Under the current lower boundary, this HIR shape is the right one.

It is:

- minimal enough to avoid over-design
- structured enough to support later MLIR-style lowering
- separate enough from MIR to keep normalization and effect reasoning clean

The next correct growth path is not redesigning HIR again. It is:

- preserving the rewritten V2/V3 path as the default
- then opening later spec and sugar features in later phases
- while preserving the same HIR principles
