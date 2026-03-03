# Tabula: Complete Technical Narrative

> Audience: engineering, protocol, and product teams  
> Use case: single-source presentation and deep onboarding document  
> Language: English  
> Scope: problem definition, semantic model, architecture, proof system, and end-to-end flow

---

## Executive Summary

Tabula is a zero-knowledge kernel designed for **typed, tabular state transitions**.  
Its core architectural move is to stop proving machine execution as the primary abstraction and instead prove application-relevant state transitions directly.

The system is built on five pillars:

1. A strict semantic contract (typed IR + canonical normal form).
2. Deterministic execution separated from cryptography.
3. A column-aware commitment model (hybrid SSMC/SMT).
4. A multi-chip AIR architecture connected by explicit LogUp buses.
5. A fail-closed compatibility spine (driver + contract metadata + artifact model).

The result is an end-to-end architecture where language, runtime, commitment, and proof layers share one coherent model of state.

---

## Reading Map

For presentation and onboarding, this document is structured as a strict narrative:

1. **Why Tabula exists**: Section 1 (problem) and Section 2 (design principles).
2. **What Tabula means semantically**: Sections 3–5 (state model, IR contract, normal form).
3. **How Tabula is built**: Sections 6–13 (pipeline, crates, compiler/runtime/commitment/proof, contract spine, product surfaces).
4. **How Tabula works end to end**: Section 14 (canonical transfer walkthrough).
5. **What guarantees Tabula provides**: Sections 15–16 (correctness boundary and engineering quality model).
6. **How to reason beyond v1 scope**: Sections 17–18 (extension discipline and synthesis).

---

## 1. Problem Definition

### 1.1 The structural inefficiency in machine-centric proving

Stateful applications spend most of their semantic effort on a small set of persistent reads and writes.  
Machine-centric proving systems, by design, force this through instruction-level execution, which introduces structural overhead:

1. A single logical state read/write expands into many low-level instructions.
2. Memory-consistency arguments must cover stack, heap, and temporaries, not only persistent state.
3. State commitment updates are paid through the same execution abstraction.
4. Type information is flattened, reducing opportunities for proof-aware specialization.

### 1.2 Formal objective

Given:

1. `oldRoot` (pre-state commitment),
2. an ordered batch of typed transactions,

prove:

1. instruction semantics are correct,
2. reads are bound to valid state values at correct times,
3. writes are applied according to semantics,
4. the resulting commitment is exactly `newRoot`.

### 1.3 Tabula’s thesis

Tabula places the abstraction boundary at **schema-typed state transitions**:

1. State keys are explicit `(table, column, row)` coordinates.
2. Local computation uses SSA slots, not mutable memory.
3. Intra-transaction memory ambiguity is eliminated structurally.
4. Inter-transaction consistency is proven with explicit memory buses.

The architecture is not “a VM plus a proof wrapper.”  
It is a state-transition proof system with a language and runtime co-designed for that proof model.

---

## 2. Design Principles

### 2.1 State-native first

Persistent state is first-class:

1. Explicit table and column coordinates.
2. Typed values at schema boundaries.
3. Per-column commitment and proof routing.

### 2.2 Determinism by construction

Determinism is an architectural invariant, not a runtime preference:

1. deterministic collections and ordering,
2. stable slot semantics,
3. canonicalized program forms,
4. explicit trace identity (`tx_index`, `effect_ordinal_in_tx`).

### 2.3 Semantic and cryptographic separation

Execution is crypto-agnostic and testable in isolation.  
Commitment and proving layers consume execution outputs through typed boundaries.

### 2.4 Fail-closed compatibility

Profile, schema, and statement-binding mismatches are terminal errors.  
No silent fallback paths are acceptable.

### 2.5 Spec-code traceability

Semantics and proof specs are normative references.  
Implementation is expected to map explicitly to those invariants and constraints.

---

## 3. Semantic Foundation

This section defines the conceptual state machine independent of any one adapter.

### 3.1 State addressing

A state cell is identified by:

1. `TableId`
2. `ColId`
3. `RowKey`

Combined as:

1. `CellKey { table, col, row }`

Canonical ordering is `(table, col, row)`, and this ordering is protocol-relevant for sorting and hashing pipelines.

### 3.2 Value domains

Application-level value domains:

1. `U64`
2. `I64`
3. `Bool`
4. `Bytes32`

These are semantic domains visible to language, IR, and runtime.  
Field-element encodings are commitment/proof concerns.

### 3.3 Null and absence model

Tabula does not model null as a value variant.

1. Presence/absence is represented separately.
2. Reads produce `(value, is_null)`.
3. Writes consume `(value, is_null)`.
4. Null payloads are canonicalized to type-zero values.

This prevents “hidden payload in null rows” and stabilizes witness encoding.

### 3.4 Transaction and batch semantics

A transaction executes against a state snapshot and yields effects.  
A batch executes transactions in order and composes updates deterministically.

Batch execution returns:

1. read set from committed base state,
2. final coalesced write set,
3. ordered execution events,
4. per-transaction outcomes,
5. emitted application events.

---

## 4. IR Contract

### 4.1 Program entities

The IR contract includes:

1. program (set of transaction type definitions),
2. transaction body (instruction list),
3. slot identifiers (SSA locals),
4. typed parameter schema.

### 4.2 True SSA

SSA invariants:

1. each destination slot is assigned at most once,
2. all slot uses are def-before-use,
3. multi-output instructions define distinct slots.

This removes local mutable-memory ambiguity.

### 4.3 Instruction set

Core instruction families:

1. state: `Read`, `Write`, `Lookup`
2. arithmetic/comparison: `Arith`, `DivMod`, `Cmp`
3. boolean logic: `Not`, `And`, `Or`
4. control/value shaping: `Assert`, `Select`
5. hashing/output: `Hash`, `Emit`

Each instruction has explicit typed operands and destinations.

### 4.4 Failure semantics

A transaction fails on:

1. failed assert,
2. arithmetic overflow,
3. division by zero,
4. invalid access/type conditions.

Failed transactions do not persist state mutations.

### 4.5 Hash semantics

Protocol hash semantics are domain-separated and canonicalized.  
Runtime hash interfaces and proof hash circuits are aligned through explicit encoding contracts.

---

## 5. Canonical Normal Form (NF)

Normal form is the structural core of Tabula’s proof tractability.

### 5.1 NF invariants

Per transaction, per `(table, col, row)`:

1. NF-1: at most one read,
2. NF-2: at most one write,
3. NF-3: no read-after-write,
4. NF-4: key-alias resolvability.

### 5.2 Why NF matters

NF converts runtime ambiguity into compile-time structure:

1. no intra-transaction read-coherence argument,
2. no intra-transaction write-coalescing ambiguity,
3. deterministic key semantics for proof routing.

### 5.3 Compiler and registration pipeline

IR admission pipeline:

1. canonicalize,
2. typecheck,
3. validate.

Canonicalization performs fixups where safe (for example read deduplication).  
Validation enforces remaining invariants as hard constraints.

---

## 6. Three-Stage Architecture

Tabula separates concerns into a strict dataflow pipeline.

### 6.1 Stage A: Execution

Input:

1. registered program,
2. snapshot state,
3. transaction batch.

Output:

1. deterministic `ExecutionResult`.

No cryptographic commitment operations are required in this stage.

### 6.2 Stage B: Commitment

Input:

1. execution outputs,
2. column states,
3. schema/type metadata.

Output:

1. updated commitments,
2. old/new root bindings,
3. metadata required by proof circuits.

### 6.3 Stage C: Proving

Input:

1. execution evidence,
2. commitment evidence,
3. statement public inputs.

Output:

1. STARK proof artifacts and verification data.

---

## 7. Workspace Architecture

### 7.1 Core crates and responsibilities

| Crate | Responsibility |
|---|---|
| `tabula-core` | foundational types, traits, errors, execution event model |
| `tabula-ir` | instruction model, canonicalization, typing, NF validation |
| `tabula-lang` | DSL lexer/parser/lowering to IR |
| `tabula-executor` | deterministic interpreter, overlay, batch orchestration |
| `tabula-commitment` | field-native hashing, SSMC/SMT, hybrid commitment routing |
| `tabula-proof` | witness generation, trace assembly, AIR chips, STARK prove/verify |
| `tabula-contract` | statement and metadata compatibility contract |
| `tabula-driver` | canonical compile/register/compatibility entrypoint |
| `tabula-artifact` | shared artifact schemas and serialization helpers |
| `tabula-cli` | command-line adapter |
| `tabula-daemon` | local HTTP adapter and run orchestration interface |
| `tabula-web-ide` | browser adapter over daemon APIs |

### 7.2 Dependency intent

Boundary intent:

1. semantics core stays independent of heavy proving dependencies,
2. execution stays decoupled from proof machinery,
3. adapters remain transport/product surfaces, not semantic owners.

---

## 8. Compiler and Language Layer

### 8.1 DSL goals

The DSL is designed to preserve proof clarity:

1. explicit state access,
2. typed expressions,
3. deterministic lowering behavior.

### 8.2 Lowering model

Lowering performs:

1. table/column resolution,
2. parameter/local binding management,
3. slot allocation and instruction emission,
4. type-directed expression lowering,
5. explicit null handling through write flags.

### 8.3 Registration model

After lowering:

1. IR bodies are canonicalized,
2. slot and type constraints are validated,
3. NF constraints are enforced,
4. validated tx type definitions are admitted into program registry.

---

## 9. Runtime Execution Engine

### 9.1 Interpreter core

The interpreter executes IR instruction-by-instruction with:

1. typed operand resolution,
2. slot value updates,
3. state read/write dispatch through overlay,
4. deterministic event emission.

### 9.2 Overlay architecture

Overlay subcomponents:

1. write buffer,
2. read cache,
3. undo log,
4. trace recorder.

Semantics:

1. read-your-writes precedence,
2. snapshot read caching,
3. last-write-wins buffering.

### 9.3 Transaction rollback behavior

Execution is checkpointed per transaction:

1. success commits buffered changes,
2. failure reverts state mutations,
3. failure metadata remains visible in outcomes.

### 9.4 Consistency checker

A dedicated key-local consistency checker verifies:

1. read values match latest prior write or base value,
2. event identity sequencing remains valid.

---

## 10. Commitment Architecture

### 10.1 Field-native commitment layer

Commitment operations are field-native and include:

1. value codec into field limbs,
2. domain-separated hashing,
3. sparse commitment structures.

### 10.2 SSMC path

SSMC supports:

1. sorted sparse entry commitments,
2. merge traces for old/write/new transitions,
3. commitment outputs for proof wiring.

### 10.3 SMT path

SMT supports:

1. sparse large-domain key commitments,
2. inclusion/update path evidence,
3. root composition compatibility.

### 10.4 Hybrid routing

HybridVC selects commitment strategy per column using policy thresholds and column characteristics, while preserving a uniform commitment interface for downstream proving.

---

## 11. Proof Architecture

### 11.1 Public statement

The proof statement binds:

1. pre-state root,
2. post-state root,
3. program commitment root,
4. applied transaction digest,
5. static table root,
6. resource budgets.

### 11.2 Witness generation

Witness generation bridges runtime outputs to proof inputs:

1. groups accesses by column identity,
2. builds init/access rows,
3. applies commitment transitions,
4. materializes root-linked metadata.

### 11.3 Trace builder

Trace assembly orchestrates:

1. execution trace construction,
2. memory-order traces,
3. static lookup traces,
4. SMT path traces,
5. Poseidon and range-check traces.

### 11.4 AIR chip set

The proof system uses specialized chips:

1. `Execution` for instruction semantics and operand-slot linkage,
2. `InterTxOrder` for per-key transaction ordering semantics,
3. `StateColumn` for state-entry transitions and commitment chain constraints,
4. `ColumnMeta` for column metadata and root-link wiring,
5. `StaticTable` for static lookup bus matching,
6. `Poseidon` for shared permutation constraints,
7. `RangeCheck` for bounded integer/range gadgets,
8. `SmtColPath` for column-level path constraints,
9. `SmtTablePath` for table/root-level path constraints.

### 11.5 Interaction bus model

Cross-chip consistency is explicit through typed bus schemas.

| Bus | Purpose | Canonical tuple shape |
|---|---|---|
| `RangeCheck` | bounded-range lookup checks | `(value)` |
| `PoseidonPermutation` | shared permutation service | `(input[16], output[8])` |
| `CommitmentVerification` | commitment digest joins | `(t, c, comm_type, is_touched, digest[8])` |
| `ReadAccess` | read access alignment | `(t, c, key[3], tx_index, value[W], is_null)` |
| `WriteAccess` | write access alignment | `(t, c, key[3], tx_index, value[W], is_null)` |
| `EmptyColRead` | empty-column read evidence | `(t, c)` |
| `BaseStateEntry` | base-entry handoff | `(t, c, key[3], value[W], is_null)` |
| `CoalescedWrite` | final write-set handoff | `(t, c, key[3], value[W], is_null)` |
| `StaticTableLookup` | static lookup alignment | `(t, c, key[3], value[W])` |
| `SmtLeafDigest` | leaf digest handoff | `(table_id, col_id, old_leaf[8], new_leaf[8])` |
| `SmtTableRoot` | table-root handoff | `(table_id, old_root[8], new_root[8])` |

This schema-first bus model removes ad hoc tuple wiring and localizes interaction correctness.

### 11.6 STARK prove/verify composition

Proving flow:

1. per-chip STARK proving on assembled traces,
2. cross-chip interaction balance accounting,
3. cumulative-sum consistency checks over interaction field.

Verification flow:

1. verify each chip proof against the same config assumptions,
2. verify global interaction balance condition.

---

## 12. Contract, Metadata, and Artifact Spine

### 12.1 Contract metadata envelope

Metadata is canonicalized and hashed with stable serialization rules.  
It includes:

1. profile hash,
2. contract schema version,
3. statement binding version,
4. optional semantic-hash extension slot.

### 12.2 Fail-closed compatibility policy

Validation policy rejects:

1. unknown schema versions,
2. profile mismatches,
3. statement binding version mismatches,
4. semantic hash mismatches where required.

### 12.3 Binding registry model

Statement fields are classified explicitly (for example bound-in-air versus deferred).  
Completeness is enforced so no field is silently unclassified.

### 12.4 Artifact model

Shared artifacts define consistent schemas for:

1. programs,
2. state files,
3. batches,
4. execution receipts and transport payloads.

This keeps CLI, daemon, and web surfaces aligned on the same data contract.

---

## 13. External Product Surfaces

### 13.1 CLI surface

The CLI provides:

1. compile and check paths for static validation,
2. execute and inspect paths for runtime workflows,
3. example generation for reproducible demos.

### 13.2 Daemon surface

The daemon exposes a stateful API for:

1. program registration/listing,
2. instance lifecycle operations,
3. run submission and retrieval,
4. optional prove/verify transitions.

### 13.3 Web IDE surface

The web IDE uses daemon APIs to provide:

1. program editing and compilation flows,
2. state and batch authoring,
3. execute/prove/verify/apply user journeys.

---

## 14. End-to-End Walkthrough (Canonical Transfer Scenario)

This section demonstrates the full narrative from source to proof.

### 14.1 Source-level intent

A transfer transaction expresses:

1. read sender and receiver balances,
2. assert sender sufficiency,
3. compute new balances,
4. write final balances.

### 14.2 IR lowering

Lowering maps this to typed SSA instructions:

1. two reads with `(value, is_null)`,
2. comparison and assert,
3. arithmetic updates,
4. two writes with explicit null flags.

### 14.3 Runtime execution

Execution produces:

1. ordered events tagged by transaction/effect identity,
2. read set from base state,
3. final write set after rollback-safe processing.

### 14.4 Commitment transition

Commitment logic:

1. opens affected column commitments,
2. applies writes through strategy-specific update logic,
3. derives updated column and state roots.

### 14.5 Witness and trace synthesis

Proof preprocessing:

1. groups column evidence,
2. emits chip-specific rows,
3. constructs bus-linked interactions.

### 14.6 Proof generation and verification

Final step:

1. produce chip proofs and global interaction evidence,
2. verify statement-bound root transition and interaction consistency.

The verifier receives a cryptographic guarantee of transition correctness, not merely a runtime log.

---

## 15. Correctness and Trust Boundaries

### 15.1 What is proven

The proof system establishes:

1. instruction semantics correctness,
2. access-order and value consistency constraints,
3. commitment-transition validity from old to new root.

### 15.2 What remains policy/operations

System-level concerns such as transaction inclusion policy and adapter-level request handling remain outside pure arithmetic proof constraints and are governed by runtime policy layers.

### 15.3 Security posture fundamentals

The architecture enforces:

1. explicit domain separation in hash and interaction channels,
2. canonical encoding at contract boundaries,
3. strict compatibility checks with fail-closed behavior.

---

## 16. Engineering Quality Model

### 16.1 Validation layers

Quality is enforced at multiple layers:

1. compile-time canonicalization and typing,
2. runtime deterministic consistency checks,
3. chip-level debug constraint validation,
4. cross-chip interaction balance checks,
5. integration and end-to-end tests.

### 16.2 Architectural hygiene

The project emphasizes:

1. clear crate responsibility boundaries,
2. constrained trait interfaces at abstraction seams,
3. deterministic data structures and ordering,
4. spec-oriented design documentation.

---

## 17. Extension Trajectories

The architecture naturally supports future expansion without breaking core principles:

1. richer control-flow lowering paths,
2. additional chip specializations,
3. more advanced proving backends and policies,
4. stronger orchestration centralization,
5. expanded artifact and semantic-profile governance.

The key requirement is that extensions preserve semantic determinism and explicit proof contracts.

---

## 18. Final Synthesis

Tabula presents a full-stack answer to one question:

> How do we prove stateful application correctness without paying machine-level proving tax that does not represent business semantics?

Its answer is architectural, not cosmetic:

1. model state explicitly as typed tables,
2. enforce canonical IR structure,
3. execute deterministically in a crypto-agnostic runtime,
4. commit state with column-aware strategies,
5. prove with chip-specialized AIR and explicit interaction buses,
6. protect boundaries with fail-closed contract metadata.

This is what makes Tabula coherent as a kernel, a proof system, and a developer-facing platform at the same time.

---

## 19. Normative Source Map

This narrative is aligned to the project’s primary design and specification sources:

1. `README.md` — thesis-level system framing and crate-level architecture.
2. `docs/thesis.md` — problem thesis and abstraction-boundary argument.
3. `docs/spec/semantics-spec.md` — normative IR semantics and NF invariants.
4. `docs/spec/proof-spec.md` — proof architecture, commitment design, and interaction model.
5. `docs/design/architecture.md` — workspace structure and cross-layer design intent.
6. `docs/design/final-target-architecture.md` — target-state ownership model and contract-first direction.

This document is intentionally not a replacement for those specs; it is the unifying narrative that makes them jointly readable as one system.

---

## 20. Implementation Anchor Map

The following files are the operational anchors for each architectural plane:

1. Semantics core:
   1. `crates/core/src/state/mod.rs`
   2. `crates/core/src/event.rs`
   3. `crates/ir/src/instruction.rs`
   4. `crates/ir/src/program.rs`
   5. `crates/ir/src/pass/validate.rs`
2. Language and lowering:
   1. `crates/lang/src/parser/mod.rs`
   2. `crates/lang/src/lower/mod.rs`
3. Execution:
   1. `crates/executor/src/interpreter.rs`
   2. `crates/executor/src/overlay.rs`
   3. `crates/executor/src/batch.rs`
   4. `crates/executor/src/consistency.rs`
4. Commitment:
   1. `crates/commitment/src/poseidon.rs`
   2. `crates/commitment/src/codec.rs`
   3. `crates/commitment/src/ssmc.rs`
   4. `crates/commitment/src/smt.rs`
   5. `crates/commitment/src/hybrid.rs`
5. Proof system:
   1. `crates/proof/src/witness/generator.rs`
   2. `crates/proof/src/trace_builder/builder.rs`
   3. `crates/proof/src/air/chips/mod.rs`
   4. `crates/proof/src/air/interaction.rs`
   5. `crates/proof/src/air/bus.rs`
   6. `crates/proof/src/stark/prover.rs`
   7. `crates/proof/src/stark/verifier.rs`
6. Compatibility spine:
   1. `crates/contract/src/lib.rs`
   2. `crates/driver/src/lib.rs`
   3. `crates/artifact/src/lib.rs`
7. Product adapters:
   1. `crates/cli/src/main.rs`
   2. `crates/daemon/src/service/engine.rs`
   3. `crates/web-ide/src/main.rs`
