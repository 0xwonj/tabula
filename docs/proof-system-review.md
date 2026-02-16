# Tabula Proof System Review

**Date:** 2026-02-15
**Scope:** End-to-end proof system (proof-spec v0.9, semantics-spec v0.2.1, all code in tabula-proof, tabula-commitment, tabula-executor)
**Status:** Post-M6 (AIR foundation complete), pre-M7 (baseline chips)

---

## 1. Executive Summary

Tabula's proof system is architecturally sound. Three design decisions provide fundamental advantages over general-purpose zkVMs:

1. **NF rules eliminate intra-tx RAM consistency** — zero proof cost for intra-tx memory
2. **Static (t,c) partitioning** — memory argument sharded per-column, not global
3. **Hybrid VC (SSMC + SMT)** — optimal commitment strategy per column size

The dominant bottleneck is the **execution trace (86% of baseline proof cost)**, not the memory layer. Template chips (Phase 3) are the critical optimization, offering 84% execution trace reduction. The most urgent technical risk is **M9 LogUp wiring** — all chips are standalone-unsound until cross-chip interactions are connected.

---

## 2. Architecture Overview

### 2.1 Three-Layer Separation

```
Stage 1: Execution    (tabula-executor)   — pure logic, no cryptography
Stage 2: Commitment   (tabula-commitment) — state VC (SSMC/SMT/Poseidon)
Stage 3: Proving      (tabula-proof)      — AIR constraints, STARK proof
```

Key property: executor uses only `dyn Hasher`; Poseidon appears only in commitment/proof layers. Mock implementations enable independent testing of each layer.

### 2.2 Data Flow

```
Batch Input
    │
    ▼
BatchExecutor::execute_batch()
    │  (deterministic, per-tx checkpoint/rollback)
    ▼
ExecutionResult { read_set_old, write_set_final, events, tx_outcomes }
    │
    ▼
WitnessGenerator::generate()
    │  (encode values → ComEnc, route keys, compute state roots)
    ▼
BatchWitness { columns, column_metas, old/new_state_root, key_routes }
    │
    ▼
AIR Trace Generation → STARK Proof
```

### 2.3 Proof Layers

| Layer | Scope | Mechanism | Cost |
|-------|-------|-----------|------|
| **Layer B** (intra-tx) | Single tx against snapshot | NF rules + SSA wiring, no RAM argument | O(instructions) |
| **Layer C** (inter-tx) | Batch of N txs | GlobalSortedMem + LogUp | O(accesses) |

---

## 3. Core Design Principles

### 3.1 Intra-Tx Cost Elimination via NF Rules

The canonical state normal form (semantics-spec §2.3) enforces four structural invariants at compile time:

| Rule | Effect | Proof Cost Saved |
|------|--------|-----------------|
| **NF-1** (Unique-Read) | Each (t,c,r) read at most once per tx | No read-cache verification |
| **NF-2** (Unique-Write) | Each (t,c,r) written at most once per tx | No intra-tx write coalescing |
| **NF-3** (No-Read-After-Write) | SSA wire semantics, no re-opening | No forwarding proof |
| **NF-4** (Key-Alias Resolvability) | Static conflict detection | No runtime key comparison |

**Result**: Zero intra-tx RAM consistency cost. This is Tabula's most distinctive design advantage over zkVMs, which pay O(N log N) for global memory sorting over millions of operations.

### 3.2 Static (t,c) Partitioning

IR requires `(t, c)` to be compile-time literals (MUST invariant, proof-spec §3.2). Only row key `r` is dynamic. This enables:

- Per-column memory argument sharding (no global sort)
- Bounded access counts per partition
- Compile-time width-class determination

Compared to zkVM's single global address space with dynamic addresses, Tabula's sorting scope is ~1000x smaller.

### 3.3 Hybrid State Commitment

Per-column automatic selection (proof-spec §10.1):

| Strategy | When | Opening Cost | Update Cost |
|----------|------|-------------|-------------|
| **SSMC** | m ≤ threshold (~100-300 rows) | O(LogUp) per access | O(m+w) merge proof |
| **SMT** | m > threshold | O(64 × Poseidon) per path | O(w × 64 × Poseidon) |

Break-even `m*` estimated at 100-300 rows; must calibrate via Plonky3 benchmarks (B7).

### 3.4 Two-Tier Value Encoding

Schema-typed, digest-native encoding (proof-spec §10.3):

| Tier | Usage | Format | Null Handling |
|------|-------|--------|--------------|
| **Tier 1 (ComEnc)** | SSMC hash chain, SMT leaf, merge trace | w(T) FE only | No null (absence = structural) |
| **Tier 2 (TraceEnc)** | GlobalSortedMem, instruction trace | w(T) FE + val_is_null | Null = canonical zero + flag |

Width per type: `w(Bool)=1, w(U64)=w(I64)=3, w(Digest)=8`.

---

## 4. Cost Analysis and Bottlenecks

### 4.1 Baseline Cost Distribution (M7-M9)

```
┌─────────────────────────────────────────────┐
│ Execution Trace             ████████████ 86% │  ← dominant bottleneck
│ GlobalSortedMem             ██           11% │
│ GlobalSSMC + GlobalMerge    █           2.5% │
│ ColumnMeta                               0.5%│
└─────────────────────────────────────────────┘
```

Source: proof-optimization-architecture.md §1, based on 20 instructions, 5 accesses, 1000 txs.

### 4.2 Why Execution Trace Dominates

The interpreter chip processes all instruction types in a single AIR chip:

```
Transfer tx (6 instructions):
  6 rows × ~30 cols = 180 cells per tx

Template chip equivalent:
  1 row × ~28 cols = 28 cells per tx
```

Every row carries opcode dispatch selectors and unused columns for non-active instruction types.

### 4.3 Optimization Roadmap

| Optimization | Reduction | Phase | Status |
|-------------|-----------|-------|--------|
| ReadOnly Opening (§2) | 5% | Phase 2 | Designed |
| ShortRun (§3) | 7% | Phase 2 | Designed |
| **Template Chips** | **84%** | **Phase 3** | Designed, not implemented |
| Both axes combined | 90% | Phase 3 | — |
| Literal carry (§5) | 92% | Phase 4 | — |

Template chips are the game-changer: 1 row/tx for matched tx types (e.g., Transfer) vs I rows/tx for the generic interpreter.

### 4.4 Per-Component Cost Formulas

From proof-spec §9, let P = Poseidon permutation cost, R = u64 comparison cost (~8-12 constraints), L = LogUp per-access cost (~5-15 constraints):

| Component | SSMC Cost | SMT Cost |
|-----------|-----------|----------|
| Column commitment setup | m × (R + P_stream) | N/A |
| Per-read opening | L | 64 × P |
| Non-membership | L + 2R | 64 × P |
| State update | (m+w) × (R + L + P_stream) | w × 64 × P |
| Memory consistency | A × L | A × L |

### 4.5 Comparison with zkVM

| Aspect | zkVM (e.g., SP1) | Tabula |
|--------|-------------------|--------|
| Memory ops per proof | 10^6 - 10^9 | ~400 (100 tx × 4 access) |
| Sorting scope | Global address space | Per-(t,c) segment |
| Sorted table rows | ~10^6 | ~600 |
| RAM argument | Mandatory, global | Inter-tx only, sharded |
| Intra-execution cost | O(N) sorted memory | Zero (NF rules) |

---

## 5. Problems and Issues

### 5.1 CRITICAL: Range Check Absence

**Location**: `tabula-proof/src/air/gadgets/integer.rs`
**Status**: Known, deferred to M9

U64Limbs and StrictIneq gadgets constrain limb reconstruction but do NOT range-check individual limbs. Without LogUp RangeCheck bus wiring:

```rust
// Current: limb0 + limb1 * 2^30 + limb2 * 2^60 = expected  ✓
// Missing: limb0 ∈ [0, 2^30), limb1 ∈ [0, 2^30), limb2 ∈ [0, 16)  ✗
```

A malicious prover can use out-of-range field elements as "limbs" that satisfy the reconstruction constraint but represent different u64 values. **All integer comparison and ordering constraints are unsound until M9.**

This is by design (LogUp wiring is a final integration step), but means no chip is standalone-sound before M9.

**Mitigation**: M9 connects RangeCheck chip (preprocessed table of [0, 2^16)) via LogUp to all limb columns.

### 5.2 HIGH: Value::Null Semantic Divergence

**Location**: `tabula-core` Value enum vs semantics-spec §2
**Impact**: Potential confusion during AIR constraint authoring

```
Spec (normative):
  Read(dst_val: T, dst_is_null: Bool) → 2-slot result
  Null is NOT a value type; absence = separate boolean flag

Code (current):
  Value::Null exists as enum variant
  Interpreter produces Value::Null in slots
  Witness generator converts to (canonical_zero, val_is_null=true)
```

The conversion is correct, but the semantic mismatch between spec and code risks subtle bugs when implementing AIR constraints that directly reference Value types.

**Recommendation**: Document the divergence explicitly in interpreter.rs. Consider migrating to `Option<Value>` in a future refactor to align with spec.

### 5.3 ~~HIGH~~ N/A: Failed Tx read_set_old Contamination (Non-Issue)

**Location**: `tabula-executor/src/overlay.rs`
**Status**: Non-issue. The overlay's undo log already handles rollback of read_cache entries correctly. When a checkpoint is active, `ReadCacheFill` undo entries are recorded, and `rollback()` removes those entries from the read cache. The scenario described below does NOT occur.

```
Actual behavior:
  tx_1 succeeds: read(K1) → read_cache = {K1}, discard_checkpoint()
  tx_2 fails:    read(K2) → read_cache = {K1, K2}, undo_log = [ReadCacheFill(K2)]
                 rollback() → read_cache = {K1}  ← K2 correctly removed
  Result:         read_set_old = {K1}  ← correct
```

**Note**: The overlay has been internally separated into `ExecutionState` (state + undo log) and `TraceRecorder` (events + time). This prepares for Phase 4 (ok-gating), where `rollback_state_only()` will roll back state while preserving events for failed-tx trace inclusion. The separation is a future enabler, not a bug fix.

### 5.4 HIGH: Key Routing Invariant Undefended

**Location**: `tabula-proof/src/route.rs` lines 32-39
**Impact**: Key mis-routing if executor invariant violated

`route_keys()` assumes `result.events` contains only successful-tx accesses (failed-tx events rolled back). No defensive check exists. If an executor bug leaks a failed-tx write event:

- A written key could be classified as `ReadOnly`
- ReadOnly keys skip state-update proofs
- Result: invalid proof (write not reflected in newRoot)

**Recommendation**: Add `debug_assert!` validating that no event references a key written only by failed txs. Alternatively, filter events by tx_outcome before routing.

### 5.5 MEDIUM: ColumnMeta Ordering Direction Unverified

**Location**: `tabula-proof/src/air/chips/column_meta/air.rs`
**Status**: Known, deferred to M9

Current constraints enforce uniqueness (`table_same * col_same = 0`) but NOT strict increasing direction. A malicious prover could reverse row order. Full direction enforcement requires range checks on diff columns, arriving with M9 LogUp integration.

### 5.6 MEDIUM: Cross-Layer Integration Tests Absent

**Location**: Entire test suite
**Impact**: No verification that Executor → Witness → AIR constraint check works end-to-end

Individual layer coverage is strong:
- Executor: 72 tests
- Commitment: 97 tests
- Proof (witness + AIR): 53 tests

But no test exercises the full pipeline: `execute_batch() → WitnessGenerator::generate() → generate_sorted_mem_trace() → debug_check()`.

**Recommendation**: Add integration tests in tabula-proof that:
1. Execute a batch via executor with real overlay
2. Generate witness via WitnessGenerator
3. Generate AIR traces for ColumnMeta and GlobalSortedMem
4. Run debug_check on generated traces

### 5.7 LOW: SSMC Hash Chain vs Sponge

**Location**: `tabula-commitment/src/ssmc.rs`
**Impact**: 8x potential Poseidon amortization not captured

Current SSMC commitment uses iterative hash chain (1 Poseidon permutation per entry). Sponge mode with rate=8 would amortize to P/8 per entry. Documented in proof-spec §4.2 as optimization path.

**Recommendation**: Implement after M8 Poseidon chip integration; no correctness impact.

---

## 6. Soundness Dependency Graph

All chips depend on M9 LogUp wiring for standalone soundness:

```
                    M9 LogUp Wiring
                   ╱       │        ╲
                  ╱        │         ╲
        RangeCheck    Memory Bus    ColumnMetaJoin
        (limb checks) (Exec↔GSM)   (meta lookups)
             │            │              │
        ┌────┴────┐  ┌────┴────┐   ┌────┴────┐
        │U64Limbs │  │timestamp│   │root     │
        │StrictIneq│ │binding  │   │inclusion│
        │ordering │  │(§8.7)   │   │proofs   │
        └────┬────┘  └────┬────┘   └────┬────┘
             │            │              │
        SortedMem     Execution      ColumnMeta
        SSMC order    Clock proof    State root
```

**Before M9**: Each chip's constraints are correct in isolation (debug_check passes for valid traces), but cross-chip invariants (LogUp multiset equality, range-checked limbs, timestamp binding) are unverified.

**After M9**: Full system soundness — LogUp connects all buses, RangeCheck validates all limbs, execution timestamps are bound to instruction clock.

### 6.1 LogUp Bus Inventory

| Bus | Direction | Purpose | Status |
|-----|-----------|---------|--------|
| `Memory` | Execution ↔ GlobalSortedMem | Access events ↔ sorted memory | Declared (M6) |
| `SsmcMembership` | Init rows ↔ GlobalSSMC | Base-state membership proofs | Declared (M6) |
| `MergeCompleteness` | GlobalMerge ↔ {GSM, ShortRun} | Write-set contribution | Declared (M6) |
| `ColumnMetaJoin` | Any chip ↔ ColumnMeta | Metadata lookups | Declared (M6) |
| `RangeCheck` | Any chip → RangeCheck table | Limb range validation | Declared (M6) |
| `ReadOnlyOpening` | Execution ↔ ReadOnlyOpeningChip | Read-only key VC opening | Declared (M6) |
| `PoseidonPermutation` | Any chip ↔ Poseidon | In-circuit hashing | Declared (M6) |
| `SmtOpening` | Init rows ↔ MerkleVerifier | SMT opening proofs | Not yet declared |

All buses are type-declared in `bus.rs` but LogUp wiring is deferred to M9.

---

## 7. Architectural Analysis

### 7.1 Template Chips: The Critical Optimization

Template chips replace the generic interpreter for known tx patterns:

```
Generic Interpreter:  I rows/tx × ~30 cols = ~180 cells/tx (Transfer)
TransferTemplate:     1 row/tx  × ~28 cols = ~28  cells/tx

Reduction: 84% per tx, ~70% of total proof cost
```

Template chips emit the **same LogUp bus fingerprints** as the interpreter — the memory layer is unaffected. This orthogonality is a key architectural strength.

**Implementation path**: Phase 3 (after M7-M8 baseline chips), estimated ~640 lines.

**Trade-off**: Each template is tx-type-specific. A batch with N distinct tx types needs up to N templates plus the interpreter as fallback. Partial matching (Phase 5) would generalize.

### 7.2 ok-gating vs Failed Tx Exclusion

Current design excludes failed txs from the proof trace entirely.

| Aspect | Current (Exclusion) | Alternative (ok-gating) |
|--------|-------------------|------------------------|
| Trace size | Minimal (only successful txs) | Larger (all txs, gated writes) |
| Template compatibility | Variable row count per batch | Fixed row count (all txs) |
| Censorship resistance | Prover can omit valid txs | All txs visible, auditable |
| Complexity | Simpler (current) | Additional ok flag + gating |

ok-gating synergizes with template chips: fixed-format traces enable 100% template matching. Consider adopting when template chips are implemented (Phase 3).

### 7.3 Width-Class Proliferation

Width classes (Narrow=1, Standard=3, Wide=8) applied to global tables create up to 9 chip variants:

```
3 width classes × 3 global tables = 9 chip variants
  GlobalSortedMem_{narrow, standard, wide}
  GlobalSSMC_{narrow, standard, wide}
  GlobalMerge_{narrow, standard, wide}
```

**Alternative**: Use MAX_W=8 for all, zero-pad narrower types.

```
Waste analysis (Standard W=3 vs MAX_W=8):
  val columns: 3 → 8 (+5 columns)
  Total width: 34 → 39 (+15%)

  For Bool (W=1 vs MAX_W=8):
  val columns: 1 → 8 (+7 columns)
  Total width: 32 → 39 (+22%)
```

**Recommendation**: Start with MAX_W=8 (simpler, one chip per global table). Introduce width-class splitting only after benchmarks show >15% waste matters for real workloads. Plonky3 supports 30+ chips natively (SP1 pattern), so splitting later is straightforward.

### 7.4 STARK Splitting Possibility

Architecture doc mentions future option to split into Exec-STARK + State-STARK:

```
Current: Single STARK (all chips, one proof)

Possible:
  Exec-STARK: instruction trace + SSA wiring
  State-STARK: GlobalSortedMem + SSMC + Merge + SMT
  Link: access log digest as shared public input
```

| Aspect | Benefit | Cost |
|--------|---------|------|
| Parallel proving | Yes — prove execution and state independently | Two proofs to verify |
| Parameter tuning | Different FRI params per STARK | More complex setup |
| Prover memory | Lower peak (smaller per-STARK) | Coordination overhead |

**Recommendation**: Not justified at current scale (batches of 100-1000 txs). Revisit if batch sizes reach 10,000+ txs or prover memory becomes limiting.

### 7.5 Overlay Dual-Role Concern

Overlay serves two roles simultaneously:

1. **Runtime correctness**: read-your-writes, checkpoint/rollback
2. **Trace collection**: event recording, read_set/write_set extraction

This coupling was initially suspected of causing read_set_old contamination (§5.3), but the undo log already handles read_cache rollback correctly.

The overlay has now been internally separated:

```
Current (separated):
  struct ExecutionState { write_buffer, read_cache, undo_log, checkpoints }
  struct TraceRecorder { events, time, tx_index, checkpoints }
  struct Overlay { state: ExecutionState, recorder: TraceRecorder }
```

This prepares for Phase 4 (ok-gating), where `rollback_state_only()` will roll back state while preserving events for failed-tx trace inclusion.

---

## 8. Implementation Status

### 8.1 Completed (M1-M6)

| Milestone | Deliverable | Tests |
|-----------|------------|-------|
| M1-M3 | Executor, overlay, batch, NF validation | 72 |
| M4 | Commitment primitives (Poseidon, SMT, SSMC, Hybrid VC) | 97 |
| M5 | Witness generation (ExecutionResult → BatchWitness) | 50 |
| M6 | AIR foundation (column pattern, gadgets, ColumnMeta chip, debug checker) | 53 |

**Total: 272 tests across workspace.**

### 8.2 Current (M7)

GlobalSortedMemChip implemented with 10 constraint groups:

1. Boolean fields (9)
2. is_real prefix
3. Null canonicality
4. Init format (τ=0, is_write=0, mem=val)
5. Segment-first init
6. Same-key detection (IsZero gadgets)
7. Ordering (StrictIneq, dual r/τ selector)
8. Memory transitions (read/write/carry)
9. Init-row uniqueness (implicit in ordering)
10. Write-set extraction (is_last_for_key, has_written)

Width: 34 columns (Standard, W=3). Const generic `<const W: usize>` for width-class support.

### 8.3 Remaining (M8-M9+)

| Milestone | Scope | Dependency |
|-----------|-------|------------|
| M8 | GlobalSSMC, GlobalMerge, SmtPath, Poseidon chip | M7 |
| M9 | LogUp wiring, full STARK integration, end-to-end proof | M8 |
| Phase 2 | ReadOnlyOpening + ShortRun chips (~620 lines) | M7-M8 |
| Phase 3 | Template chips (~640 lines) | Phase 2 |
| Phase 4 | Literal carry (~165 lines) | Phase 3 |

---

## 9. Recommendations

### 9.1 Immediate (High Impact, Low Cost)

| # | Action | Effort | Impact |
|---|--------|--------|--------|
| 1 | **Cross-layer integration tests**: Executor → Witness → AIR debug_check | 1-2 days | Catches pipeline mismatches early |
| 2 | **Filter read_set_old**: Remove keys from failed-tx-only reads | Hours | Eliminates wasted VC openings |
| 3 | **Mock LogUp verification**: Check multiplicity sums balance in debug_check | 1 day | Early detection of bus mismatches |
| 4 | **Document Value::Null divergence**: Comment in interpreter.rs + witness.rs | Minutes | Prevents confusion for AIR authors |

### 9.2 Medium-Term (M8-M9 Timeframe)

| # | Action | Effort | Impact |
|---|--------|--------|--------|
| 5 | **Width-class decision**: Benchmark MAX_W=8 vs per-width chips | After M8 | Determines chip count (7 vs 15+) |
| 6 | **SSMC sponge mode**: Replace hash chain with rate-8 sponge | M8 | 8x SSMC commitment speedup |
| 7 | **Defensive route_keys**: Add debug_assert for failed-tx invariant | Hours | Catches executor bugs in tests |
| 8 | **SSMC threshold calibration** (B7): Benchmark SSMC vs SMT crossover | After M8 | Optimal per-column strategy selection |

### 9.3 Long-Term (Post-M9)

| # | Action | Effort | Impact |
|---|--------|--------|--------|
| 9 | **Template chips** (Phase 3) | ~640 lines | 84% execution trace reduction |
| 10 | **ok-gating**: Execute all txs, gate writes by ok flag | Design + impl | Template synergy + censorship resistance |
| 11 | **Overlay separation**: Split runtime overlay from trace recorder | Refactor | Clean failed-tx handling |
| 12 | **Value::Null migration**: Replace with Option\<Value\> throughout | Breaking refactor | Spec alignment |

---

## 10. Key Metrics

### 10.1 Test Coverage

| Crate | Tests | Focus |
|-------|-------|-------|
| tabula-executor | 72 | Instruction semantics, overlay, batch, consistency |
| tabula-commitment | 97 | Codec, hashing, SMT, SSMC, hybrid VC |
| tabula-proof | 53 | Witness gen, AIR gadgets, ColumnMeta, GlobalSortedMem |
| tabula-lang | ~50 | Lexer, parser, lowerer |
| **Total** | **272+** | |

### 10.2 Code Size

| Crate | LOC | Chips/Gadgets |
|-------|-----|--------------|
| tabula-proof/src/air/ | ~2,000 | 2 chips (ColumnMeta, GlobalSortedMem), 3 gadget files |
| tabula-commitment/src/ | ~2,300 | 6 modules (field, codec, poseidon, smt, ssmc, hybrid) |
| tabula-executor/src/ | ~4,200 | 5 modules (interpreter, overlay, batch, consistency, resolve) |

### 10.3 Constraint Counts (Estimated)

| Chip | Columns | Constraints per Row | Rows (100-tx batch) |
|------|---------|--------------------|--------------------|
| Execution (interpreter) | ~30 | ~20 | ~2,000 (20 instr × 100 tx) |
| GlobalSortedMem (W=3) | 34 | ~25 | ~600 (400 access + 200 init) |
| GlobalSSMC | ~20 | ~15 | ~500 (5 cols × 100 rows) |
| GlobalMerge | ~20 | ~15 | ~500 (5 cols × 100 rows) |
| ColumnMeta | 27 | ~10 | ~8 (5 touched + padding) |
| RangeCheck | 2 | 0 (LogUp only) | 65,536 (preprocessed) |

---

## 11. Conclusion

Tabula's proof system design is **fundamentally sound**:

- **NF rules** (intra-tx) + **static (t,c) sharding** (inter-tx) + **hybrid VC** (state commitment) create a proof system that is orders of magnitude cheaper than equivalent zkVM execution for structured state workloads.

- The **execution trace dominates at 86%** of baseline cost. This is addressed by the template chip optimization path (Phase 3, 84% reduction), which is orthogonal to the memory layer.

- **M9 LogUp wiring** is the single most critical remaining milestone — it transforms individually-tested chips into a sound, integrated proof system.

- The most **urgent actionable item** is cross-layer integration testing (§9.1 #1), which catches pipeline mismatches before M9 forces debugging them under full STARK complexity.

The design is clean, the separation of concerns is strong, and the optimization roadmap is well-defined. The path from current state to a working end-to-end proof is: M7 (baseline chips) → M8 (VC chips + Poseidon) → M9 (LogUp wiring + STARK integration) → Phase 2-4 (optimizations).
