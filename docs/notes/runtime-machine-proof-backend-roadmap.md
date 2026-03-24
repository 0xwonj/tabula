# Runtime-Machine Proof Backend Roadmap

> **Status**: Roadmap note, with Stages 1-3 implemented
> **Date**: 2026-03-24
> **Scope**: Records the current big-picture roadmap for runtime-owned proof
> preparation, machine boundary cleanup, witness partition planning, and the
> next backend architecture steps.
> **Related**: [../design/architecture.md](../design/architecture.md),
> [execution-proof-redesign-workplan.md](execution-proof-redesign-workplan.md),
> [proof-front-end-journal-architecture.md](proof-front-end-journal-architecture.md),
> [witness-partition-and-batch-proof-plan-architecture.md](witness-partition-and-batch-proof-plan-architecture.md),
> [proof-hierarchy-and-grouping.md](proof-hierarchy-and-grouping.md)

---

## 1. Purpose

This note is the current big-picture roadmap for the proof-side architecture
work around:

- `tabula-runtime`
- `tabula-machine`
- `tabula-witness`
- `tabula-stark`
- `tabula-ext`

It exists to answer four questions clearly:

1. what is already done,
2. what still needs to be corrected immediately,
3. what the next architectural stages are,
4. what is optional future work rather than current priority.

This is a sequencing document, not a canonical architecture document. For
cross-crate ownership and dependency direction, prefer
[../design/architecture.md](../design/architecture.md).

---

## 2. Current State

The codebase is in a materially better state than before the recent machine
cleanup, and the first two roadmap stages are now landed.

### 2.1 What is already in good shape

The following direction is now largely established:

- `tabula-runtime` owns semantic execution/proving policy and machine-facing
  preparation
- `tabula-machine` exposes a narrower prepared-input proving boundary
- runtime no longer inspects machine setup internals directly
- machine input, proof model, and setup internals are much more clearly named
- preprocessed trace metadata is no longer carried through a clone-backed
  adapter hack
- runtime now owns batch-local `BatchProofPlan`
- machine input is already tier-partitioned (`execution` / `columns` / `root`)
- machine no longer performs label-based execution/root repartitioning

In other words, the broad boundary reset succeeded.

### 2.2 What remains unresolved

The remaining issues are no longer "clean up random leftovers." They now fall
into a few coherent architecture buckets:

1. extension seam cleanup
2. canonical contract/kernel consolidation
3. optional future topology generalization

That is good news. It means the remaining work can be staged deliberately
rather than as another wide cleanup pass.

---

## 3. Guiding Principles

Every stage below should preserve the same architectural rules.

### 3.1 Runtime owns proof policy

Runtime should decide:

- what must be proved,
- how one batch is reduced,
- how prepared machine payloads are assembled,
- what backend grouping or packaging policy is used.

Machine should not silently recover runtime policy by inspecting labels or
topology internals.

### 3.2 Machine owns proof mechanics

Machine should own:

- consuming prepared backend inputs,
- building traces from those inputs,
- proving and verifying,
- enforcing proof-shape correctness.

Machine should not own batch-local routing policy.

### 3.3 Witness owns kernels, not orchestration

Witness should expose:

- narrow lowering helpers,
- reusable witness/store builders,
- backend-oriented materialization kernels.

Witness should not become a second runtime orchestration layer.

### 3.4 Extension authoring should stay above runtime and machine

Stable authoring seams belong in `tabula-ext`, not as ad hoc wrappers spread
through runtime and machine.

### 3.5 Do not generalize topology before the planning layer is correct

Grouped proofs, sharding, or non-`C+2` layouts only make sense after the code
has a clean runtime-owned batch-local planning layer.

---

## 4. The Roadmap In One View

The recommended roadmap is:

1. harden correctness and authority boundaries,
2. introduce runtime-owned `BatchProofPlan`,
3. reshape machine input to already-partitioned tier inputs,
4. bundle root proof and root witness authority,
5. move extension authoring seams upward into `tabula-ext`,
6. optionally generalize proof topology when the product roadmap needs it.

The key structural turning points are Stages 2 and 3.

Those stages are now complete. The next architectural work should build on the
runtime-owned planning layer and the landed root bundle rather than reopening
witness partitioning or contract-tag authority.

---

## 5. Stage 1: Correctness And Authority Hardening

### Goal

Fix the remaining correctness-sensitive gaps before the larger planning work.

### Why this comes first

The next architectural work will move boundaries and types. That is easier and
safer if the current proof validation and authority boundaries are already
sound.

### Required work

#### 5.1 Restore `num_public_values` validation

Machine verification metadata should again carry the public-value arity needed
to validate proof shape. Proving and verification should reject mismatched
public-value lengths structurally.

This is protocol hardening, not a cosmetic cleanup.

#### 5.2 Make root witness authority explicit

The current code still has an implicit mismatch:

- custom proof-side root behavior is configurable,
- root witness preparation is effectively SMT-shaped,
- machine still partitions root inputs using SMT-specific labels.

Before larger plan work begins, this authority mismatch should be made
explicitly representable in code.

The first step does not need to solve the whole root bundle story yet, but it
should stop pretending root witness routing is backend-agnostic when it is not.

Stage 1 intentionally left the codebase in this explicit temporary state so
that Stage 2 and Stage 3 could remove it cleanly rather than implicitly.

### Exit gate

- public-value arity is validated structurally
- root witness authority is no longer implicit or misleading
- no correctness-sensitive gap is left for later refactors to paper over

---

## 6. Stage 2: Runtime-Owned Batch Proof Planning

### Goal

Introduce the missing runtime-owned batch-local planning layer:

- `BatchProofPlan`

This is the stage that resolves the witness partition problem correctly.

### Current status

Implemented.

The codebase now has:

- runtime-internal `BatchProofPlan`
- split `ExecutionStoreBuilder` / `SmtRootStoreBuilder`
- tier-partitioned `PreparedMachineInput`
- runtime-owned artifact preparation for execution/root/columns
- no machine-side label repartitioning

### Why this is the central stage

Right now the code already has:

- static runtime `ProofPlan`
- batch-local semantic `ProofJournal`
- machine-facing `PreparedMachineInput`

But it is still missing the batch-local backend-aware planning layer between
the last two.

That missing layer is the real reason machine-side partition logic still
exists.

### Required work

#### 6.1 Add runtime-internal `BatchProofPlan`

This should be an internal runtime planning object, not a new public SDK noun.

It should own:

- tier routing,
- proof-unit planning,
- current `C+2` packaging decisions,
- future insertion points for grouped proof units.

#### 6.2 Split shared execution/root store construction

The former shared-store path has been decomposed into narrower kernels so
runtime can prepare execution-tier and root-tier payloads independently.

#### 6.3 Reshape `PreparedMachineInput`

The machine payload now carries already-partitioned tier inputs:

- execution tier input,
- ordered column inputs,
- root tier input.

#### 6.4 Remove machine-side label partitioning

Machine no longer drains labels to rediscover tier routing.

### Exit gate

- `BatchProofPlan` exists in runtime proving
- `PreparedMachineInput` is tier-partitioned
- machine does not use label-based partition recovery
- witness partitioning is runtime-owned policy rather than machine-owned
  fallback behavior

---

## 7. Stage 3: Root Backend Bundle

### Goal

Unify root proof mechanics and root witness preparation under one authority
boundary.

### Current status

Implemented.

### Why this is a separate stage

Stage 2 can establish the planning layer without fully redesigning root
backend authoring. That is useful because it keeps the central witness
partition work focused.

After Stage 2, the next architectural cleanup was to make the root path as
coherent as the column path. Stage 3 landed that cleanup by introducing an
ext-owned root backend family object, wrapped by a bundle, in `tabula-ext`.

### Landed state

The landed end state is a root backend family abstraction with two
responsibilities behind one prove-path authority:

- proof-side root backend behavior
- runtime-side root witness preparation

This ensures that configurable root proving is matched by configurable root
witness preparation, while verifier-only configuration remains proof-side only.

### Exit gate

- root backend configuration is not only proof-side
- runtime no longer relies on backend-specific root witness conventions hidden
  outside the root authority boundary
- `RootWitnessContract` is gone from the active architecture

---

## 8. Stage 4: Extension Seam Cleanup

### Goal

Move extension authoring seams toward `tabula-ext` so runtime and machine stay
consumers rather than accidental authoring authorities.

### Current status

Implemented for execution-tier authoring.

### Why this follows the root bundle work

The extension story is easier to clean up once the runtime/machine proof
planning and root authority boundaries are already explicit.

Otherwise the extension cleanup tends to encode current incidental boundaries
instead of the intended architecture.

### Required work

The main direction is:

- runtime and machine consume resolved backend bundles or descriptors
- stable authoring traits and bundle shapes live in `tabula-ext`

This especially matters for:

- execution-tier extensions
- future root backend authoring
- verifier-facing extension contracts

### Landed state

- `tabula-ext::backend::execution::ExecutionBackend` is the stable advanced
  execution-tier authoring seam
- built-in execution backends such as IR hash and precompile transcript live in
  `tabula-ext`, not runtime
- `PrecompileProofSystem` is also an `ExecutionBackend`
- runtime and verifier attach ext-owned execution backends through one internal
  machine bridge instead of hand-written per-backend wrappers

### Exit gate

- runtime stops hand-rolling extension-authoring wrappers around backend traits
- execution authoring contracts are visibly centered in `tabula-ext`

---

## 9. Stage 5: Contract Consolidation And Kernel Cleanup

### Goal

Tighten the remaining internal contracts after the major boundary changes land.

### Why this is not first

This stage is valuable, but it should follow the planning-layer and root-bundle
work rather than lead it. Otherwise the code risks consolidating the wrong
intermediate shapes.

### Likely work items

#### 9.1 Make `PreparedMachineInput` the true internal center of gravity

After Stage 2, runtime, machine, and tests should consistently treat
`PreparedMachineInput` as the machine handoff contract, while intermediate
runtime objects remain clearly runtime-local.

#### 9.2 Clean up metadata source-of-truth duplication

Any residual duplicated capability descriptions, especially around
preprocessed-trace behavior, should be tightened once the main boundary changes
stop moving.

#### 9.3 Keep witness as reusable kernels

The witness crate should expose the smallest useful kernel layer rather than a
mix of reusable builders and whole-batch proving assumptions.

### Landed state

- `PreparedMachineInput` is now the real machine handoff center inside the
  runtime as well as at the public machine boundary
- runtime proving converges on one private `PreparedProofRequest` carrying the
  final `Statement` plus `PreparedMachineInput`
- `ProofArtifacts` remains a proving-internal carrier and now uses
  `PreparedTierInput` / `PreparedColumnInput` directly
- `RuntimeProofConfig` was removed; runtime proving holds one
  `RootBackendBundle` authority directly
- `BaseAir` is the single authority for public-value arity and
  preprocessed-next-row behavior; `ChipSpec` is narrowed to backend mechanics
- `tabula-witness::stark` now exposes function-shaped kernels
  (`prepare_execution_store`, `prepare_smt_root_store`) instead of zero-config
  builder wrappers

### Exit gate

- the new boundary is not only correct but internally tidy
- the main contracts are easy to name and explain without caveats

---

## 10. Stage 6: Optional Proof Topology Generalization

> See also:
> [stage6-proof-topology-generalization-deferred.md](stage6-proof-topology-generalization-deferred.md)

### Goal

Generalize from the current `C+2` proof layout only if real product or research
needs justify it.

### Why this is optional

The current topology is not the main architectural problem anymore.

The code first needs:

- a correct planning layer,
- explicit root authority,
- a stable extension surface.

Only then does it make sense to generalize proof topology for things like:

- grouped column proofs,
- extra root stages,
- sharding-aware proof packaging,
- future recursive aggregation layouts.

### Exit gate

- there is an actual roadmap need beyond `C+2`
- the generalized topology reduces future churn rather than creating premature abstraction

### Current decision

Stage 6 is intentionally deferred.

Earlier stages fixed the real architectural problems:

- planning ownership,
- root authority,
- extension seams,
- and contract/kernel cleanup.

That means Stage 6 should start only from measured need, not from a desire to
make topology code more generic.

---

## 11. Dependency Graph

The stages are not fully independent.

The intended dependency order is:

```text
Stage 1
  -> Stage 2
  -> Stage 3
  -> Stage 4
  -> Stage 5
  -> Stage 6 (optional)
```

More specifically:

- Stage 1 should happen before Stage 2
- Stage 2 should happen before Stage 3
- Stage 3 should happen before or alongside Stage 4
- Stage 5 should follow Stages 2 and 3
- Stage 6 should wait until the earlier stages are stable

The only stage that is truly foundational is Stage 2. That is the stage that
makes the rest of the backend architecture legible again.

---

## 12. Suggested PR / Epic Breakdown

The work should be grouped into a few coherent epics rather than many tiny
unrelated cleanups.

### Epic A: Hardening

Contains:

- public-value arity validation
- explicit root witness authority cleanup

This is the safest place to start because it is correctness-oriented and
smaller than the planning work.

### Epic B: Batch Proof Plan

Contains:

- `BatchProofPlan`
- split tier input preparation
- reshaped `PreparedMachineInput`
- removal of machine-side label partitioning

This is the central architectural epic.

### Epic C: Root Bundle And Extension Boundary

Contains:

- root backend bundle work
- `tabula-ext`-centered extension seam cleanup

These are related enough that they should be planned together, even if they
land as separate PRs.

### Epic D: Cleanup And Consolidation

Contains:

- contract tightening
- metadata cleanup
- witness kernel cleanup

This should come after the main structural epics.

### Epic E: Optional Topology Work

Contains:

- grouped proof planning
- generalized proof topology

This should remain separate and optional.

---

## 13. What Should Start Next

The next implementation stage should be:

- **Epic A: Hardening**

That means:

1. restore `num_public_values` validation,
2. make root witness authority explicit enough to stop hiding SMT assumptions.

Only after that should the code move into:

- **Epic B: Batch Proof Plan**

This order minimizes risk and keeps the main planning refactor from having to
solve correctness cleanup and architecture migration at the same time.

---

## 14. One-Sentence Summary

The roadmap is:

> **first harden correctness and authority boundaries, then introduce
> runtime-owned `BatchProofPlan`, then bundle root authority, then clean up
> extension seams, and only after that consider generalized proof topology.**
