# Tabula Workspace Architecture

> **Status**: Current workspace map
> **Date**: 2026-03-21
> **Scope**: Crate boundaries, dependency rules, and end-to-end flow in the
> current repository

---

## 1. System Model

Tabula is a proving-oriented state-transition system for typed tabular
state.

Three layers are intentionally separated:

1. **Compilation**: parse source or load JSON, validate IR invariants,
   and register a canonical program artifact.
2. **Execution**: run a batch deterministically against a state snapshot,
   producing execution results and state diffs.
3. **Proof**: compute native commitment products, prepare witness data,
   assemble proof instances, and prove or verify the resulting claim.

The key architectural property is that deterministic execution remains
usable without the proving stack.

---

## 2. End-To-End Flow

```text
source / JSON
  -> tabula-lang + tabula-ir
  -> tabula-compiler
  -> tabula-runtime (execute)
  -> tabula-executor
  -> ExecutionEnvelope / state diff / events
  -> tabula-runtime --features verify|prove
  -> tabula-commitment + tabula-witness + tabula-chips + tabula-stark + tabula-machine
  -> proof / receipt / verification result
```

In practice:

- `tabula-compiler` owns program loading, lowering, registration, and
  compatibility checks.
- `tabula-runtime` is the integration boundary used by CLI and daemon.
- `tabula-executor` owns deterministic state transition semantics.
- `tabula-commitment` computes native hashes, paths, and roots.
- `tabula-witness` prepares proof-facing data from runtime outputs.
- `tabula-chips`, `tabula-stark`, and `tabula-machine` own AIR, protocol,
  and proof assembly.

---

## 3. Workspace Layout

### 3.1 Semantic Surface

| Crate | Responsibility |
|-------|----------------|
| `tabula-core` | shared types, traits, errors, execution outputs |
| `tabula-contract` | compatibility contracts and statement/profile checks |
| `tabula-artifact` | canonical artifact, batch, and state data models |

These crates define the data model that higher layers share.

### 3.2 Authoring And Compilation

| Crate | Responsibility |
|-------|----------------|
| `tabula-ir` | IR types and validation |
| `tabula-lang` | `.tab` parsing and lowering |
| `tabula-compiler` | load/register/validate programs across source and JSON inputs |

This layer turns human-authored or serialized inputs into registered
program artifacts.

### 3.3 Deterministic Execution

| Crate | Responsibility |
|-------|----------------|
| `tabula-executor` | interpreter and deterministic execution engine |
| `tabula-runtime` | compile/execute/verify/prove integration surface |

`tabula-runtime` is feature-gated:

- default: compile + execute only
- `verify`: verification stack
- `prove`: witness generation + proof construction

This keeps the fast path lightweight while allowing the full proof stack
to remain available behind explicit features.

### 3.4 Native Commitment And Proof Stack

| Crate | Responsibility |
|-------|----------------|
| `tabula-commitment` | native Poseidon/SMT/SSMC and root-binding logic |
| `tabula-witness` | witness preparation and proof-facing lowering |
| `tabula-gadgets` | reusable AIR gadgets |
| `tabula-chips` | AIR chips and trace builders |
| `tabula-stark` | shared STARK protocol and trace infrastructure |
| `tabula-machine` | proof assembly, setup, proving, verification |

This is the current proof stack split. Older documents that describe a
single `tabula-proof` crate are historical.

### 3.5 Adapters And Test Support

| Crate | Responsibility |
|-------|----------------|
| `tabula-testing` | shared fixtures, assertions, and integration helpers |
| `tabula-cli` | local CLI for check/compile/execute/inspect/example |
| `tabula-daemon` | local HTTP control plane |
| `tabula-web` | browser IDE |

Adapters should depend on `tabula-runtime` or other stable boundaries,
not re-implement domain flows on their own.

---

## 4. Dependency Rules

The current repository follows these rules:

1. `tabula-core` sits at the bottom of the stack.
2. `tabula-executor` must stay free of proof-specific crate dependencies.
3. `tabula-compiler` owns program registration and compatibility entry
   points; adapters should not duplicate that logic.
4. `tabula-runtime` is the integration boundary for execute/verify/prove
   workflows.
5. `tabula-commitment` computes native commitment products; it does not
   own AIR or proof orchestration.
6. `tabula-witness` prepares proof inputs; `tabula-chips` defines AIR;
   `tabula-stark` provides shared protocol machinery; `tabula-machine`
   assembles concrete proofs.
7. `tabula-cli`, `tabula-daemon`, and `tabula-web` sit above these layers
   as adapters.

A useful mental model is:

```text
core -> {contract, artifact, ir}
ir -> lang -> compiler
{artifact, compiler, executor} -> runtime
commitment -> {witness, chips, stark, machine}
runtime -> optional proof stack via features
{runtime, compiler} -> {cli, daemon, web}
```

---

## 5. Current Entry Docs

Use these as the primary reading order:

1. [`../../README.md`](../../README.md)
2. [`../README.md`](../README.md)
3. [`master-roadmap.md`](master-roadmap.md)
4. [`testing-architecture.md`](testing-architecture.md)
5. [`../spec/semantics-spec.md`](../spec/semantics-spec.md)

`proof-spec.md` is still valuable, but it predates parts of the current
KoalaBear and sharded architecture. Treat it as a draft that still needs
refresh work.

---

## 6. Archived Material

The following documents were moved out of the active reading path because
they describe superseded plans or removed crate boundaries:

- [`../archive/final-target-architecture.md`](../archive/final-target-architecture.md)
- [`../archive/orchestrator-state-machine-blueprint.md`](../archive/orchestrator-state-machine-blueprint.md)
- [`../archive/runtime-implementation-gate.md`](../archive/runtime-implementation-gate.md)
- [`../archive/state-machine-centric-runtime-architecture.md`](../archive/state-machine-centric-runtime-architecture.md)
- [`../archive/showcase-ide-design.md`](../archive/showcase-ide-design.md)
- [`../archive/project-complete-guide.md`](../archive/project-complete-guide.md)

If a document mentions `tabula-proof`, `tabula-orchestrator`, `driver`,
or an unrealized multi-IR tower as if it already existed in the current
workspace, treat it as historical unless explicitly refreshed.
