# Executor and Proof Co-Design Architecture

> **Status**: Implemented architecture note
> **Date**: 2026-03-24
> **Scope**: Defines the ideal future co-design of execution, runtime proving,
> witness materialization, and machine handoff after the profile-native
> migration has completed.
> **Related**: [proof-front-end-journal-architecture.md](proof-front-end-journal-architecture.md),
> [execution-proof-redesign-workplan.md](execution-proof-redesign-workplan.md),
> [profile-native-runtime-migration-plan.md](profile-native-runtime-migration-plan.md),
> [verification vocabulary](../design/architecture.md#verification-vocabulary),
> [proof-hierarchy-and-grouping.md](proof-hierarchy-and-grouping.md),
> [../research/symbolic-air-compilation.md](../research/symbolic-air-compilation.md)

---

## 1. Why This Note Exists

The proof-front-end journal architecture solves the downstream half of the
current problem:

- runtime proving should consume one canonical journal,
- witness should stop owning whole-batch orchestration,
- and proof preparation should become deterministic and naturally parallel.

However, that design is only half-finished if execution still produces a
general-purpose reporting result and leaves proving to rediscover semantic
structure later.

This note therefore makes a stronger claim:

> **The ideal Tabula architecture is not "better proving on top of the current
> executor." It is a co-designed execution-and-proof pipeline where execution
> emits the canonical semantic journal that proving consumes.**

The executor and proof front-end should be treated as one architecture track.

---

## 2. Core Thesis

The ideal future structure is:

1. runtime resolves the sealed program into an execution contract and a proof
   contract,
2. executor executes against the resolved execution contract,
3. executor produces a typed canonical `ExecutionJournal`,
4. runtime proving reduces that journal into a plan-aligned
   `ProofJournal`,
5. witness materializes stores and traces from those prepared proof inputs,
6. machine consumes only machine-native proof artifacts and knows nothing about
   execution semantics.

The runtime boundary is fail-closed on state shape:

- execution rejects state cells outside the declared execution surface,
- proving rejects state cells outside the declared proof surface,
- proving also rejects any normalized pre-state that does not equal
  `executed.state_before`.

This replaces the current shape where:

- executor interprets raw runtime metadata repeatedly,
- execution output is partly reporting-oriented,
- runtime proving rescans and re-groups generic execution output,
- and witness still owns batch-level orchestration.

---

## 3. Architectural Pattern

The pattern that best fits Tabula is:

- **resolved-plan execution**
- **typed effect sourcing**
- **deterministic columnar reduction**
- **functional core, orchestration shell**

### 3.1 Resolved-plan execution

Execution should not repeatedly consult raw schemas, raw program metadata, or
sealed profile catalogs on the instruction hot path.

Runtime should resolve the program once into an execution-specific contract:

- tx bodies already resolved,
- parameter schema already resolved,
- read/write column type metadata already resolved,
- property routes already resolved,
- capability routes and typed signatures already resolved,
- future relation routes already resolved.

The executor should run that resolved plan, not rediscover it.

### 3.2 Typed effect sourcing

Execution should emit typed primary effects, not portable reporting projections.

The canonical internal output should be an append-only semantic journal whose
effect families include:

- state access,
- property read,
- capability call,
- IR hash,
- emitted application event,
- future relation or lookup effect.

Failed transaction diagnostics should not reuse the same semantic effect type as
successful committed execution. Failed partial access data is useful for
reporting and debugging, but it is not canonical proof semantics and should be
typed separately inside the journal.

Portable or user-facing result views should be derived later. They should not
be the internal source of truth for proving.

### 3.3 Deterministic columnar reduction

The proof system does not consume "a bag of execution output." It consumes:

- column-local deltas,
- capability-call-local call groups,
- execution-lane witness inputs,
- root-binding-relevant commitments.

That means the natural proof front-end is columnar and plan-aligned, not an
ad hoc series of maps and rescans over one generic batch result.

### 3.4 Functional core, orchestration shell

The semantic heart of the system should be concrete and predictable:

- resolved execution plans,
- typed transaction machines,
- immutable tx-local shards,
- deterministic reducers.

The orchestration shell around that core should be the only place that deals
with:

- process-local host resources,
- transaction loop control,
- checkpoints and rollback,
- machine setup and witness-store merging.

This keeps semantic logic testable and prevents orchestration concerns from
leaking into the proof model.

---

## 4. Ownership Model

### 4.1 Runtime owns the canonical contracts

`tabula-runtime` should remain the first layer that knows both:

- the sealed program plus host-installed runtime registries and schemes,
- and the proof backends and machine setup that must later consume execution.

For that reason, runtime should own the canonical resolved forms:

- `ResolvedExecutionProgram`
- `ResolvedProofProgram`
- the proof plan
- the final reduction rules

Runtime is the right owner of contract resolution, not executor and not
machine.

### 4.2 Executor owns deterministic semantic execution

`tabula-executor` should own:

- transaction execution semantics,
- transactional overlay semantics,
- tx-local effect journaling,
- execution-level validation of resolved plans.

Executor should not own:

- proof grouping,
- proof backend selection,
- machine topology,
- witness-store assembly,
- verifier-facing statement assembly.

Executor is a deterministic semantic engine, not a proof planner.

### 4.3 Witness owns narrow materialization kernels

`tabula-witness` should own only:

- lowering one successful tx,
- materializing one column witness,
- materializing one capability witness,
- materializing one IR-hash witness,
- future relation witness kernels.

Witness should not own:

- whole-batch execution reinterpretation,
- proof-plan materialization,
- multi-pass grouping logic,
- global proving orchestration.

Runtime proving should feed typed success-path facts directly into these witness
kernels. Portable reporting carriers remain boundary artifacts for reporting
and verifier-facing capability transcript contracts, not a lowering seam.

On the proof hot path, typed access and capability effects should be projected
once and reused, not re-encoded through duplicate lowering or transcript
materialization passes.

### 4.4 Machine owns backend execution only

`tabula-machine` should remain below the semantic layer.

Machine should see:

- proof traces,
- witness stores,
- proof setups,
- backend chip extensions.

Machine should not know:

- execution IR semantics,
- runtime type meaning,
- property-read routing,
- capability grouping policy,
- any generic batch result shape.

This boundary keeps backend concerns clean and replaceable.

---

## 5. Canonical Boundaries

The ideal pipeline has four explicit boundaries.

### 5.1 `ResolvedExecutionProgram`

This is the execution contract created by runtime for executor consumption.

It is a hot-path object. It should contain only the information execution needs
repeatedly and must not require executor to resolve schema or profile metadata
on demand.

### 5.2 `ExecutionJournal`

This is the canonical internal output of execution.

It is:

- typed,
- deterministic,
- immutable after batch completion,
- proof-friendly,
- not verifier-visible,
- not a portable protocol artifact.

The journal is the semantic truth that later proof preparation consumes.

Its batch-level state view should be explicit and nested:

- canonical tx-local semantic truth lives in success-path execution shards,
- failed tx access observations live in diagnostic failure shards,
- derived batch-level state views such as `read_set_old` and
  `write_set_final` live inside a nested `ExecutionStateSummary`.

This keeps the semantic core clean while still preserving the exact overlay
summary that runtime and reporting need.

Failed diagnostics are intentionally excluded from canonical proof reduction.

### 5.3 `ProofJournal`

This is the canonical runtime-owned proof input.

It is:

- aligned to the proof plan,
- already grouped by proof-facing semantic families,
- already deterministic,
- ready for slot-local backend preparation.

### 5.4 `ProofArtifacts`

This is the backend-prepared machine-facing output.

It includes:

- witness stores,
- trace maps,
- root-binding-visible digests,
- machine-ready per-tier artifacts.

This is the only layer machine and prover need to see.

---

## 6. The Ideal Executor

The ideal Tabula executor is not a generic interpreter facade. It is a
**deterministic transactional effect engine**.

Its job is:

1. execute resolved transaction bodies,
2. mutate transactional overlay state,
3. record typed semantic effects,
4. commit or roll back effects per tx,
5. finalize one canonical execution journal.

### 6.1 Recommended internal structure

The internal structure should be decomposed into:

- `BatchExecutor`
  - owns the batch loop and transaction lifecycle,
- `TxMachine`
  - executes one resolved tx body against one slot frame and one overlay,
- `TxnOverlay`
  - owns state-view semantics, caching, buffering, and rollback,
- `TxJournalBuilder`
  - owns tx-local typed effect capture,
- `ExecutionReporter`
  - derives public or reporting views such as `BatchReport`.

The current overlay split between state and trace is directionally correct, but
the final design should move from portable event recording to typed semantic
effect recording.

### 6.2 What the executor should not do

The executor should not:

- look up column type information from raw schemas on every read or write,
- encode effects to portable values just because reporting types currently use
  portable carriers,
- know about proof grouping or backend partitioning,
- construct witness-specific grouping structures,
- depend directly on machine or proof-column types.

---

## 7. Effect Model

The effect model is the central abstraction that makes executor and proof
co-design work.

### 7.1 Effects are primary truth

The executor should treat effects as the primary semantic result of execution.

Examples include:

- read or write access to committed or overlay state,
- property query resolution result,
- capability call input and output,
- IR-hash canonical input set and digest,
- emitted event payload.

Global summary structures such as old-state reads and final writes are derived
from those effects plus overlay state, not the other way around.

### 7.2 Effects need stable identity

Every effect family should carry stable identity sufficient for:

- deterministic ordering,
- reduction,
- test comparison,
- future streaming,
- future relation integration.

The minimum identity model should be thought of as:

- tx index,
- instruction index when applicable,
- effect ordinal within tx,
- logical time when applicable,
- effect family.

### 7.3 Effects should be family-partitioned

A single heterogeneous event bus is not the ideal internal representation.

The better model for Tabula is a tx-local shard that partitions effects by
semantic family:

- access effects,
- property-read effects,
- capability-call effects,
- IR-hash effects,
- emitted events,
- future relation effects.

This is more efficient because the proof front-end already consumes them by
family.

---

## 8. Proof Co-Design

The executor design should be chosen so that proof preparation becomes a
projection step rather than a reinterpretation step.

### 8.1 Runtime proving should consume `ExecutionJournal`

Runtime proving should stop treating `BatchReport` as canonical truth.

Instead:

- executor produces `ExecutionJournal`,
- runtime proving projects tx-local proof shards from it in parallel,
- runtime proving reduces those shards into a plan-aligned
  `ProofJournal`.

This eliminates repeated decoding, repeated grouping, and repeated scanning of
general-purpose execution output.

The same rule should hold inside runtime execution itself: once journal-first
execution exists, runtime should not keep `BatchReport` as an internal semantic
dependency for state evolution or consistency decisions.

### 8.2 Proof preparation should be plan-first

Runtime proving should first know:

- which column proof slots exist,
- which capability transcript proof slots exist,
- which grouping rules exist,
- how the execution tier and root tier are wired.

Then it should reduce execution effects directly into vectors aligned to that
plan.

This means:

- fewer maps,
- fewer order bugs,
- simpler backend preparation,
- stronger determinism.

### 8.3 Witness remains a kernel layer

Once executor and runtime have this structure, witness no longer needs to own
whole-batch orchestration. It becomes a focused materialization layer below the
semantic planning boundary.

That separation is cleaner and easier to evolve.

---

## 9. Machine Co-Design

The machine layer constrains the ideal design in important ways.

### 9.1 Machine wants stable, already-planned inputs

Machine setup is already built around proof tiers and proof columns, not around
execution semantics. The executor and runtime design should respect that.

The correct relationship is:

- runtime resolves semantic contracts and proof plans,
- runtime produces prepared journal inputs in machine order,
- witness materializes those into tier-local stores,
- machine consumes those stores without reinterpreting semantics.

### 9.2 Execution tier should stay semantic-light

The execution tier may carry extensions such as:

- IR hash,
- capability transcript,
- future relation proof systems.

But those extensions should remain backend execution-tier extensions, not
semantic ownership sites. The semantic source of truth remains the execution
journal and prepared proof journal in runtime space.

### 9.3 Prepared order should match machine order

The ideal model is that prepared vectors are already aligned to machine setup
order.

That means:

- column proof inputs should already be in column-setup order,
- capability transcript proof inputs should already be in their plan order,
- tier partitioning should be a final backend step, not a semantic regrouping
  step.

This reduces impedance mismatch between runtime proving and machine setup.

---

## 10. Parallelism Model

The ideal co-designed system uses coarse-grained deterministic parallelism.

### 10.1 Sequential where semantics require it

Transaction execution remains sequential in the current model because state
transitions, rollback semantics, and effect ordering are inherently ordered.

The ideal design does not force artificial tx-level parallel execution.

### 10.2 Parallel where structure allows it

The right top-level parallel stages are:

1. tx-local proof projection from the execution journal,
2. per-slot column proof preparation,
3. per-slot capability transcript proof preparation,
4. future per-slot relation proof preparation,
5. trace building where the machine setup already supports independent tiers or
   independent columns.

### 10.3 Future streaming compatibility

The architecture should also be friendly to a later deterministic streaming
pipeline:

- tx executes sequentially,
- tx shard is emitted immediately,
- proof projection begins before the batch finishes,
- deterministic reduction merges completed shards by explicit keys.

This is a future optimization path, not a required first implementation, but
the core data model should not block it.

---

## 11. Rejected Patterns

The ideal design is easier to understand by stating what it should not become.

### 11.1 Not a generic event bus framework

The system does not need:

- a pluggable internal bus,
- a generic reducer registry,
- a framework for arbitrary subscriber pipelines.

Concrete semantic families are enough.

### 11.2 Not a machine-aware executor

The executor should not know:

- proof chips,
- proof tiers,
- grouped proof packaging,
- FRI or PCS concerns.

Those belong below runtime or below machine.

### 11.3 Not "keep `BatchReport` and add more adapters"

That would preserve the core problem: semantic truth would still be ambiguous
between execution and proof views.

The point is to give the system one canonical internal semantic output.

### 11.4 Not actor-heavy or async-first

The executor and proof front-end should remain synchronous, concrete, and
deterministic by default.

Future streaming can still be achieved without turning the architecture into an
actor framework.

---

## 12. Why This Is the Right Fit for Tabula

This approach fits Tabula especially well because Tabula has:

- bounded transaction bodies,
- deterministic execution,
- strong sealed type and profile contracts,
- proof-centric downstream consumers,
- per-column proof specialization,
- no need for a general-purpose virtual machine abstraction.

In that environment, a resolved-plan effect engine is more natural than a
generic interpreter returning a generic execution result.

The design also fits future directions:

- proof front-end journalization,
- future static relation integration,
- grouped logical column proofs,
- symbolic AIR compilation.

It is therefore not a narrow optimization. It is the correct semantic boundary
for the long-term architecture.

---

## 13. Definition of Success

This co-designed architecture is achieved when all of the following are true:

- executor runs a resolved execution contract rather than repeatedly consulting
  raw schema and profile metadata,
- executor produces one typed canonical `ExecutionJournal`,
- runtime proving consumes that journal directly,
- runtime reduces journal data into a plan-aligned `ProofJournal`,
- witness owns kernels rather than whole-batch orchestration,
- runtime proving crosses no portable reporting boundary before lowering,
- machine consumes only machine-native prepared artifacts,
- reporting views such as `BatchReport` are derived projections rather than the
  internal semantic truth,
- failed partial access observations are diagnostic-only rather than canonical
  proof-facing execution effects,
- the design remains compatible with future static relation work and future
  symbolic AIR compilation.

That is the ideal future executor and proof architecture for Tabula.
