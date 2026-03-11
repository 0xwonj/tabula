# Full Sharding Infrastructure

> Status: ✅ Complete — all phases done, migration complete
> Design: [docs/design/sharded-protocol-design.md](../docs/design/sharded-protocol-design.md)
> Related: [docs/design/commitment-architecture-research.md](../docs/design/commitment-architecture-research.md), [docs/design/proving-layer-architecture.md](../docs/design/proving-layer-architecture.md)

## Goal

Build the sharded proving/verifying infrastructure. 25 code-level gaps identified
across machine (G1-G13), witness (W1-W11), and composition layers.

Target: same batch produces C+2 independent proofs (1 execution + C column + 1 root)
that verify together.

## Current State

**Sharding IS the base architecture.** No monolithic code paths remain.

- TabulaMachine::new(&col_configs) → build_traces() → prove() → verify()
- C+2 sub-proofs with shared Fiat-Shamir synchronization
- Cross-proof bus balance verified at both prover and verifier
- 971 tests passing, zero failures

---

## Phase A: Proof Infrastructure (~1,200 LOC) ✅

### G1: ProofInstance Abstraction (Large) ✅

- [x] `ProofInstance` struct — owns chip set, trace matrices, PCS config, own keys
- [x] `ProofInstance::commit_main()` → main trace commitment
- [x] `ProofInstance::build_perm_trace(alpha, beta)` → permutation trace
- [x] `ProofInstance::prove()` → quotient + FRI, produces sub-proof
- [x] Factor current pipeline into ProofInstance operations

### G2: Independent PCS per Proof (Medium) ✅

- [x] Each ProofInstance has its own MMCS (separate Merkle tree)
- [x] Column proof [i] commits only its chip subset
- [x] Execution proof commits only its chip subset

### G3: Public Value Cumsum Export (Medium) ✅

- [x] Split cumsums: internal (within proof, must be 0) vs external (cross-proof, exported)
- [x] Per-bus cumsum tracking via `cumsums_by_bus()`
- [x] EXTERNAL_BUSES: ReadAccess, WriteAccess, EmptyColRead

### G5: Cross-Proof Fiat-Shamir (Medium) ✅

- [x] Global transcript: observes statement + all C+1 main commitments (canonical order)
- [x] Shared challenge derivation: (α, β) from global transcript
- [x] Per-proof transcript fork: `challenger.clone()` per proof
- [x] Verifier reconstructs same two-level transcript structure

### G7: ShardedVerifier (Medium) ✅

- [x] `TabulaMachine::verify(proof)` — top-level API
- [x] Reconstruct global (α, β) from all proof commitments
- [x] Verify each sub-proof independently

### G10: Per-Proof Keys (Small) ✅

- [x] Per-ProofInstance key generation via `TabulaProvingKey::from_registry()`
- [x] Each TierSetup creates its own keys from its chip subset registry

### G11: Per-Proof Quotients (Small) ✅

- [x] Each ProofInstance computes quotients only for its own chips

### G12: Per-Proof Chip Manifest (Small) ✅

- [x] Each proof's manifest matches only its ProofInstance's chip subset

### A6: TabulaProof (Small) ✅

- [x] `TabulaProof` envelope: exec_proof + col_proofs + root_proof
- [ ] Serialization / deserialization (deferred)

---

## Phase B: Witness Pipeline Decomposition (~1,500 LOC) ✅

### W10: PartitionedWitness Structure (Medium) ✅

- [x] `PartitionedStores` struct with per-tier stores (execution, per-column, root)
- [x] `partition_by_tier()` splits global WitnessStore into tier stores
- [x] Each column store contains single-column `SsmcWitness`

### W4: Per-Column Memory Input (Large) ✅

- [x] `prepare_shard_witness()` converts `BatchWitness` → per-column `SsmcWitness`
- [x] Per-column `build_inter_tx_rows()` → `MemoryShardRow`
- [x] Per-column `build_state_rows()` → sort → chain accumulators → `StateShardRow`
- [x] Per-column `MetaShardRow` from `ColumnMeta` + empty_read_counts

### W5: Per-Column Hash Chain (Medium) ✅

- [x] `populate_state_chain_accumulators()` already handles per-column segments
- [x] Per-column hash chain accumulation within each StateShard

### W8: Per-Proof Orchestration (Large) ✅

- [x] `build_all_traces()` works with any chip subset + bus consumers
- [x] Per-proof bus consumer dispatch (PoseidonChip/RangeCheckChip per tier)
- [x] Phase ordering within each proof instance via `TracePhase`

### W7: Per-Proof TraceBuilder Inputs (Medium) ✅

- [x] `PartitionedStores` provides per-tier witness stores
- [x] Execution store: EXECUTION_RECORDS + STATIC_TABLE_ROWS
- [x] Column store: SSMC_WITNESS_LABEL (single-column SsmcWitness)
- [x] Root store: COLUMN_META_INPUT + SMT paths

### Deferred (not needed for sharded flow)

- W1 (WitnessGenerator partitioning) — upstream witness generation unchanged
- W2 (State Root → Root Tier) — root still computes from existing data
- W3 (SMT Paths → Root Tier) — root still uses existing SMT infrastructure
- W6 (Per-Column Inter-Tx Sort) — sorting already per-column in shard path
- W9 (Two-Level Validation) — validation via `check_internal_balance` / `check_cross_proof_balance` in prover
- W11 (Key Routing) — trivial in sharded model, no code change needed

---

## Phase C: Column Proof Self-Containment (~600 LOC) ✅

### G4: PoseidonLocal / RangeCheckLocal (Medium) ✅

- [x] Each tier includes PoseidonChip + RangeCheckChip in its own registry
- [x] Per-tier BusConsumer dispatch via `build_all_traces()` with tier chip subset
- [x] Preprocessed traces generated per-proof (PoseidonChip generates own preprocessed)
- [x] No code change needed — existing chips are stateless and work in any registry

### G13: Per-Tier Setup Functions (Medium) ✅

- [x] `execution_tier_setup()` — registry + keys for ExecutionChip + StaticTableChip + bus consumers
- [x] `column_tier_setup(table, col)` — registry + keys for MemoryShard + StateShard + MetaShard + bus consumers
- [x] `root_tier_setup()` — registry + keys for SmtPath chips + bus consumers
- [x] `ChipIdAllocator::for_shards()` starts at 100, independent per column proof
- [x] `TierSetup::build_traces()` — per-tier trace building from WitnessStore
- [x] `create_proof_setups()` + `build_proof_traces()` — orchestrates all tiers from PartitionedStores

### G6: ColumnMeta in Sharded Model (Small) — Deferred

- [ ] Com_old, Com_new as public values from StateShard
- [ ] MetaShard simplified to public-value extractor
- [ ] SMT leaf computation moved to root proof
- Note: Current design works without decomposition — MetaShardChip handles leaf digest within column proof

---

## Phase D: Root Proof (~400 LOC) ✅

- [x] Root proof chip set: SmtColPathChip + SmtTablePathChip (ColumnMetaChip removed — handled by per-column MetaShardChip)
- [x] Input: all Com_old/Com_new + cumsum values from column + execution proofs
- [x] Cumsum balance verification: `cumsum_exec + Σ cumsum_col[i] = 0`
- [x] SMT path verification: Com values consistent with old_root → new_root
- [x] Root proof integration test (via E2E)

Key design decision: ColumnMetaChip is NOT in the root tier. In the sharded model,
MetaShardChip handles commitment verification + leaf digest computation within each
column proof. Leaf digests reach the root tier via the external SMT_LEAF_DIGEST bus.

---

## Phase E: End-to-End Validation (~500 LOC) ✅

- [x] E1: E2E test — DSL → compile → execute → witness → shard → prove → verify
- [x] E2: Statement consistency test — identity tx preserves old_root == new_root
- [ ] E3: Benchmark — prover speedup (sequential vs parallel)
- [x] E4: Multi-column test — 2 tests passing:
  - SmtPathCols sibling split: old_sibling/new_sibling (82→90 cols, ~10% width increase)
  - Untouched column proof skipping: prepare_shard_witness + build_smt_paths filter untouched
  - Tests: `multi_column_touched_and_untouched`, `multi_column_all_touched`
- [x] E5: Proof structure validation — C+2 architecture verified

---

## Phase P: Parallelization (~250 LOC, independent track)

> Tracked in [optimization.md](optimization.md) §Tier 1b

Cross-proof parallelism (C+2 sub-proofs) + within-proof chip-level parallelism. Uses rayon with adaptive work-stealing.

- [ ] P-1: Chip-level quotient parallelism — `compute_chip_quotients()` `par_iter` (highest ROI)
- [ ] P-2: Cross-proof sub-proof parallelism — `prove_impl()` exec ‖ cols ‖ root
- [ ] P-3: Chip-level perm trace parallelism — `build_perm_traces()` `par_iter`
- [ ] P-4: Column-level trace building — `build_proof_traces()` `par_iter`
- [ ] P-5: Cross-proof verification — `verify_impl()` parallel sub-proof checks

---

## Migration (Goal 4) ✅

- [x] TabulaMachine wraps sharded ProofSetups (sharded IS the base)
- [x] Removed monolithic code: MachineBuilder, MonolithicProof, prove_with_key, verify_with_key
- [x] Removed global-only chips from machine: InterTxOrderChip, StateColumnChip, ColumnMetaChip
- [x] Removed dead abstractions: CommitmentScheme/SmtScheme, GlobalSortedMemory, MemoryModel, SsmcScheme
- [x] Merged modules: proof.rs + sharded_proof.rs → proof.rs; setup.rs from sharded_setup.rs
- [x] Updated all consumers: daemon prove.rs, benchmarks, all integration tests
- [x] 971 tests passing, zero warnings

---

## Verification

```bash
cargo check --workspace
cargo test --workspace
# E2E tests
cargo test -p tabula-machine --test e2e
# Machine tests
cargo test -p tabula-machine --test machine
```
