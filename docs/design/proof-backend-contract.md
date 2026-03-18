# Proof Backend Contract

> Canonical source of truth for proof-stack crate boundaries.
> Related: [proving-layer-architecture.md](proving-layer-architecture.md), [zkvm-library-architecture.md](zkvm-library-architecture.md)

---

## Status

This document is normative for proof-backend crate boundaries.

- Other design docs may explain historical evolution or higher-level product structure.
- They must not redefine crate responsibilities in conflict with this document.

---

## Goals

The proof stack should be split so each crate has one clear reason to exist:

- `tabula-stark` owns STARK protocol math
- `tabula-gadgets` owns reusable constraint/data primitives
- `tabula-chips` owns AIR chip implementations
- `tabula-witness` owns witness models and trace assembly infrastructure
- `tabula-machine` owns proof orchestration over prepared traces
- `tabula-runtime` owns the default user-facing prove/verify integration flow

This contract is purity-first:

- prefer narrower crate knowledge even when it requires short-term internal churn
- keep default users on `runtime`
- keep `machine` backend-oriented
- defer any standalone verifier surface until these boundaries are clean

---

## Responsibility Matrix

| Crate | Owns | Public Role | Must Not Own |
|------|------|-------------|--------------|
| `tabula-stark` | AIR framework, interactions, permutation math, RAP helpers, protocol-facing trace primitives | Foundation crate for proof internals | program semantics, witness assembly policy, runtime proof planning |
| `tabula-gadgets` | reusable gadgets and typed data helpers used by chips | shared component library for proof code | transcript orchestration, proving APIs, second gadget namespace inside `stark` |
| `tabula-chips` | concrete AIR chips and chip-local trace logic | chip implementation crate | witness planning, proof-tier orchestration, runtime-facing APIs |
| `tabula-witness` | witness models, partitioning, trace assembly core, builtin lowering helpers | witness/trace preparation crate | proof transcript orchestration, machine policy, user-facing runtime facade |
| `tabula-machine` | proof setup, tier orchestration, transcript sync, prove/verify over prepared traces | advanced backend API | witness-model imports in public API, runtime proof planning, property-query facade |
| `tabula-runtime` | program materialization, proof planning, proof-input assembly, default prove/verify surface | primary integration surface | low-level STARK math, chip internals beyond construction/configuration |

---

## Dependency Rules

Allowed dependency direction:

```text
tabula-stark
    ^
    |
tabula-gadgets
    ^
    |
tabula-chips
    ^         ^
    |         |
tabula-witness |
       ^       |
       |       |
   tabula-machine
          ^
          |
    tabula-runtime
```

Interpreted rules:

- `tabula-machine` may depend on `tabula-stark`, `tabula-gadgets`, and `tabula-chips`
- `tabula-machine` must not depend on `tabula-witness` or `tabula-runtime`
- `tabula-runtime` may depend on `tabula-machine` and `tabula-witness`
- `tabula-chips` must not depend on `tabula-machine` or `tabula-witness`
- lower crates must remain acyclic

Notes:

- `tabula-witness` may depend on `tabula-chips` because builtin trace assembly needs chip witness types
- this contract does not require adding new crates; tighten boundaries first

---

## Crate Contracts

### `tabula-stark`

Owns:

- AIR abstractions
- bus and interaction types
- permutation trace generation
- RAP prover/verifier folders
- protocol-facing trace primitives needed by macros and core AIR utilities

Must not know:

- `BatchWitness`
- property-query semantics
- column materialization plans
- runtime proving policies

Public expectation:

- foundational, reusable, and not a second gadget library

### `tabula-gadgets`

Owns:

- public gadget namespace for reusable constraint fragments
- integer helpers, zero tests, comparisons, limb utilities

Must not know:

- runtime proof plans
- witness orchestration policy
- transcript sequencing

Public expectation:

- the only public gadget home in the workspace

### `tabula-chips`

Owns:

- concrete AIR chips
- chip-local columns and constraints
- chip-local witness row formats

Must not know:

- tier orchestration
- proof transcript policy
- runtime proving entrypoints

### `tabula-witness`

Owns:

- witness models (`BatchWitness`, per-column witness data)
- generic trace orchestration and partitioning
- builtin lowering helpers under a builtin-specific namespace

Must not know:

- proof transcript sequencing
- machine-level setup policies
- runtime-facing prove/verify surface

Public expectation:

- generic trace APIs stay generic
- builtin-specific lowering stays namespaced as builtin support, not root generic API

### `tabula-machine`

Owns:

- machine setup and proof tiers
- `ProofColumn` as backend metadata/chip construction only
- transcript synchronization across proof instances
- prove/verify over already-prepared traces and inputs

Must not know:

- `BatchWitness`
- `ColumnWitness`
- `PropertyReadRecord`
- runtime column planning or scheme materialization
- property-query facade types as part of its public API

Public expectation:

- backend and advanced usage only
- accepts prepared proof inputs instead of assembling them itself

### `tabula-runtime`

Owns:

- proof-plan resolution
- scheme materialization
- per-column proof-input assembly
- default prove/verify entrypoints

Must not know:

- low-level STARK arithmetic details beyond calling backend APIs

Public expectation:

- obvious default proving and verifying path for most users

---

## Public Integration Surfaces

Default:

- `tabula-runtime` for prove and verify

Advanced:

- `tabula-machine` for backend-oriented proving from prepared proof inputs

Internal implementation layers:

- `tabula-witness`
- `tabula-chips`
- `tabula-gadgets`
- `tabula-stark`

If an API addition would make a default user reach for `tabula-machine` instead of `tabula-runtime`, it requires an explicit design justification.

---

## Non-Goals

This cleanup does not:

- introduce a new crate split as the first step
- add a standalone verifier crate now
- redesign the proof protocol itself
- generalize builtin witness lowering before crate boundaries are clean

---

## Future Work

After this boundary cleanup is complete and stable:

- evaluate a dedicated standalone verifier surface such as `verify-stark`
- version verifier-facing statement/commitment types independently from runtime internals

That work is explicitly deferred. It must not be folded into this cleanup without a new design pass.
