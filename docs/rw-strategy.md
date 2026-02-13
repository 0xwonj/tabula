# Reads/Writes Strategy for Tabula Kernel

## (How to Make “Everything Becomes READ/Openings” Actually Fast)

This section summarizes the **end-to-end strategy** Tabula must implement to keep table-native state transitions efficient, even though **all value access ultimately reduces to READs** (i.e., point openings of table commitments).

The core idea is simple:

> **Never “re-open” state during execution.**
> Execute deterministically using a local overlay (SSA/write-buffer), then perform **one batched opening phase** against `oldStateRoot`, and **one batched update phase** to produce `newStateRoot`.

---

## 1) Adopt a Two-Phase Kernel Model

### Stage 1 — Deterministic Execution (Trace Collection)

Execute the batch with a reference semantics, but instead of proving every READ immediately, record **just enough** to prove correctness later:

Produce:

* **`ReadSet_old`**: the set (or multiset) of cells that must be opened against `oldStateRoot`.
* **`WriteSet_batch_final`**: the final writes that will be applied to the committed state to obtain `newStateRoot`.

Critically:

* Reads that occur after a write to the same key (**read-your-writes**) must **not** be included in `ReadSet_old`.

### Stage 2+3 — Commitment + Proving (Opening Plan + Updates)

Given `ReadSet_old` and `WriteSet_batch_final`, build a **batched opening plan**, generate **batched opening proofs**, and then generate **batched update proofs** to transition commitments.

This separation is what prevents “READ ≈ expensive proof per instruction.”

---

## 2) Enforce Three Core Semantics Rules (This *Is* the Scheduling)

### Rule A — Read-Your-Writes (No Re-Opening)

If a key `k := (table, row, col)` has been written earlier in the same transaction/batch:

* `READ(k)` **must return** the value from the local overlay (write-buffer / SSA),
* **not** by opening the committed state again.

This avoids the worst pattern: *update → open → update → open*.

### Rule B — Read Deduplication

Within a batch, the same cell may be read multiple times.

* Open it **once** from `oldStateRoot`,
* reuse the value thereafter.

This turns repeated reads into free variable references.

### Rule C — Write Coalescing (Last-Write Wins)

If the same key is written multiple times:

* only the **last** write is applied to the committed state (`WriteSet_batch_final`),
* intermediate values remain purely local (SSA versions / buffer state).

This reduces update proof work and simplifies state transition accounting.

---

## 3) Use a Local Overlay During Execution

Tabula execution should behave like a DB engine:

* **Committed State Snapshot**: `S_old` (represented by `oldStateRoot`)
* **Local Overlay**: `Δ` (write-buffer / SSA environment)
* `READ(k)` checks `Δ` first; if absent, reads from `S_old` (and records it)
* `WRITE(k,v)` only updates `Δ` (and records a logical write event)
* At the end, `Δ` is collapsed into `WriteSet_batch_final`

This is the cleanest way to support realistic read-after-write patterns without exploding opening proofs.

---

## 4) The Opening Planner: Reduce “#Reads” to “#Groups”

### 4.1 Group by Column Commitment

Tabula state is columnar; therefore openings should be grouped by `(tableId, colId)`:

For each group:

* collect all `row` indices requested in `ReadSet_old`,
* **deduplicate + sort** (order doesn’t matter logically, but helps implementation),
* produce a **multi-opening** proof for that column.

Example:

* `Entity.hp`: open rows `{1, 9, 15, ...}`
* `Entity.atk`: open rows `{1, 2, ...}`

### 4.2 Require a Multi-Opening Interface from the Commitment Scheme

Tabula’s PCS/VC must support batched openings:

* `BatchOpen(colCom, rows[]) -> (values[], openProof)`

and ideally, proof aggregation across multiple commitments:

* `AggregateOpen({(colCom_i, rows_i[])}) -> single openProof`

The goal is:

> **proofs scale with the number of (table, column) groups**,
> not with the number of `READ` instructions.

---

## 5) Exploit “Same Row, Many Fields” Access Patterns

If a transaction frequently reads many fields from the **same entity row**, Tabula should optimize further. There are two standard options.

### Option 1 — Multi-Commitment, Same-Point Aggregation

Keep columns separate, but aggregate openings of multiple column commitments at the same row.

### Option 2 — Packed Record Columns (Selective Denormalization)

Introduce packed columns such as:

* `Entity.core_stats[row] = (hp, atk, def, ...)`

Then a hot path like `attack` can fetch all needed stats via **one opening**.

Tradeoff:

* packing reduces opening count,
* but may increase update cost if only one field changes frequently.

In practice:

* pack small “hot bundles” (e.g., `(atk, def, crit)`)
* keep very frequently updated fields (e.g., `hp`) separate if beneficial.

---

## 6) Cross-Table “Joins”: Prefer Point-Joins, Avoid Scan-Joins

### 6.1 What Tabula Can Do Efficiently

Tabula can do **point-joins** (foreign-key style):

* read a key from table A,
* use it to read a few cells in table B,
* enforce integrity via `ASSERT(exists == 1)` and other constraints.

This is just a small number of additional READs, and remains efficient when batched.

### 6.2 What Tabula Should Not Try to Be

General SQL-style joins (scan/merge joins, broad predicates, sorting, grouping) are fundamentally “scan-heavy” and will be expensive in any proof system.

If needed, handle those via:

* explicit secondary index tables maintained by the application, or
* a separate query coprocessor / offchain candidate generation + onchain verification pattern.

---

## 7) Handling “Item Bonuses” (Cross-Table References)

If attack power depends on item data in other tables, Tabula simply issues additional READs and ASSERTs—**but should aggressively minimize hot-path reads**.

### Baseline (Normalized) Pattern

`Entity -> ItemInstance -> ItemTemplate` (point-join chain):

* open a few cells per table and assert existence/ownership.

### Preferred (Hot-Path) Pattern: Cache Derived Values

Maintain:

* `Entity.effective_atk`
  updated on `equip/unequip` transactions.

Then `attack` becomes:

* `READ(Entity.effective_atk[attacker])`
* `READ(Entity.hp[target])`
* `WRITE(Entity.hp[target])`

This is the most Tabula-friendly design: **hot tx reads are minimal**.

---

## 8) Consistency (RAM Semantics) Must Be Key-Local

Even with table commitments, Tabula must prove last-write semantics:

* Reads from `oldStateRoot` are justified by opening proofs.
* Reads after writes are justified by the local overlay / SSA semantics.
* The final state applies `WriteSet_batch_final` correctly.

Implement a **key-local consistency module**:

* correctness is enforced only for touched keys (structured keys: `(table,row,col)`),
* avoiding zkVM-style global RAM trace costs.

(Exact implementation can vary; the kernel spec requires the logical structure.)

---

## 9) Canonical Pipeline (What You Actually Implement)

1. **Execute batch deterministically** with overlay `Δ`

   * produce `ReadSet_old`, `WriteSet_batch_final`
   * enforce read-your-writes, dedup reads, coalesce writes

2. **Build Opening Plan**

   * group `ReadSet_old` by `(tableId,colId)`
   * deduplicate/sort rows per group

3. **Generate Batched Opening Proofs**

   * column-level `BatchOpen`
   * optionally aggregate across columns/tables

4. **Generate Batched Update Proofs**

   * apply `WriteSet_batch_final` to commitments
   * produce the updated column/table commitments

5. **Compute `newStateRoot`**

   * hash updated table commitments into the global root

6. **Prove end-to-end correctness**

   * program membership (typed tx)
   * authentication/policy
   * execution semantics + key-local consistency
   * `oldRoot -> newRoot`

---

## One-Sentence Summary

**Tabula must treat committed state as a snapshot, execute with a local overlay, and then prove (i) one batched opening against `oldRoot` and (ii) one coalesced update to `newRoot`, so that proofs scale with grouped access patterns—not with raw READ count.**
