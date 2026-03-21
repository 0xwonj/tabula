# Scheme Artifact Boundary Refactor

> Status: Proposed strategic architecture memo
> Date: 2026-03-20
> Audience: maintainers across runtime, proof, witness, chips, compiler, and platform
> Related:
> - [proof-backend-contract.md](proof-backend-contract.md)
> - [column-scheme-refactor-roadmap.md](column-scheme-refactor-roadmap.md)
> - [final-target-architecture.md](final-target-architecture.md)
> - [../spec/proof-spec.md](../spec/proof-spec.md)

---

## 1. Purpose

This document records the full architectural context around the column-scheme refactor and the remaining proof/runtime boundary work.

It exists to prevent design drift and lost context while the codebase moves from:

- a backend-aware runtime public seam
- a multi-stage, partially duplicated proving pipeline
- a large witness crate with mixed concerns

to:

- a backend-neutral public column-scheme seam
- a single prepared proving session object
- clean ownership across `runtime`, `machine`, `witness`, `chips`, `commitment`, and future semantic-artifact layers

This is not only a patch log.
It is the canonical strategic memo for the next major proof-platform refactor after the recent boundary cleanup work.

---

## 2. Executive Summary

Current state is good, but not yet ideal.

Recent work already achieved:

- `tabula-machine` was pushed toward a pure prepared-trace backend
- `tabula-runtime` became the owner of per-column proof input assembly
- `is_touched` semantics were corrected to mean effective write existence
- `proof_column_commitment()` was moved out of `tabula-witness` into `tabula-commitment`
- `tabula-witness` root public API was reduced to a minimal seam
- `SMT` support was implemented and integrated into the column-scheme platform
- architecture guardrails and proving regressions were strengthened

The largest remaining architectural problem is this:

`tabula-runtime` still exposes STARK backend representation details in its public custom-scheme seam.

In practical terms:

- public runtime abstractions still move `WitnessStore`
- some public proving flow types still encode backend-specific witness details
- custom scheme authors still need to understand internal STARK proof storage shape

That means the runtime surface is not yet truly a product-level proving platform.

The ideal end-state is:

- `public seam = typed scheme artifact`
- `internal seam = WitnessStore`

The highest-value next initiative is therefore:

**remove `WitnessStore` from the runtime public seam and replace it with a backend-neutral scheme artifact boundary.**

---

## 3. What Has Already Been Fixed

The following recent changes matter because they define the starting point for the next refactor.

### 3.1 Proof backend cleanup already landed

The proof stack was tightened so crate roles are clearer:

- `tabula-stark`: STARK protocol math
- `tabula-gadgets`: reusable gadgets
- `tabula-chips`: AIR chip implementations
- `tabula-witness`: witness models and trace assembly infrastructure
- `tabula-machine`: prepared-trace proof backend
- `tabula-runtime`: default prove/verify integration surface

This is captured normatively in [proof-backend-contract.md](proof-backend-contract.md).

### 3.2 `tabula-machine` is much closer to a pure backend

Recent refactors removed witness-level assembly responsibility from `machine`.
`machine` now consumes prepared proof inputs and prepared traces instead of reaching upward into runtime/witness concerns.

This was the correct direction and should not be reversed.

### 3.3 `is_touched` semantics were corrected

The proving path no longer treats reads as writes for touched semantics.
The contract is now:

- `ColumnTransitionInput.is_touched == effective final write exists`

This aligned the runtime proving path with the proof spec and chip semantics.

### 3.4 `tabula-witness` root seam was narrowed

The root public surface now centers on minimal preparation types:

- `BatchInputPreparer`
- `PreparedExecutionInputs`
- `AccessRow`
- `InitRow`

Builtin trace helpers were pushed under `trace::builtin` instead of being broadly re-exported.

### 3.5 `proof_column_commitment()` ownership was corrected

The proof-compatible commitment helper was moved from `tabula-witness` to `tabula-commitment`.

That was important because it is a commitment rule, not a witness policy.

### 3.6 `SMT` is now on the column-scheme platform

The runtime column scheme machinery now supports `SMT` through:

- runtime materialization
- transition backend integration
- chips/state shard support
- tests and proving regressions

This means the remaining work is no longer about proving that multiple schemes can exist.
It is now about making the platform boundary itself correct.

---

## 4. Current Strengths

The current design is already strong in several ways.

### 4.1 Crate ownership is much clearer than before

The current layering is defensible:

- `runtime` owns user-facing prove/verify orchestration
- `machine` owns proof setup and transcript orchestration
- `witness` owns preparation and trace assembly infrastructure
- `chips` own AIR implementations
- `commitment` owns commitment-level helpers

### 4.2 Builtin scheme support is no longer hardcoded into one path

Both `SSMC` and `SMT` now fit into the same column-scheme platform concept.
That is a major improvement over ad hoc builtin-only logic.

### 4.3 The public default path is closer to the intended product story

Most users should use `tabula-runtime`.
That direction is now visible in both code and documentation.

### 4.4 Architecture guardrails exist

The workspace now has explicit dependency checks and regression tests that protect several proof-boundary decisions.

This does not solve everything, but it is an important base for future refactors.

---

## 5. Remaining Architectural Issues

This section records the remaining issues in order of strategic importance, not implementation convenience.

### 5.1 Issue A: `runtime` public proving seam still leaks `WitnessStore`

Severity: highest

Relevant code:

- `crates/runtime/src/columns/transition.rs`
- `crates/runtime/src/proving/prepare.rs`

Problem:

- `ColumnTransitionBackend` is part of the runtime-side custom-scheme seam
- `ColumnProofInput` still carries a `WitnessStore`
- that means the runtime public API still exposes backend-specific proof representation

Why this matters:

- custom scheme authors should not need to understand STARK witness-store internals
- `runtime` is supposed to be the product-facing layer
- the current API is "public" in syntax but "internal" in conceptual cost

Architectural diagnosis:

This is the main remaining contract violation.
It is the most important thing to fix before any deeper platform generalization.

### 5.2 Issue B: proving preparation is still multi-stage and partially duplicated

Severity: very high

Relevant code:

- `crates/runtime/src/proving/prepare.rs`
- `crates/runtime/src/proving/traces.rs`
- `crates/witness/src/trace/builder.rs`
- `crates/witness/src/trace/lowering/orchestration.rs`

Problem:

The current pipeline builds and passes around several intermediate products:

- lowering output
- property reads
- prepared execution inputs
- planned per-column proof inputs
- batch proof input
- shared witness stores

Some of the same conceptual work appears in more than one layer, especially around lowering outputs and property-read handling.

Why this matters:

- more allocations and more memory pressure
- more places to keep in sync during semantic changes
- harder reasoning about source-of-truth ownership
- harder future backend-neutralization

Architectural diagnosis:

This is not merely a performance issue.
It is an ownership and modeling issue.

### 5.3 Issue C: `ColumnSchemeFactory` still couples too many capabilities

Severity: high

Relevant code:

- `crates/runtime/src/columns/factory.rs`
- `crates/runtime/src/columns/views.rs`

Problem:

`build_column()` still materializes all major per-column views in one operation:

- runtime column view
- proof/backend column view
- transition/proving view

Why this matters:

- execution-only flows still conceptually drag proving facets along
- verify-only flows still conceptually depend on prove-time materialization rules
- scheme authors must think about all three concerns at once

Architectural diagnosis:

This is manageable today, but it will become a drag if the custom-scheme surface grows.

### 5.4 Issue D: `tabula-witness` is still a large mixed-concern crate

Severity: medium-high

Relevant code:

- `crates/witness/Cargo.toml`

Problem:

Even with a narrowed root API, the crate still depends on many proof/builtin layers:

- `chips`
- `gadgets`
- `ir`
- `stark`
- `commitment`

Why this matters:

- compile-time cost
- conceptual boundary blur
- minimal preparation consumers still compile far more code than they need

Architectural diagnosis:

The recent cleanup improved the public surface, but not the actual crate boundary.

### 5.5 Issue E: `runtime` still depends directly on compiler-internal semantic types

Severity: medium-high

Relevant code:

- `crates/runtime/src/builder.rs`
- `crates/runtime/src/verifier.rs`

Problem:

`runtime` still consumes `CompiledProgram` directly in some paths while other paths operate on artifact-like forms.

Why this matters:

- runtime and compiler are more tightly coupled than they need to be
- product-facing runtime evolution is tied to compiler-internal representation choices
- it complicates the future target of a stable semantic artifact boundary

Architectural diagnosis:

This is important, but it should follow the proving-boundary refactor, not lead it.

### 5.6 Issue F: dormant or weakly owned semantic helper types still exist

Severity: low

Relevant code:

- `crates/witness/src/witness/program_info.rs`

Problem:

Some types exist in places that no longer look like their eventual home.
They are not currently the main source of architectural pain.

Architectural diagnosis:

These should be cleaned up opportunistically after the major seam work is done.

---

## 6. Dependency and Priority Analysis

This section is the decision framework for sequencing work.

### 6.1 Priority Table

| ID | Issue | Priority | Value | Cost | Risk if delayed | Depends on |
|---|---|---|---|---|---|---|
| A | Remove `WitnessStore` from runtime public seam | P1 | Very high | High | Public platform remains backend-specific | none |
| B | Unify proving preparation into one prepared session object | P1 | Very high | Medium-high | duplicated logic and ownership drift continue | A strongly helps, but partial work can begin before completion |
| C | Split scheme materialization by capability/phase | P2 | High | High | materialization API grows more coupled over time | A should land first |
| D | Split `tabula-witness` into smaller crates | P3 | Medium-high | Medium-high | compile/dependency cost and mixed ownership remain | A and B should land first |
| E | Introduce stable semantic model between compiler and runtime | P3 | High long-term | High | runtime stays tied to compiler internals | A, B, and preferably C first |
| F | Move/remove dormant helpers like `ProgramInfo` | P4 | Low | Low | local confusion only | none |

### 6.2 Why Issue A is first

Issue A is the true boundary defect.

Without fixing it:

- custom schemes are not actually backend-neutral
- runtime is not truly the right product-level abstraction
- future crate splits will be based on the wrong seam

This means Issue A is not just another cleanup task.
It defines whether the next generation of the platform is conceptually correct.

### 6.3 Why Issue B should be paired with Issue A

If `WitnessStore` is removed from the runtime public seam, the pipeline naturally needs a new internal shape.
That is the right time to introduce a single proving session object and remove duplicated intermediate stages.

Doing B without A risks optimizing the wrong abstraction.
Doing A without B leaves too much transitional duplication in place.

They should be designed together even if they land in stages.

### 6.4 Why Issue C should not go first

It is tempting to split the factory API first because the coupling is visible.
That would be the wrong order.

If the proving facet is still backend-specific, then splitting the factory only freezes the wrong prove-side abstraction into more types.

The prove-side contract must be corrected first.

### 6.5 Why Issue D is valuable but not first

Actual crate splitting is worthwhile, but only after the correct seam exists.

Splitting `tabula-witness` too early risks:

- hardening temporary boundaries
- adding migration cost twice
- scattering code before the long-term API is stable

### 6.6 Why Issue E is strategically important but not immediate

A stable semantic artifact layer between compiler and runtime is clearly part of the final architecture.
But it is not the most leveraged next move.

If the proving seam is still wrong, compiler/runtime decoupling will not solve the largest product-facing design defect.

### 6.7 What not to do first

Avoid these sequences:

1. splitting `tabula-witness` before the runtime proving seam is corrected
2. separating `ColumnSchemeFactory` capabilities before the prove facet is backend-neutral
3. starting compiler/runtime decoupling before the proof preparation model is simplified
4. polishing dormant helper ownership before fixing the main proving boundary

---

## 7. Worth-It Analysis

This section answers whether the proposed large refactors are genuinely worth the cost.

### 7.1 If Tabula only supports builtins internally

If the system remains an internal builtin-only platform, the minimum high-value work is:

- Issue A
- Issue B

That already gives:

- cleaner product-level proving API
- better ownership
- lower long-term reasoning cost

In that scenario, Issues C through E are still good ideas, but they become less urgent.

### 7.2 If Tabula intends to support external or semi-external custom schemes

Then the following become mandatory platform work:

- Issue A
- Issue B
- Issue C

Reason:

- scheme authors cannot be asked to understand backend witness storage
- scheme capability boundaries must be explicit
- per-phase ownership must be legible and testable

### 7.3 If Tabula intends to become a multi-surface product platform

Meaning:

- compiler
- runtime
- verifier
- daemon
- web
- external artifact exchange

Then all of the following eventually matter:

- Issue A
- Issue B
- Issue C
- Issue D
- Issue E

Reason:

the more surfaces exist, the more expensive implicit coupling becomes.

### 7.4 Conclusion

The next large refactor is worth doing.

But the most valuable scope is not "everything at once."
The most valuable scope is:

1. backend-neutral proving seam
2. unified proving session model
3. then capability-aware materialization

That is the highest leverage path.

---

## 8. Ideal End-State Architecture

This section defines the architectural target that the next refactors should converge toward.

### 8.1 Layer overview

```text
User/API plane
  tabula-runtime
    - execute / prove / verify
    - program registration
    - scheme registry
    - public typed scheme artifact seam

Stable semantic/artifact plane
  tabula-program (proposed)
    - registered program
    - validated column plan
    - semantic bindings
    - stable runtime-facing program model

  tabula-artifact
    - canonical serialized artifact
    - ids, hashes, compatibility

Proof preparation plane
  tabula-witness-core (proposed split target)
    - access/init rows
    - execution preparation
    - backend-neutral preparation model

  tabula-witness-stark (proposed split target)
    - builtin lowering
    - builtin property extraction
    - builtin shared trace preparation

Proof backend plane
  tabula-machine
    - setup / prove / verify over prepared traces

  tabula-chips
    - AIR chips

  tabula-gadgets
    - reusable gadgets

  tabula-stark
    - protocol math

  tabula-commitment
    - commitment rules and hashing helpers
```

### 8.2 Core seam rule

The critical seam rule is:

- public seam = typed scheme artifact
- internal seam = `WitnessStore`

Implications:

- custom scheme authors return typed artifacts, not `WitnessStore`
- runtime internally adapts typed artifacts into backend witness stores
- `machine` and `chips` continue to consume internal proof representations
- backend representation remains free to evolve without changing the public scheme API

### 8.3 Ownership by crate in the ideal model

#### `tabula-runtime`

Owns:

- public scheme registration
- per-phase orchestration
- public proving session lifecycle
- typed scheme artifact interfaces
- internal adaptation into proof-backend representations

Must not expose:

- `WitnessStore`
- backend-local witness labels
- chip-internal row encoding requirements

#### `tabula-machine`

Owns:

- proof setup
- transcript coordination
- prove/verify over prepared traces and proof inputs

Must not know:

- runtime planning
- compiler semantic models
- public custom-scheme typed artifacts

#### `tabula-witness-core` or equivalent minimal layer

Owns:

- backend-neutral execution preparation outputs
- deterministic row preparation and neutral bookkeeping

Must not own:

- builtin-only lowering policy
- backend-specific witness encoding

#### builtin witness/lowering layer

Owns:

- builtin trace lowering
- builtin property extraction
- builtin shared trace preparation

Must not pretend to be generic preparation infrastructure.

#### `tabula-program` or equivalent semantic layer

Owns:

- stable runtime-facing program representation
- validated column plan
- semantic bindings required by runtime and verifier

Must not be compiler-private if it is needed across runtime/verifier surfaces.

---

## 9. Proposed Replacement Abstractions

This section describes the main new abstractions that should replace the current leaking or duplicated ones.

### 9.1 Replace `ColumnTransitionBackend` with a backend-neutral scheme artifact builder

Current problem:

- the prove-side scheme seam exposes backend representation

Target:

```text
shared column inputs
  -> scheme-owned typed artifact
  -> runtime internal adapter
  -> WitnessStore / backend input
```

The public scheme API should expose:

- neutral inputs
- neutral metadata
- scheme-owned typed artifacts

The runtime internal layer should own:

- conversion from typed artifact to `WitnessStore`
- binding between artifact and `ProofColumn`
- backend-specific witness labeling details

This is the single most important abstraction change.

### 9.2 Introduce a single `PreparedProofBatch` or `ProvingSession`

Current problem:

- too many intermediate structures and duplicated ownership

Target:

One internal object should own:

- lowered builtin/shared inputs
- property reads
- prepared execution inputs
- per-column typed scheme artifacts
- column metadata
- shared witness stores or other backend inputs
- public statement fragments

The principle is:

- compute once
- own once
- adapt many times

This object should become the main internal handoff between runtime preparation and machine proving.

### 9.3 Split scheme materialization by capability or phase

Current problem:

- one factory call builds all three major facets together

Target options:

Option A:

- `RuntimeColumnFactory`
- `ProofColumnFactory`
- `TransitionArtifactFactory`

Option B:

- one registry entry
- multiple explicit capability methods

For example:

- build execution facet
- build proof facet
- build transition/proving facet

Either is acceptable if phase ownership becomes explicit.

### 9.4 Split `tabula-witness` by actual responsibility

Current problem:

- crate root is clean, but actual crate scope is still wide

Target:

- `witness-core`: neutral preparation
- `witness-stark` or `witness-lowering`: STARK-specific lowering and trace preparation

This split should happen only after the proving seam and proving session model are stabilized.

### 9.5 Introduce a stable semantic model between compiler and runtime

Current problem:

- runtime still depends directly on compiler-internal types in some flows

Target:

- compiler emits a stable program model or artifact binding
- runtime consumes that stable program model
- verifier consumes the same family of models

This does not need to happen in the next immediate refactor, but it is part of the final system shape.

---

## 10. Recommended Refactor Sequence

This is the recommended implementation order.

### Phase 1: Backend-neutralize the runtime proving seam

Goals:

- remove `WitnessStore` from the runtime public custom-scheme interface
- replace it with typed scheme artifacts
- keep runtime as the only owner of backend witness adaptation

Why first:

- highest-value boundary correction
- unblocks all later structural cleanup

### Phase 2: Introduce `PreparedProofBatch`

Goals:

- consolidate proving preparation into one internal session object
- remove duplicated lowerings and fragmented ownership
- make property extraction and shared-store prep single-source-of-truth

Why second:

- natural companion to Phase 1
- simplifies later factory and crate work

### Phase 3: Split scheme materialization by phase

Goals:

- decouple runtime/execution facet from prove facet
- make verify-only or execution-only flows conceptually lighter
- make custom scheme capabilities explicit

Why third:

- only worth doing after the prove facet has the right shape

### Phase 4: Split `tabula-witness`

Goals:

- align physical crate boundaries with the new conceptual seam
- reduce dependency and compile-scope drag

Why fourth:

- best done once the new minimal neutral seam has stabilized

### Phase 5: Introduce stable semantic program layer

Goals:

- decouple runtime from compiler-internal semantic types
- create one stable program representation used across runtime and verifier

Why fifth:

- strategically important, but not the highest-leverage next proving change

### Phase 6: Clean up dormant helpers and local ownership drift

Goals:

- move or remove types like `ProgramInfo`
- eliminate leftover transitional abstractions

Why last:

- this work becomes safer and more obvious after the primary architecture is settled

---

## 11. Non-Goals

The next major refactor should not try to solve everything.

Explicit non-goals for the immediate initiative:

- redesigning the STARK protocol kernel
- changing chip-local AIR ownership
- introducing foreign commitment families such as `KZG`, `IPA`, or `Verkle`
- replacing `machine` as the prepared-trace backend
- speculative crate splitting before the public seam is corrected

---

## 12. Success Criteria

The next major initiative is successful only if all of the following are true.

### 12.1 Public API criteria

- runtime custom scheme authors do not need to know `WitnessStore`
- runtime public proving types do not expose backend-local witness storage
- typed scheme artifacts are the public prove-side integration object

### 12.2 Internal architecture criteria

- runtime owns artifact-to-backend adaptation
- machine still consumes prepared proof inputs only
- property extraction and lowering have one clear owner
- proving preparation uses one session object instead of many loosely coupled intermediates

### 12.3 Crate ownership criteria

- `tabula-witness` or its split successors reflect actual responsibilities
- `tabula-runtime` no longer depends directly on compiler-private program semantics in its stable API
- backend crates stay hidden behind runtime product abstractions

### 12.4 Quality criteria

- workspace remains green
- architecture guardrails are updated to defend the new seam
- docs explicitly state the public-vs-internal proof boundary

---

## 13. Open Design Questions

These questions should be answered during design, not deferred until after coding starts.

1. Should the typed scheme artifact be trait-object based, enum-based, or descriptor-keyed plus erased payload?
2. Should the runtime internal adapter from typed artifact to backend input live in `runtime`, `commitment`, or a new proof-preparation internal module?
3. Should `PreparedProofBatch` own shared witness stores directly, or only normalized inputs from which stores are lazily derived?
4. Should phase splitting in scheme materialization use separate traits or one trait with capability-returning methods?
5. When introducing a stable semantic model layer, should it be a new crate or a sealed module inside an existing artifact/program crate first?

These are real design choices, but they should be answered within the architecture defined here, not by reopening the whole boundary question.

---

## 14. Final Recommendation

The next major architecture initiative should be named and treated as a coherent project:

**Scheme Artifact Boundary Refactor**

Its minimum meaningful scope is:

1. remove `WitnessStore` from the runtime public seam
2. introduce a unified `PreparedProofBatch` or `ProvingSession`
3. then split scheme materialization by capability/phase

Everything else should follow that center of gravity.

If this project is executed successfully, then:

- custom scheme support becomes real instead of backend-shaped
- proof preparation becomes simpler and easier to reason about
- future crate splits become safer
- runtime/compiler decoupling becomes easier
- the proof platform gets a much cleaner long-term base

That is the highest-value architectural direction from the current state.
