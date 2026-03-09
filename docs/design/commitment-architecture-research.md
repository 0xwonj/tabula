# Commitment Architecture Research

> **Version**: 1.0
> **Date**: 2026-03-07
> **Status**: Complete — architectural decision made
> **References**: [master-roadmap.md](master-roadmap.md), [implementation-workplan.md](implementation-workplan.md)

---

## 1. Problem Statement

Phase 2 of the master roadmap proposed migrating three global chips (InterTxOrderChip, StateColumnChip, ColumnMetaChip) to per-column shard chips (MemoryShardChip, StateShardChip, MetaShardChip). This research evaluates whether that migration is optimal from a proof perspective.

**Core tension**: Global chips minimize proof size but waste padding and prevent per-column parallelism. Shard chips enable parallelism and extensibility but multiply trace width by column count.

---

## 2. Quantitative Analysis

### 2.1 Chip Widths (W=3, Standard)

| Role | Global Chip | Width | Shard Chip | Width | Delta |
|------|-------------|-------|------------|-------|-------|
| Memory consistency | InterTxOrderChip | 56 | MemoryShardChip | 48 | -8 (SameKeyDetection + LexDir) |
| State commitment | StateColumnChip | 101 | StateShardChip | 93 | -8 (same) |
| Column metadata | ColumnMetaChip | 104 | MetaShardChip | 96 | -8 (IsZero×2 + LexDir + tag) |
| **Total** | | **261** | | **237** | **-24 (9.2%)** |

Width savings per column: 24 cols (9.2%). This is the ONLY advantage shard chips have in trace area per row.

### 2.2 Proof Size Analysis

Tabula uses batched MMCS (Plonky3): all chip traces in one Merkle tree per commitment round. One FRI opening proof covers all rounds.

**Per FRI query, the opening data = Σ(width of every matrix in the round).**

Merkle authentication paths are SHARED (one path per query per round regardless of matrix count). The overhead is purely in field elements.

| Metric | Global (3 chips) | Shard (3C chips, C=50) | Ratio |
|--------|-------------------|------------------------|-------|
| Memory/State/Meta width | 261 | 50 × 237 = **11,850** | **45x** |
| + other core chips | ~400 | ~400 | 1x |
| **Total main round width** | **~661** | **~12,250** | **18.5x** |

**Per-query opening field elements**:
- Perm round: ~24 (global) vs ~1,200 (shard) — ~50x
- Quotient round: ~48 (global) vs ~2,400 (shard) — ~50x

**Total proof size growth with pure sharding**: ~**18-20x** for C=50 columns.

With production FRI parameters (Q=100 queries):
- Global: 100 × 661 × 4 bytes ≈ **264 KB** (main round)
- Shard: 100 × 12,250 × 4 bytes ≈ **4.9 MB** (main round)

### 2.3 Prover Time

NTT cost = O(W × H × log H) per chip.

For C=50, each column with 100 rows:

| | Global | Shard |
|---|---|---|
| H (padded) | 8,192 (5000→8192) | 50 × 128 (100→128 each) = 6,400 |
| NTT ops | 261 × 8,192 × 13 ≈ **27.8M** | 50 × 237 × 128 × 7 ≈ **10.6M** |
| Parallelism | Sequential (1 big NTT) | 50-way parallel |

Shard is **2.6x cheaper** in NTT and trivially parallelizable. For uneven column sizes, the advantage grows further (small columns avoid large-H padding).

### 2.4 Conclusion

| Metric | Winner | Magnitude |
|--------|--------|-----------|
| **Proof size** | Global | **18-20x smaller** |
| **Verifier time** | Global | **18-20x faster** |
| **Prover NTT** | Shard | 2-3x faster (uniform), 5-10x (uneven) |
| **Parallelism** | Shard | Embarrassingly parallel |
| **Untouched skip** | Shard | Zero cost vs minimal-segment cost |

**Proof size is the dominant constraint** for on-chain verification and recursive compression. Global chips are strictly better for the default path.

---

## 3. Research Directions

### 3.1 Direction D1: Poseidon Chain Delegation

**Idea**: Move hash chain computation from StateShard/StateColumn into the global PoseidonChip.

The PoseidonChip already processes all Poseidon permutations. StateColumn's running accumulator (old_hash_acc[8], new_hash_acc[8]) + hash input (old_hash_chain[16], new_hash_chain[16]) = 48 columns exist solely to maintain the chain state between Poseidon calls. If PoseidonChip tracked chain continuity internally (chain_id + step_index + chaining constraint), StateColumn could be eliminated.

**Effect**: StateColumn (101 cols) → eliminated. PoseidonChip: 93 → 96 cols (+3 shared).

**Feasibility**: HIGH — engineering optimization, no new cryptography.

### 3.2 Direction D2+D3: Algebraic Accumulator in Global Chip

**Idea**: Replace Poseidon hash chain with an order-independent algebraic accumulator embedded in the global memory chip.

```
Com_i = Σ_j H(encode(key_j, value_j))    over EF4 (BabyBear⁴)
```

Sum-based commitment is order-independent → no sorting needed → segment detection in GlobalMemoryChip suffices for accumulation.

**In-chip accumulation**: Add ~17 columns to UnifiedMemoryChip:
- h_old[4], h_new[4]: entry hash (EF4)
- acc_old[4], acc_new[4]: running sum (EF4)
- is_state_entry: boolean

**Effect**: UnifiedMemoryChip = 56 + 17 = 73 cols (fixed, C-independent). StateColumn/StateShard eliminated entirely. Width reduction from 157 → 73 for memory+state combined.

**Security concern**: Sum-based multiset hash over EF4 (2^{124}) has birthday bound ~2^{62}. Insufficient for 128-bit security. Mitigations:
- Double accumulator: (Σ H₁, Σ H₂) → ~2^{124} security (needs formal proof)
- Power-sum symmetric hash → higher security margin
- Multiplicative accumulator → equivalent to polynomial evaluation

**Consensus compatibility**: Sum-based commitment is deterministic (given deterministic H). Can serve as SMT leaf. Compatible with existing state root structure.

**Non-membership proof**: SSMC uses sorted-list adjacency for non-membership. Sum-based accumulator is orderless → needs alternative (e.g., explicit table-size commitment, or delegation to SMT).

**Feasibility**: MEDIUM — requires security proof for chosen accumulator construction. 1-2 month research effort.

### 3.3 Direction D4: Recursive Composition

**Idea**: Per-column inner STARK proofs + recursive tree aggregation → fixed-size final proof.

```
Layer 1: C parallel column proofs (MemoryShard + StateShard + MetaShard each)
Layer 2: Tree reduction — verify 2 proofs at each node
Layer 3: Final proof (execution + meta + recursive verifications)
```

**Proof size**: O(W_verifier × Q) — fixed, independent of C. But W_verifier ≈ 10K cols (STARK verifier circuit in AIR), so intermediate proof is ~4 MB before Groth16 wrapping.

**Prover time**: Dominated by recursive verification overhead.
- C=50: ~425G ops for tree reduction vs ~100M ops for single global STARK
- Wall-clock: ~60s (recursive) vs ~2-5s (global)
- Crossover: column count C > ~1000 with R > ~100K rows each (global trace > 30 GB)

**Security**: Standard recursive STARK composition. Well-proven by SP1, RISC Zero, Polygon.

**Feasibility**: HIGH (well-known pattern) but HIGH effort (6+ months for recursive verifier circuit).

### 3.4 Comparative Summary

| | Proof Width | Prover Parallel | Untouched Skip | New Crypto | Effort |
|---|---|---|---|---|---|
| **Current Global** | 261 (fixed) | No | Partial | None | Current |
| **Pure Shard** | C×237 | Yes | Yes | None | 2-4 weeks |
| **D1: Poseidon delegation** | 163 (fixed) | No | Partial | None | 2-3 weeks |
| **D2+D3: Algebraic accum** | 73 (fixed) | No | Yes | **Needs proof** | 1-2 months |
| **D4: Recursive** | O(1) final | Yes | Yes | None | 6+ months |

---

## 4. Architectural Decision

### 4.1 Chosen Architecture: Global-First + Extensible Shard

**Keep global chips as the default path.** The 18-20x proof size advantage is decisive. Shard chips remain as infrastructure for custom commitment schemes via the `ColumnCommitment` trait.

```
Default path (SSMC/SMT):
  InterTxOrderChip (global, 56 cols)  — all columns in one trace
  StateColumnChip  (global, 101 cols) — all SSMC columns in one trace
  ColumnMetaChip   (global, 104 cols) — all columns in one trace

Custom commitment path:
  ColumnCommitment trait → creates shard chips per column
  SsmcCommitment, SmtCommitment → available but NOT default
  Custom schemes → implement ColumnCommitment, produce shard chips
```

### 4.2 ColumnCommitment Trait Revision

The current trait takes `col: &ColumnPlan` (per-column). To support global-style implementations, revise to batch API:

```rust
trait ColumnCommitment: Send + Sync {
    fn name(&self) -> &str;
    fn chip_ids(&self) -> Vec<ChipId>;
    fn build_traces(
        &self,
        cols: &[ColumnPlan],  // batch, not single column
        store: &WitnessStore,
    ) -> Result<Vec<(ChipId, TraceEntry)>, TabulaError>;
    fn output_buses(&self) -> Vec<BusId>;
}
```

This allows:
- **Global-style**: One implementation processes all columns of its scheme together (fixed width)
- **Shard-style**: Implementation creates per-column chips (width scales with C)
- **Hybrid**: Group columns into batches of ~10 for balanced tradeoff

### 4.3 Future Optimization Path

```
Phase 2 (now):   Keep global chips + ColumnCommitment for extensibility
Phase 4:         D1 — Poseidon chain delegation (eliminate StateColumn, save 101 cols)
Phase 5:         D2+D3 — Algebraic accumulator (if security proof succeeds)
                 D4 — Recursive composition (when scale demands it)
```

D1 → D2+D3 → D4 is a natural progression: each step reduces global chip width and eventually transitions to per-column parallelism at scale, with recursive compression for final proof size.

---

## 5. Impact on Existing Code

### 5.1 Files to Revert

The previous session partially edited 6 witness pipeline files toward pure shard migration. Since global chips are kept, these should be reverted:

| File | Change | Action |
|------|--------|--------|
| `witness/src/trace/builder.rs` | Rewritten (BROKEN) | **Revert** |
| `witness/src/trace/memory/mod.rs` | Rewritten for shard | **Revert** |
| `witness/src/trace/memory/inter_tx.rs` | Rewritten for shard | **Revert** |
| `witness/src/trace/memory/state.rs` | Rewritten for shard | **Revert** |
| `witness/src/trace/memory/chain.rs` | Rewritten for shard | **Revert** |
| `chips/src/shards/state/trace.rs` | EntrySource moved | **Keep** (valid regardless) |
| `chips/src/shards/state/mod.rs` | Re-export added | **Keep** (valid regardless) |

### 5.2 What Stays

- Shard chip implementations (`shards/memory/`, `shards/state/`, `shards/meta/`) — infrastructure for ColumnCommitment
- `SsmcCommitment`, `SmtCommitment` — available for custom scheme path
- `ColumnCommitment` trait, `BusConsumer` trait, `ProofPlan` — extensibility framework
- Global chips (InterTxOrder, StateColumn, ColumnMeta) — default path

---

## 6. Related Work

| System | Approach | Proof Size | Parallelism |
|--------|----------|------------|-------------|
| SP1 | Per-chip traces, batched MMCS, recursive compression | ~280 chips in one MMCS | Segment-level |
| OpenVM | Per-AIR traces, MMCS | Similar to SP1 | Per-AIR |
| RISC Zero | Monolithic trace | Minimal | Segment sharding |
| Stwo | Mixed-degree Circle STARK | Degree-aware | Per-component |
| **Tabula (chosen)** | Global chips (default) + shard chips (custom) | Fixed width | Future recursive |

---

## Appendix: Exact Column Counts

### Global InterTxOrderChip (56 cols, W=3)
- Identity (3): is_real, table_id, col_id
- Key (11): KeyRangeChecked
- Tx ordering (2): tx_index, tx_diff
- Row type (3): is_init, has_read, has_write
- Input value (4): input_val[3], input_is_null
- Output value (4): output_val[3], output_is_null
- Chain tracking (2): is_last_for_key, has_ever_written
- Same-key detection (11): SameKeyDetection(5) + IsZero×3(6)
- Key ordering (13): OrderingRangeChecked
- Lex direction (3): LexOrderingDirection

### Shard MemoryShardChip (48 cols, W=3)
- Same as above minus SameKeyDetection(5) and LexDir(3) = 56 - 8 = 48

### Global StateColumnChip (101 cols, W=3)
- Identity (3), Key (11), Source (3), Values (6), Segment flag (1)
- Old hash chain (24): acc[8] + input[16]
- New hash chain (24): acc[8] + input[16]
- Chain tracking (6): 5 flags + write_seen_prefix
- Key ordering (13), Segment detection (5), Lex direction (3)
- LogUp multiplicity (2)

### Shard StateShardChip (93 cols, W=3)
- Same minus Segment(5) and Lex(3) = 101 - 8 = 93

### Global ColumnMetaChip (104 cols)
- Identity (4): is_real, table_id, col_id, tag
- Commitments (16): com_old[8], com_new[8]
- Flags (3): is_empty_old, is_empty_new, is_touched
- empty_read_mult (1)
- Ordering (7): IsZero×2(4) + LexDir(3)
- Com_empty verification (25): perm_input[16], perm_output[8], has_empty_check
- Leaf digest (48): 2×(perm_input[16] + digest[8])

### Shard MetaShardChip (96 cols)
- Same minus tag(1), IsZero×2(4), LexDir(3) = 104 - 8 = 96
