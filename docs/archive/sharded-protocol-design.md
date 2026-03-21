# Fully Sharded Protocol Design

> **Version**: 1.0
> **Date**: 2026-03-09
> **Status**: Design proposal
> **References**: [proof-hierarchy-and-grouping.md](proof-hierarchy-and-grouping.md), [commitment-architecture-research.md](commitment-architecture-research.md), [implementation-workplan.md](implementation-workplan.md), [master-roadmap.md](master-roadmap.md)

---

## 1. Motivation

The current Tabula proof system generates a **single monolithic proof** containing all chips
in one shared PCS. While this minimizes proof size (261 cols total), it forces sequential
proving and wastes padding for uneven column sizes.

A fully sharded architecture produces **C+2 independent proofs** (1 execution + C column +
1 root), enabling embarrassingly parallel proving with significant speedup:

| Metric (C=50, 100 rows/col) | Global (current) | Sharded | Ratio |
|------------------------------|-------------------|---------|-------|
| NTT ops (sequential)        | 27.8M             | 10.6M   | 2.6x faster |
| NTT ops (50-way parallel)   | 27.8M             | 212K    | ~131x faster |
| Padding waste                | 63% (5000→8192)   | 28% (100→128) | 2.3x less |
| Proof count                  | 1                 | 52      | — |
| Proof size (no recursion)    | ~264 KB           | ~4.9 MB | 18.5x larger |
| Proof size (with recursion)  | ~264 KB           | O(1)    | comparable |

**Design principle**: Optimize for prover speed now. Solve proof size with recursive
aggregation (Phase 5/D4) later. The architecture should make both paths natural.

---

## 2. Protocol Overview

### 2.1 Proof Decomposition

The monolithic proof is decomposed into three tiers:

```
Tier 1: Execution Proof (1, global)
  Chips: ExecutionChip, StaticTableChip
  Scope: All instructions across all transactions
  Public outputs: cumsum_exec (EF4), batch_digest

Tier 2: Column Proofs (C, parallel)
  For each (t, c):
    Chips: MemoryShard, StateShard, PoseidonLocal, RangeCheckLocal
    Scope: All memory accesses + state transitions for this column
    Public outputs: cumsum_col (EF4), Com_old (8 FE), Com_new (8 FE)

Tier 3: Root Proof (1, global, lightweight)
  Chips: SmtColPathChip, SmtTablePathChip
  Scope: SMT path verification + cross-proof bus balance
  Public inputs: all Com_old[t,c], Com_new[t,c], all cumsums
  Verifies: Σ cumsums = 0, old_root → new_root transition
```

### 2.2 Self-Containment Principle

Each column proof is **fully self-contained**:

- **MemoryShard**: sorted memory accesses, timestamp ordering, read consistency
- **StateShard**: SSMC old→new commitment transition (merge proof)
- **PoseidonLocal**: all Poseidon permutations needed by this column's SSMC hash chains
- **RangeCheckLocal**: all range checks for this column's keys, timestamps, and gaps

Internal buses (BaseStateEntry, CoalescedWrite, PoseidonPerm, RangeCheck) balance
**within** each column proof. The only cross-proof bus is the **Memory bus** (ReadAccess
+ WriteAccess), balanced via public cumsum values.

### 2.3 Bus Architecture

```
Cross-proof buses (balanced via public cumsums):
  ReadAccess (C10):   Execution sends  ←→  MemoryShards receive
  WriteAccess (C11):  Execution sends  ←→  MemoryShards receive

Intra-column buses (balanced within each column proof):
  BaseStateEntry (C13):     MemoryShard sends   →  StateShard receives
  CoalescedWrite (C14):     MemoryShard sends   →  StateShard receives
  PoseidonPerm (C5):        StateShard sends    →  PoseidonLocal receives
  RangeCheck (C8):          All shard chips send →  RangeCheckLocal receives
  CommitmentVerif (C6):     StateShard sends    →  (public output, not bus)

Intra-execution buses (balanced within execution proof):
  StaticTableLookup (C9):   Execution sends → StaticTable receives
  PoseidonPerm (C5):        Execution sends → (partial sum exported)
```

Note: Execution's PoseidonPerm and RangeCheck sends become cross-proof partial sums
unless Execution embeds its own local Poseidon/RangeCheck instances. See §4.2 for
the design choice.

---

## 3. Protocol Flow

### 3.0 Witness Generation

```
Input: Program, Batch, ExecutionResult, Schemas

Sequential:
  1. Lower IR → InstructionRecords (requires program + batch)
  2. Classify memory accesses by (t, c)

Parallel (per column):
  3. Build per-column sorted memory witness
  4. Build per-column SSMC merge witness
  5. Build per-column SMT paths
```

### 3.1 Phase 1: Main Trace Commit (parallel, C+1 way)

All proofs construct their main traces and commit independently:

```
Execution proof:
  Build ExecutionChip + StaticTableChip main traces
  Commit → C_exec

Column proof [i] (parallel for all i):
  Build MemoryShard + StateShard + PoseidonLocal + RangeCheckLocal main traces
  Commit → C_col[i]
```

Each proof uses its own independent PCS instance (separate Merkle tree).

### 3.2 Phase 2: Challenge Derivation (synchronization point)

All main commitments must be collected before LogUp challenges can be derived:

```
transcript = new FiatShamirTranscript()
transcript.observe(statement)              // public inputs
transcript.observe(C_exec)                 // execution commitment
for i in 0..C:
    transcript.observe(C_col[i])           // column commitments (deterministic order)
(α, β) = transcript.sample_pair()         // shared LogUp challenges
```

**Soundness requirement**: The prover must not know (α, β) before constructing main
traces. Deriving from commitments ensures this — the prover commits to traces before
challenges are revealed.

**Ordering**: Column commitments are observed in canonical `(t, c)` lexicographic order
to ensure deterministic transcript.

### 3.3 Phase 3: Permutation Traces (parallel, C+1 way)

All proofs compute their permutation traces using the shared (α, β):

```
Execution proof:
  Evaluate interactions → recorded sends/receives
  Generate perm trace with (α, β)
  cumsum_exec = final cumulative sum (EF4)
  Commit perm → C_exec_perm

Column proof [i] (parallel for all i):
  Evaluate interactions → recorded sends/receives
  Generate perm trace with (α, β)
  cumsum_col[i] = final cumulative sum (EF4)
  Commit perm → C_col_perm[i]
```

### 3.4 Phase 4: Quotient + FRI (parallel, C+1 way)

Each proof completes independently:

```
For each proof (parallel):
  1. Sample alpha from transcript (observe perm commitment)
  2. Compute quotient polynomials
  3. Commit quotient LDE
  4. Sample zeta from transcript
  5. Evaluate at OOD point (zeta)
  6. Run FRI opening proof

Output: independent proof object per proof instance
```

### 3.5 Phase 5: Root Proof (sequential, after all column proofs)

```
Input:
  - From column proofs: Com_old[i], Com_new[i], cumsum_col[i]  for all i
  - From execution proof: cumsum_exec
  - From statement: old_root, new_root

Root proof verifies:
  1. SMT inclusion: Com_old[t,c] consistent with old_root
  2. SMT inclusion: Com_new[t,c] consistent with new_root
  3. Bus balance: cumsum_exec + Σᵢ cumsum_col[i] = 0  (arithmetic check)

Output: root_proof
```

### 3.6 Verification

```
Verifier receives: {exec_proof, col_proof[0..C], root_proof}

1. Reconstruct (α, β) from commitments (same transcript as prover)
2. Verify exec_proof (constraints + FRI)
3. For each col_proof[i]: verify independently (constraints + FRI)
4. Verify root_proof (SMT paths + bus balance)
5. Check public value consistency across proofs
```

---

## 4. Design Decisions

### 4.1 Per-Column Poseidon and RangeCheck

**Decision**: Each column proof embeds its own PoseidonLocal and RangeCheckLocal chips.

**Rationale**: Making Poseidon/RangeCheck global would create additional cross-proof bus
dependencies (PoseidonPerm bus, RangeCheck bus). Each column's hash chain and range checks
are independent — embedding them locally makes column proofs fully self-contained.

**Cost**: Poseidon preprocessed trace (round constants) is duplicated C times. For a
28-row Poseidon permutation with 17 preprocessed columns, this is 28 × 17 = 476 field
elements per column — negligible.

**Alternative considered**: Global Poseidon/RangeCheck with cross-proof partial sums.
Rejected because it adds 2 more cross-proof bus balances and prevents true column
independence.

### 4.2 Execution Proof's Hash and Range Check

The ExecutionChip currently sends on PoseidonPerm bus (for Hash opcode) and RangeCheck bus.
Two options:

**Option A**: Execution proof embeds its own PoseidonLocal and RangeCheckLocal.
All buses balance within the execution proof. Zero cross-proof buses except Memory.

**Option B**: Execution's Poseidon/RC sends become additional cross-proof partial sums.
Requires tracking which column proof handles execution's hash/RC requests.

**Decision**: **Option A**. Execution proof is self-contained for Poseidon and RangeCheck.
Only Memory bus (ReadAccess + WriteAccess) crosses proof boundaries.

### 4.3 EmptyColRead Bus

The EmptyColRead bus (C12) currently goes from ExecutionChip → ColumnMetaChip. In the
sharded model:

- ExecutionChip sends EmptyColRead in the execution proof
- ColumnMeta functionality is split: per-column commitment in column proofs, SMT in root

**Decision**: EmptyColRead partial sum exported from execution proof. Root proof verifies
the balance against MetaShard public outputs (empty column indicators).

### 4.4 Challenge Derivation: Commit-Then-Derive

**Decision**: Derive LogUp challenges from all main commitments (not from public inputs
alone).

**Rationale**: LogUp soundness requires that the prover cannot predict (α, β) before
constructing traces. Public-input-derived challenges are predictable, allowing a
malicious prover to craft traces that balance by coincidence.

**Cost**: One synchronization point between Phase 1 (commit) and Phase 3 (perm trace).
All column proofs are still parallel within each phase.

### 4.5 Proof Size and Recursive Aggregation

The sharded protocol produces C+2 independent proofs, each with its own FRI opening.
Total proof size scales linearly with C. This is acceptable during development but
requires recursive aggregation for production.

**Near-term** (no recursion): C+2 proofs verified independently. Proof size ~O(C).
Acceptable for off-chain proving with on-chain state root verification.

**Long-term** (with recursion): Binary tree reduction of column proofs. Each node
verifies 2 inner proofs. Final proof is O(1). This is Phase 5/D4 work and does not
block the sharded architecture.

---

## 5. Proof Structure

### 5.1 Execution Proof

```rust
struct ExecutionProof {
    // Standard STARK proof fields
    main_commitment: PcsCommitment,
    perm_commitment: PcsCommitment,
    quotient_commitment: PcsCommitment,
    opening_proof: PcsOpeningProof,
    chip_openings: Vec<ChipOpening>,  // ExecutionChip, StaticTableChip, PoseidonLocal, RangeCheckLocal

    // Cross-proof public outputs
    cumsum_memory: EF4,               // partial sum for ReadAccess + WriteAccess buses
    cumsum_empty_col: EF4,            // partial sum for EmptyColRead bus
    batch_digest: [KoalaBear; 8],
}
```

### 5.2 Column Proof

```rust
struct ColumnProof {
    // Identity
    table_id: u32,
    col_id: u16,

    // Standard STARK proof fields
    main_commitment: PcsCommitment,
    perm_commitment: PcsCommitment,
    quotient_commitment: PcsCommitment,
    opening_proof: PcsOpeningProof,
    chip_openings: Vec<ChipOpening>,  // MemoryShard, StateShard, PoseidonLocal, RangeCheckLocal

    // Cross-proof public outputs
    cumsum_memory: EF4,               // partial sum for ReadAccess + WriteAccess buses
    com_old: [KoalaBear; 8],           // old SSMC commitment for this column
    com_new: [KoalaBear; 8],           // new SSMC commitment for this column
    is_empty_old: bool,
    is_empty_new: bool,
}
```

### 5.3 Root Proof

```rust
struct RootProof {
    // Standard STARK proof fields (SmtColPath + SmtTablePath chips)
    main_commitment: PcsCommitment,
    perm_commitment: Option<PcsCommitment>,
    quotient_commitment: PcsCommitment,
    opening_proof: PcsOpeningProof,
    chip_openings: Vec<ChipOpening>,

    // Verification inputs (from other proofs)
    column_commitments: Vec<ColumnCommitmentEntry>,  // Com_old, Com_new per (t,c)
    cumsum_total: EF4,               // must be zero
    statement: PublicStatement,       // old_root, new_root
}
```

### 5.4 Aggregate Proof Envelope

```rust
struct TabulaProof {
    execution: ExecutionProof,
    columns: Vec<ColumnProof>,        // C column proofs
    root: RootProof,

    // Shared data for verification
    statement: PublicStatement,
    main_commitments_digest: [u8; 32], // hash of all main commitments (for challenge reconstruction)
}
```

---

## 6. Column Proof Chip Widths

Each column proof contains 4 chips:

| Chip | W=1 | W=3 | W=8 | Role |
|------|-----|-----|-----|------|
| MemoryShard | 44 | 48 | 58 | Sorted memory, read/write consistency |
| StateShard | ~89 | 93 | ~103 | SSMC commitment transition |
| PoseidonLocal | 93 | 93 | 93 | Hash chain computation (width-independent) |
| RangeCheckLocal | 2 | 2 | 2 | Range check accumulation |
| **Total** | **~228** | **236** | **~256** | |

For comparison, the global equivalent (InterTxOrder + StateColumn + Poseidon + RangeCheck)
is 261 cols but shared across all columns.

Per-column proof opening: 236 FE per FRI query (W=3).
Total for C=50: 50 × 236 = 11,800 FE per query.
With execution (278 + 93 + 2 = 373) + root (~50): 11,800 + 373 + 50 ≈ 12,223 FE per query.

---

## 7. Gap Analysis: Current → Sharded

This section is based on a complete code-level audit of the `machine`, `witness`,
`stark`, and `chips` crates.

### 7.1 No Changes Needed

These components are already sharding-ready:

| Component | Location | Why It Works |
|-----------|----------|--------------|
| `ChipId(u16)` + `ChipIdAllocator` | `stark/src/chips.rs` | Open newtype, dynamic allocation, 65K IDs |
| `BusId(u16)` + `define_bus!` | `stark/src/air/bus.rs` | Multi-instance safe; same bus ID aggregates correctly |
| `ChipSpec`, `AnyRap`, `DynChip` | `stark/src/chips.rs`, `machine/src/any_rap.rs` | Stateless traits, per-instance independent |
| `TraceMap` | `stark/src/trace/trace_map.rs` | `BTreeMap<ChipId, TraceEntry>` — unlimited entries |
| MemoryShard, StateShard, MetaShard | `chips/src/shards/` | Implemented + tested (48, 93, 96 cols) |
| LogUp fingerprint computation | `machine/src/permutation/` | Pure function; multi-instance aggregation automatic |
| Inter-chip coupling | All chip `air.rs` | **Zero** direct code references between chips; all via LogUp buses |
| `MemoryModel`, `CommitmentScheme`, `RootProof` traits | `machine/src/composition.rs` | Interface supports sharded impls (add new structs) |
| PoseidonChip, RangeCheckChip | `chips/src/` | Stateless; can instantiate per-column without code changes |

### 7.2 Summary of All Gaps

| ID | Area | Gap | Severity | Effort |
|----|------|-----|----------|--------|
| **Machine crate** | | | | |
| G1 | `prove/mod.rs` | Single-proof prover → multi-proof orchestrator | Critical | Large |
| G2 | `prove/mod.rs:137,198,231` | Shared PCS → independent PCS per proof | Critical | Medium |
| G3 | `prove/mod.rs:174`, `verify/mod.rs:202` | Internal cumsum check → public value export | Critical | Medium |
| G4 | `composition.rs` | Global Poseidon/RC → per-proof local instances | Medium | Medium |
| G5 | `prove/mod.rs:141-149` | Single Fiat-Shamir → cross-proof transcript with sync | Critical | Medium |
| G6 | `composition.rs` | Global ColumnMeta → per-column public values + root proof | Medium | Small |
| G7 | `verify/mod.rs:34-89` | Single-proof verifier → multi-proof verifier | Critical | Medium |
| G10 | `keys.rs:16-116` | Single ProvingKey/VerifyingKey → per-proof-instance keys | Medium | Small |
| G11 | `prove/quotient.rs:359-459` | Batched quotient LDEs → per-proof quotient | Medium | Small |
| G12 | `verify/mod.rs:231-250` | `validate_chip_manifest()` expects all chips → per-proof subset | Medium | Small |
| **Witness crate** | | | | |
| W1 | `generator.rs:134-135` | All-column single loop → per-column partitioning | High | Medium |
| W2 | `encoding.rs:159-177` | Global state root → root proof tier responsibility | High | Medium |
| W3 | `smt.rs:71-197` | Global SMT tree build → root proof tier | High | Medium |
| W4 | `memory/mod.rs:49-55` | Global memory input aggregation → per-column | Critical | Large |
| W5 | `memory/chain.rs:7-45` | Cross-column sequential hash chain → per-column in StateShard | High | Medium |
| W6 | `memory/inter_tx.rs:134-142` | Global inter-tx sort `(t,c,key,tx)` → per-column sort | High | Small |
| W7 | `builder.rs:145-162` | Global `prepare_inputs()` → per-proof partitioned inputs | Medium | Medium |
| W8 | `orchestration.rs:34-62` | Single-pipeline `build_all_traces()` → per-proof-instance dispatch | Critical | Large |
| W9 | `validation.rs:28-66` | Global bus balance check → per-proof + cross-proof check | Low | Small |
| W10 | `types.rs:85-115` | `BatchWitness.columns: Vec` flat → `PartitionedWitness` | Medium | Medium |
| W11 | `route.rs:60-84` | Global write-set key routing → per-column (trivial in sharded) | Low | Small |
| **Composition** | | | | |
| G13 | `composition.rs:38-224` | Only global impls exist → `ShardedMemory` + `ShardedCommitment` | High | Medium |

### 7.3 Machine Crate Gaps (Detailed)

#### G1: Multi-Proof Orchestrator

**Current** (`prove/mod.rs:65-263`): `prove_with_key()` is an 11-phase sequential
pipeline. Phase 3 (line 137) commits ALL chip main traces in one `PcsCommitment`.
Phase 5 (line 154) accumulates a single `cumsum_total` across all chips. Phase 9
(line 231) commits all quotient LDEs together. Single FRI proof covers everything.

**Target**: `ShardedProver` orchestrates C+2 `ProofInstance` objects:
1. Parallel main trace building (C+1 independent trace sets)
2. Parallel main commits (C+1 independent PCS instances)
3. **Sync point**: collect all commitments, derive shared (α, β)
4. Parallel perm trace building + commit
5. Parallel quotient + FRI completion
6. Root proof construction (sequential, after all column proofs)

**Implementation**: `ShardedProver` wraps C+1 instances of a new `ProofInstance`
abstraction. Each instance operates on a subset of chips with its own PCS. The sync
point is a barrier where commitments are exchanged for challenge derivation.

#### G2: Independent PCS per Proof

**Current** (`prove/mod.rs:137,198,231`): Three `commit()` calls each batch ALL chips:
- Line 137: `commit(pcs, main_pairs)` — all chip main traces in one Merkle tree
- Line 198: `commit(pcs, perm_pairs)` — all perm traces together
- Line 231: `commit_ldes(pcs, all_quotient_ldes)` — all quotients together

**Target**: Each `ProofInstance` has its own MMCS. Column proof [i] commits only
MemoryShard[i] + StateShard[i] + PoseidonLocal[i] + RangeCheckLocal[i].

#### G3: Public Value Cumsums

**Current** (`prove/mod.rs:154-178`, `verify/mod.rs:197-209`):
```
cumsum_total = Σ (all chip cumsums)
if cumsum_total ≠ 0 → Error::LogUpImbalance
```
Single accumulator checked within one proof.

**Target**: Split cumsums into two categories per proof:
- **Internal** (BaseStateEntry + CoalescedWrite + PoseidonPerm + RangeCheck): must
  sum to 0 within each column proof
- **External** (ReadAccess + WriteAccess): exported as `cumsum_memory` public value

Root proof verifier checks: `cumsum_exec + Σ cumsum_col[i] = 0`.

#### G5: Cross-Proof Fiat-Shamir

**Current** (`prove/mod.rs:141-149`): Single challenger observes one `main_commitment`
(line 146), samples one pair of LogUp challenges (lines 148-149).

**Target**: Two-level Fiat-Shamir:
1. **Global transcript**: observes all C+1 main commitments in canonical `(t,c)` order,
   samples shared (α, β)
2. **Per-proof transcript**: each proof observes its own perm commitment, samples its
   own alpha and zeta independently

Per-proof alpha and zeta are safe to derive independently — they only affect constraint
folding within each proof, not cross-proof bus balance.

#### G7: Multi-Proof Verifier

**Current** (`verify/mod.rs:34-89`): `validate_chip_manifest()` (line 45) requires ALL
registered chips to appear in proof. Reconstructs single Fiat-Shamir transcript. Verifies
one PCS opening proof. Sums all `cumsum_final` values (line 202) for global balance.

**Target**: `ShardedVerifier`:
1. Reconstruct global (α, β) from all main commitments
2. Verify each proof independently (parallel)
3. Verify root proof (SMT paths + bus balance arithmetic)

#### G10: Per-Proof Keys

**Current** (`keys.rs:16-116`): `TabulaProvingKey::from_registry()` iterates ALL chips
in the registry (line 24) to produce one key. `TabulaVerifyingKey` mirrors this.

**Target**: Per-`ProofInstance` keys. Each proof instance generates its own
ProvingKey/VerifyingKey from its chip subset.

#### G11: Per-Proof Quotients

**Current** (`prove/quotient.rs:359-459`): `compute_chip_quotients()` iterates all
`chip_infos`, accumulates quotient LDEs into a single `Vec`, using shared `alpha` and
`logup_challenges`.

**Target**: Each `ProofInstance` computes quotients only for its own chips.

#### G12: Per-Proof Chip Manifest

**Current** (`verify/mod.rs:231-250`): `validate_chip_manifest()` rejects proofs that
don't contain ALL registered chips.

**Target**: Each proof's manifest matches only its proof instance's chip subset.

### 7.4 Witness Crate Gaps (Detailed)

The witness pipeline has **11 global assumptions** — this is the most underestimated
area. The entire pipeline is architected as a single monolithic pass over all columns.

#### W1: WitnessGenerator Column Loop

**Current** (`generator.rs:134-135`):
```rust
for (&(table, col), old_state) in old_column_states {
    // processes ALL columns in one loop
}
```

**Target**: Partition `old_column_states` by proof tier before witness generation.
Execution proof gets instruction-level data. Each column proof gets its own
`(table, col)` subset. Root proof gets SMT path data.

#### W2: Global State Root Computation

**Current** (`encoding.rs:159-177`): `compute_state_root()` takes ALL column states,
groups by table (line 164), computes per-table roots, combines into single global root.

**Target**: State root computation moves to root proof tier. Column proofs output
`Com_old`, `Com_new` as public values. Root proof takes these as inputs and verifies
SMT paths to `old_root` / `new_root`.

#### W3: Global SMT Path Building

**Current** (`smt.rs:71-197`): `build_smt_paths()` groups ALL ColumnMeta entries by
table (line 80), builds per-table SMT trees with ALL column leaves (lines 92-102),
then builds a global table-level SMT tree (line 138).

**Target**: SMT path computation is root proof tier responsibility. It receives
commitment values from all column proofs and builds the merkle proof. Column proofs
do not touch SMT.

#### W4: Global Memory Input Preparation

**Current** (`memory/mod.rs:49-55`):
```rust
for column in &witness.columns {
    inter_tx_rows.extend(build_inter_tx_rows(column)?);
    state_rows.extend(build_state_rows(column)?);
}
// sorts ALL rows globally (line 54-55)
```

All columns' memory rows are aggregated and globally sorted. Hash chain accumulators
are populated sequentially across all columns (line 58).

**Target**: Per-column memory input preparation. Each column proof builds its own
sorted memory rows and hash chain accumulators independently. No cross-column
aggregation. This is the largest witness pipeline change.

#### W5: Cross-Column Hash Chain Accumulation

**Current** (`memory/chain.rs:7-45`): `populate_state_chain_accumulators()` identifies
column boundaries by `(table_id, col_id)` pairs, chains hash accumulators sequentially
across all columns. `prev_old` and `prev_new` carry state between columns.

**Target**: Per-column hash chain accumulation within each StateShard. No cross-column
chaining. Each column proof's StateShard independently maintains its own hash chain
from first entry to last.

#### W6: Global Inter-Tx Row Sorting

**Current** (`memory/inter_tx.rs:134-142`): `sort_inter_tx_rows()` sorts ALL inter-tx
rows globally by `(table_id, col_id, key, tx_index)`.

**Target**: Per-column sort within each MemoryShard. Since each column proof handles
one `(t,c)`, the sort key simplifies to just `(key, tx_index)`.

#### W7: Global TraceBuilder Inputs

**Current** (`builder.rs:145-162`): `prepare_inputs()` derives `empty_columns` from all
`column_metas` together (line 146-152), calls lowering and SMT path building for the
entire batch.

**Target**: Partitioned input preparation. Execution proof gets instruction records.
Each column proof gets its column-specific witness. Root proof gets SMT paths and
commitment values.

#### W8: Single-Pipeline Orchestration

**Current** (`orchestration.rs:34-62`): `build_all_traces()` groups ALL chips by phase,
dispatches them in phase order. Between Memory and Dependent phases (line 53), evaluates
Phase 0+1 chip traces collectively for bus consumer dispatch.

**Target**: Per-proof-instance orchestration. Each `ProofInstance` builds traces only
for its own chips. Bus consumer dispatch (Poseidon/RC) happens within each proof
instance, not globally.

This is the second-largest change — the entire orchestration loop becomes per-instance.

#### W9: Global Bus Balance Validation

**Current** (`validation.rs:28-66`): `debug_validate_trace_map()` iterates all chips,
evaluates constraints, checks global bus balance across all chips for every bus ID.

**Target**: Two-level validation:
1. Per-proof: internal buses must balance within each proof
2. Cross-proof: external buses (ReadAccess, WriteAccess) export partial sums for
   root proof verification

#### W10: Flat BatchWitness Structure

**Current** (`types.rs:85-115`):
```rust
pub struct BatchWitness<H> {
    pub columns: Vec<ColumnWitness<H>>,    // flat list, all columns
    pub column_metas: Vec<ColumnMeta>,      // flat list, sorted by (t,c)
    pub old_state_root: NativeDigest,       // single global root
    pub new_state_root: NativeDigest,       // single global root
    pub key_routes: BTreeMap<CellKey, KeyRoute>,  // global routing map
}
```

**Target**: Partitioned witness structure:
```rust
pub struct PartitionedWitness<H> {
    pub execution: ExecutionWitness,           // instruction records, static tables
    pub columns: BTreeMap<(TableId, ColId), ColumnWitness<H>>,  // per-column
    pub root: RootWitness,                     // SMT paths, commitment values
}
```

#### W11: Global Key Routing

**Current** (`route.rs:60-84`): `route_keys()` collects ALL written keys into a global
set, iterates all events globally, assigns `KeyRoute` per key.

**Target**: Per-column routing (trivial in sharded model — each column has its own
MemoryShard, so routing is implicit).

### 7.5 Composition Gap

#### G13: Sharded Implementations of Composition Traits

**Current** (`composition.rs:38-224`): Only global implementations exist:
```rust
impl MemoryModel for GlobalSortedMemory {
    fn airs(&self) -> Vec<Box<dyn AnyRap>> {
        vec![Box::new(InterTxOrderChip::<3>)]  // ONE chip for all columns
    }
}
impl CommitmentScheme for SsmcScheme {
    fn airs(&self) -> Vec<Box<dyn AnyRap>> {
        vec![Box::new(StateColumnChip::<3>)]   // ONE chip for all columns
    }
}
```

**Target**: New implementations that return per-column shard chips:
```rust
impl MemoryModel for ShardedMemory {
    fn airs(&self) -> Vec<Box<dyn AnyRap>> {
        self.columns.iter().map(|(t, c)| {
            Box::new(MemoryShardChip::<3>::new(self.alloc.next(), *t, *c))
        }).collect()  // C chips, one per column
    }
}
impl CommitmentScheme for ShardedSsmc {
    fn airs(&self) -> Vec<Box<dyn AnyRap>> {
        self.columns.iter().map(|(t, c)| {
            Box::new(StateShardChip::<3>::new(self.alloc.next(), *t, *c))
        }).collect()  // C chips, one per column
    }
}
```

Note: the trait interfaces themselves are fine. Only new implementing structs are
needed.

---

## 8. Implementation Roadmap

### Phase A: Proof Infrastructure

| Task | Description | Gaps Addressed | Depends On |
|------|-------------|----------------|------------|
| A1 | `ProofInstance` — encapsulates chip subset + independent PCS + own keys | G1, G2, G10 | — |
| A2 | `ShardedProver` — orchestrates C+2 ProofInstances with sync point | G1, G11 | A1 |
| A3 | Global Fiat-Shamir — collects all main commitments, derives shared (α, β) | G5 | A2 |
| A4 | Public value cumsum — split internal vs cross-proof cumsums | G3 | A1 |
| A5 | `ShardedVerifier` — verifies C+2 proofs + cross-proof bus balance | G7, G12 | A4 |
| A6 | `TabulaProof` — aggregate proof envelope | G3 | A4 |

### Phase B: Witness Pipeline Decomposition

| Task | Description | Gaps Addressed | Depends On |
|------|-------------|----------------|------------|
| B1 | `PartitionedWitness` — replace flat `BatchWitness` with per-tier structure | W10 | — |
| B2 | Per-column memory input — move `prepare_memory_inputs()` to per-column | W4, W6 | B1 |
| B3 | Per-column hash chain — decouple `populate_state_chain_accumulators()` | W5 | B2 |
| B4 | Per-column witness generator — partition `WitnessGenerator.generate()` | W1 | B1 |
| B5 | SMT/state root → root tier — move `build_smt_paths()`, `compute_state_root()` | W2, W3 | B1 |
| B6 | Per-proof orchestration — `build_all_traces()` dispatches per proof instance | W8 | B2, B5 |
| B7 | Per-proof `prepare_inputs()` — partitioned `TraceBuilder` | W7 | B6 |
| B8 | Key routing simplification — per-column routing (trivial) | W11 | B1 |
| B9 | Two-level validation — per-proof + cross-proof bus balance debug check | W9 | B6 |

### Phase C: Column Proof Self-Containment

| Task | Description | Gaps Addressed | Depends On |
|------|-------------|----------------|------------|
| C1 | Per-column PoseidonLocal + RangeCheckLocal registration | G4 | A1 |
| C2 | `ShardedMemory` + `ShardedSsmc` composition impls | G13 | C1 |
| C3 | ColumnMeta → public values from StateShard + root proof SmtPath | G6 | B5 |
| C4 | Column proof integration test — single column E2E (prove + verify) | — | B6, C1, C2, C3 |

### Phase D: Root Proof

| Task | Description | Gaps Addressed | Depends On |
|------|-------------|----------------|------------|
| D1 | Root proof chip set — SmtColPath + SmtTablePath with commitment inputs | — | C3 |
| D2 | Root proof bus balance — arithmetic Σ cumsums = 0 verification | G3 | A4, D1 |
| D3 | Root proof integration test — full root proof E2E | — | D2 |

### Phase E: End-to-End Validation

| Task | Description | Depends On |
|------|-------------|------------|
| E1 | Sharded E2E test — DSL → compile → execute → witness → shard → prove → verify | C4, D3 |
| E2 | Equivalence test — sharded proof verifies same statement as global proof | E1 |
| E3 | Benchmark — prover speedup measurement (global vs sharded, seq vs parallel) | E1 |
| E4 | Multi-column test — 10+ columns with uneven row distribution | E1 |

### Phase P: Parallel Execution (independent optimization track)

| Task | Description | Depends On |
|------|-------------|------------|
| P1 | Parallel main trace building — rayon for C+1 concurrent trace constructions | B6 |
| P2 | Parallel PCS commit — concurrent Merkle tree construction | P1 |
| P3 | Parallel FRI — concurrent FRI opening proof generation | P2 |
| P4 | Parallel verification — concurrent proof verification | A5 |

### Phase F: Recursive Aggregation (future)

| Task | Description | Depends On |
|------|-------------|------------|
| F1 | STARK verifier circuit — AIR circuit that verifies a STARK proof | E1 |
| F2 | Binary tree reduction — recursive aggregation of column proofs | F1 |
| F3 | Final proof — O(1) aggregate proof | F2 |

### Dependency Graph

```
Phase A (Proof Infrastructure)        Phase B (Witness Pipeline)
A1 ──→ A2 ──→ A3                      B1 ──→ B2 ──→ B3
 │      │                              │      │      │
 │      └──────────────────────────────│──────┤      │
 │                                     │      ▼      ▼
 ├──→ A4 ──→ A5                        ├──→ B4    B5 ──→ B6 ──→ B7
 │    │      │                         │              │      B8
 │    │      A6                        │              B9
 │    │                                │
 ▼    ▼                                ▼
Phase C (Column Proof)                Phase D (Root Proof)
C1 ──→ C2                             D1
 │      │                              │
 └──→ C3 ←── B5                        D2 ←── A4
       │                               │
       ▼                               ▼
      C4 ←── B6                        D3
       │                               │
       └───────────┬───────────────────┘
                   ▼
                  E1 ──→ E2, E3, E4

P1 ──→ P2 ──→ P3 (parallel track, after B6)
A5 ──→ P4

E1 ──→ F1 → F2 → F3 (future)
```

### Effort Estimate

| Phase | Scope | Estimated LOC |
|-------|-------|---------------|
| A | Proof infrastructure (G1-G5, G7, G10-G12) | ~1,200 |
| B | Witness pipeline decomposition (W1-W11) | ~1,500 |
| C | Column proof self-containment (G4, G6, G13) | ~600 |
| D | Root proof | ~400 |
| E | E2E validation + tests | ~500 |
| P | Parallel execution | ~400 |
| **Total (A-E)** | | **~4,200** |

---

## 9. Compatibility and Migration

### 9.1 Backward Compatibility

The sharded protocol does NOT replace the global protocol. Both coexist:

```rust
// Global (current, unchanged)
let machine = TabulaMachine::builder()
    .with_core_chips()
    .with_default_commitments()
    .build();
let proof: MonolithicProof = machine.prove(&traces, &statement);

// Sharded (new, default)
let sharded = ShardedProver::builder()
    .with_execution_chips()
    .with_column_chips()
    .with_root_chips()
    .build();
let proof: TabulaProof = sharded.prove(&witness, &statement);
```

### 9.2 Shared Components

Both protocols share:
- All chip implementations (ExecutionChip, MemoryShard, StateShard, PoseidonChip, etc.)
- AIR constraint definitions
- LogUp fingerprint computation
- FRI and PCS primitives from p3
- Value encoding, IR, executor

The difference is purely in **orchestration**: how chips are grouped into proofs and
how bus balance is verified.

### 9.3 Migration Path

1. Implement `ProofInstance` as a generalization of the current prover (Phase A)
2. Refactor `TabulaMachine::prove()` to use `ProofInstance` internally (1 instance)
3. Decompose witness pipeline into per-proof partitions (Phase B)
4. Add `ShardedProver` that creates C+2 `ProofInstance`s (Phase A+C)
5. Both paths share the same underlying prover code
6. Validate equivalence: sharded proof verifies same statement as global (Phase E)

---

## 10. Open Questions

| # | Question | Impact | Notes |
|---|----------|--------|-------|
| Q1 | Should execution proof embed its own Poseidon/RC, or export partial sums? | Proof independence | §4.2 proposes embedding. Needs validation. |
| Q2 | Can per-proof alpha and zeta be derived independently? | Soundness | §7.3 G5 argues yes. Needs formal argument. |
| Q3 | What is the minimum viable recursive aggregation for production? | Proof size | Binary tree? Groth16 wrapping? |
| Q4 | How should untouched columns be handled? | Efficiency | Skip column proof entirely? Empty proof? |
| Q5 | Should the global protocol be deprecated or maintained long-term? | Maintenance | Both have valid use cases (small vs large C). |
| Q6 | How does this interact with the Precompile framework? | Extensibility | Precompile chips could be per-column or global. |
| Q7 | Should `BatchWitness` be replaced or extended? | Migration | `PartitionedWitness` (new) vs `BatchWitness::partition()` (adapter). |

---

## 11. Conclusion

The fully sharded protocol transforms Tabula's proving from a sequential monolithic
process to an embarrassingly parallel pipeline. The code-level audit identified
**25 specific gaps** across three areas:

- **Machine crate** (G1-G7, G10-G12): Proof orchestration, PCS, Fiat-Shamir, keys,
  verifier — 10 gaps, ~1,200 LOC
- **Witness crate** (W1-W11): Pipeline decomposition, memory input, hash chain,
  orchestration, validation — 11 gaps, ~1,500 LOC (most underestimated area)
- **Composition** (G13): New sharded implementations of existing traits — 1 gap, ~600 LOC

The good news: the **foundational layer is already sharding-ready**. ChipId, BusId,
TraceMap, all chip implementations, LogUp, and the composition trait interfaces require
zero changes. The work is entirely in orchestration and witness pipeline — infrastructure
changes, not algorithmic ones.

**Critical path**: Phase A (ProofInstance) → Phase B (witness decomposition) → Phase C
(column proof) → Phase D (root proof) → Phase E (E2E validation).

**Recommended next step**: Implement A1 (`ProofInstance`) and B1 (`PartitionedWitness`)
in parallel as the two foundational abstractions, then converge at C4 (column proof
integration test).
