# Program MIR Design

> **Status**: Implemented architecture note
> **Date**: 2026-03-25
> **Scope**: Explains the intended role, structure, normalization policy, and
> research rationale for Tabula's MIR layer.
> **Related**: [program-mir-contract-and-data-model.md](program-mir-contract-and-data-model.md),
> [program-hir-contract-and-data-model.md](program-hir-contract-and-data-model.md),
> [program-canonical-ir-contract-and-data-model.md](program-canonical-ir-contract-and-data-model.md),
> [program-typing-and-effect-system.md](program-typing-and-effect-system.md),
> [verification vocabulary](../../design/architecture.md#verification-vocabulary)

---

## 1. MIR's Job

MIR exists because Tabula needs one middle layer that is:

- more normalized than HIR
- more structured than canonical IR
- compiler-owned
- analysis-backed
- and still capable of representing structured control

Without MIR:

- HIR becomes overloaded with compiler normalization work
- or canonical IR becomes polluted with source-like control and frontend detail

MIR is therefore the real middle-end of the redesigned compiler.

---

## 2. The Right Kind of MIR

The correct MIR for Tabula is **not** CFG-SSA.

It should instead be:

- **single-assignment**
- **ANF-like**
- **region-based**
- **single-block**
- **value-producing**
- **analysis-backed**

and explicitly **not**:

- arbitrary CFG
- phi-based SSA
- dominance-sensitive machine IR
- executor IR

That is the smallest structure that still makes:

- inlining
- legality checking
- effect inference
- control normalization
- and canonical lowering

clean and local.

The intended implementation pattern is MLIR-like:

- raw MIR is structural IR
- verification is a separate pass
- effect, failure, and policy reasoning are separate analyses
- inlining is a normalization pass
- canonicalization is a rewrite pass
- canonical lowering is a conversion pass

---

## 3. Exact Current Scope

Exact current MIR should contain:

- `Function`
- `Query`
- `Tx`
- `If`
- `Match`

and should not yet contain:

- `For`
- `Predicate`
- `Invariant`

Those remain reserved at the architecture or HIR layer. Keeping them out of the
exact MIR contract avoids a fake generality that the implementation does not
actually use.

---

## 4. One Callable Universe

MIR should model:

- helpers
- queries
- transactions

in one callable universe.

That means one `Vec<Callable>` with `CallableKind::{Function, Query, Tx}` is
better than separate buckets.

Why:

- call graph reasoning is uniform
- inlining works across one namespace
- query legality is naturally graph-based
- diagnostics still keep kind-specific meaning through `CallableKind`

Canonical IR removes `Function`. MIR intentionally does not.

---

## 5. Regions, Not CFG

The central MIR control abstraction is the **region**.

- root callable body is a region
- each `if` arm is a region
- each `match` arm is a region

Each region is a single sequence of ops plus an explicit terminator:

- root regions end in `Return`
- nested regions end in `Yield`

This has several advantages.

- It preserves structured control explicitly.
- It makes value-producing branches first-class.
- It avoids CFG/phi complexity.
- It aligns naturally with later canonical lowering into guards plus `Select`.

This is the strongest place where Tabula should absorb MLIR's region intuition
without adopting MLIR itself.

---

## 6. Why Single-Assignment ANF Is the Right Fit

Single-assignment ANF is the right compromise for Tabula.

It gives:

- explicit temporary values
- explicit evaluation order
- explicit op boundaries
- simple effect accounting
- simple inlining

without forcing:

- block arguments
- phi insertion
- dominance analysis
- machine-like low-level IR

Branch results do not mutate outer locals. They flow through:

- nested region `Yield`
- control-op destination locals

That keeps MIR simple and canonical lowering direct.

---

## 7. Value Model Alignment

MIR should already live in the same semantic universe as canonical IR.

So value references already use:

- literals
- params
- context fields
- locals
- constants

This is important because Tabula's later compiler work is not about inventing a
different semantic universe. It is about:

- control normalization
- callable normalization
- effect inference

on top of the same program meaning.

---

## 8. Pure Ops, Effect Ops, and Control Ops

MIR should distinguish three broad families.

### 8.1 Pure value ops

- arithmetic
- comparison
- boolean ops
- `Select`
- builtin `Hash`

These belong under `BindValue`.

### 8.2 Effectful or checked ops

- state read/write/delete
- property reads
- assertions
- relation ops
- capability calls
- event emission
- checked arithmetic such as `DivMod`

These remain explicit MIR ops because they matter for:

- legality
- effect inference
- later guard insertion

### 8.3 Control ops

- `If`
- `Match`

These remain structured in MIR and are eliminated before canonical IR.

---

## 9. Hash and Capability in MIR

MIR should preserve the same semantic distinction fixed below it.

### 9.1 Builtin `Hash`

Builtin `Hash` is:

- pure
- total
- deterministic
- value-producing
- not a journaled effect family

MIR analysis may record `uses_builtin_hash`, but that belongs to policy
analysis, not effect classification.

### 9.2 `CallCapability`

Capability is the typed operational-kernel family that remains outside the
language core.

MIR should already consume capability metadata:

- query policy
- totality
- proof visibility

but only:

- query policy
- totality

belong directly to MIR legality/effect reasoning.

Proof visibility is visible as metadata, but filtering remains runtime-owned.

---

## 10. Failure Model in MIR

MIR should carry the same failure split as the lower boundary.

- **semantic failure**
  - assertion failure
  - checked capability failure
  - checked relation failure in current bindings
  - checked arithmetic failure
- **host/runtime contract sensitivity**
  - total capability use

So the right MIR analysis does not collapse everything into a single `may_fail`
bit.

It needs a distinct failure summary with at least:

- `semantic_may_fail`
- `host_contract_sensitive`

That separation is essential because canonical/runtime/executor already make a
real semantic distinction between the two.

---

## 11. Query Legality Belongs in MIR Analysis

HIR can reject obviously wrong source forms, but MIR is where query legality
becomes precise.

Why MIR is the right place:

- `CallFunction` summaries can be propagated
- capability metadata is resolved
- structured control is explicit
- call graph analysis is explicit

So MIR is the first layer where it is easy to say:

- this query is read-only
- this query never emits events
- this query never calls tx-only capability
- this function is safe or unsafe to call from query context

This does **not** require query legality to be stored in raw MIR payload or
implemented by a second handwritten legality walk. The cleaner architecture is:

- `verify_program(program) -> VerifiedProgram`
- `analyze_program(verified) -> AnalyzedProgram`

and query legality is enforced when constructing `AnalyzedProgram` from merged
effect and policy summaries.

Rewrite passes such as inlining and canonicalization then invalidate those
derived analyses and return structural verified MIR again.

---

## 12. Lowering Boundary to Canonical IR

MIR is intentionally richer than canonical IR in only three ways:

- `Function`
- `CallFunction`
- structured control

Everything else is already close to canonical form.

That is ideal.

It means canonical lowering is a small, disciplined normalization step:

- inline functions
- recursively lower regions
- introduce guards only at canonical lowering time
- merge value results with `Select`

MIR should not invent any abstraction that canonical IR cannot explain
immediately after those steps.

---

## 13. Why MIR Should Not Carry Guards

Canonical IR has guards because it is flat and CFG-free.

MIR should not copy that mechanism upward.

MIR already has:

- explicit control structure
- explicit branch regions
- explicit region results

Adding guards to MIR would duplicate a control representation that already
exists. That would make the IR heavier without adding real expressive power.

So the right boundary is:

- MIR owns regions and yields
- canonical IR owns guard insertion and predication

---

## 14. Research Judgment

From a PL and compiler perspective, this MIR shape is a strong fit for Tabula.

It is:

- more principled than statement lists with ad hoc rewriting
- lighter than CFG-SSA
- more compiler-friendly than HIR
- and much less dangerous than inventing another low-level execution IR

The main reason it is attractive is that it respects the actual architecture:

- HIR is source-semantic
- MIR is normalization-semantic
- canonical IR is execution/proof-semantic

That separation is clean both technically and conceptually.

---

## 15. Final Recommendation

The ideal Tabula MIR is:

- exact rewritten V1/V2/V3 core surface
- single-assignment
- ANF
- region-based
- structurally small in raw payload
- explicit in its analysis boundary
- aligned with canonical value semantics
- and closed against the already-implemented lower boundary

That is the right structure to implement.
