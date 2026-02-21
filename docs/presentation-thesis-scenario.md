# Tabula Thesis-First Presentation Scenario

Audience: research, protocol, and technical leadership  
Style: thesis-driven, core-technology focused, minimal implementation detail  
Length: ~25 minutes

---

## 0. Opening Framing (30 sec)

Use this sentence to set the frame:

> "This talk is not about product architecture or implementation plumbing.  
> It is about one thesis: for stateful ZK systems, we should prove state transitions directly, not machine execution indirectly."

---

## 1. Slide-by-Slide Scenario

## Slide 1 — Title: "From zkVM Overhead to State-Native Proofs" (1 min)

Core message:
1. The problem is structural, not incremental.
2. Tabula changes the proof abstraction boundary.

Say:
1. "Most systems still prove machine behavior and only indirectly prove state transitions."
2. "Tabula asks: what if the state transition itself is the first-class proof object?"

---

## Slide 2 — Problem: Why machine-centric proving is wasteful for stateful apps (2 min)

Core message:
1. Stateful applications mostly care about persistent reads/writes.
2. zkVM traces pay for much more than that.

Say:
1. "A single logical state read becomes many ISA-level operations."
2. "Memory consistency covers stack and temporaries, not just persistent state."
3. "Typed semantics are flattened into untyped memory."
4. "So proving cost scales with machine activity, not semantic state change."

Transition:
1. "So we need to separate semantic work from machine overhead."

---

## Slide 3 — Thesis statement (1 min)

Show this exact thesis:

> "Move the proof boundary from ISA execution to schema-typed state transitions, so memory consistency scales with persistent state accesses, not total computation."

Say:
1. "This is the central claim."
2. "Everything else in Tabula is a consequence of this claim."

---

## Slide 4 — Cost decomposition: Stateful vs Stateless ZK work (2 min)

Core message:
1. ZK cost has two regimes.
2. They should not be forced through one abstraction.

Say:
1. "Stateful cost: reads, writes, commitment updates, consistency."
2. "Stateless cost: arithmetic, comparisons, hash operations."
3. "zkVMs mix both into one instruction trace."
4. "Tabula separates them and optimizes each regime with its own proof structure."

---

## Slide 5 — Data model as proof primitive: typed tables (2 min)

Core message:
1. State is modeled as `(table, column, row) -> value`.
2. Types are preserved as proof-relevant structure.

Say:
1. "Keys are explicit coordinates, not opaque pointers."
2. "Columns carry schema types like `Bool`, `U64`, `I64`, `Digest`."
3. "This enables type-specialized constraints and narrower trace layouts."
4. "Untouched columns do not pay proving cost."

---

## Slide 6 — Normal Form: removing intra-transaction memory ambiguity (3 min)

Core message:
1. Structural IR invariants replace dynamic ambiguity.
2. This is a major complexity collapse.

Introduce NF rules:
1. NF-1: at most one read per cell per tx.
2. NF-2: at most one write per cell per tx.
3. NF-3: no read-after-write for same cell.
4. NF-4: key aliasing must be decidable.

Say:
1. "With these rules, intra-transaction RAM consistency is structurally eliminated."
2. "Only inter-transaction consistency remains, which is much smaller and key-local."
3. "This is where the asymptotic and practical gain comes from."

---

## Slide 7 — SSA locals as trace wires, not mutable memory (2 min)

Core message:
1. Local computation is not part of RAM-consistency burden.
2. Proof effort is concentrated on persistent state touches.

Say:
1. "IR locals are single-assignment slots."
2. "Slots are trace values, not heap or stack addresses."
3. "So local computation does not inflate memory-order arguments."
4. "Memory argument complexity tracks state accesses."

---

## Slide 8 — Commitment strategy: per-column hybrid SSMC/SMT (3 min)

Core message:
1. One commitment scheme is suboptimal for all columns.
2. Tabula chooses commitment strategy per column.

Say:
1. "Small sparse columns: SSMC-style sorted commitments and merge proofs."
2. "Large sparse columns: SMT path proofs."
3. "A hybrid policy routes each column while preserving one uniform proof statement."
4. "This gives better cost alignment with real state distributions."

---

## Slide 9 — Proof composition: specialized chips + explicit interaction buses (3 min)

Core message:
1. Proof logic is decomposed by semantic role.
2. Cross-role consistency is explicit.

Say:
1. "Execution semantics, ordering, state transition, static lookups, hash permutations, and range checks are separated."
2. "They communicate through typed interaction buses."
3. "This avoids monolithic constraints and keeps semantics compositional."
4. "The verifier checks both per-chip validity and global interaction balance."

Important constraint for this talk:
1. Do not discuss crate/module internals.
2. Keep focus on semantic decomposition and proof composition.

---

## Slide 10 — Canonical transfer walkthrough (3 min)

Use one minimal scenario:
1. Read sender and receiver balances.
2. Assert sender balance is sufficient.
3. Compute new balances.
4. Write results.

Say:
1. "At the semantic level, this is two reads, one assertion, two arithmetic transforms, two writes."
2. "Proof-wise, reads bind to committed pre-state, writes bind to post-state."
3. "Inter-transaction ordering guarantees that later transactions observe earlier writes correctly."
4. "Verifier output is a root transition guarantee, not an execution log."

---

## Slide 11 — What exactly is proven (2 min)

State the theorem-like claim:
1. Given old root and transaction batch digest,
2. proof shows instruction semantics are satisfied,
3. read/write consistency constraints hold,
4. and new root is exactly the committed result of those semantics.

Say:
1. "This is stronger than replaying logs, and narrower than proving general machine execution."
2. "It is the right granularity for stateful applications."

---

## Slide 12 — Why this matters (2 min)

Core message:
1. Better complexity alignment.
2. Better semantic transparency.
3. Better scalability path for state-heavy workloads.

Say:
1. "Tabula is not trying to be a universal machine proof framework."
2. "It is a targeted proof kernel for typed state transitions."
3. "In that domain, it removes structural proving tax that machine-centric models cannot avoid."

Final close:

> "If the application is fundamentally stateful, the proof system should be state-native."

---

## 2. Q&A Strategy (Optional, 3–5 min)

If asked "How is this different from just adding precompiles to zkVM?":
1. "Precompiles optimize operations, not abstraction boundaries."
2. "The memory model and instruction-centric trace remain unchanged."
3. "Tabula changes the proof object itself from machine steps to typed state transitions."

If asked "What is the central technical novelty?":
1. "The combination of typed table semantics, NF structural invariants, per-column hybrid commitments, and bus-composed multi-chip proofs."

If asked "What should we remember in one line?":
1. "Prove what the application means, not what the machine happened to do."

---

## 3. Presenter Guardrails

To keep the talk thesis-first:
1. Do not discuss crate boundaries, API surfaces, or adapter workflows.
2. Do not discuss implementation milestones or project status.
3. Do not discuss documentation process or traceability workflow.
4. Keep every slide tied to one question: "How does this reduce semantic-to-proof mismatch?"
