# Tabula AI Context Note

> Status: AI handoff / research context note
> Audience: AI agents, future paper drafting, research planning
> Scope: high-context synthesis of Tabula's thesis, architecture, implementation status, paper framing, and follow-up research directions
> Authority: informative, not canonical

---

## 1. Why This Note Exists

This note is meant to give an AI agent enough context to reason about Tabula
without repeatedly re-deriving the project thesis from scattered design notes,
crate READMEs, and exploratory research documents.

This is **not** the canonical architecture document. It is a synthesis of:

- the current repository thesis,
- the current cross-crate architecture,
- the implemented execution/proving boundaries,
- the current paper framing,
- and the main follow-up research directions.

When this note disagrees with canonical architecture, prefer:

1. [`/Users/wonj/Projects/tabula/README.md`](/Users/wonj/Projects/tabula/README.md)
2. [`/Users/wonj/Projects/tabula/docs/design/architecture.md`](/Users/wonj/Projects/tabula/docs/design/architecture.md)
3. crate `README.md` files under [`/Users/wonj/Projects/tabula/crates`](/Users/wonj/Projects/tabula/crates)

---

## 2. One-Paragraph Thesis

Tabula is a zero-knowledge kernel for **typed, tabular state transitions**.
Instead of treating application logic as generic machine execution and then
proving a flattened execution trace, Tabula treats **structured application
state and typed state transitions themselves** as the thing to compile,
register, execute, reduce, and prove. The central idea is that many important
applications, especially app-specific zk rollups and other integrity-critical
state machines, do not naturally think in terms of flat VM memory. They think
in terms of **tables, columns, rows, typed fields, explicit reads and writes,
and schema-level meaning**. Tabula attempts to preserve that structure across
compilation, execution, runtime proof preparation, and backend proving.

---

## 3. Problem Diagnosis

### 3.1 The mismatch Tabula is trying to fix

The motivating problem is not simply "ZK is expensive."

The deeper problem is:

- many real applications are **structured state machines**
- but mainstream proving stacks are organized around **generic machine
  execution**

This creates an abstraction mismatch.

For many applications, the natural meaning is:

- accounts
- balances
- orders
- positions
- permissions
- ledgers
- registry entries
- typed state cells

These are closer to **typed relational/tabular state** than to:

- flat RAM
- generic ISA state
- general-purpose machine traces

### 3.2 What generic zkVM-style stacks do

Trace-first zkVM-style stacks typically:

- compile application logic to a machine model
- execute that machine model
- prove the resulting execution trace

This is powerful, but it tends to:

- flatten application semantics into machine steps
- turn application state into generic memory
- force proof-facing work to recover structure from low-level execution artifacts

Tabula's thesis is that for many structured applications, this is the wrong
abstraction boundary.

### 3.3 The alternative failure mode

If a team does not use a generic zkVM, they often build a custom proving stack.

That usually leads to another problem:

- semantics get duplicated across compiler, runtime, prover, verifier, and app
  code
- different layers become accidental semantic authorities
- the proof-facing contract becomes implicit

Tabula tries to avoid both extremes:

- not a generic trace-first machine
- not an ad hoc custom prover stack with blurred semantic ownership

---

## 4. What Tabula Is

The best short description is:

> Tabula is a compiler-sealed execution/proof substrate for structured
> zero-knowledge applications.

More concretely:

- it is **not** a generic zkVM
- it is **not** a general-purpose VM replacement
- it is **not** primarily a cryptographic primitive paper
- it is **not** primarily a "more secure than zkVM" system

It is a system whose central abstractions are:

- typed tabular state
- typed state transitions
- compiler-sealed registered semantics
- execution journals
- runtime-owned proof reduction
- column-local proof units

---

## 5. What Tabula Is Not

This is important because many wrong framings are tempting.

Tabula is **not**:

- a zkEVM replacement
- a general-purpose zkVM
- a paper whose core novelty is "we made a new cryptographic primitive"
- a paper whose main claim should be "we are more secure than zkVMs"
- a proof system for arbitrary smart contracts
- a paper whose strongest contribution is "we have an effect system"

Its strongest contributions are in:

- abstraction
- architecture
- semantic ownership
- execution/proof boundary design
- structured state modeling
- column-aware proof preparation

---

## 6. The Core Abstractions

### 6.1 Typed tabular state

The core application memory model is **not flat RAM**.

Instead, state lives in typed tables addressed by:

- `(table, column, row)`

This is fundamental. It is not just an implementation detail.

Tabula treats application state as:

- schema-aware
- typed
- field/profile aware
- naturally decomposable by column

Important implications:

- this is not transient scratch memory
- this is the application's real committed state
- proof preparation can stay aligned with the application's actual state shape

Canonical references:

- [`/Users/wonj/Projects/tabula/README.md`](/Users/wonj/Projects/tabula/README.md)
- [`/Users/wonj/Projects/tabula/docs/design/architecture.md`](/Users/wonj/Projects/tabula/docs/design/architecture.md)

### 6.2 Registered semantics

Programs are not just source text or a compiled executable blob.

Programs are compiled and then **registered** into a sealed semantic artifact.

This registered artifact is intended to fix:

- program meaning
- state schemas
- profile bindings
- field scheme bindings
- capability manifests
- static table artifacts
- metadata envelopes
- semantic/binding digests

The compiler is the semantic authority that creates this.

Key implementation:

- [`/Users/wonj/Projects/tabula/crates/compiler/src/registration/register.rs`](/Users/wonj/Projects/tabula/crates/compiler/src/registration/register.rs)

### 6.3 Execution journals

Execution does not just produce "a batch result."

The long-term architectural direction is that execution should produce a typed,
canonical semantic journal.

That journal is the internal source of truth for proving.

Important distinction:

- `BatchReport` is boundary/reporting shaped
- `ExecutionJournal` is semantic truth for proving
- `ProofJournal` is runtime-prepared proof input
- `ProofArtifacts` are backend-facing machine inputs

Canonical design docs:

- [`/Users/wonj/Projects/tabula/docs/notes/proof-front-end-journal-architecture.md`](/Users/wonj/Projects/tabula/docs/notes/proof-front-end-journal-architecture.md)
- [`/Users/wonj/Projects/tabula/docs/notes/executor-proof-codesign-architecture.md`](/Users/wonj/Projects/tabula/docs/notes/executor-proof-codesign-architecture.md)

### 6.4 Column-local proof units

The system is designed to preserve column identity into the proving pipeline.

This enables:

- per-column specialization
- width-aware encodings
- touched/untouched routing
- scheme-aware per-column preparation
- future grouping without giving up logical sharding

Column-locality is a core part of the proving architecture, not just an
optimization afterthought.

---

## 7. Canonical Cross-Crate Architecture

The current canonical architecture is:

```text
Shared Meaning
  tabula-core
  tabula-contract

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
  tabula-cli
```

Canonical source:

- [`/Users/wonj/Projects/tabula/docs/design/architecture.md`](/Users/wonj/Projects/tabula/docs/design/architecture.md)

### 7.1 Authority split

This split matters more than exact API names.

#### Compiler authority

`tabula-compiler` is the semantic registration authority.

It decides what the program means downstream.

#### Executor authority

`tabula-executor` owns deterministic execution semantics.

It does not own proof grouping or proving policy.

#### Runtime authority

`tabula-runtime` owns:

- runtime policy
- execution/proof contract resolution
- proof preparation
- statement binding
- integration above execution and below application surfaces

#### Machine authority

`tabula-machine` owns backend proof assembly and verification **after** higher
layers have already decided what should be proved.

It is not a semantic authority.

---

## 8. Crate-Level Mental Model

This section is intentionally brief. For authoritative crate-local contracts,
see each crate `README.md`.

### Shared meaning

- [`/Users/wonj/Projects/tabula/crates/core/README.md`](/Users/wonj/Projects/tabula/crates/core/README.md)
  - shared vocabulary, low-level traits, execution/result model
- [`/Users/wonj/Projects/tabula/crates/contract/README.md`](/Users/wonj/Projects/tabula/crates/contract/README.md)
  - fail-closed trust contract layer, proof-visible schemas, compatibility rules

### Authoring and registration

- [`/Users/wonj/Projects/tabula/crates/lang/README.md`](/Users/wonj/Projects/tabula/crates/lang/README.md)
  - source-facing DSL parsing/lowering
- [`/Users/wonj/Projects/tabula/crates/compiler/README.md`](/Users/wonj/Projects/tabula/crates/compiler/README.md)
  - semantic authority, sealed artifacts, registration

### Execution and runtime

- [`/Users/wonj/Projects/tabula/crates/executor/README.md`](/Users/wonj/Projects/tabula/crates/executor/README.md)
  - deterministic execution engine
- [`/Users/wonj/Projects/tabula/crates/runtime/README.md`](/Users/wonj/Projects/tabula/crates/runtime/README.md)
  - policy/orchestration between registered semantics and proving

### Proof backend

- [`/Users/wonj/Projects/tabula/crates/commitment/README.md`](/Users/wonj/Projects/tabula/crates/commitment/README.md)
  - native commitment semantics
- [`/Users/wonj/Projects/tabula/crates/witness/README.md`](/Users/wonj/Projects/tabula/crates/witness/README.md)
  - logical proof input preparation seam
- [`/Users/wonj/Projects/tabula/crates/gadgets/README.md`](/Users/wonj/Projects/tabula/crates/gadgets/README.md)
  - reusable proof gadgets
- [`/Users/wonj/Projects/tabula/crates/chips/README.md`](/Users/wonj/Projects/tabula/crates/chips/README.md)
  - concrete AIR chips
- [`/Users/wonj/Projects/tabula/crates/stark/README.md`](/Users/wonj/Projects/tabula/crates/stark/README.md)
  - chip-independent proving infrastructure
- [`/Users/wonj/Projects/tabula/crates/machine/README.md`](/Users/wonj/Projects/tabula/crates/machine/README.md)
  - backend proof assembly and verification over prepared inputs

### Surfaces

- `tabula-ext`: extension authoring seam
- `tabula-sdk`: application-facing package surface
- `tabula-cli`: developer/product CLI

---

## 9. The Compiler Story

Tabula's compiler matters a lot, but for the current main paper it is not the
entire story by itself.

The compiler's role is:

- establish semantics once
- validate structure once
- derive semantic summaries once
- register a sealed artifact once

The downstream runtime/prover should then consume that artifact, not reinterpret
the program.

### 9.1 Current compiler pipeline

At a high level:

```text
source
  -> parsing / lowering
  -> HIR-ish structure
  -> MIR
  -> analysis / canonicalization
  -> validated program
  -> registered program
```

Important files:

- [`/Users/wonj/Projects/tabula/crates/compiler/src/pipeline/compile.rs`](/Users/wonj/Projects/tabula/crates/compiler/src/pipeline/compile.rs)
- [`/Users/wonj/Projects/tabula/crates/compiler/src/registration/register.rs`](/Users/wonj/Projects/tabula/crates/compiler/src/registration/register.rs)

### 9.2 MIR / canonical IR direction

The compiler is intentionally moving toward:

- structured control
- SSA-disciplined values
- explicit effects
- proof-friendly normalization

Important nuance:

> Tabula is SSA-disciplined for **values**, not globally normalized to one
> read/write per state cell.

This distinction matters.

The current system supports:

- single-assignment locals / value discipline
- effectful state ops remaining explicit in IR

Relevant files:

- [`/Users/wonj/Projects/tabula/crates/compiler/src/mir/model.rs`](/Users/wonj/Projects/tabula/crates/compiler/src/mir/model.rs)
- [`/Users/wonj/Projects/tabula/crates/compiler/src/mir/validate.rs`](/Users/wonj/Projects/tabula/crates/compiler/src/mir/validate.rs)
- [`/Users/wonj/Projects/tabula/crates/compiler/src/mir/lower.rs`](/Users/wonj/Projects/tabula/crates/compiler/src/mir/lower.rs)
- [`/Users/wonj/Projects/tabula/docs/notes/program-redesign/program-mir-design.md`](/Users/wonj/Projects/tabula/docs/notes/program-redesign/program-mir-design.md)
- [`/Users/wonj/Projects/tabula/docs/notes/program-redesign/program-canonical-ir-design.md`](/Users/wonj/Projects/tabula/docs/notes/program-redesign/program-canonical-ir-design.md)

### 9.3 Implemented MIR analyses

Current MIR analysis computes summaries such as:

- world effects
- proof effects
- failure summaries
- policy summaries
- context-demand summaries
- call graph

Important file:

- [`/Users/wonj/Projects/tabula/crates/compiler/src/mir/analysis/summaries.rs`](/Users/wonj/Projects/tabula/crates/compiler/src/mir/analysis/summaries.rs)

This is valuable because it means the compiler already reasons about:

- state read/write/delete
- proof-observable operations like relation use / property read / capability call
- may-fail behavior
- query legality
- context field demands

Important caveat:

These analyses are strong support for the Tabula architecture, but by themselves
they are not yet the strongest standalone PL paper thesis.

### 9.4 Semantic registration

Semantic registration is one of Tabula's strongest current compiler ideas.

Registration currently seals:

- validated IR
- field schemes
- table schemas
- profile catalog
- tuple encoding defaults
- capability manifest
- static table artifact
- metadata envelope
- binding

This is the point where compiler-derived semantics become durable downstream
inputs.

Important file:

- [`/Users/wonj/Projects/tabula/crates/compiler/src/registration/register.rs`](/Users/wonj/Projects/tabula/crates/compiler/src/registration/register.rs)

---

## 10. Static Semantics: Current State vs Future Ambition

Tabula has a meaningful static-semantics story, but it is important to separate
what is already part of the implemented architecture from what remains a
research direction.

### 10.1 Current implemented static thesis

The current implemented direction is:

- value typing
- effect typing
- failure-sensitive reasoning
- proof-observable semantic effects
- context-demand tracking

The important design point is:

> Tabula distinguishes world effects, proof-observable effects, and failure
> behavior.

Reference:

- [`/Users/wonj/Projects/tabula/docs/notes/program-redesign/program-typing-and-effect-system.md`](/Users/wonj/Projects/tabula/docs/notes/program-redesign/program-typing-and-effect-system.md)

### 10.2 Exploratory static-semantics research

Exploratory directions include:

- coeffects / context-demand tracking as a stronger formal system
- obligation summaries richer than a single fail bit
- footprint-indexed effects
- bounded summaries
- tighter effect/capability integration

Reference:

- [`/Users/wonj/Projects/tabula/docs/notes/program-redesign/program-static-semantics-research-directions.md`](/Users/wonj/Projects/tabula/docs/notes/program-redesign/program-static-semantics-research-directions.md)

This material is useful future research context, but it should not be confused
with the main current paper thesis.

---

## 11. Execution Model

`tabula-executor` owns deterministic execution.

The execution model is based on:

- deterministic tx/batch execution
- transactional overlay semantics
- explicit state effects
- capability/property hooks at the execution boundary

Key reference:

- [`/Users/wonj/Projects/tabula/crates/executor/README.md`](/Users/wonj/Projects/tabula/crates/executor/README.md)

### 11.1 Overlay semantics

One key implementation detail is that execution uses an overlay state model that
already does some proof-helpful normalization:

- read deduplication
- final write coalescing

This is **not** the same as compiler-proven one-read/one-write normalization.

Relevant files:

- [`/Users/wonj/Projects/tabula/crates/executor/src/state/overlay.rs`](/Users/wonj/Projects/tabula/crates/executor/src/state/overlay.rs)
- [`/Users/wonj/Projects/tabula/crates/executor/src/state/execution_state.rs`](/Users/wonj/Projects/tabula/crates/executor/src/state/execution_state.rs)

### 11.2 What execution records

Execution records:

- ordered access events / state effects
- old read summaries
- final write summaries

The important nuance is that Tabula currently preserves both:

- **ordered semantic access history**
- **summary views useful for proof preparation**

Relevant files:

- [`/Users/wonj/Projects/tabula/crates/executor/src/surface/journal.rs`](/Users/wonj/Projects/tabula/crates/executor/src/surface/journal.rs)
- [`/Users/wonj/Projects/tabula/crates/executor/src/machine/ops/state.rs`](/Users/wonj/Projects/tabula/crates/executor/src/machine/ops/state.rs)

---

## 12. Runtime and Proof Preparation

`tabula-runtime` is the policy and orchestration layer.

It is the first layer that knows both:

- sealed semantics
- proof backend configuration

This makes runtime the natural owner of:

- resolved execution/proof contracts
- proof planning
- reduction from execution journal to proof-facing inputs

### 12.1 Key runtime claim

Runtime should own:

- what must be proved
- how semantic execution results are reduced into proof-facing units

Backend layers should own:

- how prepared inputs become traces and proofs

### 12.2 Execution journal -> proof journal

The intended dataflow is:

```text
execution
  -> ExecutionJournal
  -> runtime-owned reduction
  -> ProofJournal
  -> prepared proof artifacts
  -> machine
```

This is one of the most important Tabula ideas.

Reference:

- [`/Users/wonj/Projects/tabula/docs/notes/proof-front-end-journal-architecture.md`](/Users/wonj/Projects/tabula/docs/notes/proof-front-end-journal-architecture.md)
- [`/Users/wonj/Projects/tabula/docs/notes/executor-proof-codesign-architecture.md`](/Users/wonj/Projects/tabula/docs/notes/executor-proof-codesign-architecture.md)

### 12.3 Current proof-facing semantic decomposition

A useful current mental model is that the proof-facing semantic witness is
decomposed into:

- prior reads / old-read set
- ordered access events
- final writes

This is the strongest current candidate for Tabula's semantic witness model.

It is more precise to call this:

- semantic witness decomposition

than to call it "theoretically minimal" at the current stage.

Runtime code path:

- [`/Users/wonj/Projects/tabula/crates/runtime/src/engine.rs`](/Users/wonj/Projects/tabula/crates/runtime/src/engine.rs)
- [`/Users/wonj/Projects/tabula/crates/runtime/src/semantics.rs`](/Users/wonj/Projects/tabula/crates/runtime/src/semantics.rs)

### 12.4 Important query limitation

Query execution exists on the rewritten/native path.

Query proving remains intentionally absent in the current runtime contract.

This is relevant when framing current capabilities in papers.

---

## 13. Proof Backend

The backend is intentionally layered.

### 13.1 Native commitment semantics

`tabula-commitment` is the native authority on commitment meaning.

The proof stack mirrors these semantics rather than inventing new ones.

### 13.2 Witness seam

`tabula-witness` is the logical proof-input preparation seam.

Its purpose is not to own high-level runtime policy. It should prepare
deterministic typed proof inputs.

### 13.3 AIR chips and proving foundation

- `tabula-gadgets`: reusable gadgets
- `tabula-chips`: concrete AIR chips
- `tabula-stark`: chip-independent STARK/RAP foundation

### 13.4 Machine

`tabula-machine` consumes prepared inputs and performs:

- backend setup
- trace construction
- proof generation
- proof verification

Machine is deliberately **not** a semantic authority.

Important file:

- [`/Users/wonj/Projects/tabula/crates/machine/README.md`](/Users/wonj/Projects/tabula/crates/machine/README.md)

---

## 14. The Current Proof Architecture

The current machine architecture is best understood as:

- 1 execution-tier proof
- C logical column proofs
- 1 root-tier proof

In short:

```text
1 execution + C column + 1 root
```

This is often referred to as `C+2`.

Important files:

- [`/Users/wonj/Projects/tabula/crates/machine/src/lib.rs`](/Users/wonj/Projects/tabula/crates/machine/src/lib.rs)
- [`/Users/wonj/Projects/tabula/crates/machine/src/machine.rs`](/Users/wonj/Projects/tabula/crates/machine/src/machine.rs)

### 14.1 Logical sharding vs proof grouping

One of the most important distinctions in the current design work is:

- logical column-local specialization
- proof artifact grouping
- whole-batch statement structure

These are related but not the same.

The intended design claim is:

> Tabula should preserve column-local specialization even if multiple logical
> columns are packaged into one grouped proof artifact.

Reference:

- [`/Users/wonj/Projects/tabula/docs/notes/proof-hierarchy-and-grouping.md`](/Users/wonj/Projects/tabula/docs/notes/proof-hierarchy-and-grouping.md)

### 14.2 What already exists today

The current codebase already has substantial column-locality:

- per-column prepared slots
- width-aware backend preparation
- touched-column-aware semantics
- per-column traces/proofs in the current machine
- multi-proof parallelism

Important files:

- [`/Users/wonj/Projects/tabula/crates/ext/src/backend/column.rs`](/Users/wonj/Projects/tabula/crates/ext/src/backend/column.rs)
- [`/Users/wonj/Projects/tabula/crates/runtime/src/host/builtins/ssmc.rs`](/Users/wonj/Projects/tabula/crates/runtime/src/host/builtins/ssmc.rs)
- [`/Users/wonj/Projects/tabula/crates/runtime/src/host/builtins/smt.rs`](/Users/wonj/Projects/tabula/crates/runtime/src/host/builtins/smt.rs)
- [`/Users/wonj/Projects/tabula/crates/witness/src/stark/schemes/ssmc.rs`](/Users/wonj/Projects/tabula/crates/witness/src/stark/schemes/ssmc.rs)
- [`/Users/wonj/Projects/tabula/crates/witness/src/stark/schemes/smt.rs`](/Users/wonj/Projects/tabula/crates/witness/src/stark/schemes/smt.rs)

### 14.3 Important nuance about untouched columns

The design notes often describe a future where untouched columns can be skipped
entirely.

Current implementation nuance:

- touched/untouched semantics are real
- but artifact-level elimination is not yet the final fully optimized state

In other words:

- touched-column semantics are already part of the proof model
- full artifact skipping is a future optimization frontier

Relevant files:

- [`/Users/wonj/Projects/tabula/crates/chips/src/shards/meta/air.rs`](/Users/wonj/Projects/tabula/crates/chips/src/shards/meta/air.rs)
- [`/Users/wonj/Projects/tabula/crates/chips/src/shards/state/air.rs`](/Users/wonj/Projects/tabula/crates/chips/src/shards/state/air.rs)

---

## 15. Extension and Product Surfaces

There are two important package-facing surfaces above the core architecture.

### 15.1 `tabula-ext`

This is the extension authoring surface for:

- custom schemes
- custom semantic capabilities

### 15.2 `tabula-sdk`

This is the intended application-facing integration surface.

This is important because `tabula-runtime` is lower-level and expert-oriented.

### 15.3 `tabula-cli`

The CLI is a repo-owned consumer of the canonical surfaces. It is not a new
semantic authority.

CLI supports:

- check / compile / schema
- execute / prove / verify
- state inspection
- examples

Reference:

- [`/Users/wonj/Projects/tabula/crates/cli/README.md`](/Users/wonj/Projects/tabula/crates/cli/README.md)

---

## 16. Current Implementation Status

The best current status summary is:

> Tabula is not production-finished, but it is architecturally mature enough as
> a research kernel to support strong systems and research claims.

### 16.1 Stable enough to treat as real

These are stable enough to treat as the current main architecture:

- compiler -> registration -> runtime -> machine boundary
- semantic authority at compile time
- typed tabular state as the state model
- runtime-owned proof preparation
- column-local proof units
- `C+2` machine architecture

### 16.2 Still moving / frontier areas

These are still more frontier than fixed:

- property-read proving scope
- capability proving richness
- grouping strategy / grouped proof packaging
- stronger state-surface budgets
- richer query/proving story
- symbolic AIR compilation
- more aggressive compile-time proof planning

---

## 17. The Current Best Paper Framing

The best current framing is **not**:

- "Tabula is more secure than zkVMs"
- "Tabula is a new cryptographic primitive"

The strongest framing is:

> Tabula is a compiler-sealed execution/proof substrate for structured
> zero-knowledge applications, especially app-specific zk rollups.

### 17.1 Best thesis for the current main paper

A good thesis sentence is:

> Many important zero-knowledge applications are structured state machines, but
> current proving stacks are organized around generic execution traces. Tabula
> instead organizes execution and proving around compiler-sealed typed state
> transitions.

### 17.2 Best venue fit

At the time of writing, the most natural fit is:

- **EuroSys** or **OOPSLA**

Why:

- the strongest contributions are architecture/substrate/compiler-runtime-prover
  boundary design
- not a new crypto primitive
- not a new security property

CCS is a possible stretch fit, but it is less natural unless Tabula eventually
acquires a stronger formal metric/tradeoff/theory hook.

---

## 18. Best Current Main-Paper Contributions

For the current main paper, the strongest contribution set is:

### 18.1 Typed tabular state as the proving memory model

Tabula does not treat application state as generic machine memory.

It models state directly as typed `(table, column, row)` committed state.

### 18.2 Compiler-sealed semantic registration

Program meaning is sealed once and carried downstream.

The compiler fixes:

- semantics
- profiles
- bindings
- proof-visible metadata

### 18.3 Structured execution-to-proof lowering

Proof preparation does not rediscover meaning from a generic execution trace.

Instead, runtime reduces structured execution facts into proof-facing inputs.

### 18.4 Column-local proof architecture

The proof model preserves column identity deep into the proving stack and uses
column-local proof units rather than one monolithic universal proving unit.

---

## 19. Current Best Claims

These are the most defensible current claims.

### Good claims

- Tabula is a better-matched proving abstraction for structured applications.
- Tabula preserves application state structure across compilation, execution,
  and proof preparation.
- Tabula makes the execution/proof contract more explicit.
- Tabula enables more direct specialization for structured workloads.
- Tabula is organized around application state transitions rather than generic
  machine traces.

### Bad claims

- Tabula is more secure than zkVMs.
- Tabula universally outperforms generic zkVMs on all workloads.
- Tabula is a new cryptographic primitive.
- Tabula already fully solves optimal proving for structured applications.

---

## 20. The Formal Hooks Most Worth Strengthening

Tabula does not need a giant theorem to be valuable, but it does benefit from
some formal structure.

The two most important formal hooks are:

### 20.1 Execution/proof contract

This means making explicit:

- what the compiler seals
- what execution records
- what runtime reduces
- what backend consumes

This is the most important formal spine of the system.

### 20.2 Semantic witness decomposition

This means making explicit the proof-facing semantic units currently used in the
system, such as:

- prior reads
- ordered accesses
- final writes

This should be described as:

- semantic witness decomposition

rather than prematurely claiming full optimality/minimality.

---

## 21. What Should Stay Out of the Current Main Paper

These are promising directions, but they should not dominate the current
systems/substrate paper.

### 21.1 Symbolic access planning

This is compiler-heavy and can become a different paper thesis.

### 21.2 Cross-batch caching / reuse

This is a useful optimization direction but not central to the current
architectural identity.

### 21.3 Richer static semantics as main novelty

The effect/context/obligation story is useful and strong, but not the main paper
center.

### 21.4 Full proof grouping optimization theory

Grouping should appear as a design-space axis, not the central current thesis.

---

## 22. Follow-Up Research Directions

The strongest future research directions are listed here in priority order of
coherence, not necessarily implementation readiness.

### 22.1 Symbolic AIR compilation

This is the strongest candidate for a follow-up PL/compiler paper.

Core idea:

- perform symbolic execution over closed-world typed programs
- collapse instruction-level execution into direct algebraic relations
- compile per-tx-type specialized AIR/chips
- use degree-aware materialization and compiled AIR generation

Why it matters:

- it is not just "symbolic AIR" in the generic sense
- it is a **closed-world proof compilation** thesis
- it could become a real compiler paper

Important research note:

- [`/Users/wonj/Projects/tabula/docs/research/symbolic-air-compilation.md`](/Users/wonj/Projects/tabula/docs/research/symbolic-air-compilation.md)

### 22.2 Compiler optimization / static proof planning

This direction includes:

- ahead-of-time proof planning
- specialized lowering plans
- access planning
- partial-evaluation-like pipeline optimizations

Important note:

- [`/Users/wonj/Projects/tabula/docs/research/compiler-optimization-research.md`](/Users/wonj/Projects/tabula/docs/research/compiler-optimization-research.md)

This is promising, but weaker as a standalone thesis than symbolic AIR
compilation unless made much sharper.

### 22.3 Richer static semantics

This direction includes:

- coeffects
- obligations
- footprint-indexed effects
- stronger closed-world proof-aware static semantics

Important note:

- [`/Users/wonj/Projects/tabula/docs/notes/program-redesign/program-static-semantics-research-directions.md`](/Users/wonj/Projects/tabula/docs/notes/program-redesign/program-static-semantics-research-directions.md)

This is valuable, but likely a later paper rather than the most immediate
follow-up.

### 22.4 Grouping / packaging economics

This direction focuses on:

- full sharding vs grouped vs monolithic
- fixed proof artifact costs
- grouping cost models

This is a strong future systems/proving optimization topic.

### 22.5 Cross-batch reuse / caching

Examples:

- untouched column reuse
- read-mostly column reuse
- amortized proving state reuse

This is attractive but currently more future optimization than present thesis.

---

## 23. What Symbolic AIR Means Here

This section exists because "symbolic AIR" is an overloaded phrase.

### 23.1 What is *not* enough

Just saying:

- "we use symbolic constraints"
- "we use SymbolicAirBuilder"

is not enough for a standalone paper.

Tabula already uses symbolic AIR infrastructure in the backend stack:

- [`/Users/wonj/Projects/tabula/crates/machine/src/backend/rap.rs`](/Users/wonj/Projects/tabula/crates/machine/src/backend/rap.rs)
- [`/Users/wonj/Projects/tabula/crates/machine/src/proof/chip_ref.rs`](/Users/wonj/Projects/tabula/crates/machine/src/proof/chip_ref.rs)
- [`/Users/wonj/Projects/tabula/crates/machine/src/proof/instance.rs`](/Users/wonj/Projects/tabula/crates/machine/src/proof/instance.rs)

That by itself is not the research novelty.

### 23.2 What *would* be strong enough

What is strong is:

- closed-world program-level symbolic execution
- symbolic relation DAG extraction
- degree-aware materialization
- compiled per-program / per-tx-type AIR generation
- universal execution chip fallback

That is why a future paper should be framed as something like:

- compiling typed state-machine programs to specialized AIR
- closed-world proof compilation for typed state-machine DSLs

not just "symbolic AIR."

---

## 24. Current Nuances and Caveats AI Should Preserve

These are easy to get wrong. They should be preserved in any serious synthesis.

### 24.1 SSA nuance

Tabula is SSA-disciplined for values.

It is **not** currently a system where compiler proof-normalizes all state
reads/writes to one access per cell.

### 24.2 Query nuance

Query execution exists.

Query proving is intentionally absent in the current runtime contract.

### 24.3 Grouping nuance

Grouping is an important design axis, but current strong implemented value lies
in logical column-local specialization, not yet in a final optimal grouping
theory.

### 24.4 Untouched-column nuance

Touched/untouched semantics exist today.

Full artifact-level untouched-column elimination is still a future optimization
frontier.

### 24.5 Paper-framing nuance

The strongest current paper is not:

- crypto-primitive-first
- security-improvement-first

It is:

- architecture-first
- substrate-first
- structured proving abstraction-first

---

## 25. Best Short Descriptions for Reuse

These can be reused in AI prompting or paper drafting.

### 25.1 Very short

Tabula is a compiler-sealed proving substrate for typed tabular state
transitions.

### 25.2 Systems paper

Tabula is an execution/proof substrate for structured state-machine
applications, especially app-specific zk rollups, that preserves typed state
structure across compilation, execution, and proof preparation.

### 25.3 Contrast with zkVM

Where trace-first zkVMs organize proving around generic machine execution,
Tabula organizes execution and proving around compiler-sealed typed application
state transitions.

### 25.4 Follow-up compiler paper

Tabula's most promising follow-up compiler direction is closed-world proof
compilation from typed state-machine programs to specialized AIR.

---

## 26. If You Need To Reconstruct the Project Quickly

Read in this order:

1. [`/Users/wonj/Projects/tabula/README.md`](/Users/wonj/Projects/tabula/README.md)
2. [`/Users/wonj/Projects/tabula/docs/design/architecture.md`](/Users/wonj/Projects/tabula/docs/design/architecture.md)
3. crate `README.md` files under [`/Users/wonj/Projects/tabula/crates`](/Users/wonj/Projects/tabula/crates)
4. [`/Users/wonj/Projects/tabula/docs/notes/proof-front-end-journal-architecture.md`](/Users/wonj/Projects/tabula/docs/notes/proof-front-end-journal-architecture.md)
5. [`/Users/wonj/Projects/tabula/docs/notes/executor-proof-codesign-architecture.md`](/Users/wonj/Projects/tabula/docs/notes/executor-proof-codesign-architecture.md)
6. [`/Users/wonj/Projects/tabula/docs/notes/proof-hierarchy-and-grouping.md`](/Users/wonj/Projects/tabula/docs/notes/proof-hierarchy-and-grouping.md)
7. only then read exploratory research notes such as:
   - [`/Users/wonj/Projects/tabula/docs/research/symbolic-air-compilation.md`](/Users/wonj/Projects/tabula/docs/research/symbolic-air-compilation.md)
   - [`/Users/wonj/Projects/tabula/docs/research/compiler-optimization-research.md`](/Users/wonj/Projects/tabula/docs/research/compiler-optimization-research.md)

---

## 27. Final Summary

Tabula should be understood as a project about **better matching the proving
abstraction to the application abstraction**.

Its core ideas are:

- application state should be modeled as typed tables, not generic RAM
- program meaning should be sealed once by the compiler
- execution should emit canonical semantic effects
- runtime should reduce those effects into proof-facing units
- backend proving should consume prepared inputs, not rediscover semantics
- logical column-local specialization should remain visible deep into the proof stack

The current strongest paper is therefore a systems/substrate paper.

The strongest follow-up paper is likely a compiler paper centered on symbolic
AIR compilation / closed-world proof compilation.
