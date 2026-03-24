# Tabula Architecture

> Status: canonical current-state architecture
> Audience: maintainers and AI agents
> Scope: non-adapter workspace architecture

This document describes the current cross-crate architecture of Tabula.

It is intentionally about boundaries, authority, and dependency direction. It
is not a changelog, a roadmap, or a complete API reference.

If this document and exploratory material disagree, prefer this document for
cross-crate structure and the crate-level `README.md` files for crate-local
contracts.

## Architecture In One View

```text
Shared Meaning
  tabula-core
  tabula-contract
  tabula-artifact

Authoring And Registration
  tabula-lang
  tabula-ir
  tabula-compiler

Execution And Runtime Policy
  tabula-executor
  tabula-runtime

Proof Backend
  tabula-commitment
  tabula-witness
  tabula-gadgets
  tabula-chips
  tabula-stark
  tabula-machine

Public Package Surfaces
  tabula-ext
  tabula-sdk

Support
  tabula-testing
```

The architecture is layered on purpose:

- shared meaning sits below semantic registration
- semantic registration sits above execution and proving
- runtime owns policy and wiring above the backend
- proof crates are split by responsibility rather than collapsed into one layer
- package-facing surfaces sit above the core architecture rather than redefining it

## Core Principles

The current architecture relies on these principles:

1. Semantic facts are derived once and carried downstream.
   `tabula-compiler` is the semantic registration authority. Runtime and backend
   layers consume sealed semantics; they do not rediscover them.

2. Runtime owns policy; the backend owns proof mechanics.
   `tabula-runtime` is the default integration boundary. `tabula-machine` is an
   advanced backend API over prepared inputs.

3. Execution stays deterministic and separate from proving.
   `tabula-executor` owns execution mechanics and result formation without
   depending on proof-backend crates.

4. Native commitment meaning and proof-side verification are separate concerns.
   `tabula-commitment` defines native commitment semantics. The proof stack
   mirrors and proves those semantics.

5. Shared foundations stay small and reusable.
   `tabula-core` owns shared vocabulary. `tabula-stark` owns chip-independent
   proving infrastructure. Neither should absorb higher-level policy.

## Layer Boundaries

### Shared Meaning

`tabula-core`, `tabula-contract`, and `tabula-artifact` define the basic things
the rest of the system must agree on:

- core vocabulary and low-level traits
- compatibility and binding policy
- sealed portable models and canonicalization

This layer should describe shared meaning, not runtime behavior or proof policy.

### Authoring And Registration

`tabula-lang`, `tabula-ir`, and `tabula-compiler` move programs from authoring
input to registered semantics:

- `tabula-lang` owns source-facing authoring concerns
- `tabula-ir` owns normalized operational structure
- `tabula-compiler` owns semantic registration and sealed semantic requirements

This layer decides what a program means for the rest of the stack to trust.

### Execution And Runtime Policy

`tabula-executor` and `tabula-runtime` are intentionally separate:

- `tabula-executor` owns deterministic execution
- `tabula-runtime` owns caller-facing integration, runtime policy, statement
  binding, and preparation of backend-ready inputs

Runtime is the bridge from sealed semantics to concrete execution and proving
resources.

### Proof Backend

The proof backend is split by responsibility:

- `tabula-commitment` defines native commitment semantics
- `tabula-witness` prepares proof-oriented logical inputs
- `tabula-gadgets` provides reusable constraint fragments
- `tabula-chips` provides concrete chips
- `tabula-stark` provides chip-independent proving infrastructure
- `tabula-machine` owns typed prepared-input consumption, trace construction,
  backend proof assembly, and verification

This split is more important than any one concrete proof shape. The exact proof
layout may change; the responsibility boundaries should stay legible.

### Public Package Surfaces

Two crates sit above the core architecture rather than inside its lowest
boundaries:

- `tabula-ext` is the official extension authoring surface for custom schemes
  and semantic precompiles
- `tabula-sdk` is the intended application-facing package surface above the
  compiler, artifact, and runtime layers

These crates package and expose architecture seams; they should not become new
semantic or backend authorities.

### Support

`tabula-testing` is shared testing support. It is important to the workspace,
but it is not part of the production execution or proving architecture.

## End-To-End Shape

At a high level, the architecture flows like this:

```text
authoring input
  -> language front-end
  -> IR
  -> semantic registration
  -> sealed program / artifact
  -> runtime policy and execution
  -> prepared proof inputs
  -> proof backend
  -> statement / proof outputs
```

Two distinctions matter more than any exact type names:

- there is a difference between authoring a program and registering what it means
- there is a difference between deciding what should be proved and proving prepared inputs

## Dependency Direction

The architecture depends on these rules:

- `tabula-core` stays near the bottom of the dependency graph
- `tabula-contract` and `tabula-artifact` build on shared meaning, not on runtime or backend crates
- `tabula-lang` and `tabula-ir` stay below compiler policy
- `tabula-compiler` does not depend on runtime or backend proof crates
- `tabula-executor` does not depend on proof-backend crates
- `tabula-runtime` may assemble backend crates, but it remains a consumer of
  sealed semantics rather than a second semantic authority
- `tabula-machine` consumes prepared backend inputs and should stay ignorant of
  compiler policy and runtime registry ownership
- `tabula-runtime` should prepare machine-facing stores and statements, not
  inspect machine setup or trace topology directly
- `tabula-stark` remains chip-independent and should not depend on concrete chip crates

If a change breaks one of those directions, it is probably an architectural
change, not a local refactor.

## Extension Boundaries

Extension support is part of the architecture, but it should remain explicit:

- runtime-level extension policy belongs with `tabula-runtime`
- backend extension composition belongs with `tabula-machine`
- extension authoring contracts belong with `tabula-ext`

The extension story should make seams clearer, not blur responsibility between
semantic registration, runtime policy, and backend proving.

## How To Use This Document

Use this document when you need to answer questions like:

- which layer should own a new cross-cutting rule
- whether a dependency direction is architecturally acceptable
- whether a change belongs in compiler, runtime, or machine
- whether a document in `notes/`, `research/`, or `archive/` should be treated
  as current architecture guidance

Do not use this document as the place to record temporary plans, API inventories,
or detailed implementation recipes. Those belong elsewhere.
