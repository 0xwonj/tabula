# Proof Front-End Journal Architecture

> **Status**: Implemented architecture note
> **Date**: 2026-03-24
> **Scope**: Defines the ideal post-migration proof front-end architecture for
> runtime-owned proving preparation.
> **Related**: [profile-native-runtime-migration-plan.md](profile-native-runtime-migration-plan.md),
> [verification vocabulary](../design/architecture.md#verification-vocabulary),
> [executor-proof-codesign-architecture.md](executor-proof-codesign-architecture.md),
> [execution-proof-redesign-workplan.md](execution-proof-redesign-workplan.md),
> [witness-partition-and-batch-proof-plan-architecture.md](witness-partition-and-batch-proof-plan-architecture.md),
> [proof-hierarchy-and-grouping.md](proof-hierarchy-and-grouping.md),
> [../research/symbolic-air-compilation.md](../research/symbolic-air-compilation.md)

---

## 1. Why This Note Exists

The profile-native migration removed the legacy carrier model and finished the
typed runtime and capability transcript contracts. The main proof-front-end limitation that
remains is structural rather than semantic:

- execution produces one general-purpose `BatchReport`,
- runtime proving reinterprets that result through several independent passes,
- witness still owns some whole-batch orchestration concerns,
- and parallelism starts late, after several serial extraction and grouping
  steps have already happened.

This note defines the ideal replacement:

> **Tabula should treat execution output as a canonical typed effect journal,
> then derive all proof inputs through deterministic plan-indexed reduction.**

This is not a new carrier migration. It is a proof-front-end architecture
migration.

---

## 2. Design Claim

The intended model is:

1. execution records typed semantic effects,
2. runtime proving owns the proof plan and reduction rules,
3. witness owns only materialization kernels,
4. all proof-facing inputs are derived from one canonical journal,
5. all final proof-facing collections are aligned to proof-plan order.

The key distinction is:

- `BatchReport` is a reporting and boundary view,
- `ExecutionJournal` is the internal semantic truth for proving,
- `ProofJournal` is the canonical runtime proof input,
- `ProofArtifacts` is the backend-prepared machine-facing bundle.

The proof system should not need to rediscover meaning by rescanning or
repartitioning a general execution result.

---

## 3. Architectural Pattern

The pattern is best described as:

- **typed effect sourcing**
- **deterministic map/reduce**
- **plan-indexed columnar proof compilation**

### 3.1 Typed effect sourcing

Execution should emit typed primary effects rather than only portable boundary
projections.

The primary effects are facts such as:

- access effects,
- property-read effects,
- capability-call effects,
- IR-hash effects,
- future relation or lookup effects,
- emitted application events.

These are the canonical internal truth. Any global projections such as
`read_set_old` or `write_set_final` are derived views, not the semantic source
of truth for proving. In the final executor model, those projections live under
a nested `ExecutionStateSummary` inside `ExecutionJournal` rather than beside
the tx-local effect shards as if they were peer primary effects.

Failed transaction partial accesses are not part of the canonical proving
timeline. They remain available for reporting and debugging, but they should be
typed and reduced as diagnostic observations rather than as proof-facing access
effects.

### 3.2 Deterministic map/reduce

Proof front-end preparation should be structured as:

1. sequential transaction execution,
2. tx-local immutable shard creation,
3. parallel tx-local proof projection,
4. deterministic global reduction,
5. parallel backend preparation by proof slot.

Parallelism is therefore a property of the data model, not a later optimization
added to serial passes.

### 3.3 Plan-indexed columnar proof compilation

The final proof input shape should not be a set of maps keyed by `(table, col)`
or by capability transcript id.

Instead, runtime proving should first materialize the proof plan and then reduce
all execution effects into vectors aligned with that plan:

- `columns[i]` corresponds exactly to column proof slot `i`,
- `capabilities[i]` corresponds exactly to capability transcript proof slot `i`,
- future relation slots should follow the same rule.

This gives stronger determinism, simpler downstream code, and fewer accidental
ordering bugs.

---

## 4. Ownership Model

### 4.1 Executor owns semantic execution journaling

`tabula-executor` should own the canonical internal execution journal.

Its responsibility is to record what happened, not to know how proving will
group or materialize those effects.

The ideal canonical output of execution is an `ExecutionJournal`, not a
proof-specific structure.

### 4.2 Runtime owns proof planning and reduction

`tabula-runtime` should own:

- proof-plan materialization,
- tx-shard projection into proof-facing shards,
- deterministic reduction into slot-aligned prepared inputs,
- orchestration of per-slot backend preparation.

Runtime is the right owner because it is the first layer that knows both:

- the executed program plus host-installed runtime registries and schemes,
- and the exact proof backends and proof grouping plan.

### 4.3 Witness owns materialization kernels only

`tabula-witness` should not own whole-batch orchestration in the final model.

Witness should own only narrow kernels such as:

- lowering one successful tx,
- materializing one column witness,
- materializing one capability witness,
- materializing one IR-hash witness,
- future relation witness kernels.

In other words:

- runtime owns **what must be proved**,
- witness owns **how that proof input becomes trace or witness artifacts**.

Runtime proving should therefore lower directly from typed success-path journal
facts into witness kernels. It should not round-trip through portable
reporting carriers before lowering.

The runtime also owns fail-closed validation of the program state surface.
Execution and proving both reject state cells outside the declared program
surface, and proving additionally rejects any normalized pre-state that does
not exactly match `executed.state_before`.

---

## 5. Canonical Dataflow

The intended dataflow is:

```text
Sequential execution
-> ExecutionJournal
-> parallel tx-local proof projection
-> deterministic reduction by proof plan
-> ProofJournal
-> parallel per-slot backend preparation
-> witness stores / traces / machine handoff
```

### 5.1 `ExecutionJournal`

The execution journal is the canonical internal output of execution.

It should contain:

- tx-local semantic effect shards,
- global state projections needed by proving,
- enough identity and ordering information to support deterministic reduction.

More specifically:

- successful tx shards carry canonical semantic effects,
- failed tx shards carry diagnostic observations only,
- derived batch-level state projections live in nested summary form rather than
  as top-level primary effect families.

It is not a portable artifact and not a verifier-visible statement. It is an
internal runtime object.

Failed diagnostics never enter proof reduction.

### 5.2 `ProofJournal`

The prepared batch journal is the canonical runtime-owned proof input.

It should contain:

- lowering output,
- prepared column inputs aligned to column proof slots,
- prepared capability calls aligned to capability transcript proof slots,
- capability transcript calls,
- future relation inputs aligned to relation proof slots.

This is the final reduction boundary before backend-specific preparation.

### 5.3 `ProofArtifacts`

The prepared proof artifacts bundle is the machine-facing output of backend
preparation.

It contains:

- AIR public statement data,
- execution-tier prepared witness input,
- ordered per-column prepared inputs,
- root-tier prepared witness input.

For the built-in path, runtime reaches this bundle by combining:

- `ProofJournal`,
- `BatchProofPlan`,
- per-slot backend preparation,
- and the configured root backend bundle.

---

## 6. Effect Model

The effect model is the semantic foundation of this architecture.

An effect is a typed fact produced by execution that is later consumed by
proving.

The effect model should follow these rules:

1. Effects are primary truth; projections are derived.
2. Effects carry stable identity and ordering information.
3. Effects are immutable after tx completion.
4. Effects are typed before proof projection.
5. Effects are grouped by semantic family, not by backend accident.

In practice, the core effect families are:

- state access,
- property read,
- capability call,
- IR hash,
- emitted event,
- future relation or lookup effect.

This matters because proof reducers should work over semantic families rather
than over whatever fields happened to exist in a generic execution result type.

Projection work on the proof hot path should also be single-pass wherever
possible. Access events and capability transcript materialization should be
projected once and then reused by both lowering and slot-aligned reduction.

---

## 7. Determinism Rules

Determinism is a first-class design constraint.

The architecture should make the ordering contract explicit:

- tx merge order is ascending `tx_index`,
- instruction-local order is source instruction order,
- property-read order is stable within tx and then stable across txs,
- capability-call order is stable within tx and then stable across txs,
- per-column access-event order is derived from stable execution order,
- future multiplicity reductions must use explicit reduction keys.

The implementation must not rely on:

- Rayon collect order as an implicit contract,
- `BTreeMap` iteration as a substitute for a proof-plan order contract,
- post-hoc incidental ordering recovered from debug output or test expectations.

Global order is defined by explicit merge rules, not by whichever container
happened to be used.

---

## 8. Parallelism Model

The goal is not "parallelize everything." The goal is to use coarse-grained
parallelism where semantics allow it and to avoid oversubscription.

### 8.1 What stays sequential

Transaction execution remains sequential in the current model because:

- tx order is ordered,
- overlay state transitions are ordered,
- transaction failure and rollback semantics are ordered.

This note does not propose speculative or parallel transaction execution.

### 8.2 What becomes parallel

The ideal top-level parallel stages are:

1. tx-local proof projection,
2. per-slot column proof preparation,
3. per-slot capability transcript proof preparation,
4. merge into `ProofArtifacts`,
5. future per-slot relation proof preparation.

### 8.3 What should not be parallelized

The architecture should avoid:

- nested Rayon inside already parallel slot preparation,
- reducer micro-parallelism that adds coordination cost without reducing wall
  time,
- implicit parallel ordering contracts.

The intent is to maximize useful coarse-grained work while keeping the mental
model simple and deterministic.

---

## 9. Why This Is Not Over-Engineering

This architecture is justified because it removes real structural problems:

- repeated scans of execution output,
- repeated portable-to-typed decoding,
- separate grouping passes for column, property, and capability data,
- witness owning orchestration concerns that belong to runtime,
- late and partial use of parallelism.

It is not a framework proposal.

The architecture should remain concrete:

- concrete structs,
- concrete reducers,
- concrete plans,
- concrete slot-aligned vectors.

It should not introduce:

- generic reducer registries,
- event-bus abstractions,
- plugin frameworks,
- excessive trait hierarchies for internal dataflow.

The correct level of abstraction is a canonical journal boundary, not a generic
data-processing framework.

---

## 10. Naming Rules

The naming model should communicate role clearly.

- `...Effect`
  - primary semantic fact produced by execution
- `...Shard`
  - tx-local immutable unit
- `...Journal`
  - canonical stage boundary
- `...Plan`
  - resolved proof-slot contract
- `Prepared...`
  - reduced proof-ready runtime input
- `project_...`
  - pure local transform
- `reduce_...`
  - deterministic aggregation
- `materialize_...`
  - witness or trace generation
- `derive_...`
  - non-canonical projection or reporting view

These names should be used consistently so that code review can tell at a
glance whether a type is:

- semantic execution truth,
- proof planning metadata,
- proof-ready runtime input,
- or backend witness materialization.

---

## 11. Future Compatibility with Static Relations

This note does not redesign lookup or static relations.

However, the journal architecture should be designed so that a future relation
channel fits naturally into it.

The intended property is:

- relation or lookup evidence is just another execution effect family,
- relation proof slots are just another proof-plan axis,
- relation witness materialization is just another backend kernel family.

This means the proof front-end topology should not need to be redesigned when a
future sealed static-relation contract is introduced.

---

## 12. Relationship to Symbolic AIR Compilation

This architecture is compatible with future symbolic AIR compilation.

The reason is that the journal boundary already separates:

- execution semantics,
- proof planning,
- witness materialization.

That same separation is useful whether the proving backend uses:

- the current generic execution AIR,
- grouped logical column proofs,
- or future compiled symbolic AIR paths.

The journal model should therefore be treated as a foundational runtime/proving
boundary, not as a temporary optimization around the current backend.

---

## 13. Definition of Success

This architecture is achieved when all of the following are true:

- runtime proving consumes one canonical execution journal rather than
  rescanning `BatchReport`,
- proof front-end preparation is structured as tx-local projection plus
  deterministic reduction,
- final proof inputs are aligned to proof-plan order rather than grouped around
  ad hoc maps,
- witness owns materialization kernels but not whole-batch orchestration,
- coarse-grained parallelism begins before backend preparation,
- ordering and determinism are explicit contracts,
- future relation or lookup integration can be added without changing the
  top-level proof front-end topology.

That is the target end state for post-migration proof-front-end architecture.
