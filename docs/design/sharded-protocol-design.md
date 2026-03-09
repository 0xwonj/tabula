# Fully Sharded Protocol Design

> **Version**: 1.0
> **Date**: 2026-03-09
> **Status**: Design proposal
> **References**: [commitment-architecture-research.md](commitment-architecture-research.md), [implementation-workplan.md](implementation-workplan.md), [master-roadmap.md](master-roadmap.md)

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
21-row Poseidon permutation with 17 preprocessed columns, this is 21 × 17 = 357 field
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
    batch_digest: [BabyBear; 8],
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
    com_old: [BabyBear; 8],           // old SSMC commitment for this column
    com_new: [BabyBear; 8],           // new SSMC commitment for this column
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
struct ShardedTabulaProof {
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

### 7.1 Summary

| ID | Gap | Current | Target | Effort |
|----|-----|---------|--------|--------|
| G1 | Multi-proof orchestrator | `TabulaMachine.prove()` single proof | `ShardedProver` producing C+2 proofs | Large |
| G2 | Independent PCS per proof | Shared PCS (one Merkle tree) | Per-proof PCS instances | Medium |
| G3 | Public value cumsums | Internal cumsum balance check | Export cumsums as public outputs | Medium |
| G4 | Per-column Poseidon/RC | Global PoseidonChip + RangeCheckChip | PoseidonLocal + RangeCheckLocal per column | Medium |
| G5 | Cross-proof Fiat-Shamir | Single-proof transcript | Multi-commitment transcript with sync point | Medium |
| G6 | ColumnMeta decomposition | Global ColumnMetaChip | Per-column public outputs + SmtPath in root proof | Small |
| G7 | Multi-proof verifier | `TabulaMachine.verify()` single proof | `ShardedVerifier` checking C+2 proofs | Medium |
| G8 | Witness partitioning | Global WitnessStore | Per-column witness partitioning | Small |
| G9 | Recursive aggregation | Not needed | Binary tree proof reduction (optional) | Large (Phase 5) |

### 7.2 Detailed Gap Analysis

#### G1: Multi-Proof Orchestrator

**Current**: `TabulaMachine::prove()` builds all traces, commits all in shared PCS,
runs single FRI.

**Target**: `ShardedProver` orchestrates:
1. Parallel main trace building (C+1 independent trace sets)
2. Parallel main commits (C+1 independent PCS instances)
3. Sync: collect all commitments, derive shared (α, β)
4. Parallel perm trace building + commit
5. Parallel quotient + FRI completion
6. Root proof construction

**Key change**: The prover becomes a **two-phase parallel pipeline** with one sync point,
rather than a sequential pipeline.

**Implementation approach**: `ShardedProver` wraps C+1 instances of the existing
single-proof prover. Each instance operates on a subset of chips. The sync point is a
barrier where commitments are exchanged.

#### G2: Independent PCS per Proof

**Current**: All chip traces go into one batched MMCS. One `PcsCommitment` per round
covers all chips.

**Target**: Each proof instance has its own MMCS. Column proof [i] commits only
MemoryShard[i] + StateShard[i] + PoseidonLocal[i] + RangeCheckLocal[i].

**Key change**: `TabulaProvingKey` and `TabulaVerifyingKey` become per-proof-instance.
The machine generates C+2 key sets.

**Benefit**: FRI query opening data per proof is ~236 FE (column) instead of ~12,223 FE
(all chips combined). Individual proof sizes are small; aggregate is larger but each
can be verified independently.

#### G3: Public Value Cumsums

**Current**: `cumsum_final` is a field in `ChipOpening`. The verifier sums all cumsums
within the single proof and checks Σ = 0.

**Target**: Each proof exports its total cumsum as a public value. The root proof
verifier checks: `cumsum_exec + Σ cumsum_col[i] = 0`.

**Key change**: Split cumsums into "intra-proof" (must be zero for internal buses) and
"cross-proof" (exported as public values for Memory bus).

**Design detail**: Each column proof computes two cumsum categories:
- Internal: BaseStateEntry + CoalescedWrite + PoseidonPerm + RangeCheck → must sum to 0
  within the column proof
- External: ReadAccess + WriteAccess → exported as `cumsum_memory` public value

The verifier checks internal balance per proof, then cross-proof balance in root.

#### G4: Per-Column Poseidon and RangeCheck

**Current**: One global `PoseidonChip` (93 main + 17 preprocessed cols) processes all
Poseidon permutations. One global `RangeCheckChip` (2 cols) accumulates all range checks.
Both registered as `BusConsumer` in orchestration.

**Target**: Each column proof has local instances. The `BusConsumer` trait is replaced
by direct chip registration within the column proof's chip set.

**Key change**: `PoseidonLocal` and `RangeCheckLocal` are the same chip implementations
as the current global versions, just instantiated per-column with only that column's
data. No code change to the chips themselves — only to how they're registered and
populated.

**Preprocessed trace**: PoseidonLocal reuses the same preprocessed round constants.
Each column proof includes the same 17-column preprocessed trace. This is duplicated
C times but is tiny (21 rows × 17 cols = 357 FE per column).

#### G5: Cross-Proof Fiat-Shamir

**Current**: Single challenger observes statement → preprocessed → main → sample (α,β)
→ perm → sample alpha → quotient → sample zeta, all within one proof.

**Target**: Two-level Fiat-Shamir:
1. **Global transcript** (Phase 2): observes all main commitments from all proofs,
   samples shared (α, β). Distributed to all proof instances.
2. **Per-proof transcript** (Phase 4): each proof observes its own perm commitment,
   samples its own alpha and zeta independently.

**Soundness**: The global transcript ensures LogUp challenges are unpredictable.
Per-proof alpha and zeta are safe to derive independently because they only affect
constraint folding within each proof.

**Implementation**: The `ShardedProver` manages the global transcript. After Phase 2,
it forks into C+1 independent per-proof transcripts, each seeded with (α, β) from the
global transcript.

#### G6: ColumnMeta Decomposition

**Current**: `ColumnMetaChip` (56 cols) handles per-column metadata AND SMT leaf
computation in one global chip.

**Target**: Split into:
1. **Per-column commitment output**: Com_old, Com_new, is_empty flags become public
   values of each column proof (from StateShard's final state).
2. **SmtPath chips**: Remain in root proof. Verify that Com values are consistent
   with old_root → new_root via SMT inclusion proofs.

**Key change**: `MetaShard` is simplified. Instead of a full chip with SMT leaf
computation, it becomes a public-value extractor from StateShard. SMT verification
moves entirely to the root proof.

#### G7: Multi-Proof Verifier

**Current**: `TabulaMachine::verify()` checks one proof with one FRI opening.

**Target**: `ShardedVerifier`:
1. Reconstruct global (α, β) from all main commitments
2. Verify each proof independently (parallel)
3. Verify root proof (SMT paths + bus balance)

**Key change**: Verification becomes parallelizable. Each column proof can be verified
independently by different machines.

#### G8: Witness Partitioning

**Current**: `WitnessStore` is a single typed key-value store. `TraceBuilder` populates
it with all chip data. `build_all_traces()` iterates all chips sequentially.

**Target**: Witness data is partitioned by column during `TraceBuilder`:
1. Global partition: InstructionRecords, StaticTableRows (for execution proof)
2. Per-column partitions: ColumnMemoryAccesses, ColumnSsmcWitness (for column proofs)
3. Root partition: SmtPaths, ColumnCommitments (for root proof)

**Key change**: `WitnessStore` becomes `WitnessPartition` or similar. Each proof
instance receives only its relevant partition.

---

## 8. Implementation Roadmap

### Phase A: Foundation (enables sharded proving)

| Task | Description | Depends On |
|------|-------------|------------|
| A1 | `ProofInstance` abstraction — encapsulates a subset of chips with independent PCS | — |
| A2 | `ShardedProver` — orchestrates C+2 ProofInstances with sync point | A1 |
| A3 | Global Fiat-Shamir transcript — collects all main commitments, derives shared (α, β) | A2 |
| A4 | Public value cumsum export — split internal vs cross-proof cumsums | A1 |
| A5 | `ShardedVerifier` — verifies C+2 proofs + cross-proof bus balance | A4 |
| A6 | `ShardedTabulaProof` — aggregate proof envelope | A4 |

### Phase B: Column Proof Self-Containment

| Task | Description | Depends On |
|------|-------------|------------|
| B1 | `PoseidonLocal` — per-column Poseidon instance (same chip, different registration) | A1 |
| B2 | `RangeCheckLocal` — per-column RangeCheck instance | A1 |
| B3 | Witness partitioning — split WitnessStore into per-proof partitions | A2 |
| B4 | Column proof integration test — single column proof E2E (prove + verify) | B1, B2, B3 |

### Phase C: Root Proof

| Task | Description | Depends On |
|------|-------------|------------|
| C1 | ColumnMeta decomposition — extract Com_old/Com_new as public values from StateShard | B4 |
| C2 | SmtPath root proof — standalone proof for SMT path verification | C1 |
| C3 | Bus balance verification in root — arithmetic check Σ cumsums = 0 | A4, C2 |
| C4 | Root proof integration test — full root proof E2E | C3 |

### Phase D: Parallel Execution

| Task | Description | Depends On |
|------|-------------|------------|
| D1 | Parallel main trace building — rayon/tokio for C+1 concurrent trace constructions | B3 |
| D2 | Parallel PCS commit — concurrent Merkle tree construction | D1 |
| D3 | Parallel FRI — concurrent FRI opening proof generation | D2 |
| D4 | Parallel verification — concurrent proof verification | A5 |

### Phase E: End-to-End Validation

| Task | Description | Depends On |
|------|-------------|------------|
| E1 | Sharded E2E test — DSL → compile → execute → witness → shard → prove → verify | C4 |
| E2 | Equivalence test — sharded proof verifies same statement as global proof | E1 |
| E3 | Benchmark — prover speedup measurement (global vs sharded, sequential vs parallel) | E1 |
| E4 | Multi-column test — 10+ columns with uneven row distribution | E1 |

### Phase F: Recursive Aggregation (future, optional)

| Task | Description | Depends On |
|------|-------------|------------|
| F1 | STARK verifier circuit — AIR circuit that verifies a STARK proof | E1 |
| F2 | Binary tree reduction — recursive aggregation of column proofs | F1 |
| F3 | Final proof — O(1) aggregate proof | F2 |

### Dependency Graph

```
A1 ──→ A2 ──→ A3
 │      │
 │      └──→ B3 ──→ B4 ──→ C1 ──→ C2 ──→ C3 ──→ C4 ──→ E1 ──→ E2
 │                    ↑                                    │      E3
 ├──→ A4 ──→ A5      │                                    │      E4
 │    │      A6      │                                    │
 ├──→ B1 ────────────┘                                    └──→ F1 → F2 → F3
 └──→ B2 ────────────┘

D1 ──→ D2 ──→ D3 (parallel with C/E, independent optimization)
A5 ──→ D4
```

---

## 9. Compatibility and Migration

### 9.1 Backward Compatibility

The sharded protocol does NOT replace the global protocol. Both coexist:

```rust
// Global (current default, unchanged)
let machine = TabulaMachine::builder()
    .with_core_chips()
    .with_default_commitments()
    .build();
let proof: TabulaProof = machine.prove(&traces, &statement);

// Sharded (new)
let sharded = ShardedProver::builder()
    .with_execution_chips()
    .with_column_chips()
    .with_root_chips()
    .build();
let proof: ShardedTabulaProof = sharded.prove(&witness, &statement);
```

### 9.2 Shared Components

Both protocols share:
- All chip implementations (ExecutionChip, MemoryShard, StateShard, PoseidonChip, etc.)
- AIR constraint definitions
- LogUp fingerprint computation
- WitnessStore data structures
- FRI and PCS primitives from p3

The difference is purely in **orchestration**: how chips are grouped into proofs and
how bus balance is verified.

### 9.3 Migration Path

1. Implement `ProofInstance` as a generalization of the current prover
2. Refactor `TabulaMachine::prove()` to use `ProofInstance` internally (1 instance)
3. Add `ShardedProver` that creates C+2 `ProofInstance`s
4. Both paths share the same underlying prover code

---

## 10. Open Questions

| # | Question | Impact | Notes |
|---|----------|--------|-------|
| Q1 | Should execution proof embed its own Poseidon/RC, or export partial sums? | Proof independence | §4.2 proposes embedding. Needs validation. |
| Q2 | Can per-proof alpha and zeta be derived independently? | Soundness | §G5 argues yes. Needs formal argument. |
| Q3 | What is the minimum viable recursive aggregation for production? | Proof size | Binary tree? Groth16 wrapping? |
| Q4 | How should untouched columns be handled? | Efficiency | Skip column proof entirely? Empty proof? |
| Q5 | Should the global protocol be deprecated or maintained long-term? | Maintenance | Both have valid use cases (small vs large C). |
| Q6 | How does this interact with the Precompile framework (Phase 3b)? | Extensibility | Precompile chips could be per-column or global. |

---

## 11. Conclusion

The fully sharded protocol transforms Tabula's proving from a sequential monolithic
process to an embarrassingly parallel pipeline. The architecture change is significant
(G1-G8) but builds on existing infrastructure:

- Shard chips (MemoryShard, StateShard, MetaShard) are already implemented and tested
- The ColumnCommitment and CommitmentScheme traits provide the abstraction boundary
- PoseidonChip and RangeCheckChip can be instantiated per-column without code changes
- The bus protocol (LogUp) naturally supports cross-proof balance via public cumsums

The main engineering effort is the **multi-proof orchestrator** (G1, G5, G7) and
**proof structure redesign** (G2, G3, G6). These are infrastructure changes, not
algorithmic ones.

**Recommended next step**: Implement Phase A (ProofInstance + ShardedProver) as a
proof-of-concept, then validate with a single-column E2E test before scaling to
full parallelism.
