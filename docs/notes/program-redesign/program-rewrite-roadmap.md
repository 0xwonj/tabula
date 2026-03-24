# Program Rewrite Roadmap

> **Status**: Proposed implementation roadmap
> **Date**: 2026-03-24
> **Scope**: Defines the implementation roadmap for the Tabula language and IR
> rewrite after the new program DSL, HIR, MIR, and canonical IR architecture
> has been designed.
> **Related**: [program-dsl-and-ir-redesign.md](program-dsl-and-ir-redesign.md),
> [program-dsl-grammar-sketch.md](program-dsl-grammar-sketch.md),
> [program-hir-design.md](program-hir-design.md),
> [program-hir-contract-and-data-model.md](program-hir-contract-and-data-model.md),
> [program-mir-design.md](program-mir-design.md),
> [program-mir-contract-and-data-model.md](program-mir-contract-and-data-model.md),
> [program-canonical-ir-design.md](program-canonical-ir-design.md),
> [program-canonical-ir-contract-and-data-model.md](program-canonical-ir-contract-and-data-model.md),
> [program-typing-and-effect-system.md](program-typing-and-effect-system.md),
> [program-final-seam-decisions.md](program-final-seam-decisions.md),
> [../canonical-vocabulary.md](../canonical-vocabulary.md)

---

## 1. Why This Note Exists

The redesign has reached the point where the large architectural decisions are
mostly in place:

- Tabula is a closed-world `program` language,
- `const` and `relation` are first-class semantic categories,
- the compiler stack becomes `AST -> HIR -> MIR -> canonical IR`,
- and the canonical IR is redesigned as a flat, SSA-disciplined, CFG-free
  execution/proof contract.

What remains is not another round of ontology work. What remains is a rewrite
plan that can actually land this architecture in the repository without losing
execution/proving correctness along the way.

This note records that plan.

---

## 2. Core Delivery Strategy

The rewrite should follow one strong rule:

> **Design all layers up front, but ship language features in stages.**

That means:

- the architecture should already account for V1, V2, and V3,
- but the implementation should not attempt a big-bang launch of the full V3
  language.

Instead, the project should:

1. freeze the new architecture,
2. build the new pipeline in parallel with the old one,
3. land a V1 end-to-end path first,
4. then expand outward to V2 and V3,
5. and only then delete the old stack.

This is the safest path because it preserves:

- runtime continuity,
- proof correctness,
- bisectability,
- and the ability to compare old and new behavior during migration.

### 2.1 Current implementation scope

The rewrite scope is intentionally broad at the frontend and IR boundary.

It includes:

- replacing the current `lang` frontend with a new AST/HIR pipeline,
- introducing a real MIR layer,
- redesigning `tabula-ir` as the new canonical execution/proof contract,
- and adapting executor, journaling, and proof-frontend integration to consume
  that new canonical IR.

It does **not** initially require a full rewrite of every lower runtime
subsystem. The runtime/proof stack should instead act as an anchor while the
frontend, middle-end, and canonical IR are rebuilt around the new architecture.

### 2.2 Immediate implementation goal

The first concrete goal is not V2 or V3 surface richness.

The first goal is:

- a V1 program compiling through AST -> HIR -> MIR -> canonical IR,
- executing through the new canonical IR,
- producing the new semantic journals,
- and feeding the proof frontend for the V1 subset.

That is the first real proof that the redesign is viable.

### 2.3 Ideal end-state structure

The intended long-term structure is:

- parser and source AST as syntax-facing frontend,
- HIR as the source-semantic program model,
- MIR as the normalization and effect-analysis layer,
- canonical IR as the flat execution/proof contract,
- runtime support for context, query, and typed events,
- V2 boundary features on top of the stable V1 core,
- V3 structured control and later spec/sugar on top of the stable lower stack.

---

## 3. Guiding Principles

### 3.1 Parallel replacement, not in-place mutation

The old `lang` / `compiler` / `ir` stack should not be incrementally twisted
into the new one. That would blur the design and make it hard to preserve clear
layer boundaries.

The preferred strategy is:

- build the new pipeline beside the old one,
- route a controlled subset through it,
- cut over once it is correct,
- then delete the old path.

### 3.2 Canonical IR stability matters more than early surface richness

The canonical IR is the execution and proving contract. It should stabilize
early.

That implies:

- V1 should already use the new canonical IR,
- V2 and V3 should ideally expand mostly through HIR/MIR and lowering,
- and the canonical IR should not churn every time a new piece of syntax lands.

### 3.3 Compiler layering is not optional

The rewrite should not recreate:

- `AST -> low IR`

under a new name.

The new system should really have:

- AST,
- HIR,
- MIR,
- canonical IR,

with tests and validation at each boundary.

### 3.4 Runtime and proof are co-equal consumers

The new canonical IR is not just a compiler detail.

It must simultaneously serve:

- deterministic execution,
- execution journaling,
- proof-journal reduction,
- and proof-artifact generation.

So executor/runtime migration must proceed in lockstep with IR redesign.

---

## 4. Finalized Rewrite Contract Before Implementation

The remaining architecture seams have now been closed in
[program-final-seam-decisions.md](program-final-seam-decisions.md).

Implementation should therefore begin from fixed architectural commitments, not
from another round of seam negotiation.

### 4.1 Surface syntax source of truth

This is now mostly resolved:

- [program-dsl-grammar-sketch.md](program-dsl-grammar-sketch.md) is the syntax
  source of truth,
- broader redesign notes are architectural, not token-authoritative.

### 4.2 Hash taxonomy is fixed

The project now commits to:

- a small blessed builtin `Hash` family in canonical IR,
- and `CallCapability` for non-blessed or custom operational kernels.

### 4.3 Context and proof boundary are fixed

The project now commits to:

- public-only initial `context`,
- statement-bound context fields,
- digest-only initial event binding in proof statements,
- and query proving as a future separate mode rather than a default tx-proof
  responsibility.

### 4.4 Guarded operation frontier is fixed

The project now commits to:

- guards only on effectful or checked ops,
- unguarded total pure value ops,
- and typed inactive-default outputs for guarded ops that produce values.

### 4.5 Effect-system freeze

Before data models are finalized, the project should also freeze the core
typing/effect discipline:

- world effects
- proof-observable semantic effects
- failure or checked behavior
- callable-category policy for `fn`, `query`, and `tx`
- capability metadata requirements

Without this, HIR/MIR/canonical IR data models will drift or duplicate policy
in different forms.

---

## 5. Staging Model

There are two distinct orders:

### 5.1 Architecture order

This should be designed all at once:

- program model,
- DSL grammar target,
- HIR,
- MIR,
- canonical IR,
- executor/journal/proof integration seams.

### 5.2 Shipping order

This should remain incremental:

- **V1**: core state-machine language
- **V2**: external boundary surfaces
- **V3**: structured control and spec/sugar layer

This distinction is crucial.

The mistake to avoid is:

- only designing V1 and hoping V2/V3 can be bolted on later.

The architecture should already reserve:

- `context`,
- `query`,
- `event`,
- structured control in MIR,
- and guards in canonical IR,

even if the first implemented subset does not expose all of them.

---

## 6. Phase Roadmap

## 6.1 Phase 0: Freeze the Rewrite Contract

### Goal

Turn the current design notes into an agreed implementation contract.

### Deliverables

- grammar note accepted as syntax authority,
- HIR/MIR/canonical IR notes accepted as layering authority,
- typing/effect note accepted as static-discipline authority,
- canonical vocabulary note accepted as naming authority,
- roadmap accepted as migration authority.

### Exit criteria

- no unresolved ambiguity on root ontology,
- no unresolved ambiguity on `Lookup` versus `Relation`,
- no unresolved ambiguity on the existence of HIR/MIR,
- no unresolved ambiguity on canonical IR being CFG-free,
- no unresolved ambiguity on the four seam decisions,
- and no unresolved ambiguity on the typing/effect model used by HIR, MIR, and
  canonical IR.

---

## 6.2 Phase 1: Freeze Exact Canonical IR and Execution Contract

### Goal

Turn the canonical IR from an architecture note into an exact executable
contract.

### Main work

- define exact canonical IR Rust data structures,
- define exact manifest and schema data structures used by canonical IR,
- freeze op taxonomy and validation invariants,
- define executor semantics for each canonical op family,
- define journal projection rules for each semantic effect family,
- define the initial guard model and inactive-default semantics precisely.

### Scope

Only the V1 canonical subset needs to be modeled exactly.

### Non-goals

- no parser migration yet,
- no full frontend implementation yet,
- no V2/V3 surface enablement yet.

### Exit criteria

- exact canonical IR Rust data model exists,
- validation rules are explicit,
- executor/journal semantics are specified against the new IR rather than the
  old one.

---

## 6.3 Phase 2: Build MIR and Lowering Contract

### Goal

Create the real normalization layer that targets the new canonical IR.

### Main work

- define exact MIR Rust data structures,
- define `EffectSummary` and callable-policy data structures,
- implement MIR validation,
- define MIR -> canonical IR lowering rules,
- define inlining, effect propagation, and capability metadata checks.

### Non-goals

- no parser migration yet,
- no HIR builder yet,
- no V2/V3 feature enablement yet.

### Exit criteria

- exact MIR Rust data model exists,
- effect summaries and callable legality are explicit,
- MIR -> canonical IR lowering contract is written and testable.

---

## 6.4 Phase 3: Build HIR and Frontend Skeleton

### Goal

Create the source-semantic frontend that feeds MIR.

### Main work

- define exact HIR Rust data structures,
- implement HIR validation and body-policy checks,
- define HIR -> MIR lowering rules,
- wire parser output into HIR construction for the V1 subset,
- add boundary tests:
  - parser -> HIR
  - HIR -> MIR
  - MIR -> canonical IR

### Exit criteria

- a small V1 program can parse and lower through all new layers,
- HIR/MIR/canonical IR validation passes on that subset,
- old and new pipelines can coexist.

---

## 6.5 Phase 4: V1 End-to-End Execution and Proving

### Goal

Make the new stack real by executing and proving V1 programs through the new
canonical IR.

### Main work

- adapt executor to consume new canonical IR,
- adapt journaling to the new semantic op families,
- remove `Lookup` from the new path,
- add relation-aware execution and journaling,
- add const-pool support,
- ensure runtime proving can consume the new journals for the V1 subset.

### Exit criteria

- V1 programs execute through the new canonical IR,
- execution journaling reflects:
  - state effects,
  - relation effects,
  - capability effects,
  - event effects where enabled,
- runtime proving can consume the resulting journals for the V1 subset,
- no fallback to the old IR path is needed for V1 examples.

---

## 6.6 Phase 5: V1 Cutover

### Goal

Make the new pipeline the default for the core language.

### Main work

- migrate old core DSL examples/tests to the new surface where needed,
- compare old and new execution behavior on shared cases,
- update toolchain entrypoints to prefer the new pipeline,
- deprecate the old frontend path,
- keep compatibility shims only if they are strictly temporary.

### Exit criteria

- the new path is the default compiler path for the V1 language,
- the old path is no longer needed for core flows,
- regression suites pass against the new canonical IR.

---

## 6.7 Phase 6: V2 Boundary Features

### Goal

Add the external boundary surfaces that the architecture already reserves.

### Main work

- implement `context`,
- implement `query`,
- implement typed `event`,
- implement `emit`,
- optionally implement `requires`,
- define runtime APIs for query execution,
- implement the initial proof policy for:
  - context fields,
  - event digest binding,
  - separate future query-proof mode.

### Why this is a separate phase

These features are not core state-machine semantics.

They are:

- interface surfaces,
- proof-boundary surfaces,
- runtime API surfaces.

That makes them a clean second wave after V1 execution correctness is secured.

### Exit criteria

- new program root objects include meaningful context/query/event content,
- query validation and execution rules are enforced,
- events lower to canonical `EmitEvent`,
- runtime APIs can expose V2 surfaces coherently.

---

## 6.8 Phase 7: V3 Structured Control

### Goal

Add source-level control without compromising canonical proof shape.

### Main work

- enable `if`,
- enable `match`,
- add region-based HIR and MIR lowering for control,
- implement selector synthesis,
- implement guarded-effect lowering,
- implement canonical validation rules for guard usage,
- implement the already chosen initial guarded-op frontier under control,
- optionally add limited bounded `for` later in the same phase or after it.

### Important constraint

This phase must **not** reintroduce CFG into canonical IR.

The intended design remains:

- structured control in HIR/MIR,
- predicated flattening in canonical IR.

### Exit criteria

- `if` and `match` compile end-to-end,
- canonical IR stays flat and CFG-free,
- executor semantics match the intended guarded/predicated model,
- proving remains fixed-shape with no canonical CFG.

---

## 6.9 Phase 8: Spec Layer and Later Sugar

### Goal

Add high-level convenience and specification affordances after the core pipeline
is proven stable.

### Candidate features

- `predicate`
- restricted `invariant`
- optional `ensures`
- relation sugar such as:
  - `x in R`
  - `F(x)` for functional relations
- later bounded `for`

### Why this is last

These features are useful, but none of them should be allowed to destabilize:

- the canonical IR,
- the executor,
- the proof frontend,
- or the core language ontology.

They belong after the main rewrite has already succeeded.

---

## 7. Workstreams by Subsystem

The rewrite naturally splits into parallel workstreams, even if the phases above
remain sequential at the milestone level.

### 7.1 Language frontend

Owns:

- parsing,
- AST,
- HIR construction,
- source diagnostics.

### 7.2 Middle-end

Owns:

- HIR -> MIR,
- normalization,
- inlining,
- effect classification,
- structured-control legality,
- MIR -> canonical IR lowering.

### 7.3 Canonical IR

Owns:

- final Rust data model,
- validation,
- serialization if needed,
- executor-facing semantics.

### 7.4 Runtime and execution

Owns:

- canonical IR execution,
- execution journaling,
- query runtime API,
- context/event runtime semantics.

### 7.5 Proof frontend

Owns:

- journal reduction alignment,
- relation-aware proof inputs,
- capability/relation effect integration,
- proof-artifact preparation compatibility.

---

## 8. Testing Strategy

Each phase should have tests at the layer boundaries, not only end-to-end tests.

### 8.1 Frontend tests

- parser to AST golden tests
- AST to HIR tests
- HIR validation tests

### 8.2 Middle-end tests

- HIR -> MIR lowering tests
- MIR validation tests
- MIR optimization/normalization tests

### 8.3 Canonical IR tests

- MIR -> canonical IR lowering tests
- canonical validation tests
- op-level execution tests

### 8.4 End-to-end tests

- compile + execute
- compile + journal
- compile + prove/verify

### 8.5 Differential migration tests

Where old and new pipelines overlap, compare:

- execution success/failure,
- resulting state effects,
- emitted outputs where relevant,
- proof-visible behavior where relevant.

This is especially valuable in Phases 2 and 3.

---

## 9. Migration Strategy

The recommended migration strategy is:

### 9.1 Build beside the old stack

The new code should initially land beside the old code rather than mutating it
in place.

This may mean:

- new modules,
- new subtrees,
- or temporary `next`/`v2` namespaces.

### 9.2 Switch by capability slice

Do not switch everything at once.

Switch in the following order:

1. V1 straight-line core
2. relation and const canonicalization
3. execution and journaling
4. context/query/event
5. structured control

### 9.3 Delete aggressively after cutover

Once a slice is fully cut over and tested, the old code for that slice should be
deleted rather than preserved indefinitely as a compatibility path.

Long-lived dual stacks are expensive and blur architecture.

---

## 10. Major Risks

### 10.1 Recreating the old shallow pipeline under new names

This is the biggest architectural risk.

If HIR and MIR exist only nominally, the rewrite will fail to pay off.

### 10.2 Letting canonical IR absorb frontend concerns

If syntax pressure causes canonical IR to grow source-like structure, the design
will collapse toward another monolithic IR.

### 10.3 Delaying executor migration too long

If canonical IR is redesigned on paper but execution remains tied to the old IR
for too long, the project will lose confidence in the new stack.

### 10.4 Introducing V3 control too early

`if` / `match` / bounded loops are valuable, but they should not arrive before
the V1/V2 core path is solid.

### 10.5 Carrying stale compatibility paths forever

The rewrite only fully succeeds if the old pipeline is eventually removed.

---

## 11. Recommended Immediate Next Steps

The next concrete sequence should be:

1. adopt the finalized seam decisions as implementation baseline
2. freeze the effect taxonomy and callable policy:
   - world effects,
   - proof-observable effects,
   - `may_fail`,
   - `fn` / `query` / `tx` policy
3. define exact Rust data structures for canonical IR
4. define exact executor and journal semantics for canonical IR
5. define exact Rust data structures for MIR
6. define exact Rust data structures for HIR
7. implement V1 parser -> HIR
8. implement HIR -> MIR for the V1 subset
9. implement MIR -> canonical IR for the V1 subset
10. migrate executor + journal for the new canonical IR

This is the shortest path from architecture to a running rewritten compiler.

---

## 12. What This Note Commits To

This note is intended to settle the following.

- The rewrite is a parallel replacement followed by cutover, not an in-place
  mutation of the old stack.
- The architecture should be designed to the V3 target, even if implementation
  ships incrementally.
- The first executable milestone should be the V1 subset on the new canonical
  IR.
- The current implementation baseline includes the finalized seam decisions for
  hash, context, event/query proof-boundary policy, and guarded operations.
- The static typing/effect discipline should be frozen before data-model churn
  begins.
- Canonical IR stability matters more than early surface richness.
- Runtime and proof integration are part of the rewrite from the beginning, not
  an afterthought.
- The old stack should be removed after successful cutover rather than retained
  indefinitely.

With these commitments in place, the rewrite can proceed as an engineering
program rather than as a loose collection of syntax additions.
