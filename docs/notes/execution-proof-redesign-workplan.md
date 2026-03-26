# Execution and Proof Redesign Workplan

> **Status**: Implemented architecture work
> **Date**: 2026-03-24
> **Scope**: Canonical implementation plan for the post-migration execution and
> proof-front-end redesign.
> **Related**: [executor-proof-codesign-architecture.md](executor-proof-codesign-architecture.md),
> [proof-front-end-journal-architecture.md](proof-front-end-journal-architecture.md),
> [profile-native-runtime-migration-plan.md](profile-native-runtime-migration-plan.md),
> [canonical-vocabulary.md](canonical-vocabulary.md)

---

## 1. Purpose

This note turns the architecture direction into an implementation contract.

The goal is not incremental cleanup. The goal is to fully replace the current
execution-to-proof boundary with the ideal co-designed structure:

- runtime resolves canonical execution and proof contracts,
- executor emits a typed canonical `ExecutionJournal`,
- runtime proving reduces that journal into a canonical
  `ProofJournal`,
- runtime backend preparation derives `ProofArtifacts` from that
  journal,
- witness becomes a kernel layer,
- machine consumes only machine-native prepared artifacts.

This work is intentionally hard-break and architecture-first.

The plan below should be treated as the canonical sequencing and exit-gate
document for the redesign.

---

## 2. Core Values

Every stage of the redesign must preserve these values.

### 2.1 One internal semantic truth

Execution must have exactly one canonical internal output:

- `ExecutionJournal`

Proof preparation must have exactly one canonical runtime-owned input:

- `ProofJournal`

`BatchReport` may survive only as a derived reporting or boundary projection.
It must not remain the canonical internal proof source.

### 2.2 Runtime owns contracts, executor owns execution, witness owns kernels

The redesign is correct only if crate ownership becomes sharper, not blurrier:

- runtime owns resolved contracts and proof planning,
- executor owns deterministic semantic execution,
- witness owns materialization kernels,
- machine owns backend proving machinery only.

### 2.3 Parallelism must be structural, not accidental

The design must not rely on adding Rayon to legacy serial passes.

Instead:

- execution stays sequential where semantics require it,
- proof projection becomes tx-parallel,
- proof backend preparation becomes slot-parallel,
- ordering remains explicit and deterministic.

### 2.4 No compatibility-first compromise

This work is not a compatibility-preserving refactor.

Breaking internal APIs, renaming types, and deleting obsolete seams are allowed
and expected if they move the codebase toward the ideal structure.

### 2.5 Future relation work must fit naturally

This work does not solve static relations or lookup redesign.

However, the resulting architecture must leave a clean future insertion point
for relation or lookup effects without reworking the top-level execution or
proof topology again.

### 2.6 State surface policy must be fail-closed

Program-owned execution and proving must reject state cells outside the declared
program surface.

This policy applies uniformly to:

- free execution,
- runtime-backed execution,
- runtime proving entrypoints,
- direct proof-journal reduction.

Proof inputs must also match the executed batch pre-state after normalization.

---

## 3. Why This Work Is Split Into Four Stages

Conceptually, the redesign has two epics:

1. executor canonicalization
2. proof-front-end journal migration

In practice, implementing those as only two steps is too coarse. It makes
verification harder and encourages partially migrated intermediate states.

The correct implementation breakdown is four stages:

1. contract split and canonical boundaries
2. executor canonicalization
3. runtime proof-journal migration
4. boundary cleanup and hardening

This sequencing is important. The proof journal should be built on top of the
new canonical execution source, not on top of a transitional reinterpretation
of the old result model.

---

## 4. Stage 1: Contract Split and Canonical Boundaries

### Goal

Freeze the new architecture boundaries before implementation begins.

### Required outcomes

- runtime introduces the canonical distinction between:
  - `ResolvedExecutionProgram`
  - `ResolvedProofProgram`
- the architecture formally recognizes:
  - `ExecutionJournal`
  - `ProofJournal`
  as the future canonical internal boundaries
- `BatchReport` is explicitly demoted to a derived reporting or boundary view
- executor hot-path dependence on raw schema or profile re-resolution is marked
  for removal and no new code is allowed to deepen that dependency

### Rationale

Without this stage, later implementation tends to preserve old boundaries and
wrap them with adapters. That would leave the redesign half-finished.

### Exit gate

- the new runtime and executor nouns are stable
- ownership is decision-complete
- no later stage needs to revisit who owns execution contracts, proof plans, or
  witness orchestration

---

## 5. Stage 2: Executor Canonicalization

### Goal

Turn executor into the canonical deterministic semantic engine that emits
`ExecutionJournal`.

### Required outcomes

- executor consumes a resolved execution contract rather than repeatedly
  consulting raw program metadata on the hot path
- executor internal truth becomes typed semantic effects, not portable
  reporting projections
- batch execution returns `ExecutionJournal` as its canonical internal result
- any `BatchReport`-style structure becomes a derived view from that journal
- failed-tx partial access data is represented as diagnostic-only observations,
  not as canonical execution timeline effects
- batch-level state projections such as `read_set_old` and `write_set_final`
  remain available, but only as a nested derived `ExecutionStateSummary`
  inside the journal
- the overlay and effect recording model reflect typed semantic execution rather
  than report-friendly serialization

### Architectural intent

This stage is not primarily about speed. It is about moving semantic truth to
the correct place.

Once complete:

- execution owns what happened,
- and later proving only projects and reduces those facts.

### Exit gate

- executor no longer needs raw schema/profile lookups on the instruction hot
  path
- runtime can obtain a complete `ExecutionJournal` without reconstructing
  semantic information from reporting views
- runtime internal state transitions no longer depend on `BatchReport`
- failed diagnostic observations are type-distinct from canonical success-path
  execution effects
- execution tests validate journal truth first and reporting projections second

---

## 6. Stage 3: Runtime Proof Journal Migration

### Goal

Make runtime proving consume `ExecutionJournal` directly and reduce it into a
plan-aligned `ProofJournal`.

### Required outcomes

- runtime introduces a canonical proof-journal builder owned by
  `tabula-runtime`
- proof front-end preparation becomes:
  - tx-local projection
  - deterministic reduction
  - plan-aligned finalization
- prepared column inputs become fully column-owned and include all proof-local
  column data such as property reads
- capability transcript proof inputs become proof-plan aligned at journal-build time
- witness whole-batch orchestration stops being part of the runtime prove path

### Architectural intent

This stage replaces multi-pass extraction and re-grouping with one canonical
reduction pipeline.

At the end of this stage, proving should no longer reinterpret general-purpose
execution output. It should consume canonical semantic execution output and
perform proof-specific reduction only once.

### Exit gate

- runtime proving no longer rescans `BatchReport` as its canonical source
- prepared proof inputs are aligned to proof-plan order rather than organized
  around ad hoc maps
- tx-local proof projection is parallelizable and deterministic

---

## 7. Stage 4: Boundary Cleanup and Hardening

### Goal

Delete transitional seams, harden determinism, and leave only the final model.

### Required outcomes

- witness public orchestration surfaces that only existed for the old prove
  path are deleted or demoted
- obsolete extraction and grouping helpers are removed
- old result-model assumptions are removed from runtime prove code
- witness lowering consumes typed success-path kernel carriers rather than
  portable reporting carriers
- failed tx diagnostics are excluded from canonical proof reduction
- runtime rejects state outside the declared program surface before execution or
  proof reduction begins
- runtime proving rejects proof inputs whose normalized pre-state does not match
  `executed.state_before`
- determinism is enforced with explicit tests
- thread-count invariance is tested
- machine-facing order is aligned with prepared proof-plan order
- architecture guards prevent reintroduction of removed seams

### Architectural intent

This stage is where the redesign stops being "implemented enough" and becomes
the only sanctioned architecture.

### Exit gate

- no old orchestration path remains in production code
- `ExecutionJournal` and `ProofJournal` are the only canonical internal
  execution and proof boundaries
- proof hot paths do not re-project access effects or re-encode capability
  transcript payloads more than once per semantic effect
- deterministic and parallel behavior are verified under test
- runtime proving no longer crosses a portable reporting boundary before
  lowering

---

## 8. Final State

The redesign is complete only when the production pipeline is exactly:

```text
ResolvedExecutionProgram
-> ExecutionJournal
-> ProofJournal
-> ProofArtifacts
```

with these invariants enforced in code and tests:

- witness is a kernel-only crate,
- failed tx diagnostics never enter proof reduction,
- runtime proving reduces journals directly and does not rescan `BatchReport`,
- lowering consumes typed success-path facts,
- determinism is checked with explicit canonical digests rather than debug text.

---

## 8. Implementation Rules

The work must follow these rules in every stage.

### 8.1 Prefer concrete models over frameworks

The redesign should use:

- concrete structs,
- concrete reducers,
- concrete plans,
- explicit stage boundaries.

It should not introduce:

- generic event-bus frameworks,
- reducer registries,
- actor-heavy orchestration,
- extensibility layers that exist only for internal plumbing.

### 8.2 Keep the semantic boundary typed

Typed execution facts are the correct internal representation.

Portable forms should appear only at reporting or protocol boundaries, not as
the primary execution truth for proving.

### 8.3 Keep machine below semantics

Machine setup, trace construction, and proof packaging are backend concerns.

They should consume prepared artifacts in the right order, not participate in
semantic planning or execution reinterpretation.

### 8.4 Keep future lookup integration possible without shaping the current work around it

This redesign must not become dependent on a future lookup solution.

At the same time, the resulting journal and proof-plan topology must allow a
future relation or lookup effect family to slot into the model without
requiring another top-level redesign.

---

## 9. Success Criteria for the Full Workstream

The redesign is complete only when all of the following are true:

- runtime resolves execution and proof contracts explicitly
- executor emits one canonical `ExecutionJournal`
- `BatchReport` is no longer the internal proof source
- runtime proving consumes the execution journal and emits one canonical
  `ProofJournal`
- witness is a kernel layer, not a batch orchestration layer
- machine consumes only prepared machine-native artifacts
- parallelism starts before backend prep and is structurally supported
- ordering is explicit and deterministic
- future relation or lookup work can be added without replacing the top-level
  execution and proof topology again

That is the definition of done for this redesign track.

---

## 10. Recommended Planning Workflow

This workplan exists so that the implementation can be planned and executed one
stage at a time without losing the whole-system picture.

The recommended workflow is:

1. keep this note as the canonical sequence and value document,
2. enter detailed planning for one stage at a time,
3. do not begin a later stage until the prior stage's exit gate is met,
4. revisit the architecture notes only if a stage reveals a real conflict with
   the intended final model.

This keeps the redesign disciplined while still permitting deep, stage-specific
planning and implementation.
