# Full Sharding Infrastructure

> Status: ⬜ Blocked on [proving-layer.md](proving-layer.md) (Goal 2 — ProofInstance + protocol math in stark)
> Design: [docs/design/sharded-protocol-design.md](../docs/design/sharded-protocol-design.md)
> Related: [docs/design/commitment-architecture-research.md](../docs/design/commitment-architecture-research.md), [docs/design/proving-layer-architecture.md](../docs/design/proving-layer-architecture.md)

## Goal

Build the sharded proving/verifying infrastructure. 25 code-level gaps identified
across machine (G1-G13), witness (W1-W11), and composition layers.

Target: same batch produces C+2 independent proofs (1 execution + C column + 1 root)
that verify together.

## Current State

**Already sharding-ready (zero changes needed):**
- [x] MemoryShard\<W\> — per-column sorted memory (48 cols W=3, 29 tests)
- [x] StateShard\<W\> — per-column SSMC commitment (93 cols W=3, 18 tests)
- [x] MetaShard — per-column metadata (96 cols, 16 tests)
- [x] ChipId(u16) + ChipIdAllocator — open newtype, dynamic allocation
- [x] BusId(u16) + define_bus! — multi-instance safe
- [x] TraceMap — BTreeMap\<ChipId, TraceEntry\>, unlimited entries
- [x] LogUp fingerprint — multi-instance aggregation automatic
- [x] Inter-chip coupling — zero direct references, all via LogUp buses
- [x] PoseidonChip, RangeCheckChip — stateless, instantiable per-column
- [x] MemoryModel, CommitmentScheme, RootProof trait interfaces
- [x] Global TabulaMachine — working prove/verify (safety net)

---

## Phase A: Proof Infrastructure (~1,200 LOC)

### G1: ProofInstance Abstraction (Large)

Subset of chips with independent PCS. Extracted from `prove/mod.rs:65-263`.

- [ ] `ProofInstance` struct — owns chip set, trace matrices, PCS config, own keys
- [ ] `ProofInstance::commit_main()` → main trace commitment
- [ ] `ProofInstance::build_perm_trace(alpha, beta)` → permutation trace
- [ ] `ProofInstance::prove_quotient_fri()` → quotient + FRI, produces sub-proof
- [ ] Factor current `prove_with_key()` 11-phase pipeline into ProofInstance operations

### G2: Independent PCS per Proof (Medium)

Currently `prove/mod.rs:137,198,231` — three `commit()` calls batch ALL chips.

- [ ] Each ProofInstance has its own MMCS (separate Merkle tree)
- [ ] Column proof [i] commits only its 4 chips (MemoryShard + StateShard + PoseidonLocal + RCLocal)
- [ ] Execution proof commits only ExecutionChip + StaticTableChip + PoseidonLocal + RCLocal

### G3: Public Value Cumsum Export (Medium)

Currently `prove/mod.rs:174` — single `cumsum_total` accumulator across all chips.

- [ ] Split cumsums: internal (within proof, must be 0) vs external (cross-proof, exported)
- [ ] Execution proof: `cumsum_memory` from ReadAccess + WriteAccess bus sends
- [ ] Column proof [i]: `cumsum_memory` from ReadAccess + WriteAccess bus receives
- [ ] Root proof: arithmetic check `cumsum_exec + Σ cumsum_col[i] = 0`

### G5: Cross-Proof Fiat-Shamir (Medium)

Currently `prove/mod.rs:141-149` — single challenger, single (α,β) pair.

- [ ] Global transcript: observes statement + all C+1 main commitments (canonical order)
- [ ] Shared challenge derivation: (α, β) from global transcript
- [ ] Per-proof transcript fork: each proof derives its own alpha, zeta independently
- [ ] Verifier reconstructs same two-level transcript structure

### G7: ShardedVerifier (Medium)

Currently `verify/mod.rs:34-89` — single-proof verification.

- [ ] `ShardedVerifier::verify(proof, statement)` — top-level API
- [ ] Reconstruct global (α, β) from all proof commitments
- [ ] Verify exec_proof independently (standard STARK verify)
- [ ] Verify each col_proof[i] independently (parallelizable)
- [ ] Verify root_proof: cumsums sum to zero + SMT paths valid

### G10: Per-Proof Keys (Small)

Currently `keys.rs:16-116` — single ProvingKey/VerifyingKey for all chips.

- [ ] Per-ProofInstance key generation
- [ ] Each proof instance has its own ProvingKey/VerifyingKey from its chip subset

### G11: Per-Proof Quotients (Small)

Currently `prove/quotient.rs:359-459` — batches all quotient LDEs into single Vec.

- [ ] Each ProofInstance computes quotients only for its own chips

### G12: Per-Proof Chip Manifest (Small)

Currently `verify/mod.rs:231-250` — rejects proofs missing any registered chip.

- [ ] Each proof's manifest matches only its ProofInstance's chip subset

### A6: ShardedTabulaProof (Small)

- [ ] `ShardedTabulaProof` envelope: exec_proof + col_proofs + root_proof
- [ ] Serialization / deserialization
- [ ] `main_commitments_digest` for challenge reconstruction

---

## Phase B: Witness Pipeline Decomposition (~1,500 LOC)

The entire witness pipeline is architected as a single monolithic pass over all columns.
11 specific global assumptions need decomposition.

### W10: PartitionedWitness Structure (Medium)

Currently `types.rs:85-115` — `BatchWitness.columns: Vec<ColumnWitness>` flat list.

- [ ] `PartitionedWitness` struct with per-tier partitions
- [ ] Execution partition: InstructionRecords, StaticTableRows
- [ ] Per-column partitions: ColumnWitness per (t,c)
- [ ] Root partition: SMT paths, commitment values

### W4: Per-Column Memory Input (Large — critical path)

Currently `memory/mod.rs:49-55` — aggregates ALL columns, sorts globally.

- [ ] Per-column `build_inter_tx_rows()` (already per-column, just remove aggregation)
- [ ] Per-column `build_state_rows()` (already per-column, just remove aggregation)
- [ ] Remove global sort — each MemoryShard handles its own (key, tx_index) sort
- [ ] Remove global `prepare_memory_inputs()` aggregation loop

### W5: Per-Column Hash Chain (Medium)

Currently `memory/chain.rs:7-45` — sequential chaining across all columns.

- [ ] Per-column hash chain accumulation within each StateShard
- [ ] Remove `prev_old`/`prev_new` cross-column carry
- [ ] Each column proof independently maintains its hash chain

### W6: Per-Column Inter-Tx Sort (Small)

Currently `memory/inter_tx.rs:134-142` — global sort by `(t,c,key,tx)`.

- [ ] Per-column sort: key simplifies to `(key, tx_index)` within single (t,c)

### W1: WitnessGenerator Partitioning (Medium)

Currently `generator.rs:134-135` — all columns in one loop.

- [ ] Partition `old_column_states` by proof tier before witness generation
- [ ] Per-column witness output for each column proof

### W2: State Root → Root Tier (Medium)

Currently `encoding.rs:159-177` — global `compute_state_root()`.

- [ ] Move state root computation to root proof tier
- [ ] Column proofs output Com_old, Com_new as public values
- [ ] Root proof takes commitment values as inputs

### W3: SMT Paths → Root Tier (Medium)

Currently `smt.rs:71-197` — global `build_smt_paths()` builds full merkle tree.

- [ ] SMT path computation is root proof responsibility only
- [ ] Column proofs do not touch SMT
- [ ] Root proof receives commitment values from all column proofs

### W8: Per-Proof Orchestration (Large — critical path)

Currently `orchestration.rs:34-62` — `build_all_traces()` dispatches all chips.

- [ ] Per-ProofInstance trace building
- [ ] Per-proof bus consumer dispatch (PoseidonLocal/RCLocal within each proof)
- [ ] Phase ordering within each proof instance

### W7: Per-Proof TraceBuilder Inputs (Medium)

Currently `builder.rs:145-162` — `prepare_inputs()` processes all columns.

- [ ] Partitioned input preparation per proof instance
- [ ] Execution proof: instruction records + static table rows
- [ ] Column proof: column-specific witness data
- [ ] Root proof: SMT paths + commitment values

### W9: Two-Level Validation (Small)

Currently `validation.rs:28-66` — global bus balance debug check.

- [ ] Per-proof: internal buses must balance within each proof
- [ ] Cross-proof: external buses export partial sums for root verification

### W11: Key Routing Simplification (Small)

Currently `route.rs:60-84` — global write-set analysis.

- [ ] Per-column routing (trivial in sharded model — each column is its own shard)

---

## Phase C: Column Proof Self-Containment (~600 LOC)

### G4: PoseidonLocal / RangeCheckLocal (Medium)

- [ ] PoseidonLocal — same AIR as PoseidonChip, separate trace per proof
- [ ] RangeCheckLocal — same AIR as RangeCheckChip, separate trace per proof
- [ ] Dynamic ChipId allocation for per-proof instances
- [ ] Preprocessed trace reuse (same round constants, duplicated per proof)

### G13: Sharded Composition Implementations (Medium)

Currently `composition.rs:38-224` — only global impls exist.

- [ ] `ShardedMemory` implementing `MemoryModel` — returns C × MemoryShardChip
- [ ] `ShardedSsmc` implementing `CommitmentScheme` — returns C × StateShardChip
- [ ] ChipIdAllocator for dynamic per-column ID assignment
- [ ] Builder convenience: `with_sharded_memory()`, `with_sharded_commitments()`

### G6: ColumnMeta Decomposition (Small)

- [ ] Com_old, Com_new as public values from StateShard
- [ ] MetaShard simplified to public-value extractor
- [ ] SMT leaf computation moved to root proof

---

## Phase D: Root Proof (~400 LOC)

- [ ] Root proof chip set: SmtColPathChip + SmtTablePathChip
- [ ] Input: all Com_old/Com_new + cumsum values from column + execution proofs
- [ ] Cumsum balance verification: `cumsum_exec + Σ cumsum_col[i] = 0`
- [ ] SMT path verification: Com values consistent with old_root → new_root
- [ ] Root proof integration test

---

## Phase E: End-to-End Validation (~500 LOC)

- [ ] E1: Sharded E2E test — DSL → compile → execute → witness → shard → prove → verify
- [ ] E2: Equivalence test — sharded proof verifies same statement as global proof
- [ ] E3: Benchmark — prover speedup (global vs sharded, sequential vs parallel)
- [ ] E4: Multi-column test — 10+ columns with uneven row distribution

---

## Phase P: Parallel Execution (~400 LOC, independent track)

- [ ] P1: Parallel main trace building (rayon, C+1 concurrent)
- [ ] P2: Parallel PCS commit (concurrent Merkle tree construction)
- [ ] P3: Parallel FRI (concurrent per proof)
- [ ] P4: Parallel verification (concurrent proof verification)

---

## Migration (Goal 4)

After Phase A-E complete and sharded E2E test passes:

- [ ] Equivalence test: global proof ≡ sharded proof for same batch
- [ ] ShardedMachine as default (TabulaMachine wraps ShardedProver)
- [ ] Deprecate global chips: InterTxOrderChip, StateColumnChip, ColumnMetaChip
- [ ] Remove global-only code paths
- [ ] Update all E2E tests to use sharded prover

---

## Dependency Graph

```
Phase A (Proof Infrastructure)        Phase B (Witness Pipeline)
G1 ──→ G2                             W10 ──→ W4 ──→ W5
 │      │                              │       │      │
 │      └──→ G5 ──→ G7                 │       ▼      ▼
 │                                     ├──→ W1     W6
 ├──→ G3 ──→ A6                        │
 │    │                                ├──→ W2, W3 ──→ W8 ──→ W7
 ├──→ G10                              │              │
 ├──→ G11                              │              W9
 └──→ G12                              └──→ W11, W8

Phase C (Column Proof)                Phase D (Root Proof)
G4                                     D ←── G3, G6
 │
G13 ←── G4
 │
G6 ←── W2, W3
 │
C4 (integration test) ←── W8, G4, G13, G6

Phase E ←── C4, D
Phase P (parallel, independent after W8)
```

## Effort Estimate

| Phase | Scope | LOC |
|-------|-------|-----|
| A | Proof infrastructure (G1-G3, G5, G7, G10-G12, A6) | ~1,200 |
| B | Witness pipeline decomposition (W1-W11) | ~1,500 |
| C | Column proof self-containment (G4, G6, G13) | ~600 |
| D | Root proof | ~400 |
| E | E2E validation + tests | ~500 |
| P | Parallel execution | ~400 |
| **Total** | | **~4,600** |

## Verification

```bash
cargo check --workspace
cargo test --workspace
# Sharded E2E
cargo test -p tabula-machine --test sharded_e2e
# Equivalence
cargo test -p tabula-machine --test sharded_equivalence
```
