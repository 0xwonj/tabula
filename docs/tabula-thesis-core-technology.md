# Tabula: Thesis-Centered Technical Narrative

## Abstract

Tabula is a zero-knowledge kernel for typed, tabular state transitions. Its central claim is simple:

> For stateful applications, proving machine execution is the wrong default abstraction.  
> We should prove state transitions directly, with data models and proof systems designed around persistent state semantics.

This document explains that claim end-to-end, focusing on core technical ideas and why they are better than machine-centric proving for state-heavy workloads.

---

## 1. The Problem Tabula Solves

### 1.1 The mismatch in machine-centric proving

General-purpose zkVMs prove low-level execution traces. That is powerful, but for stateful systems it creates structural overhead:

1. A single logical state read/write expands into many ISA-level instructions.
2. Memory-consistency arguments cover stack, heap, and temporaries, not just persistent state.
3. Type information is flattened into byte memory, reducing proof specialization opportunities.
4. State root updates are paid through the same machine-execution abstraction.

Result: proving cost grows with "how the machine executes," not with "what state changed."

### 1.2 What stateful applications actually need

Most stateful applications only need to prove:

1. Reads were valid against committed pre-state.
2. Transition logic was applied correctly.
3. Writes were consistent and ordered.
4. Post-state commitment is exactly the result of those writes.

That is a state-transition statement, not a machine-execution statement.

---

## 2. Core Thesis

Tabula moves the proof boundary from ISA execution to schema-typed state transitions.

Concretely:

1. State is explicit as `(table, column, row) -> value`.
2. Local computation uses SSA slots rather than mutable RAM.
3. Program structure enforces normal-form constraints that remove intra-transaction memory ambiguity.
4. Commitments and proof constraints are co-designed for typed columns.

The consequence is the key complexity goal:

> Memory-consistency cost should scale with persistent state accesses, not total machine operations.

---

## 3. Semantic Model as a First-Class Proof Object

### 3.1 Typed tabular state

A state cell is addressed by:

1. `TableId`
2. `ColId`
3. `RowKey`

Combined key: `CellKey { table, col, row }`.

This key model is not cosmetic. It gives proof-level structure:

1. Accesses are naturally partitioned by `(table, col)`.
2. Column-local commitment and proof routing becomes possible.
3. Typed column schemas drive encoding and constraint specialization.

### 3.2 Null/absence semantics

Tabula models absence explicitly:

1. Reads produce `(value, is_null)`.
2. Writes consume `(value, is_null)`.
3. Null payloads are canonicalized.

This prevents hidden witness degrees of freedom and stabilizes proof semantics around absence.

### 3.3 Deterministic transaction and batch semantics

Transactions execute against committed state snapshots and produce deterministic effects; batches compose these effects in strict order. The proof statement is therefore naturally a root transition statement:

1. from `oldRoot`,
2. through semantically valid reads/writes,
3. to `newRoot`.

---

## 4. IR Discipline: Why Structure Matters

### 4.1 True SSA as proof discipline

Each destination slot is assigned once. This removes local mutable-memory ambiguity and turns locals into trace-level wires rather than RAM cells.

Why this is good:

1. Less memory-order burden from local computation.
2. Cleaner operand-to-result dependency constraints.
3. More predictable trace shape.

### 4.2 Normal Form (NF)

Tabula enforces four structural constraints per transaction:

1. NF-1: at most one read per `(table, col, row)`.
2. NF-2: at most one write per `(table, col, row)`.
3. NF-3: no read-after-write for same key.
4. NF-4: key aliasing must be statically resolvable.

Why this is good:

1. Intra-transaction RAM consistency collapses structurally.
2. Write ambiguity/coalescing complexity is eliminated at semantic level.
3. Proof logic focuses on inter-transaction consistency, where real state interaction lives.

NF is a major source of proof simplification, not a compiler nicety.

---

## 5. Separation of Concerns: Execution, Commitment, Proving

Tabula conceptually decomposes the flow into three semantic stages:

1. Execution: deterministic state transition evaluation.
2. Commitment: cryptographic state commitment updates.
3. Proving: AIR/STARK constraints over execution and commitment evidence.

Why this is good:

1. Execution semantics remain analyzable independently from cryptography.
2. Commitment logic can be optimized without redefining language semantics.
3. Proof layer can specialize constraints without contaminating runtime semantics.

The split preserves semantic clarity and avoids coupling every optimization to every layer.

---

## 6. Commitment Strategy: Column-Aware Hybrid Design

Tabula uses hybrid per-column commitment routing:

1. SSMC-style path for small/sparse columns.
2. SMT path for larger domains.

Why this is good:

1. Different column distributions get cost-appropriate treatment.
2. Small columns avoid unnecessary Merkle-path overhead.
3. Large sparse columns retain scalable path-based proofs.
4. One uniform root-transition statement is preserved at the top level.

This avoids the "one commitment strategy must fit all state" inefficiency.

---

## 7. Proof Composition: Specialized Chips + Explicit Buses

Tabula proof logic is decomposed by semantic role (execution, ordering, state transition, hash, range, static lookup, SMT paths), and linked through explicit interaction buses.

Why this is good:

1. Constraint sets stay role-local and conceptually clean.
2. Cross-domain consistency is explicit instead of implicit.
3. Verification is compositional: per-chip validity plus global interaction balance.

This is a principled way to scale proof complexity without collapsing everything into one monolith.

---

## 8. What Is Actually Proven

For an `ApplyBatch`-style statement, Tabula proves:

1. Instruction semantics are respected for executed transactions.
2. Read values are consistent with committed pre-state or prior in-batch writes under ordering rules.
3. Writes are semantically valid and correctly coalesced as final effects.
4. Commitment transition from `oldRoot` to `newRoot` is valid for exactly those effects.

This is a semantic correctness guarantee for state transition, not merely execution replay evidence.

---

## 9. Why This Is Better (Thesis Value)

### 9.1 Better complexity alignment

Machine-centric proving pays for machine details. Tabula pays for state semantics. For stateful workloads, that alignment is the dominant advantage.

### 9.2 Better semantic transparency

The proof object matches application meaning:

1. typed state cells,
2. explicit reads/writes,
3. explicit root transition.

This makes correctness claims easier to reason about at protocol level.

### 9.3 Better specialization leverage

Typed columns, NF structure, and hybrid commitments allow targeted optimization that flat-memory machine traces cannot express cleanly.

### 9.4 Better scalability path for state-heavy systems

State-heavy workloads benefit when proof effort tracks touched state, not machine control/temporary behavior.

---

## 10. Tradeoffs and Scope Boundaries

Tabula is not claiming to replace universal zkVMs. It is a domain-focused thesis:

1. Strength: stateful transition proving with typed semantic structure.
2. Tradeoff: less about universal machine-program expressiveness.

This is a deliberate choice: optimize the abstraction boundary for stateful systems rather than optimize a general machine model for all cases.

---

## 11. Canonical Example: Balance Transfer, Semantically

A transfer transaction is semantically:

1. read sender balance,
2. read receiver balance,
3. assert sender has enough funds,
4. compute two new balances,
5. write sender and receiver balances.

In Tabula’s thesis model:

1. the proof binds reads to committed state,
2. binds arithmetic and predicate logic to instruction semantics,
3. binds writes to final commitment updates,
4. binds the entire batch to a root transition guarantee.

The verifier learns exactly what matters: the state transition is correct.

---

## 12. Final Synthesis

Tabula’s contribution is not "a faster VM implementation detail."  
It is a different proving philosophy:

1. model persistent state explicitly and typefully,
2. enforce structural IR invariants that remove avoidable ambiguity,
3. commit state with column-aware strategies,
4. compose proofs with role-specialized constraints and explicit interactions,
5. verify a semantically meaningful root transition.

If a system is fundamentally stateful, this is the natural proof boundary.

That is why Tabula is good: it reduces abstraction mismatch, aligns cost with semantics, and turns state-transition correctness into the primary cryptographic object.
