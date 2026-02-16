# Tabula Proof Spec (v0.9)

## Efficient RAM Consistency for Table-Native State Transitions

### Scope: **B (Intra-Tx Core)** + **C (Inter-Tx Sequential Batch)**

*(D / snapshot-conflict mode intentionally excluded for now.)*

---

## 1. Goal and Problem Statement

Tabula is a state-transition VM where “memory” is not a flat byte array but a **structured database** of tables and columns. The core proving risk is the classic **RAM consistency** question:

> When the execution trace performs `READ(k)` at some step, how do we prove it returned the value from the **most recent** `WRITE(k)`?

In generic zkVMs this is hard because addresses are computed dynamically and access counts are massive. Tabula can do better because:

* Keys are **structured**: `CellKey { t, c, r }` — table, column, row
* `(table_id, col_id)` are **static in IR** (no pointer arithmetic), and access counts are **small and bounded**.
* Intra-transaction execution satisfies **canonical state normal form** (unique-read, unique-write, no-read-after-write) so that each key is opened from committed state at most once per tx.

This spec designs an end-to-end ZKP that proves:

* **Correct tx execution** over table state
* **Correct reads** (from committed state or from the latest in-batch write)
* **Correct writes and state root transition** `oldRoot → newRoot`

---

## 2. High-Level Architecture

Tabula proof is layered:

### Layer B (mandatory): Intra-Tx Core Semantics

Each transaction executes against:

* A **snapshot** of committed state (`oldRoot`) via explicit openings (ReadSet)
* **SSA locals** for intermediate computation (no re-opening of state)
* A **final write set** as the only persistent effect

This eliminates most “intra-tx RAM” pain.

### Layer C (optional but next step): Inter-Tx Sequential Batch Semantics

A batch contains many transactions executed **in order**, where later tx must observe earlier writes. We keep the "current state" as a **small map of touched keys**, and prove its RAM consistency using a dedicated **memory argument**.

---

## 3. Data Model

### 3.1 Keys and Cells

A cell key is a struct with named fields:

```
CellKey { t: TableId, c: ColId, r: RowKey }
```

* `t`: table identifier
* `c`: column identifier — together `(t, c)` are **static** (compile-time constants, §3.2)
* `r`: row key (primary key) — the only **dynamic** component

A cell value is a vector over the field:

* `val ∈ 𝔽^{w(T)}` where `w(T)` is the **schema-typed encoding width** — the number of field elements determined by the column's schema type `T` (see §10.3). For v1: `w(Bool)=1`, `w(U64)=w(I64)=3`, `w(Digest)=8`.

### 3.2 Tables, Columns, and "Static Coordinates"

**Static Coordinates Invariant (MUST).** In Tabula IR, the table identifier `t` and column identifier `c` are **compile-time constants**. They MUST appear directly as instruction fields (e.g., `Read { table: TableId, col: ColId, ... }`) and MUST NOT be computed at runtime or represented by expression types (no `TableExpr::Slot(...)` or `ColExpr::Param(...)`). Only the row key `r` may be dynamic (`RowExpr::Slot` / `RowExpr::Param`). This invariant is enforced by the IR type structure and program validation.

> **Lemma (Static Coordinates).** For every instruction in a well-formed Tabula IR program, `(t,c)` are compile-time constants. This enables per-`(t,c)` sharding of memory arguments without runtime dispatch, and guarantees that the set of touched `(t,c)` groups is statically determinable from the program text (though which groups are *actually* touched depends on runtime control flow and row-key values).

This is central to both performance and proof structure:

* We can split memory arguments **per (table, column)**.
* We can batch openings **per (table, column)**.

### 3.3 Row Key Space (Design Choice)

You must choose one of the following (both work with this spec):

**(A) Fixed index space**

* Each table column is a vector of length `N = 2^d` indexed by `r ∈ [0..N-1]`.
* Best for predictable layouts; requires bounded `r`.

**(B) Sparse key space**

* `r` is an arbitrary bitstring (e.g., 64-bit or 256-bit), and the storage is a sparse commitment (e.g., sparse Merkle).
* Best for dynamic IDs; proofs include membership paths.

This spec will describe the commitment layer abstractly so either choice fits.

---

## 4. Cryptographic Building Blocks

### 4.1 Field and Hash

* A prime field `𝔽` used by the proof system.
* A proof-friendly hash `H` (algebraic hash recommended if hashing is verified inside the proof).

### 4.2 Vector Commitments for State (Column Commitments) — Hybrid SSMC + SMT

For each `(t, c)` we maintain a commitment to the column vector. The commitment **strategy is selected per-column based on column size**:

#### Strategy A: SSMC (Small Sparse Map Commitment) — for small columns (≤ threshold rows)

The prover provides the **entire column** as a sorted witness list laid out as an AIR trace sub-table. Membership and non-membership are proven via **LogUp lookups** into this table.

**Witness format**: `W[t,c] = [(k_0, v_0), (k_1, v_1), ..., (k_{m-1}, v_{m-1})]`

**SSMC Table (AIR trace sub-table)**: Each witness entry becomes a trace row. In practice, all per-(t,c) SSMC rows are merged into a single **GlobalSSMC** table with `is_real` gating — see §4.2.G. Logical columns per entry:

| Column | Type | Description |
|--------|------|-------------|
| `key` | u64 | Row key |
| `value` | 𝔽^{w(T)} | Cell value — Tier 1 commitment encoding (§10.3), always non-null |
| `next_key` | u64 | Key of next entry in sorted order |
| `is_first` | bool | 1 only for the first entry |
| `is_last` | bool | 1 only for the last entry |

**Constraints (enforced in-circuit)**:

1. **Sorted uniqueness**: For each row where `is_last = 0`: `next_key > key` (strict integer inequality). Enforced via the u64 strict inequality gadget (§4.2.R): decompose the difference `next_key - key - 1` into 3 BabyBear limbs with borrow propagation, range-check each limb. Cost: O(range-check-gadget) per pair — exact cost depends on Plonky3's range-check implementation, estimated ~8-12 constraints per comparison pending benchmark.
2. **Boundary consistency**: `is_first = 1` only on the first row, `is_last = 1` only on the last row. Where `is_last = 0`, `next_key` equals the following row's `key`.
3. **Commitment (domain-separated, Poseidon-based streaming).** `Com[t,c]` is computed by a domain-separated Poseidon-based streaming construction over the ordered `(key, value)` sequence with prefix `(0x00, t, c)`. The domain prefix `0x00` prevents cross-column and cross-strategy collisions (SSMC tag). The AIR realizes this using one of the following equivalent strategies:

   * **(Prototype) Iterative hash chain:** `h_0 = Poseidon(0x00 || t || c || k_0 || v_0)`, and `h_i = Poseidon(h_{i-1} || k_i || v_i)` for `i ≥ 1`. `Com[t,c] = h_{m-1}`. In the AIR, each SSMC row carries a `hash_acc` column; the transition constraint enforces `hash_acc_{i+1} = Poseidon(hash_acc_i || key_{i+1} || value_{i+1})`. Cost: one Poseidon permutation per entry.

   * **(Optimization) Sponge/duplex streaming:** A Poseidon sponge absorbs multiple `(key, value)` elements per permutation, amortizing cost. This reduces the number of permutations at the cost of carrying sponge-state columns in the AIR. The choice does not affect soundness; it only affects prover cost and trace width. The commitment digest is the sponge squeeze output after all entries are absorbed.

   The prototype implementation uses the iterative hash chain for simplicity. The sponge optimization is available for future cost reduction and does not require spec changes (same digest under canonical input ordering).

**Per-cell opening (membership)**: To prove `Read(t, c, r) = val`:
* The execution trace emits a **LogUp lookup** into GlobalSSMC: the tuple `(t, c, key=r, value=val)` is looked up against the GlobalSSMC table's `(t, c, key, value)` columns. The `(t,c)` prefix prevents cross-column collisions.
* LogUp proves the tuple exists in the GlobalSSMC table.
* Cost: O(LogUp-access) — typically ~5-15 constraints per lookup (Plonky3 native LogUp).

**Per-cell opening (non-membership)**: To prove key `r` does NOT exist (value is `Null`):
* The execution trace emits a LogUp lookup for a **gap witness**: a row in GlobalSSMC (for this `(t,c)`) that brackets `r`.
* **Interior gap** (`r` between two entries): lookup `(t, c, key, next_key, is_first, is_last)` where `key < r` and `next_key > r`. Two additional **strict inequality** range-check constraints prove the bounds: decompose `r - key - 1` and `next_key - r - 1` into non-negative BabyBear limbs.
* **Before first entry** (`r < k_0`): lookup the row where `is_first = 1`, prove `r < key` via strict range check (`key - r - 1 ≥ 0`).
* **After last entry** (`r > k_{m-1}`): lookup the row where `is_last = 1`, prove `key < r` via strict range check (`r - key - 1 ≥ 0`).
* **Empty column** (`m = 0`): The commitment is a fixed empty-column digest `Com_empty(t,c) = Poseidon(0x00 || t || c)`. An empty column is represented **only in ColumnMeta** (`is_empty_old=1`, `Com_old = Com_empty`). There are **no real rows** for `(t,c)` in GlobalSSMC — see §4.2 ColumnMeta for existence rules. For non-membership against an empty column, the init row's opening checks `ColumnMeta.is_empty_old(t,c) = 1`, which immediately implies `init_value = Null` without requiring any SSMC gap-witness lookup. (If the column becomes non-empty via writes, `GlobalMerge` rows exist because `is_touched=1`.)
* Cost: O(LogUp-access) + O(range-check) per comparison — estimated ~20-30 constraints total pending benchmark.

**Why LogUp, not array indexing**: In STARK/AIR, the trace is a fixed set of columns evaluated row-by-row. There is no "random access into witness array at position i" primitive. LogUp is the standard mechanism for proving that a value exists in a lookup table, and Plonky3 provides native support for it.

**State update (3-way merge proof)**: When writes modify an SSMC column, the prover must prove that `NewList` is correctly derived from `OldList + WriteSet`. Simply re-hashing with new values is **insufficient** — without a merge proof, the prover could silently insert, delete, or modify entries not in the write set.

**Null semantics**: `WRITE(k, Null)` is a **delete** — it removes key `k` from the column. The merge trace must handle this case explicitly.

The merge is verified via a **linear scan trace**:

| Column | Description |
|--------|-------------|
| `key` | Entry key (strictly increasing across all rows) |
| `s1, s0` | Source encoding (2-bit boolean, see below) |
| `old_val` | `𝔽^{w(T)}` — value from OldList (valid when source ∈ {old_only, both, delete}) |
| `write_val` | `𝔽^{w(T)}` — value from WriteSet (valid when source ∈ {write_only, both}) |
| `new_val` | `𝔽^{w(T)}` — value in NewList (zeroed for `delete` rows) |
| `in_new` | 1 if this row appears in NewList, 0 if deleted |

**Merge `source` encoding validity.** The `source` enum is encoded using two boolean columns `(s1, s0)` with constraints `s1 · (1 - s1) = 0` and `s0 · (1 - s0) = 0`. The mapping is:

| s1 | s0 | source |
|----|----|--------|
| 0 | 0 | `old_only` |
| 0 | 1 | `write_only` |
| 1 | 0 | `both` |
| 1 | 1 | `delete` |

Derived selectors:
* `is_old_only  = (1 - s1)(1 - s0)`
* `is_write_only = (1 - s1) · s0`
* `is_both      = s1 · (1 - s0)`
* `is_delete    = s1 · s0`

All merge constraints below are gated by these selectors.

Merge constraints:
1. Keys are strictly increasing (same sorted uniqueness check as SSMC).
2. `is_old_only ⟹ new_val = old_val ∧ in_new = 1` (entry unchanged by writes).
3. `is_write_only ⟹ new_val = write_val ∧ in_new = 1` (new entry inserted).
4. `is_both ⟹ new_val = write_val ∧ in_new = 1` (overwrite; `write_val ≠ Null`).
5. `is_delete ⟹ in_new = 0` (entry removed from NewList; `write_val = Null`).
6. **Completeness via LogUp**: Every OldList entry appears exactly once in the merge trace (as `old_only`, `both`, or `delete`). Every WriteSet entry appears exactly once (as `write_only`, `both`, or `delete`). Proved by two LogUp arguments linking OldList and WriteSet to the merge trace.
7. **NewList commitment**: `Poseidon(0x00 || t || c || entries where in_new=1) = Com_new[t,c]`. Only rows with `in_new = 1` contribute to the new hash chain.

Cost: O(m + w) where m = OldList size, w = write count. Linear in total entries.

**Total cost per SSMC column** (parametric, exact values from benchmark):
* Sorted uniqueness: `O(m) × RangeCheckCost` (estimated ~8-12 constraints per pair)
* Commitment hash: `O(⌈m × (key_width + val_width) / rate⌉) × PoseidonPermCost`
* Per-access opening: `O(LogUpCost)` + optional `O(RangeCheckCost)` for non-membership
* Update (merge proof): `O(m + w) × (SortCheck + LogUp + hash)` per merge row

For m = 100, 5 reads, 5 writes (rough order-of-magnitude): ~100×10 (sort) + ~100×5 (hash) + ~10×15 (access) + ~105×15 (merge) ≈ **3,000-4,000 constraints**. Exact values will be calibrated via Plonky3 benchmarks (see B7).

#### Strategy B: Sparse Merkle Tree (SMT) — for large columns (> threshold rows)

* `Com[t,c] = SMT_root(Col[t,c])`
* Poseidon-based SMT, 64 levels (matching `RowKey = u64`). Future optimization: higher arity (4-ary/8-ary) to reduce depth.
* Domain-separated internal nodes: `H(0x01 || t || c || level || left || right)` to prevent second-preimage and cross-column attacks. The `0x01` tag distinguishes SMT from SSMC (`0x00`).
* Cost per opening/update: `O(64) × PoseidonPermCost` hash constraints. Exact count depends on Poseidon configuration; estimated ~15,000+ constraints.
* Membership: standard Merkle path verification.
* Non-membership: opening to the default value (all-zero leaf) at path `r`.
* Efficient for large/sparse columns where loading the entire column as witness is impractical.

#### Threshold Selection (Benchmark-Dependent)

* **Threshold**: The SSMC/SMT crossover threshold is **TBD**, to be calibrated via Plonky3 benchmarks (B7). Analytical considerations (§9 break-even analysis) suggest the break-even point falls in the range of **100–300 rows** for the hash-chain streaming realization, primarily driven by the O(m) Poseidon chain cost. The provisional prototyping value is 500 rows, but this is expected to decrease after benchmarking.
* Crossover depends on Poseidon constraint cost in BabyBear, the streaming realization (hash-chain vs sponge), and the access pattern.
* Strategy selection happens at **witness generation time** — the column size is known before proof generation.
* **Advanced threshold policy**: Beyond `m`, the optimal strategy may also depend on `U` (unique touched keys) and `W` (writes per batch). If `m` is small but `U` is large, SSMC may still win because all accesses are via LogUp (no Merkle paths). Initial implementation uses `m` only; evolve to (m, U, W)-based policy after benchmarking.

#### Global State Root — Two-Level SMT with Inclusion Proofs

The proof frequently needs to verify openings and updates for only a *small touched subset* of `(t,c)` columns. Therefore, `oldRoot` / `newRoot` MUST support **inclusion proofs** for individual column commitments. A flat hash-chain aggregation does **not** admit sub-opening proofs without materializing the full list.

We define a **two-level Sparse Merkle commitment** over *commitment digests* (not cell values):

**Table Commitment Tree (per table)**:

For each table `t`, a Merkle tree over columns:
* Leaf key: `col_key = c`
* Leaf value: `LeafDigest(t,c) = Poseidon( 0x10 || t || c || tag_c || Com[t,c] )`

Where `tag_c ∈ {0,1}` identifies the per-column commitment strategy (0=SSMC, 1=SMT).

```
TableRoot[t] = SMT_cols.Root( key=c, value=LeafDigest(t,c) )
```

`SMT_cols` is a sparse Merkle tree keyed by `c` (e.g., u32/u64), using Poseidon for internal nodes with domain separation.

**Global State Tree (over tables)**:

```
oldRoot = SMT_tables.Root( key=t, value=TableRoot[t] )
newRoot = SMT_tables'.Root( key=t, value=TableRoot'[t] )
```

**Inclusion proof interface**: Whenever the proof verifies an opening/update for column `(t,c)`, it MUST also prove `Com[t,c]` is part of `oldRoot`/`newRoot`:

* Provide `π_table(t)` proving `TableRoot[t]` is included in `SMT_tables` under key `t`.
* Provide `π_col(t,c)` proving `LeafDigest(t,c)` is included in `SMT_cols` under key `c`, whose root equals `TableRoot[t]`.

This binds all per-column `Com_old[t,c]` used by `VC.Verify` to the public `oldRoot`, and similarly binds `Com_new[t,c]` to `newRoot`.

**Node domain separation**:

```
NodeHash_tables(level, left, right) = Poseidon( 0x11 || level || left || right )
NodeHash_cols  (level, left, right) = Poseidon( 0x12 || level || left || right )
```

Keys are expanded to bit paths deterministically (fixed-width little-endian).

**Depth optimization**: Unlike the cell-level SMT (64 levels for u64 row keys), the meta-level trees can use **shorter depths** since table and column ID domains are small:
* `SMT_tables`: depth 16-24 (sufficient for up to 2^16-2^24 tables)
* `SMT_cols`: depth 16-24 (sufficient for up to 2^16-2^24 columns per table)
* Higher arity (4-ary/8-ary) can further reduce depth. Exact depths set at deployment time based on schema bounds.
* When multiple columns in the same table are updated, batch the `SMT_cols` updates by sorting update keys and scanning the tree once.

**Meta-level SMT Update Proof (Required)**: In addition to per-leaf inclusion proofs, the prover MUST provide a **meta-level SMT update proof** showing that applying exactly the set of updated `(t,c)` leaves transforms `oldRoot` into `newRoot`, and that all other leaves remain unchanged. Concretely:

* For each updated `(t,c)` (where `ColumnMeta.is_touched = 1`), define:
  * `Leaf_old(t,c) = Poseidon(0x10 || t || c || tag || Com_old[t,c])`
  * `Leaf_new(t,c) = Poseidon(0x10 || t || c || tag || Com_new[t,c])`
* The prover provides a **batched update proof** for:
  * `SMT_cols` within each table `t` (updating the leaves keyed by `c`)
  * `SMT_tables` at the top level (updating the table roots keyed by `t`)
* This update proof ensures:
  * The set of modified leaves is **exactly** the set claimed by the witness (no silent modifications).
  * The resulting root equals the public `newRoot`.

Inclusion proofs remain available as an interface convenience for per-column openings, but the `oldRoot → newRoot` transition MUST be validated by this update proof.

> **Note:** This root structure commits only to *column commitment digests*. The *cell-level* commitment inside each column remains hybrid (SSMC or SMT). The two layers compose cleanly: the global trees provide inclusion proofs for `Com[t,c]`, while the hybrid VC provides membership/non-membership for cells within that column.

#### VC Interface (strategy-agnostic)

Both strategies implement the same abstract interface:

* `Commit(column) -> Digest`
* `Open(column, key) -> (value, proof)` — SSMC: proof is LogUp witness entry; SMT: proof is Merkle path
* `Verify(digest, key, value, proof) -> bool` — SSMC: LogUp + gap check; SMT: check Merkle path
* `Update(old_digest, writes, old_column) -> (Digest', update_proof)` — SSMC: merge proof + re-hash; SMT: path updates

> **Important:** `VC.Verify` only proves a cell value against a column digest `Com[t,c]`. The caller MUST separately prove that `Com[t,c]` is part of `oldRoot`/`newRoot` via the two-level root inclusion proofs (see "Global State Root" above). Both proofs are required for soundness.

#### ColumnMeta Table (Wiring Commitments to Root)

The proof needs a way to connect the `Com[t,c]` values (produced by hash chains in GlobalSSMC or by SMT roots) to the root inclusion proofs. A **ColumnMeta** table provides this wiring:

| Column | Description |
|--------|-------------|
| `t, c` | table and column identifiers |
| `tag` | commitment strategy (0=SSMC, 1=SMT) |
| `Com_old` | column commitment under `oldRoot` |
| `Com_new` | column commitment under `newRoot` (= `Com_old` if untouched) |
| `is_empty_old` | 1 iff the column is empty under `oldRoot` |
| `is_empty_new` | 1 iff the column is empty under `newRoot` |
| `is_touched` | 1 iff at least one effective write targets this `(t,c)` in the batch |
| `is_real` | 1 for real rows, 0 for padding |

**Constraints**:
* **Old empty binding**: If `is_empty_old = 1`, then `Com_old = Com_empty(t,c)` where `Com_empty(t,c) = Poseidon(0x00 || t || c)`.
* **New empty binding**: If `is_empty_new = 1`, then `Com_new = Com_empty(t,c)`.
* **Untouched columns**: If `is_touched = 0`, then `Com_new = Com_old` (no update occurs at the column level).
* **Touched columns**: If `is_touched = 1`, then `Com_new` must be justified by the column update proof (SSMC: GlobalMerge hash chain; SMT: Merkle updates), even if `is_empty_new = 1` (delete-only case).
* **SSMC columns** (`tag=0`, `is_empty_old=0`): `Com_old` equals the hash chain output from the GlobalSSMC segment for `(t,c)`. If `is_touched=1`, `Com_new` equals the hash chain output from the GlobalMerge segment for `(t,c)`.
* **SMT columns** (`tag=1`): `Com_old` / `Com_new` equal the column-level SMT roots.
* **Root binding**: For each row, `LeafDigest_old = Poseidon(0x10 || t || c || tag || Com_old)` must match the leaf in `SMT_cols` inclusion proof against `oldRoot`. Similarly `LeafDigest_new` for `Com_new` → `newRoot`.

**GlobalSSMC / GlobalMerge existence rules (SSMC only)**:
* `GlobalSSMC` contains real rows for `(t,c)` iff `tag=0` and `is_empty_old=0`. (If old state is empty, there is no SSMC witness to provide.)
* `GlobalMerge` contains real rows for `(t,c)` iff `tag=0` and `is_touched=1`. (This includes delete-only updates that result in `is_empty_new=1`.)

**ColumnMeta uniqueness (strict sorted order)**: ColumnMeta rows are sorted by `(t,c)` in strict lexicographic order over real rows:
* `is_real ∈ {0,1}`
* **Prefix property**: `is_real_{i+1} ≤ is_real_i` (real rows form a prefix; padding at end).
* For consecutive real rows `i, i+1`: `(t_i, c_i) <lex (t_{i+1}, c_{i+1})`.
* This makes the mapping `(t,c) → (tag, Com_old, Com_new, is_empty_old, is_empty_new, is_touched)` **functional** (each `(t,c)` appears at most once).

**ColumnMeta join / wiring lookups**: Any global-table row that depends on per-column metadata MUST perform a LogUp lookup into ColumnMeta keyed by `(t,c)` to retrieve `(tag, Com_old, Com_new, is_empty_old, is_empty_new, is_touched)`. At minimum:

* Each `GlobalSSMC` real row performs a lookup to obtain `is_empty_old`, and enforces `is_real · is_empty_old = 0` (no SSMC rows for old-empty columns).
* Each `GlobalMerge` real row performs a lookup to obtain `is_touched`, and enforces `is_real · (1 - is_touched) = 0` (merge rows only for touched columns).
* Each init-row in `GlobalSortedMem` performs a lookup to obtain `is_empty_old`; if `is_empty_old = 1`, enforce `init_value = Null` and skip SSMC membership/non-membership proofs.

This table makes the "commitment → root" binding explicit and auditable in the AIR, and centralizes all empty-column and touched-column logic to prevent contradictory encodings.

#### 4.2.G Globalization and Padding of Variable-Length Tables

STARK/AIR traces are fixed-width, fixed-length tables. Tabula's design introduces several *logically variable-length* subtables (per-column SSMC tables, per-column merge traces, per-(t,c) SortedMemTables). To make these implementable in a single STARK proof, each structure is represented as a **global table** with an `is_real` flag that gates all constraints.

**`is_real` prefix constraint (all global tables)**: For every global padded table (`GlobalSSMC`, `GlobalMerge`, `GlobalSortedMem`, and `ColumnMeta`), enforce:
* `is_real ∈ {0,1}`
* **Prefix property**: `is_real_{i+1} ≤ is_real_i`

This guarantees all real rows form a contiguous prefix and all padding rows form a suffix. Without this, a prover could interleave padding rows and break segment logic. All pairwise/transition constraints are then gated by `is_real_i ∧ is_real_{i+1}`.

Three global tables:

1. **GlobalSSMC** — all SSMC witnesses across `(t,c)`
2. **GlobalMerge** — all SSMC update merge traces across `(t,c)`
3. **GlobalSortedMem** — all sorted memory-event rows across `(t,c)` (see §8.8)

Each row includes `(t,c)` identifiers so that lookups and constraints are properly namespaced.

**GlobalSSMC columns**:

| Column | Description |
|--------|-------------|
| `t, c` | table and column identifiers |
| `key` | row key `r` |
| `value` | `𝔽^{w(T)}` — Tier 1 commitment encoding (§10.3), always non-null |
| `next_key` | next key in sorted order |
| `is_first`, `is_last` | boundary flags |
| `is_real` | 1 for real rows, 0 for padding |

All SSMC constraints are **gated by `is_real`**:
* Sorted uniqueness: enforced only when `is_real=1` and `is_last=0`
* `next_key` equals next row's key: enforced only when `is_real=1` and `is_last=0`
* Boundary consistency flags: enforced only when `is_real=1`

**Namespaced lookups**: All LogUp lookups into SSMC MUST include `(t,c)` in the lookup tuple:
* Membership lookup key: `(t, c, key, value)`
* Gap witness lookup key: `(t, c, key, next_key, is_first, is_last)`

This prevents cross-column collisions.

**GlobalMerge columns**:

| Column | Description |
|--------|-------------|
| `t, c` | table and column identifiers |
| `key` | merged key (strictly increasing within each `(t,c)` segment) |
| `source` | encoded enum: `old_only` / `write_only` / `both` / `delete` |
| `old_val`, `write_val`, `new_val` | `𝔽^{w(T)}` — Tier 1 encoding; delete zeroes `new_val` |
| `in_new` | 1 if entry appears in NewList |
| `is_real` | 1 for real rows, 0 for padding |

All merge constraints are gated by `is_real`. Completeness LogUp arguments are also gated.

**Global ordering and single-segment guarantee**: Each `(t,c)` group MUST appear as **exactly one contiguous segment** in each global table. The AIR enforces this via:

```
same_group_{i,i+1} := (t_i = t_{i+1}) ∧ (c_i = c_{i+1})
```

At every segment boundary (where `same_group = 0` and both rows are real):

```
enforce( ¬same_group ∧ is_real_i ∧ is_real_{i+1} ⟹ (t_i, c_i) <lex (t_{i+1}, c_{i+1}) )
```

This **strict lexicographic ordering** at boundaries guarantees each `(t,c)` appears at most once — if the same `(t,c)` tried to appear again later, the ordering would be violated. Padding rows (`is_real=0`) are placed at the end of the trace.

Pairwise constraints (sorted uniqueness, next_key consistency, sorted memory transitions) are enforced only when `same_group_{i,i+1} ∧ is_real_i ∧ is_real_{i+1}`.

This ensures per-(t,c) proofs remain isolated, and each group's hash chain / sorted scan / merge trace forms a single continuous sequence.

**LogUp multiplicity for padding exclusion**: Padding rows MUST NOT enter any LogUp multiset. Each LogUp instance uses an explicit **multiplicity column** `m` to control which rows participate:

* **GlobalSSMC** (lookup-table side): `m = is_real · [tag=0] · (1 - is_empty_old)` — only real SSMC rows for non-empty *old* columns are lookup targets. (`is_empty_old` is obtained via the ColumnMeta join; `[tag=0]` is a boolean selector for SSMC strategy.)
* **GlobalMerge** (completeness + update side): `m = is_real · [tag=0] · is_touched` — merge rows exist iff the column is touched, including delete-only updates that result in `is_empty_new=1`.
* **GlobalSortedMem** (sorted side of memory argument): `m = is_real × (1 - is_init)` — only real non-init rows match execution accesses.
* **Execution access log** (unsorted side of memory argument): `m = is_access` — only instruction rows that emit accesses participate.

These multiplicities are enforced as columns in the AIR. The LogUp running sum only accumulates rows where `m = 1`.

#### 4.2.R Integer Encoding and Comparison Gadgets (BabyBear)

**Motivation.** BabyBear is a 31-bit prime field (`p = 2^31 - 2^27 + 1 = 2013265921`). Therefore, 64-bit integers (row keys, timestamps) and other bounded identifiers MUST be represented using a multi-limb encoding and manipulated via integer-emulation constraints. The proof MUST NOT rely on interpreting a `u64` as a single field element — a single BabyBear element can only hold values in `[0, p-1]`, and field arithmetic wraps modulo `p`, which does not correspond to integer semantics for values ≥ `p`.

**Notation convention.** Throughout this spec, any `u64`-typed field in an AIR table (e.g., `r`, `key`, `next_key`, `τ`) is represented as **3 BabyBear limbs** `(x0, x1, x2)` as defined below. Table schemas use the shorthand `key` to mean the 3-limb representation; implementations expand this to three AIR columns (e.g., `key_0, key_1, key_2`).

**u64 limb encoding.** Represent any `u64` quantity `x` as three limbs `(x0, x1, x2)`:

* `x = x0 + x1 · 2^30 + x2 · 2^60` (in the integers)
* `x0 ∈ [0, 2^30)` — 30-bit range-check
* `x1 ∈ [0, 2^30)` — 30-bit range-check
* `x2 ∈ [0, 16)` — 4-bit range-check (e.g., decompose into 4 boolean bits, or lookup)

This covers the full `u64` range: `15 · 2^60 + (2^30-1) · 2^30 + (2^30-1) = 2^64 - 1`. Constraint cost: 2 range-checks (30-bit each, implementation-dependent) + 1 range-check (4-bit).

**Why 30+30+4 (not 31+31+2).** BabyBear's modulus `p = 2013265921 < 2^31 - 1 = 2147483647`. A 31-bit limb value can exceed `p`, causing lossy reduction when stored as a field element. 30-bit limbs (max `2^30 - 1 = 1073741823 < p`) are always canonical, ensuring lossless round-trip both in-circuit and out-of-circuit (commitment hashing).

**Reconstruction constraint.** The prover provides `(x0, x1, x2)` as witness columns. The verifier checks `x0 + x1 · 2^30 + x2 · 2^60 = x_combined` where `x_combined` is a multi-limb representation (NOT a single field element). In practice, this means **all comparisons and arithmetic on u64 values are performed limb-wise** with explicit carry/borrow propagation.

**Equality gadget (u64).** To enforce `x = y` for two u64-encoded values, enforce limb-wise equality: `x0 = y0 ∧ x1 = y1 ∧ x2 = y2`.

**Non-negativity of difference (x ≥ y).** To enforce integer `x ≥ y`, introduce borrow bits `(b0, b1) ∈ {0,1}` and difference limbs `(d0, d1, d2)`:

* `d0 = x0 - y0 + b0 · 2^30`, with `d0 ∈ [0, 2^30)` and `b0 ∈ {0,1}`
* `d1 = x1 - y1 - b0 + b1 · 2^30`, with `d1 ∈ [0, 2^30)` and `b1 ∈ {0,1}`
* `d2 = x2 - y2 - b1`, with `d2 ∈ [0, 16)`

The final result `d2` must be non-negative (no underflow), which is guaranteed by the range constraint `d2 ∈ [0, 16)`. This enforces integer `x - y = d0 + d1 · 2^30 + d2 · 2^60 ≥ 0`.

**Strict inequality (x > y).** Enforce `x ≥ y + 1` using the non-negativity gadget. Since `y + 1` may carry, compute `(y0+1, y1, y2)` with carry propagation, then apply the non-negativity check.

**Zero-test gadget (u64 equality detection).** To produce a boolean `eq ∈ {0,1}` indicating whether two u64 values are equal: compute limb-wise differences `(δ0, δ1, δ2)` where `δi = xi - yi`, then define `δ_combined = δ0 + δ1 · α + δ2 · α²` for a random challenge `α` (Fiat-Shamir), and apply the standard field zero-test gadget (inverse helper column). Alternatively, enforce `δ0 = 0 ∧ δ1 = 0 ∧ δ2 = 0` directly if a boolean "is-equal" flag is needed.

**Application.** All ordering constraints in this spec (SSMC sorted uniqueness, gap witnesses, GlobalSortedMem `(r,τ)` lexicographic ordering, `same_key` detection) MUST use these integer gadgets. Specifically:

* "sorted uniqueness" (`next_key > key`): strict inequality gadget on u64 limbs
* "gap witness" (`key < r < next_key`): two strict inequality checks on u64 limbs
* "`(r,τ)` lexicographic ordering": first compare `r` (u64 limbs); if equal, compare `τ` (which may use a smaller encoding if bounded — see below)

**Timestamp width optimization.** The timestamp `τ` is bounded by the maximum number of accesses per batch. If this bound fits in a smaller integer (e.g., `u32` with 2 limbs), implementations SHOULD use the tighter encoding to reduce constraint cost. The spec uses `u64` for generality; implementations may narrow this with an explicit range bound as a system parameter.

#### 4.3 Memory Consistency via LogUp

Inter-tx memory read/write consistency is verified using **LogUp (logarithmic derivative lookup argument)**. (Intra-tx consistency requires no RAM argument — it is enforced structurally by the canonical state normal form, see §6.3.3.)

* LogUp is natively supported by Plonky3.
* The per-(t,c) SortedMemTable structure from §8 remains unchanged — LogUp replaces only the multiset equality proof mechanism.
* Actual per-access cost includes auxiliary columns (running sum, range checks for sorting) and is expected to be in the range of **5-15 constraints per access** (not 1-2 as initially estimated). This is still negligible compared to state commitment costs.

### 4.4 STARK Backend: Plonky3 over BabyBear

**Decision: Plonky3 with BabyBear field.**

* **Plonky3**: Production-ready STARK framework, audited, used by SP1 (Succinct). Modular AIR, native LogUp support.
* **BabyBear** (p = 2^31 - 2^27 + 1 = 2013265921): 31-bit prime field. Fast native arithmetic, well-suited for Tabula's small-value workloads (u64 = 3 limbs, i64 = 3 limbs, bool = 1 limb).
* FRI is the polynomial commitment backend for the STARK itself (proving polynomial evaluations). This is distinct from the state VC (§4.2) — FRI proves the AIR, SSMC/SMT commits state.

**Why not Stwo**: Faster (100x over Stone) but API is experimental and requires Rust nightly. Can migrate later if it stabilizes.

**SNARK wrapping**: For on-chain verification, a STARK→Groth16 wrapper (SP1 approach) can produce ~200-byte proofs. This is an optional future layer, not part of B+C.

### 4.5 LogUp Argument (Core Tool for Memory Consistency)

To relate an **unsorted access list** (from execution) to a **sorted memory table**, we use the **LogUp (logarithmic derivative)** argument.

Given two multisets of "fingerprints" `{u_i}` (unsorted) and `{s_j}` (sorted), LogUp proves multiset equality via:

* `Σ_i 1/(γ + u_i) = Σ_j m_j/(γ + s_j)`

where `m_j` is the multiplicity of `s_j`. In STARK form, this becomes a running sum constraint. Including auxiliary columns (running sum, range checks for (r,τ) sorting), the actual cost is **~5-15 constraints per access row**.

**Why LogUp over grand product**: LogUp is numerically more stable, has lower degree constraints, and is natively supported by Plonky3. The per-(t,c) sharding of memory tables makes the argument sizes small, so the efficiency gain is moderate — but it simplifies implementation by using Plonky3's built-in LogUp machinery. The per-access cost is still small relative to state commitment costs.

`Φ(·)` is a linear combination of columns with random coefficients (Fiat-Shamir challenges):

* `Φ(row) = α*t + β*c + a*r + b*τ + d*is_write + f*val_is_null + Σ_j e_j * val[j]`

Including `(t,c)` in the fingerprint enables a single global LogUp argument that correctly separates per-(t,c) memory tables. Each element of `val ∈ 𝔽^{w(T)}` gets its own challenge coefficient. The `val_is_null` flag (Tier 2, §10.3) is included to prevent confusion between null and zero-valued entries.

---

## 5. Public Statement and Witness

### 5.1 Public Inputs

| Input | Status | Description |
|-------|--------|-------------|
| `oldRoot` | **Required** | State root before batch execution |
| `newRoot` | **Required** | State root after batch execution |
| `AppliedTxDigest` | **Required** | Commitment to the ordered list of successfully applied transactions (§6.3) |
| `ProgramRoot` | **Required** | Commitment to allowed tx types / code templates (membership proofs for tx type validation) |
| `StaticTableRoot` | **Required** | Commitment to all static (immutable) tables used by `Lookup` instructions. `Lookup` results are verified via LogUp/Lasso arguments against this root. |
| `budgets` | **Required** | `max_ops`, `max_slots`, `max_accesses` — bound trace dimensions (semantics-spec §1.8) |
| Batch metadata | Optional | e.g., batch id, sequencer id |

**Explicitly NOT included:**

| Output | Status | Reason |
|--------|--------|--------|
| `receiptsDigest` | **Not in statement** | `Emit` is out-of-protocol (semantics-spec §1.5.3). `receiptsDigest` is a convenience output for debugging/UX, not proven or committed. |

### 5.2 Witness Inputs (Per Batch)

* Tx list (typed)
* For every base read cell opened from committed state:

  * `(t, c, r, value, open_proof)`
* For every written cell:

  * `(t, c, r, new_value)`
* Any authorization material (signatures, nonces) if enforced at proof level

---

## 6. Layer B — Intra-Tx Core Semantics (Mandatory)

> **Assumption**: All programs satisfy the Core IR contract and canonical state normal form defined in [semantics-spec.md](./semantics-spec.md). This section summarizes the proof-relevant consequences.

### 6.1 Execution Model (B)

> **Full definition**: see [semantics-spec.md](./semantics-spec.md) §2.1–§2.2.

Each transaction executes as a pure function `execute(BaseState, tx) → WriteSet`. Key properties:

* **Unique-Read**: each base cell opened at most once per tx (IR structural invariant).
* **Unique-Write**: at most one `Write` instruction per key per tx (IR structural invariant).
* **No-Read-After-Write**: read-your-writes via SSA reuse, not re-opening base state.
* `WriteSet = { k -> val for each Write(k, val) in the IR body }` — exactly one entry per written key.
* **Null** is represented as `(val=0^{w(T)}, val_is_null=1)` — key absence, not a value.

### 6.2 Intra-Tx IR Rules and Canonical Normal Form

> **Full definition**: see [semantics-spec.md](./semantics-spec.md) §1 (Core IR Contract) and §2 (Canonical State Normal Form).

The proof system assumes all programs satisfy these invariants (proven in semantics-spec):

1. **Unique-Read**: each base cell opened at most once per tx (IR structural invariant, semantics-spec §2.3).
2. **Unique-Write**: at most one `Write` instruction per key per tx (IR structural invariant, semantics-spec §2.3).
3. **No-Read-After-Write**: read-your-writes via SSA reuse, not re-opening (IR structural invariant).
4. **True SSA**: each destination slot assigned at most once. Def-before-use.
5. **Static Coordinates**: `(t, c)` are compile-time constants.
6. **Budgets**: `max_ops`, `max_slots`, `max_accesses` bound trace dimensions.

#### SSA Trace Wiring (Proof-Side)

The STARK must enforce **def-use equality** for SSA values. Two equivalent trace layouts:

**Layout A (Slot columns + carry constraints; default for Layer B).**
* `S` slot columns in the trace. When slot `s` is defined at row `i`: `slot_s[i] = f(operands)`, then `slot_s[r] = slot_s[r-1]` for subsequent rows.
* Simple wire/copy constraint. NOT a RAM argument.

**Layout B (ValueTable + LogUp def-use; planned optimization).**
* Separate `ValueTable` commits `(value_id -> value)` once. Each operand use performs a LogUp lookup.
* Lower trace width; higher LogUp overhead.

### 6.3 B Proof Obligations

For a single tx:

1. **Base openings correctness**
   For every base read `(t,c,r)`, prove it matches `oldRoot` via:
   * **Root inclusion**: prove `Com[t,c]` is part of `oldRoot` via `SMT_tables` + `SMT_cols` inclusion proofs (§4.2).
   * **Cell opening**: prove `(r, value)` is committed in `Com[t,c]` via `VC.Verify(Com[t,c], r, value, proof)`.

2. **Instruction correctness**
   AIR/constraints prove that each instruction’s output matches the computation over returned read values.

3. **Intra-tx consistency (no RAM argument needed)**

   Enforced by the canonical state normal form ([semantics-spec.md](./semantics-spec.md) §2):

   * **State cells**: IR NF invariants (unique-read, unique-write, no-read-after-write) guarantee at most one base opening and one write per key.
   * **Local variables**: True SSA + trace wiring (Layout A carry / Layout B LogUp) guarantees def-use equality (§6.2).

   No intra-tx RAM-consistency argument is needed.

4. **State transition correctness**
   Prove `newRoot` is obtained by applying the tx's `WriteSet` to `oldRoot` using VC update proofs.

5. **Failed transaction policy (AppliedTxDigest)**
   The proof statement covers **only the applied (successful) transactions**. Failed transactions are excluded from the trace entirely.

   * **Public input**: `AppliedTxDigest = Poseidon(applied_tx_0 || applied_tx_1 || ...)` — a commitment to the ordered list of successfully applied transactions.
   * The proof guarantees: "executing the applied tx list against `oldRoot` produces `newRoot` correctly."
   * The prover **cannot fabricate a success** (instruction correctness and Assert constraints prevent it).
   * The prover **can omit transactions** (treat them as failed/dropped). `AppliedTxDigest` as public input makes this auditable — anyone can compare the applied list against the submitted list.
   * **Censorship resistance** (ensuring the prover doesn't drop valid txs) is a sequencer/policy concern, not a proof concern. It is handled at the protocol layer above the proof system.
   * **Failure proofs** (proving Assert was legitimately false for excluded txs) are deferred as an optional future upgrade.
   * **Optional upgrade**: Ok-gating canonical form — see [semantics-spec.md](./semantics-spec.md) §2.7.

> **Important:** If you chain tx one-by-one (even before implementing C), you can already validate end-to-end behavior: `root0 -> root1 -> root2 -> ...` where each link is a B proof.

---

## 7. Layer C — Inter-Tx Sequential Batch (Next Step)

Layer C defines a **single proof** for a batch of transactions executed in order, while minimizing base openings and updates.

### 7.1 C Semantics

The batch has:

* A fixed `oldRoot` (base snapshot)
* A batch state map `BatchState` holding the latest value of touched keys so far

For each transaction in sequence:

* reads consult `BatchState` first, else base snapshot
* writes update `BatchState`

At the end:

* `WriteSet_batch_final = { (k -> BatchState[k]) }` (coalesced across the entire batch)
* Apply updates once to get `newRoot`

### 7.2 Why C Exists (Beyond B)

C provides:

* **Correct inter-tx RAW semantics** (tx i+1 sees tx i’s writes)
* **Fewer base openings** (a base cell is opened once per batch, then cached)
* **Fewer state updates** (apply only final coalesced writes, not per-tx)

But C introduces the key proof challenge:

> How do we prove that reads served from `BatchState` return exactly the most recent writes in the batch?

That is a RAM consistency problem — except the "memory" is only the **touched set**, which is small.

---

## 8. C Memory Consistency Proof (The Core Cryptographic Design)

### 8.1 Key Idea: Per-(table,column) Memory Tables

Because `(t,c)` are static, we prove RAM consistency separately for each touched `(t,c)`.

For each fixed `(t,c)`, define:

* `InitSet[t,c] := { r | (t,c,r) appears in AccessLog[t,c] }` — every touched key gets exactly one init row at `τ=0`
* `AccessLog[t,c]`: chronological list of all reads/writes to that column during batch execution

We will build a **SortedMemTable[t,c]** sorted by `(r, time)` that allows local transition constraints. In practice, all per-(t,c) tables are merged into a single **GlobalSortedMem** table with `is_real` gating and `same_group` segment boundaries — see §8.8.

### 8.2 Tables and Columns (Per (t,c))

We represent each memory event as a row with:

* `r`: row key
* `τ`: timestamp (monotone increasing for real accesses)
* `is_init ∈ {0,1}`
* `is_write ∈ {0,1}`
* `val`: the value observed (read) or written (write) — `𝔽^{w(T)}` (Tier 2 trace encoding, §10.3)
* `val_is_null ∈ {0,1}`: 1 iff the value is null (Tier 2 metadata, §10.3). When `val_is_null = 1`, `val = 0^{w(T)}`.
* `mem`: the "current stored value" after processing this row (aux column) — `𝔽^{w(T)}`
* `mem_is_null ∈ {0,1}`: tracks null state of `mem` through transitions

We include **init rows**:

* For each `r ∈ InitSet[t,c]`, add a row:

  * `(r, τ=0, is_init=1, is_write=0, val=init_value, val_is_null=n, mem=init_value, mem_is_null=n)` where `n = 1` if `r` does not exist in base state, `0` otherwise.

Init rows seed the per-key memory value but **do not count as user writes** — `is_write = 0`. This is critical: a key that is only *read* (never written during the batch) must not appear in `WriteSet_batch_final` (see §8.6).

**Init row format constraints (enforced in AIR)**:
* `is_init ∈ {0,1}`, `is_write ∈ {0,1}`, `val_is_null ∈ {0,1}`, `mem_is_null ∈ {0,1}`
* `is_init = 1 ⇒ τ = 0`
* `is_init = 0 ⇒ τ ≥ 1`
* `is_init = 1 ⇒ is_write = 0` (init rows are not effective writes)
* `is_init = 1 ⇒ mem = val ∧ mem_is_null = val_is_null` (init seeds memory)
* `val_is_null = 1 ⇒ val = 0^{w(T)}` (null payload is zero)
* `mem_is_null = 1 ⇒ mem = 0^{w(T)}` (null memory is zero)

* **Init value source**:

  * If `r` exists in base state: `init_value` is the committed value, proven via `VC.Verify(Com_old[t,c], r, init_value, π)`.
  * If `r` does NOT exist in base state (first write to a new key): `init_value = Null` (field zero). Proven via the column's non-membership mechanism:
    * **SSMC columns**: gap witness — `k_i < r < k_{i+1}` proves `r` is absent.
    * **SMT columns**: opening to the default (all-zero) leaf at path `r`.

And **access rows** from execution:

* For each execution step that touches `(t,c,r)` at time `τ>0`:

  * If it’s a read: `(is_write=0, val=read_value)`
  * If it’s a write: `(is_write=1, val=written_value)`

### 8.3 Local Transition Constraints in SortedMemTable

Let row `i` and `i+1` in `SortedMemTable[t,c]` be consecutive.

**Sorting constraint (done by prover, checked by constraints):**

* `(r_i, τ_i) ≤lex (r_{i+1}, τ_{i+1})`

**Memory consistency constraint:**

* If `r_{i+1} = r_i` (same address):

  * If `is_write_{i+1} = 0` (read):

    * `val_{i+1} = mem_i` and `val_is_null_{i+1} = mem_is_null_i`
    * `mem_{i+1} = mem_i` and `mem_is_null_{i+1} = mem_is_null_i`
  * If `is_write_{i+1} = 1` (write):

    * `mem_{i+1} = val_{i+1}` and `mem_is_null_{i+1} = val_is_null_{i+1}`
* If `r_{i+1} ≠ r_i` (new address starts):

  * Then row `i+1` must be an init row (`τ=0, is_init=1, is_write=0`).
    **This spec requires an init row for each touched address** to keep the proof clean. The memory consistency constraint still applies: `mem_{i+1} = val_{i+1}` and `mem_is_null_{i+1} = val_is_null_{i+1}` (init rows set the initial memory value).

**Init row uniqueness**: Each `(t,c,r)` may have at most one init row (`τ=0`). Enforced by: within a `same_group` segment, if `r_{i+1} = r_i` and `τ_i = 0`, then `τ_{i+1} > 0`. This prevents duplicate init rows for the same key.

This ensures: **every read equals the most recent write (or init) for that address.**

### 8.4 Multiset Link: Execution Accesses == Sorted Memory Events (Up to Init Rows)

We must connect:

* the accesses claimed by the execution trace (`AccessLog[t,c]`)
* the sorted memory table rows (`SortedMemTable[t,c]` excluding init rows)

We do this via **LogUp** (logarithmic derivative argument):

* Let `U[t,c]` be the multiset of fingerprints of execution access rows
* Let `S[t,c]` be the multiset of fingerprints of sorted table rows with `is_init=0`

Prove `U[t,c] = S[t,c]` using LogUp: `Σ 1/(γ + Φ(exec_access_i)) = Σ 1/(γ + Φ(sorted_noninit_j))`.

**Fingerprint** (namespaced by `(t,c)` to prevent cross-group collisions):

* `Φ(row) = α*t + β*c + a*r + b*τ + d*is_write + f*val_is_null + Σ_j e_j * val[j]`
  with random coefficients `(α,β,a,b,d,f,e_0..e_{w(T)-1})` derived by Fiat–Shamir.

Note: `val ∈ 𝔽^{w(T)}` is multi-element, so the fingerprint expands each component with its own challenge coefficient. The `val_is_null` flag (Tier 2, §10.3) is included to prevent confusion between null and zero.

This is implemented using Plonky3's native LogUp machinery (~5-15 constraints per access row including auxiliary columns). A single global LogUp argument can be used (with `(t,c)` in the fingerprint), or multiple per-(t,c) arguments — both are correct.

### 8.5 Base Snapshot Binding (Init Rows ↔ oldRoot Openings)

Each init row carries `init_value` which must match committed state at `oldRoot`.

For every `(t,c,r)` in `InitSet[t,c]`:

* Provide **root inclusion proof** showing `Com_old[t,c]` is part of `oldRoot` (via `SMT_tables` + `SMT_cols` paths, §4.2).
* Provide opening proof `π` s.t. `VC.Verify(Com_old[t,c], r, init_value, π)=true`

The proof mechanism depends on the column's commitment strategy:

* **SSMC columns**: The entire column is laid out as an SSMC Table (§4.2). Membership: LogUp lookup proves `(r, init_value)` exists in the table. Non-membership: LogUp lookup for a gap witness row where `key < r < next_key` (or boundary case), plus range-check constraints on the bounds. The SSMC table's sorted uniqueness and commitment hash are verified once; individual openings use LogUp.
* **SMT columns**: Standard 64-level Merkle path verification. Cost: `O(64) × PoseidonPermCost` constraints per opening.

All verification happens **inside** the STARK proof (single artifact).

### 8.6 Producing `WriteSet_batch_final` and `newRoot`

After sequential execution, define:

* `WriteSet_batch_final[t,c] = { (r -> mem_last(r)) }` for keys where an effective write (`is_write=1`) occurred at least once.
  Only keys that were actually written during the batch are included — read-only keys are excluded even though they appear in GlobalSortedMem.
  Since init rows have `is_write=0` (§8.2), they do not count as writes.

**Effective write predicate**: `is_eff_write := is_write`. Since init rows have `is_write=0`, this naturally excludes them. The `is_eff_write` predicate is used for both "has this key been written?" tracking and write-set extraction.

**WriteSet extraction (provable in AIR).** We extract the batch write set from `GlobalSortedMem` using two auxiliary boolean columns:

**`same_key` (zero-test gadget)**: For consecutive real rows `i`, `i+1` in the same `(t,c)` segment:

* `same_key_{i,i+1} ∈ {0,1}`: 1 iff `r_{i+1} = r_i`.

  Since `r` is a u64 represented as 3 BabyBear limbs (§4.2.R), equality detection uses the u64 zero-test gadget. Concretely, define `δ_j = r_{i+1,j} - r_{i,j}` for each limb `j ∈ {0,1,2}`, and combine via random challenge: `diff = δ_0 + α · δ_1 + α² · δ_2`. Introduce auxiliary column `inv_i` and enforce:

  * `same_key_i ∈ {0,1}` (boolean constraint)
  * `diff_i · inv_i = 1 - same_key_i` (if diff ≠ 0, forces same_key = 0)
  * `diff_i · same_key_i = 0` (if same_key = 1, forces diff = 0)

  **Soundness**: if `diff ≠ 0`, the first constraint forces `same_key = 0`; if `diff = 0`, the second is trivially satisfied and `same_key = 1` is allowed. **Completeness**: the prover sets `inv = diff⁻¹` when `diff ≠ 0` and `inv = 0` when `diff = 0`. Cost: 3 constraints + 1 auxiliary column. Only applied when `same_group ∧ is_real_i ∧ is_real_{i+1}`.

**`is_last_for_key`**: For each real row `i`:

Let `next_same_group_i := same_group_{i,i+1} · is_real_{i+1}`. Then:

* `is_last_for_key_i = 1 - (next_same_group_i · same_key_{i,i+1})` for real rows
* `is_last_for_key_i = 0` for padding rows

(Equivalently: last iff "no next row for the same key".)

**`has_written` (running flag)**: A per-key running flag that becomes 1 once any effective write to that key has occurred within its key-run in sorted order. Let `w_i := is_eff_write_i`. Then:

* When `same_key_{i,i+1} = 1`: `has_written_{i+1} = has_written_i + w_{i+1} - has_written_i · w_{i+1}` (boolean OR)
* When `same_key_{i,i+1} = 0` (new key begins): `has_written_{i+1} = w_{i+1}`

All gated by `same_group ∧ is_real_i ∧ is_real_{i+1}`.

**UpdateSet definition**: The batch write set is the multiset of rows satisfying `is_real ∧ is_last_for_key ∧ has_written`. Each such row contributes the tuple `(t, c, r, mem)`. This ensures:

* Read-only keys (no effective writes) are excluded.
* Each written key contributes exactly one final value (the last `mem` for that key).

The UpdateSet is linked to the SSMC merge trace's WriteSet (or SMT update set) via LogUp.

Then apply all unique writes to base commitments:

* For each `(t,c)` with writes, prove `Com_new[t,c]` is obtained correctly from `Com_old[t,c]` via VC update proofs (SSMC: merge proof, SMT: path updates). The ColumnMeta table wires `Com_new` to the `newRoot` inclusion proof.
* Finally derive `newRoot` from `Com_new` values via `SMT_tables` update.

### 8.7 Timestamp Binding (Clock Constraints)

Layer C's RAM consistency relies on `(r, τ)` ordering to ensure reads observe the **most recent** prior write (or init) for the same key. Therefore, the `τ` values carried by access events MUST be **bound to the execution trace order** by explicit AIR constraints. `τ` MUST NOT be an unconstrained witness — otherwise a prover could reorder events to violate sequential semantics.

We define an **access counter** `clk` derived from the instruction trace:

**Instruction-level access flag**: Each instruction row `i` has a boolean flag:

```
is_access_i ∈ {0,1}
```

* `is_access_i = 1` iff the instruction is `Read` or `Write` (emits an access event)
* otherwise `is_access_i = 0`

**Clock recurrence**:

```
clk_0 = 0
clk_{i+1} = clk_i + is_access_i
```

This guarantees `clk_i` is exactly the number of accesses emitted up to (but not including) row `i`.

**Access timestamp assignment**: For each access event emitted at instruction row `i`:

```
τ = clk_i + 1
```

The `+1` offset ensures the first access gets `τ=1`, avoiding collision with init rows which use `τ=0`. Init rows use the reserved timestamp `τ = 0`. All real accesses MUST satisfy `τ ≥ 1`.

**Binding on both sides of LogUp**: The timestamp `τ` appears in two places:

* **(a) Execution access log** (unsorted multiset `U[t,c]`): Each instruction row that emits an access (`is_access=1`) produces an access-log entry whose `τ` is constrained by the AIR to equal `clk_i + 1` from the instruction trace. This is a direct column equality constraint in the AIR.
* **(b) GlobalSortedMem** (sorted multiset `S[t,c]`): The non-init rows carry the same `τ` values. LogUp proves `U = S` as multisets (§8.4), which ensures the sorted side's timestamps match the execution side exactly.

Because the execution-side `τ` is constrained to `clk_i + 1` (not a free witness), and LogUp forces `U = S`, neither side can be manipulated independently.

**Consequence**: `clk` is monotonically increasing and tied to instruction ordering. A prover cannot "move" a read event in time to incorrectly observe a future write. The multiset linking (LogUp) and the sorted transition constraints (§8.3–§8.4) jointly enforce correct sequential semantics.

> **Implementation note:** This supports both "one row per instruction" traces and compressed traces, as long as a canonical `is_access` flag, `clk` recurrence, and `τ = clk + 1` equality are enforced in the AIR.

### 8.8 Global Sorted Memory Table

All per-(t,c) SortedMemTable rows are represented in a single **GlobalSortedMem** table (following the globalization pattern from §4.2.G):

| Column | Description |
|--------|-------------|
| `t, c` | table and column identifiers |
| `r` | row key |
| `τ` | timestamp (bound to instruction clock, §8.7) |
| `is_init` | 1 for init rows |
| `is_write` | 1 for write rows |
| `val` | `𝔽^{w(T)}` — observed/written value (Tier 2 trace encoding, §10.3) |
| `val_is_null` | 1 iff value is null (Tier 2 metadata); when 1, `val = 0^{w(T)}` |
| `mem` | `𝔽^{w(T)}` — running memory value after this row |
| `mem_is_null` | tracks null state of `mem` through transitions |
| `is_real` | 1 for real rows, 0 for padding |

All sorting and transition constraints from §8.3 are **gated by `is_real`** and **`same_group`** (adjacent rows must share `(t,c)`). Specifically:
* Sorting: `(r_i, τ_i) ≤lex (r_{i+1}, τ_{i+1})` only when `same_group ∧ is_real_i ∧ is_real_{i+1}`
* Memory consistency: transition constraints only when `same_group ∧ is_real_i ∧ is_real_{i+1}`

**Segment-first init constraint**: The first real row of each `(t,c)` segment MUST be an init row. Define `is_first_in_group_i` as true when:
* `i = 0`, or
* `same_group_{i-1,i} = 0` (previous row belongs to a different group), or
* `is_real_{i-1} = 0` (previous row is padding — should not occur given prefix property, but included for completeness)

Then enforce: `is_first_in_group_i ∧ is_real_i ⇒ (is_init_i = 1 ∧ τ_i = 0 ∧ is_write_i = 0 ∧ mem_i = val_i)`

This, combined with the existing rule that `r_{i+1} ≠ r_i` within a group requires an init row, guarantees that every key-group starts with a properly initialized memory value.

The LogUp argument linking execution accesses to sorted memory rows (§8.4) uses the namespaced fingerprint `Φ(row) = α*t + β*c + a*r + b*τ + d*is_write + f*val_is_null + Σ_j e_j * val[j]`, preventing cross-group collisions.

**LogUp domain separation**: SSMC membership lookups (§4.2) and SortedMemTable multiset-equality (§8.4) both use LogUp within the same proof. To prevent cross-contamination, each LogUp instance uses **independent challenge sets** (separate `γ` values derived via Fiat-Shamir with distinct domain tags), or equivalently, each LogUp instance operates on a distinct set of columns.

---

## 9. Complexity Analysis

Let:

* `A` = total number of accesses (reads+writes) in batch
* `U` = number of unique touched keys in batch
* `G` = number of touched `(table,column)` groups
* `m_g` = materialized row count for column group `g`
* `w_g` = write count for column group `g`

In typical Tabula workloads: `A` is small (tx touches tens of cells), `U` is small, `G` is much smaller than total schema size.

### Hybrid State Commitment Costs (Parametric)

Let:
* `P` = Poseidon permutation constraint cost (implementation-dependent)
* `R` = u64 integer comparison constraint cost (§4.2.R borrow-chain gadget, ~8-12 constraints per comparison pending benchmark)
* `L` = LogUp per-access cost (~5-15 constraints, Plonky3 native)
* `P_stream` = amortized Poseidon streaming cost per `(key,value)` entry (depends on hash-chain vs sponge realization; hash-chain: `P` per entry; sponge: `P / rate` amortized)

| Component | SSMC columns (m ≤ threshold) | SMT columns (m > threshold) |
|-----------|------------------------------|----------------------------|
| Column commitment setup | `m × P_stream` (streaming hash) + `m × R` (sorted uniqueness) | N/A (commitment is the root) |
| Per-read verification (membership) | `L` (LogUp lookup) | `64 × P` (Merkle path) |
| Non-membership | `L` + `2R` (gap witness + range checks) | `64 × P` (opening to default leaf) |
| State update | `O(m+w) × (R + L + P_stream)` (merge proof) | `w × 64 × P` (Merkle path updates) |
| Memory consistency (LogUp) | `A × L` per (t,c) group | `A × L` per (t,c) group |

### Break-Even Analysis and Workload-Dependent Advantage

The hybrid advantage depends on the column-size distribution and access pattern. Define the **SSMC break-even size** `m*` as the column size at which the total SSMC cost equals the SMT opening/update cost for the same workload:

```
m* · (R + P_stream) + A_g · L + (m* + w_g) · M_row ≈ U_g · D · P
```

where `A_g` = accesses to group `g`, `U_g` = unique keys in `g`, `D` = SMT depth (64), `M_row` = merge-row cost.

**When SSMC wins** (hybrid advantage is significant):
* Columns are small (`m ≪ m*`) and frequently accessed
* Many accesses per column (amortizes the O(m) setup cost)
* Few columns touched overall (G is small)

**When SSMC loses** (hybrid advantage diminishes or reverses):
* `m` approaches or exceeds `m*` — the O(m) hash chain cost dominates
* Many columns touched with few accesses each — G × m grows while per-column access count is small (each column pays full O(m) setup for few lookups)
* Workload is dominated by large columns (most traffic goes through SMT anyway)

**The hybrid is not universally superior.** When most columns exceed `m*`, the primary benefit reduces to the inter-tx key-local memory argument savings (§8) rather than per-column commitment savings. The exact value of `m*` depends on the Poseidon streaming realization; analytical estimates suggest `m*` falls in the range of **100–300 rows** for the hash-chain prototype. This MUST be calibrated via Plonky3 benchmarks (B7).

### Qualitative Comparison (25 unique keys, 5 columns, 100 rows per column)

With hybrid (all columns ≤ threshold → SSMC):
* SSMC setup: `5 × (100 × R + 100 × P_stream)` — **O(500 × (R + P_stream))**
* Per-access opening: `25 × L` — **O(25 × L)**
* Merge proof (update): `5 × (100+w) × merge_row_cost` — **O(500 × (R+L+P_stream))**
* LogUp memory consistency: `25 × L` — **O(25 × L)**
* **Dominant cost: O(m × G × (R + P_stream))**

Without hybrid (SMT only):
* SMT openings: `25 × 64 × P` — **O(1600 × P)**
* SMT updates: `w × 64 × P` — **O(additional 64P per write)**
* **Dominant cost: O(U × 64 × P)**

For this specific workload (m=100, small columns), SSMC is expected to have significantly lower constraint cost than SMT — the exact ratio depends on benchmarked values of P, R, and P_stream (see B7).

### Compared to zkVM

* zkVM pays memory checking over *millions* of accesses and dynamic addresses via a global sorted-memory argument.
* Tabula pays memory checking over **touched keys only**, sharded by `(t,c)`, reducing the sorted-memory trace size by orders of magnitude for state-heavy workloads.
* For workloads where `(t,c)` sharding applies (structured state, not flat byte arrays), Tabula's memory argument cost scales with the touched set size, not the total execution step count.

---

## 10. Design Decisions (Resolved)

### 10.1 Commitment Choice (State VC) — Hybrid SSMC + SMT

**Decision: Per-column automatic selection between SSMC and SMT, based on column size.**

* **SSMC** (columns ≤ threshold): Sorted sparse map commitment. Prover provides entire column as a sorted AIR trace sub-table with `(key, value, next_key, is_first, is_last)` columns. In-circuit: sorted uniqueness via range checks, domain-separated Poseidon hash chain for commitment, per-cell membership/non-membership via **LogUp lookups** into the SSMC table (not array indexing). Update correctness via **3-way merge proof** (OldList + WriteSet → NewList). Cost: O(m) setup + O(LogUp) per access + O(m+w) for updates.
* **SMT** (columns > threshold): Sparse Merkle tree with Poseidon, 64-level (binary). Cost per opening/update is O(64) hashes ≈ ~15,360 constraints. Future: 4-ary/8-ary to reduce depth.
* **Threshold**: 500 materialized rows (default). Exact crossover calibrated after benchmarking. Initial policy uses `m` only; later evolve to (m, U, W)-based selection.
* **Hash function**: Poseidon for all in-circuit hashing (both SSMC and SMT).
* **Root structure**: Two-level SMT (`SMT_cols` per table, `SMT_tables` global) with inclusion proofs for each touched `Com[t,c]` (§4.2). Domain-tagged leaf digests include column type tag (0=SSMC, 1=SMT).
* **Memory consistency**: LogUp argument for all read/write access verification, regardless of column commitment strategy.
* Strategy selection at witness generation time — both produce a `Digest` that feeds into the same table/state root aggregation.

**Why SSMC, not "ColHash"**: A naive column hash (just hash the values) cannot prove per-cell membership or non-membership. SSMC adds the missing opening mechanism: sorted uniqueness enables membership (LogUp proves key exists in table) and non-membership (LogUp proves gap witness brackets the missing key) proofs. The 3-way merge proof ensures update correctness — the prover cannot silently modify entries not in the write set. This is essential for base snapshot binding (§8.5) and state transition correctness (§6.3.4).

**Why hybrid**: SMT dominates constraint cost for small-table workloads. SSMC eliminates this bottleneck for columns below the break-even size `m*` (estimated 100-300 rows, §9). The advantage is workload-dependent and MUST be calibrated via benchmarks.

**Why not PCS-VC**: Polynomial commitments (KZG/FRI) offer O(1) opening verification but require trusted setup (KZG) or complex update mechanics (FRI). The hybrid approach is transparent, update-friendly, and optimized for Tabula's mixed column sizes.

### 10.2 Row Key Model

**Decision: Sparse key space with `u64` keys.**

* Row keys are arbitrary `u64` values (account IDs, item IDs, etc.).
* The SMT naturally handles sparse keys: unused leaves are default (zero).
* Non-membership: proving a leaf is zero/default at path `r` is just a standard Merkle opening to the default value — no separate non-membership proof type needed.

**Why not fixed domain**: A dense `2^64` vector is impractical. Even `2^20` would waste space for tables with few rows. Sparse Merkle is the natural fit for Tabula's key-value model.

### 10.3 Value Encoding — Schema-Typed Digest-Native Two-Tier

**Decision: Per-column schema-typed encoding with two tiers (commitment vs trace), digest-native hash output. No runtime type tag.**

#### Core Principle

In Tabula, every column has a schema-declared type (§3.2 Static Coordinates), and `(t,c)` is static in IR. Therefore, the value encoding width `w(T)` is a **compile-time constant per column**, determined by the column's schema type. No runtime type tag is needed — the column schema provides the type.

#### v1 Proof-Scope Value Types

| Type | Application (`Value` enum) | Proof Encoding | `w(T)` | Limb Constraints |
|------|---------------------------|---------------|--------|------------------|
| Bool | `Value::Bool(b)` | `[b]` | 1 FE | `b·(b-1) = 0` |
| U64 | `Value::U64(x)` | `[x0, x1, x2]` | 3 FE | §4.2.R u64 limb decomposition |
| I64 | `Value::I64(x)` | `[y0, y1, y2]` where `y = x + 2^63` | 3 FE | Offset encoding (see below), then u64 limb decomposition |
| Digest | `Value::Bytes32(h)` | `[f0, ..., f7]` | 8 FE | Native BabyBear elements (no range-check needed) |

**Null** is not a value type — it represents **absence** (key does not exist). Null is handled structurally: SSMC non-membership, SMT default leaf, or `val_is_null` flag in trace rows (see Tier 2 below).

#### I64 Offset Encoding

Signed integers are mapped to unsigned via constant offset, enabling reuse of all u64 gadgets (§4.2.R):

```
encode(x: i64) = (x as u64).wrapping_add(2^63)
decode(y: u64) = (y.wrapping_sub(2^63)) as i64
```

Mapping: `i64::MIN (-2^63) → 0`, `0 → 2^63`, `i64::MAX (2^63-1) → 2^64-1`.

The offset constant `K = 2^63` has limb representation `(0, 0, 8)` (since `2^63 = 8 · 2^60`).

**Properties:**
* **Order-preserving**: `a >_signed b ⟺ encode(a) >_unsigned encode(b)`. All comparison gadgets from §4.2.R apply directly.
* **Arithmetic**: `encode(a + b) = encode(a) + encode(b) - K`. In the AIR: limb-wise subtraction of constant K with borrow propagation. Cost: same as u64 addition + 1 constant subtraction.
* **Overflow detection**: If `a + b` overflows i64, `encode(a) + encode(b) - K` overflows u64 — detected naturally by the borrow-chain gadget.

#### Digest-Native Encoding

The `Hash` IR instruction produces a **Poseidon2 digest** in proof mode — natively represented as BabyBear field elements, not a byte array.

**Poseidon2 over BabyBear** (standard Plonky3 configuration: width=16, capacity=8):
* Squeeze output: `[f0, f1, ..., f7] ∈ BabyBear^8`
* Security: ~124-bit collision resistance (birthday bound on 248-bit output). Consistent with SP1 and Plonky3 ecosystem practice.
* Each `f_i ∈ [0, p-1]` is already a valid BabyBear element — **no additional range-check needed**.

**Hash instruction AIR constraint chain:**
```
input_fes = concat(ComEnc(input_0), ..., ComEnc(input_n))
poseidon_sponge.absorb(input_fes)
[f0, ..., f7] = poseidon_sponge.squeeze()
slot[dst] = [f0, ..., f7]   // direct storage, no byte conversion
```

Cost: Poseidon permutation constraints only. **Zero byte-conversion overhead.** Compare with a raw Bytes32 encoding (9 FE from byte decomposition) which would require additional bit-decomposition + reconstruction constraints at every Hash instruction.

**32-byte serialization (out-of-circuit only):**
```
bytes = f0.to_le_bytes(4) || f1.to_le_bytes(4) || ... || f7.to_le_bytes(4)
// 8 × 4 bytes = 32 bytes = Value::Bytes32([u8; 32])
```

**Future extension**: `Bytes32Raw` type (9 BabyBear limbs: 8×31-bit + 1×8-bit = 256 bits) for raw byte-level operations on external data (addresses, external hashes). Not in v1 proof scope.

#### Two-Tier Encoding

**Tier 1 — Commitment Encoding (`ComEnc(T)`):**
* Used for: SSMC hash chain entries, SMT leaf values, merge trace `old_val`/`new_val`.
* Format: `w(T)` field elements. **No null flag, no type tag.**
* Rationale: Committed values are always non-null (null = key absence in SSMC/SMT). The column schema determines the type, so no tag is needed.
* SSMC hash chain entry cost: `key(3 FE) + ComEnc(T)(w(T) FE)` = {4, 6, 6, 11} FE for {Bool, U64, I64, Digest}.

**Tier 2 — Trace Encoding (`TraceEnc(T)`):**
* Used for: GlobalSortedMem `val`/`mem` columns, instruction trace slot columns.
* Format: `w(T)` field elements + `val_is_null` (1 FE, boolean) as a **row-level metadata column**.
* Constraint: `val_is_null ∈ {0,1}` and `val_is_null = 1 ⇒ val = 0^{w(T)}`.
* `val_is_null` is included in the LogUp fingerprint (§4.5) to prevent confusion between null and zero.

The two tiers compose cleanly:
* **Commitment layer** never encounters null — null is structural (absence).
* **Trace layer** encounters null in init rows (non-membership) and delete writes — represented by the `val_is_null` flag.
* **Merge trace** needs no null flag — delete is identified by the source encoding `(s1,s0) = (1,1)`, and `old_val`/`new_val` (where `in_new=1`) are always non-null.

#### Width-Class AIR Architecture

Since `w(T)` varies by type, global tables (GlobalSSMC, GlobalMerge, GlobalSortedMem) contain rows with different value widths across `(t,c)` segments. Two implementation strategies:

**Canonical (optimal)**: Width-class partitioning. Group `(t,c)` segments by their value width into separate AIR chips:

| Width Class | Types | `w(T)` | Chip |
|-------------|-------|--------|------|
| Narrow | Bool | 1 FE | `GlobalSSMC_narrow`, `GlobalSortedMem_narrow`, ... |
| Standard | U64, I64 | 3 FE | `GlobalSSMC_standard`, `GlobalSortedMem_standard`, ... |
| Wide | Digest | 8 FE | `GlobalSSMC_wide`, `GlobalSortedMem_wide`, ... |

Cross-chip wiring via LogUp: ColumnMeta determines which width class each `(t,c)` belongs to; execution trace instruction type routes to the correct SortedMem chip. Plonky3's multi-chip architecture (used by SP1 with 30+ chips) supports this natively.

**Simplification (implementation option)**: Use max-width (8 FE) for all value columns across all chips, zero-padding narrower types. Correct but suboptimal — wastes `(8-w(T))/8` of value column trace width for narrow types.

#### Quantitative Impact (vs. old "4 FE fixed + type tag")

SSMC hash chain for a U64 column, m=100 entries, Poseidon rate=8:

| Design | Entry FE (key+val) | Total hash input | Poseidon perms |
|--------|-------------------|-----------------|----------------|
| Old (tag + 4 FE) | 3 + 5 = 8 | 800 FE | 100 |
| New (ComEnc, w=3) | 3 + 3 = 6 | 600 FE | 75 |
| **Reduction** | **-25%** | **-25%** | **-25%** |

Bool column, m=100:

| Design | Entry FE | Total | Perms |
|--------|---------|-------|-------|
| Old | 8 | 800 | 100 |
| New (w=1) | 4 | 400 | 50 |
| **Reduction** | **-50%** | **-50%** | **-50%** |

**Record column packing**: Deferred. Not needed for B+C correctness.

### 10.4 Timestamp Semantics

**Decision: Global monotone counter across the entire batch, bound to instruction ordering by clock constraints (§8.7).**

* `τ` is the `LogicalTime` value from the executor, which auto-increments on every read/write across all transactions.
* Init rows use `τ=0` (reserved, before any real access).
* Real accesses start at `τ=1` and increase strictly.
* This matches the existing `ExecutionEvent.time` field in the executor — no new bookkeeping needed.
* **In-circuit binding**: `τ` is NOT an unconstrained witness. The AIR enforces `τ = clk_i + 1` for access events (where `clk` is a running access counter derived from the instruction trace) and `τ = 0` for init rows (see §8.7). This prevents time-manipulation attacks.

**Why not (tx_index, step_index)**: A single global counter is simpler, already implemented, and sufficient for ordering. Lex ordering adds complexity with no benefit.

### 10.5 Execution Trace Coupling

**Decision: One access row per Read/Write instruction. Lookup and all others emit zero.**

| Instruction | Access rows | Details |
|-------------|-------------|---------|
| `Read`      | 1 (is_write=0) | `(t, c, r, val, τ)` from base state |
| `Write`     | 1 (is_write=1) | `(t, c, r, val, τ)` |
| `Lookup`    | 0 | Static table reads are NOT memory events (see below) |
| `Add/Sub/Mul/DivMod` | 0 | Pure computation on slots |
| `Assert`    | 0 | Predicate evaluation only |
| `Hash`      | 0 | Computation only (see §10.7) |
| `Emit`      | 0 | Output only |

**Static table lookups** (`Lookup` instruction): Static tables are immutable and committed via `StaticTableRoot` (public input, §5.1). Lookup values are verified by a **LogUp/Lasso lookup argument** against the static table commitment — they do NOT participate in the state memory argument (`GlobalSortedMem`). Static tables are total functions: every valid `(static_table, c, r)` has a value; out-of-domain keys are program errors (see semantics-spec §1.5.4).

### 10.6 Intra-Tx Proof Strategy (No RAM Argument)

**Decision: No intra-tx RAM-consistency argument of any kind.** Enforced by the canonical state normal form ([semantics-spec.md](./semantics-spec.md) §2).

1. **State cells**: IR NF invariants (semantics-spec §2.3) guarantee unique-read, unique-write, and no-read-after-write as structural properties of the IR. The proof needs at most one base opening per `(t,c,r)` per tx.

2. **Local variables (SSA values)**: True SSA (semantics-spec §1.2) eliminates register-file consistency. The STARK enforces def-use equality via trace wiring:
   * Layer B default: **slot columns + carry constraints** (Layout A, §6.2).
   * Planned optimization: **ValueTable + LogUp def-use** (Layout B, §6.2).

Together, **no intra-tx RAM-consistency argument is needed** — neither for state cells nor for local variables.

In Layer C, the per-(t,c) memory argument (§8) handles **inter-tx** read-after-write consistency (tx i+1 reading tx i's writes). Intra-tx and inter-tx are complementary, not overlapping.

### 10.7 In-Circuit Hash Strategy

**Decision: Poseidon for all in-circuit hashing.**

* **Canonical `hash_id`**: Poseidon2 over BabyBear. See [semantics-spec.md](./semantics-spec.md) §1.6 for the semantic definition. Blake3 = non-canonical debug mode only.
* Inside the proof, `Hash` is verified using **Poseidon** constraints (~300 constraints per hash call).
* The witness generator MUST use the canonical hasher (Poseidon), not the Blake3 mock.
* Merkle tree hashing (SMT for state commitments, §10.1) also uses Poseidon.
* Total Poseidon cost per tx: ~(hash_calls × 300) + (SMT_openings × 64 × 300) constraints.

---

## 11. Implementation Plan (B then C)

### Prerequisites (before Phase B)

1. **T1-T3 trait polish** — `SigVerifier`/`NoncePolicy` → `Result<()>`, `hash_many` default method, remove `column_len`
2. **Plonky3 integration** — add `p3-*` crates as workspace dependencies behind feature flags
3. **Poseidon hasher** — `Hasher` trait impl using Poseidon over BabyBear
4. **Sparse Merkle Tree** — 64-level, Poseidon-based, with `Open`, `Verify`, `Update` (standalone, testable without STARK). Also used for the two-level root structure (`SMT_cols`, `SMT_tables`)
5. **SSMC** — sorted sparse map commitment: AIR trace sub-table + Poseidon hash chain + LogUp membership/non-membership + 3-way merge update proof (with delete support)

### Phase B: Single-tx end-to-end proof

**Goal**: Prove `oldRoot → newRoot` for one transaction.

1. **Two-level root structure** — `SMT_cols` (per table) + `SMT_tables` (global), with inclusion proof generation and verification for `Com[t,c]` → `oldRoot`/`newRoot`
2. **Hybrid VC layer** — per-column strategy selection (SSMC ≤ threshold, SMT > threshold), producing uniform digests
3. **Witness generator**: executor runs tx → collects `(ReadSet_old, WriteSet, slot trace)` → formats as AIR witness columns, including GlobalSSMC / GlobalMerge tables and SMT Merkle paths + root inclusion proofs
4. **AIR trace layout**: instruction column (with `is_access` flag + `clk` recurrence) + slot columns + access columns + GlobalSSMC table + GlobalMerge table + SMT path columns + root inclusion path columns. All variable-length tables use `is_real` gating and `same_group` segment boundaries (§4.2.G)
5. **Constraints**: instruction correctness (per-opcode), clock binding (§8.7), slot consistency (SSA), base opening verification (SSMC: LogUp + gap checks, SMT: Merkle path), root inclusion proofs, state update verification (SSMC: merge proof with delete support, SMT: path update)
6. **End-to-end test**: `Read → Add → Write` tx, generate proof, verify proof
7. **Chaining test**: `root0 → root1 → root2` via two B proofs
8. **Benchmark**: measure constraint count for SSMC vs SMT at various column sizes and access patterns, calibrate threshold, validate parametric cost formulas from §9

**Exit criteria**: A single-tx proof generates and verifies correctly. Both SSMC and SMT paths produce valid proofs. Root inclusion proofs bind `Com[t,c]` to `oldRoot`/`newRoot`. SSMC membership AND non-membership openings (via LogUp) work correctly. SSMC merge update proof (including delete) works correctly. Clock constraints bind `τ` to instruction ordering. Hybrid selection works correctly.

### Phase C: Batch proof with memory argument

**Goal**: Prove `oldRoot → newRoot` for a batch of N transactions in one proof.

1. **GlobalSortedMem** construction from batch execution events — per-(t,c) segments in a single global table with `is_real` gating and `same_group` boundaries (§8.8)
2. **Init row generation** with hybrid opening proofs (SSMC membership/non-membership or SMT opening) + root inclusion proofs
3. **Sorting + transition constraints** in AIR (range checks for ordering, read/write consistency), gated by `is_real ∧ same_group`
4. **Clock binding** — `τ = clk_i + 1` for access events, `τ = 0` for init rows (§8.7)
5. **LogUp argument** linking execution access log to GlobalSortedMem (namespaced fingerprint with `(t,c)`, §8.4)
6. **Write coalescing**: extract `WriteSet_batch_final` from last `mem` per written address (writes-only, §8.6)
7. **Single root update**: apply coalesced writes via hybrid VC, re-derive `newRoot` with root inclusion proofs
8. **AppliedTxDigest**: compute `AppliedTxDigest` from applied tx list as public input; only applied txs enter the access log
9. **End-to-end test**: multi-tx batch with inter-tx RAW dependencies, generate + verify

**Exit criteria**: A batch proof with inter-tx read-after-write semantics generates and verifies correctly. Memory consistency is proven per-(t,c) group via LogUp with namespaced fingerprints. Clock constraints prevent timestamp manipulation.

---

## 12. Summary

**B (intra-tx core)** is Tabula's identity: snapshot openings + SSA locals + final write set. It shrinks RAM consistency inside tx into a bounded, structured problem and reduces committed reads/writes to "one per key." Intra-tx consistency is enforced by the canonical state normal form ([semantics-spec.md](./semantics-spec.md) §2) — no RAM-consistency argument needed.

**C (sequential batch)** adds correct inter-tx semantics while keeping memory checking cheap by:

* maintaining only a small map of touched keys
* proving RAM consistency per `(table,column)` using **sorted memory tables + LogUp arguments**
* binding initial values to `oldRoot` via hybrid openings (SSMC membership/non-membership or SMT opening)
* producing `newRoot` via coalesced hybrid VC updates

**Resolved decisions**:

| Decision | Choice |
|----------|--------|
| STARK framework | **Plonky3** over **BabyBear** field (p = 2^31 − 2^27 + 1 = 2013265921) |
| State VC | **Hybrid**: SSMC (≤ threshold) + SMT (> threshold), auto-selected per-column. Threshold TBD (estimated 100-300 rows, calibrated via B7) |
| SSMC design | Sorted AIR trace sub-table + Poseidon hash chain + LogUp membership/non-membership + 3-way merge update proof |
| Hash function | **Poseidon** for all in-circuit hashing (canonical `hash_id`); Blake3 = non-canonical debug mode only |
| Memory consistency | **LogUp** argument (~5-15 constraints/access), per-(t,c) SortedMemTable |
| State root | Two-level SMT: `SMT_cols` per table + `SMT_tables` global, with inclusion proofs + **meta-level update proof** for `oldRoot → newRoot` |
| Row key model | Sparse u64 key space |
| Value encoding | **Schema-Typed Digest-Native Two-Tier** (§10.3): `w(Bool)=1, w(U64)=w(I64)=3, w(Digest)=8`; Tier 1 (commitment) = `w(T)` FE, Tier 2 (trace) = `w(T)` FE + `val_is_null` |
| Null semantics | `WRITE(k, Null)` = delete; merge trace supports `delete` case |
| Init rows | `is_write=0` (seed memory, not user writes); read-only keys excluded from WriteSet; explicit format constraints in AIR |
| Empty columns | Via ColumnMeta (`is_empty_old`, `is_empty_new`, `is_touched`); GlobalSSMC exists iff `is_empty_old=0`; GlobalMerge exists iff `is_touched=1` |
| Timestamp | Global monotone counter (`τ = clk_i + 1` for accesses, `τ = 0` for init), **bound to instruction clock** via AIR constraints (§8.7) |
| Trace layout | Global tables (GlobalSSMC, GlobalMerge, GlobalSortedMem, ColumnMeta) with `is_real` prefix property and `(t,c)` namespacing |
| ColumnMeta | Functional mapping `(t,c) → metadata`; strict sorted uniqueness; join lookups enforce existence rules |
| SSMC commitment | Poseidon sponge (variable-length); domain-separated by `(0x00, t, c)` |
| Intra-tx semantics | Canonical state NF ([semantics-spec](./semantics-spec.md) §2): unique-read, unique-write, no-read-after-write as IR structural invariants + True SSA. No intra-tx RAM-consistency argument. SSA wiring: Layout A (carry) or Layout B (LogUp) |
| Program budgets | `max_ops`, `max_slots`, `max_accesses`; optional per-(t,c) bounds. See [semantics-spec](./semantics-spec.md) §1.8 |
| Failed txs | Excluded from trace; `AppliedTxDigest` as public input. Optional: ok-gating ([semantics-spec](./semantics-spec.md) §2.7) |

**Why SSMC+SMT hybrid wins**: For columns below the break-even size `m*` (estimated 100-300 rows, calibrated via Plonky3 benchmarks), SSMC significantly reduces state commitment cost compared to SMT-only. The advantage is **workload-dependent** — it is strongest for small, frequently-accessed columns and diminishes for large columns or access patterns with high G×m cost (see §9 break-even analysis). Unlike a naive "column hash," SSMC provides proper per-cell membership and non-membership proofs via LogUp lookups into the sorted witness table, and provably correct updates via the 3-way merge proof. SMT remains available for large/sparse columns where loading the full witness is impractical.

This is the cleanest path to a working Tabula proof system with strong performance characteristics, before considering D-mode semantics.
