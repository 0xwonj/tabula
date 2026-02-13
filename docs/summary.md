# Tabula Kernel

## Table-Native State Transition VM (Contract-Agnostic Design Spec)

> **Scope**: This document specifies the **Tabula Kernel** only: a table-native execution + proving machine for state transitions.
> **Out of scope**: contracts, bridges, DA, sequencers, MEV/censorship policy. Those can be layered on top later.

---

## 1. Executive Summary

**Tabula** is a **state-transition VM** whose native state is a **Table/DB** (rows/columns with typed schemas), not a flat byte-addressable memory. Its goal is to avoid the “hardware emulation tax” of RISC-V zkVMs (PC/decoder/register updates, low-level memory trace maintenance) by proving **semantic DB operations** directly.

Tabula provides:

* A **table commitment** state root (`stateRoot`) representing all tables/columns.
* A **typed transaction model**: only pre-defined transaction types are executable.
* A deterministic **execution semantics** over a small **DB-IR** (READ/WRITE/ASSERT/LOOKUP…).
* A proof statement for `ApplyBatch`: given `oldStateRoot`, a committed program (`programRoot`), and a batch digest, prove correct execution producing `newStateRoot`.

Tabula is compared directly against **zkVM kernels** (RISC-V zkVMs, etc.). If later embedded into a rollup, Tabula becomes “the VM” of that rollup (similar to zkEVM being the VM of a zkEVM rollup).

---

## 2. Goals & Non-Goals

### 2.1 Goals

1. **Table-native state**: state is (Table, RowKey, Column) with typed columns.
2. **Table commitment**: state root is built from **column/table commitments**, not SMTs.
3. **Typed transactions**: users submit *transaction calls* (tx_type + params), not arbitrary code.
4. **Batch proving**: execute a batch deterministically and prove `oldRoot → newRoot`.
5. **Performance potential**: concentrate proving work on meaningful state transitions, not ISA maintenance.

### 2.2 Non-Goals

* Full “general purpose compute VM” parity with zkVMs.
* Optimizing workloads dominated by heavy cryptographic compute (hashing/signature verification) as the primary bottleneck.
* Designing DA/bridges or onchain integration details (kept host-agnostic).

---

## 3. System Model (Kernel Perspective)

Tabula Kernel exposes one main operation:

### 3.1 ApplyBatch API (Kernel Interface)

**Input (public):**

* `oldStateRoot`
* `programRoot`
* `batchDigest`
* `meta` (optional, e.g., epoch number)

**Input (witness / private):**

* `batchTxs` (transactions, signatures, nonces, etc.)
* `program` (definitions of tx types, or membership proofs under `programRoot`)
* execution witness: intermediate values, constraint checks, etc.
* state openings / update witnesses required by the commitment scheme
* RAM consistency witnesses (for read-after-write semantics)

**Output (public):**

* `newStateRoot`
* `proof`
* `receiptsDigest` (optional; can be derived/committed)

> **Host-agnostic**: a “host” (contract, server, other chain) only needs to verify the proof for these public inputs.

---

## 4. Data Model: Tables & Schemas

### 4.1 Tables

A Tabula state consists of multiple tables (T_1,\dots,T_m).
Each table (T) has:

* a **primary key** domain `Key(T)` (application-defined)
* `k` typed columns: (C_1,\dots,C_k)
* optional constraints (range, uniqueness, conservation invariants, etc.)

### 4.2 Columnar Layout

Tabula uses a **columnar** logical model:

* each column (C_j) is a vector of length (N_T): (C_j[0..N_T-1])

Key→row mapping options:

1. **Dense Key**: `row = key` (or small affine mapping), fixed address space.
2. **Index Table**: an additional table `Index_T(key → row)` managed like any other state.

> For kernel v1.0, either is acceptable. Dense keys simplify commitment/update mechanics; index tables improve flexibility at extra state management cost.

---

## 5. State Commitment: Table Commitment Root

Tabula’s global state root is built from **table commitments**, which are built from **column commitments**.

### 5.1 Column Commitment

For each column vector (C_j), compute:

* `colCom_j = Com(C_j)` using a **Vector Commitment / PCS** supporting openings

### 5.2 Table Commitment

* `tableCom(T) = H(colCom_1 || ... || colCom_k || tableId || schemaHash)`

### 5.3 Global State Root

* `stateRoot = H(tableCom(T_1) || ... || tableCom(T_m) || versionTag)`

**Properties required from Com():**

* Efficient **open**: prove (C_j[row]=v)
* Support **batched opens** (many openings per batch)
* Support **updates** and proofs that the updated commitment matches intended writes

> Exact PCS choice is modular (KZG/IPA/FRI-style), but kernel spec requires the *abstract interface*:

* `Open(colCom, row) -> (v, openProof)`
* `Update(colCom, row, vNew, aux) -> (colComNew, updateProof)`
  (where `aux` may include old value or delta depending on scheme)

---

## 6. Programming Model: Typed Transactions

### 6.1 Transaction Type Definitions

A **transaction type** is a committed procedure:

* name/ID: `tx_type`
* parameter schema
* deterministic logic in Tabula IR (Section 7)
* policy metadata:

  * required role/authority
  * nonce rules / replay protection
  * optional gas/fee model (host-level)

### 6.2 programRoot

`programRoot` commits to the set of allowed tx types:

* `programRoot = H(Commit(txType_1) || ... || Commit(txType_n))`

Kernel must prove:

* Each executed `tx_type` is a member of `programRoot`
* The executed logic corresponds to that committed definition

---

## 7. Execution Semantics: Tabula DB-IR

Tabula executes a small set of **semantic DB operations**, not CPU instructions.

### 7.1 Core IR Primitives

* `READ(tableId, key, colId) -> value`
* `WRITE(tableId, key, colId, value)`
* `ASSERT(predicate)` (boolean constraint)
* `LOOKUP(staticTableId, tuple)` (fixed tables: ranges, byte-decomp, enum sets, etc.)
* `HASH/PRF` (deterministic randomness if needed)
* `EMIT(event)` (optional, for receipts)

### 7.2 Determinism

Kernel execution must be deterministic given:

* `oldStateRoot` (state snapshot)
* the batch transactions (and a fixed ordering policy)
* the program definitions (under `programRoot`)

---

## 8. Proof Statement: What ApplyBatch Proves

Given public inputs `(oldStateRoot, newStateRoot, programRoot, batchDigest, meta)`:

The proof asserts existence of witness such that:

1. **Batch binding**

   * `batchTxs` hash/commit to `batchDigest`

2. **Program binding**

   * For each tx, `tx_type` is included in `programRoot`
   * Executed IR is exactly the committed definition (or proven equivalent)

3. **Authentication & policy**

   * signatures verify against tx payload
   * nonce / replay protection rules satisfy the policy
   * role/authority constraints are satisfied

4. **Correct execution**

   * Executing txs in order under Tabula IR produces a sequence of reads/writes/assertions

5. **State correctness**

   * Every `READ` returns the value consistent with the state *at that point in execution* (**read-after-write / last-write semantics**)
   * Every `WRITE` updates the committed state accordingly
   * Final committed state equals `newStateRoot`

---

## 9. The Hard Part: RAM Semantics over Table Commitment

### 9.1 Why Table Structure Alone Doesn’t Eliminate Consistency Proofs

A table lookup proves “value belongs to (row,col) in some committed table.”
But RAM semantics require “the read at time t equals the **most recent prior write** to that key.”

This is a **time/ordering** property, not only a spatial indexing property.

### 9.2 Tabula’s Approach (Kernel-Level)

Tabula must implement a **State Consistency Module** that supports:

* many keys
* many reads/writes
* rich read-after-write patterns
* batch proving

Tabula’s key insight is **locality**:

* Key is structured: `(tableId, row, colId)`
* Touch set is typically far smaller than the entire state
* Therefore, consistency can be proven **key-locally** (touched keys only), rather than global RAM over all addresses.

### 9.3 Recommended Design Pattern: “Key-Local Transcript + Multiset Link”

**Conceptual components:**

1. **Execution event multiset** produced by IR:

   * events like `(key, op, value, time)` for READ/WRITE
2. **Per-key transcript** that orders events by time for each key:

   * enforces “reads see last write”

**Core requirement:**

* Prove that the execution events and the per-key transcripts are the **same multiset** (no missing/extra events)
* Prove per-key correctness constraints (last-write)

> Implementation choices vary (sorting, lookup-based linking, etc.). The spec requires this *logical structure*.

### 9.4 Optional Optimization: SSA / Versioned Compilation (Compiler-Level)

For transaction logic with repeated updates to the same key:

* compile internal state updates into versioned wires (`x0→x1→x2`)
* reduce the number of explicit READ events (reads become references to the latest version)
* keep WRITE events only at “boundary points” where state is committed

This does **not** replace the consistency module, but it reduces its workload.

---

## 10. Lookup Strategy: Static vs Dynamic

Tabula separates lookup usage into two buckets:

### 10.1 Static Lookups (Great fit for LogUp/Lasso-style)

* range checks, byte decomposition, small fixed tables, enum membership
* can be handled by fast lookup arguments

### 10.2 Dynamic State (Not a pure lookup problem)

* the state table changes every batch
* requires update + consistency semantics
* handled by the State Consistency Module (Section 9)

---

## 11. Performance Profile vs zkVM (What Gets Better / Worse)

### 11.1 Expected Advantages

* **Higher instruction density**: DB-IR steps can represent much more semantics than RISC-V steps.
* **Less “ISA overhead”**: no PC/decoder/register maintenance.
* **Structured state** enables:

  * pruning touched columns/tables
  * sharding by table/column/key-ranges
  * batch openings/updates on column commitments

### 11.2 Potential Disadvantages / Risks

* If workload is dominated by heavy cryptographic computation (hash/signatures), Tabula’s structural wins matter less.
* Dynamic indexing / large scans may:

  * increase touched keys (u)
  * push consistency proofs toward expensive regimes
* Engineering complexity:

  * compiler/tooling (typed tx, IR, SSA)
  * state commitment update proofs
  * consistency module correctness

---

## 12. Parallelism & Multi-Writer Reality (Kernel view)

Even without contracts, a state machine with a single `stateRoot` is **logically serial** at commit time:

* updates must apply in a deterministic order to produce one `newStateRoot`.

However:

* proving can be **parallelized** via

  * table/column sharding
  * key-range partitioning
  * recursive aggregation of partial proofs

Kernel remains compatible with:

* single batch prover
* distributed provers producing shard proofs + aggregation

(Host/sequencer policy is outside kernel scope.)

---

## 13. Minimal “Complete Kernel” Deliverables

To claim a functional Tabula Kernel v1.0, you need:

1. **State commitment format**

* exact `stateRoot` hashing layout
* schema hashing and table identifiers

2. **IR + deterministic executor**

* reference interpreter producing execution events

3. **Program commitment format**

* tx type encoding + `programRoot` computation
* membership proof mechanism or inclusion list

4. **Proof statement implementation**

* correctness of tx membership, auth, execution, and state transition

5. **State consistency module**

* supports read-after-write semantics across batch execution
* at least one baseline instantiation (even if not 최적)

6. **Benchmark harness**

* compare against a zkVM baseline on 1–2 representative workloads

---

## 14. Suggested Research “Core Contribution” Angles (Optional)

If targeting a paper, Tabula’s strongest novelty usually comes from:

* a **state consistency module** that is asymptotically/practically better than zkVM-style global RAM checks for table-structured workloads
* plus compiler-driven **transaction typing + SSA-like lowering** that reduces state events
* plus table commitment sharding (column/table pruning)

---

## 15. Appendix A — Canonical ApplyBatch Statement (Formal-ish)

Let `S_old` be the state represented by `oldStateRoot`.
Let `P` be the program set committed by `programRoot`.
Let `B` be the ordered transaction list committed by `batchDigest`.

Proof asserts existence of witness such that:

* `Hash(B)=batchDigest`
* For each `tx∈B`, `tx.type∈P` and `VerifySig(tx)=true` and `PolicyOK(tx)=true`
* Deterministic execution of `B` over `S_old` under Tabula semantics produces `S_new`
* `Commit(S_new)=newStateRoot`
* All reads in execution satisfy last-write semantics with respect to the induced write history

---

## 16. Appendix B — Naming & Positioning

**Tabula Kernel** = “table-native zk execution kernel”
Comparable to:

* “RISC-V zkVM kernel” (Jolt/SP1/R0-style)
  but with a different execution model:
* typed tx + DB-IR + table commitment
