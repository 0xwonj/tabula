# Tabula-Native Optimizations

> Proving patterns and optimizations uniquely enabled by Tabula's architectural properties.
> These are impossible in general-purpose zkVMs and do not exist in any current framework.

## Why Tabula Is Different

| Property | zkVM | Tabula | Proof Impact |
|----------|------|--------|-------------|
| Memory access pattern | Arbitrary | NF-1~4 constrained (SSA) | No intra-tx RAM consistency |
| Address structure | Flat address space | `(t,c)` static + `r` dynamic | Column-parallel proving |
| Type information | None (everything is word) | Schema-typed (Bool/U64/I64/Digest) | Width-specialized chips |
| Resource bounds | Gas (runtime) | ProgramBudgets (compile-time) | Pre-determined trace size |
| State structure | Flat key-value | 2-layer (table → column → row) | Hierarchical proof composition |
| Compiler control | None (arbitrary binaries) | Full (tabula-lang) | Compilation-proving co-design |

---

## Optimization 1: NF-Aware Constraint Elision

### Principle

NF rules (NF-1 through NF-4) are enforced by the compiler in `Program::register()`,
not by runtime checks. Constraints that re-enforce these properties at AIR level
are structurally redundant and can be elided.

### Current Redundancy

```
NF-1 (Unique-Read):
  Compiler guarantees each (t,c,r) is read at most once per tx.
  → SortedMem's per-key read deduplication is structurally guaranteed.
  → Init-row uniqueness constraint is partially redundant.

NF-2 (Unique-Write):
  Compiler guarantees each (t,c,r) is written at most once per tx.
  → Intra-tx write coalescing logic is never triggered.

NF-3 (No-Read-After-Write):
  SSA guarantee: no instruction reads a cell after writing it.
  → Slot carry logic need not handle "read-then-overwrite" case.

NF-4 (Key-Alias Resolvability):
  Compiler inserts Cmp(Ne)+Assert guards for ambiguous pairs.
  → Runtime alias detection is not needed; guards are in the IR.
```

### Concrete Optimization

**Program-specific preprocessed selectors** replace generic slot_written flags:

```
Current:  16 × slot_written flags (boolean columns) in ExecutionChip
          + transition constraints checking "carry unless written"

Proposed: 1 preprocessed selector column per program
          encoding which (instruction, slot) pairs allow writes
          → ~15 column reduction in ExecutionChip
```

**Trust model**: The compiler is part of the trusted setup. If `Program::register()`
accepts a program, NF properties are guaranteed. This is no different from trusting
that the circuit description is correct.

### Estimated Savings

- ExecutionChip: 278 → ~263 columns (-5.4%)
- Constraint count reduction: ~32 transition constraints eliminated

---

## Optimization 2: Static-Coordinate Proof Sharding

### Principle

`(table, col)` are compile-time constants in every IR instruction. Only `row` is
dynamic. This means all memory operations can be partitioned by `(t,c)` group
with zero runtime dispatch.

### Architecture

```
Current (monolithic):
  ┌─────────────────────────────────────────┐
  │ StateColumnChip (all columns together)  │
  │ GlobalSortedMem (all columns together)  │
  │ Single proof encompassing all (t,c)     │
  └─────────────────────────────────────────┘

Proposed (sharded):
  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
  │ Shard (t=0, c=1) │  │ Shard (t=0, c=2) │  │ Shard (t=1, c=0) │
  │ SortedMem rows   │  │ SortedMem rows   │  │ SortedMem rows   │
  │ SSMC chain       │  │ SSMC chain       │  │ SSMC chain       │
  │ Merge trace      │  │ Merge trace      │  │ Merge trace      │
  │ SMT col-path     │  │ SMT col-path     │  │ SMT col-path     │
  └────────┬─────────┘  └────────┬─────────┘  └────────┬─────────┘
           │                     │                     │
           └──────────┬──────────┘─────────────────────┘
                      │
  ┌───────────────────┴────────────────────┐
  │ Global Composition                     │
  │ ExecutionChip (shared, all txs)        │
  │ SMT table root binding                 │
  │ Cross-shard LogUp balance              │
  └────────────────────────────────────────┘
```

### Why This Is Impossible in zkVMs

zkVMs have a flat address space where any instruction can access any memory
address. Global sorting across all addresses is mandatory. Tabula's static
`(t,c)` means each column is an independent namespace with no cross-column
memory ordering requirement.

### Benefits

- **Embarrassingly parallel**: Each shard is independent (no data dependency)
- **Heterogeneous sizing**: Large columns (SMT) and small columns (SSMC) coexist
- **Fault isolation**: Re-prove a single column without touching others
- **Distributed proving**: Shards can run on different machines
- **Incremental verification**: Verify shards independently, compose at the end

### LogUp Soundness

Cross-shard soundness is maintained by LogUp bus balance:
- Each shard sends/receives on the Memory bus for its `(t,c)` group
- ExecutionChip sends on the Memory bus for all `(t,c)` groups
- Global bus balance Σ = 0 ensures no shard can forge or omit memory operations

---

## Optimization 3: Witness-Driven Late-Binding Proof Strategy

### Principle

The proof strategy for each key is selected AFTER execution, based on the
actual access pattern observed in the batch. Soundness is guaranteed by
LogUp bus balance, not by strategy selection correctness.

### How It Works

```
Traditional:  Program structure → Fixed proof strategy → Execute → Prove
Tabula:       Execute → Analyze witness → Select cheapest strategy → Prove
              (soundness: LogUp bus balance, not strategy selection)
```

### KeyRoute Classification

```rust
pub enum KeyRoute {
    ReadOnly,                    // VC opening only (cheapest)
    ShortRun(AccessPattern),     // Lightweight chip (medium)
    SortedMemory,                // Full GlobalSortedMem (most expensive)
}
```

The same program and key may use different routes in different batches:

| Batch Content | Route for Key K | Cost |
|---------------|----------------|------|
| K read once, never written | ReadOnly | ~0 (amortized in batch opening) |
| K read then written (single tx) | ShortRun(InitReadWrite) | ~10 rows |
| K accessed across multiple txs | SortedMemory | ~N rows (N = access count) |

### Misclassification Attack Defense

If the prover dishonestly classifies a key:

| Attack | What Happens |
|--------|-------------|
| Written key → ReadOnly | New value ≠ base state → Memory bus send has no matching receive → bus imbalance |
| ShortRun key → SortedMemory | Valid but wasteful — no soundness risk |
| SortedMemory key → ShortRun | Access pattern mismatch → ShortRun chip constraint violation |

LogUp provides a **global consistency check** that is agnostic to which chip
produced the send/receive. The verifier does not need to know the routing.

### Uniqueness

This "open-world" bus architecture (multiple chips can contribute to the same bus)
combined with witness-driven strategy selection does not exist in any current
STARK framework. zkVMs have fixed chip-to-bus assignments.

---

## Optimization 4: Schema-Driven Chip Specialization

### Principle

Column types are known at compile time from `TableSchema`. Value encoding
width `w(T)` varies by type. Chips can be specialized per width class.

### Width Classes

| Type | w(T) | Encoding |
|------|------|----------|
| Bool | 1 FE | Single boolean bit |
| U64, I64 | 3 FE | 30+30+4 BabyBear limb split |
| Bytes32, Digest | 8 FE | Native Poseidon2 squeeze FE |

### Current vs Proposed

```
Current:
  StateColumnChip<W=3> for ALL columns (U64 width assumed)

Proposed:
  StateColumnChip<1> for Bool columns    — 66% fewer value columns
  StateColumnChip<3> for U64/I64 columns — current width
  StateColumnChip<8> for Digest columns  — native Poseidon width
```

### Table-Level Specialization (Future)

If a table's schema is fully known:

```rust
// accounts table: {balance: U64, active: Bool, pubkey: Digest}
// Compiler generates a table-specific chip bundle:
struct AccountsStateBundle {
    balance: StateColumnShard<3>,   // U64
    active:  StateColumnShard<1>,   // Bool
    pubkey:  StateColumnShard<8>,   // Digest
}
```

### Interaction with Proof Sharding

Width specialization composes naturally with static-coordinate sharding:
each `(t,c)` shard uses the width class matching its column type.

---

## Optimization 5: Dual-Axis Product Composition

### Principle

Execution optimization (interpreter vs template chips) and memory optimization
(ReadOnly vs ShortRun vs SortedMem) are **orthogonal axes** connected only
by LogUp bus contracts.

### Architecture

```
Execution Axis                    Memory Axis
┌───────────────────────┐        ┌───────────────────────┐
│ Generic Interpreter   │        │ ReadOnlyOpening       │
│ TransferTemplate      │        │ ShortRunChip          │
│ ReadComputeWrite      │  ←──→  │ GlobalSortedMem       │
│ (future: compiled)    │  Bus   │ (future: direct VC)   │
└───────────────────────┘        └───────────────────────┘
       ↓                                  ↓
   M strategies × E strategies = all valid combinations
```

### Bus as Interface Contract

```
Memory bus: (t, c, r[3], τ[3], is_write, val[W], val_is_null)
  — Any execution chip sends here
  — Any memory chip receives here

Merge bus: (t, c, old_entry, write_entry)
  — Any memory chip sends here
  — SSMC/Commitment chip receives here
```

### Incremental Optimization

New optimizations on one axis automatically compose with all strategies on
the other axis:

- Adding a new template chip: automatically works with ReadOnly, ShortRun,
  and SortedMemory
- Adding a new memory strategy: automatically works with interpreter and
  all template chips

This product-space property dramatically reduces the testing and verification
surface: each axis can be validated independently.

---

## Optimization 6: Batch-Amortized Fixed-Cost Structure

### Principle

Batch proving naturally separates into fixed costs (per-batch) and marginal
costs (per-transaction), with fixed costs dominating at small batch sizes.

### Cost Decomposition

```
Fixed cost (once per batch):
  ├── SMT root transition proof (oldRoot → newRoot)
  ├── ColumnMeta trace (one row per touched column)
  ├── SSMC commitment chains (per touched column)
  ├── RangeCheck table (shared)
  └── Poseidon chip (shared hash operations)

Variable cost (per transaction):
  ├── Execution trace rows (proportional to IR instruction count)
  ├── Memory access records (per Read/Write)
  └── Poseidon calls (per Hash instruction)

Result:
  cost(batch) = Fixed + N × Marginal(tx)
  Per-tx cost → Marginal(tx) as N → ∞
```

### Optimization Levers

**Minimize fixed cost:**
- Skip untouched columns entirely (`is_touched=0` → 1 ColumnMeta row, no SSMC)
- Empty columns: `Com_empty = Poseidon(0x00||t||c)` with no trace rows
- SMT path length proportional to touched columns (not total columns)

**Minimize marginal cost:**
- Template chips: fixed-pattern txs use pre-optimized AIR (vs generic interpreter)
- ShortRun: single-tx access patterns bypass GlobalSortedMem
- ReadOnly: read-only keys cost ~0 marginal (amortized in batch opening)

---

## Optimization 7: Compilation-Proving Co-Design

### Principle

Tabula controls both the compiler (tabula-lang) and the proof system. The
compiler can generate IR optimized for proving cost, not just execution cost.

### Co-Design Opportunities

```
tabula-lang compiler                  tabula-proof system
  ├── IR generation ──────────────→  Chip structure optimization
  ├── NF enforcement ─────────────→  Constraint elision
  ├── Slot allocation ────────────→  Trace width minimization
  ├── Access ordering ────────────→  Memory strategy selection
  ├── Template recognition ───────→  Template chip routing
  └── Budget computation ─────────→  Exact trace sizing
```

### Concrete Examples

**1. Proving-cost-aware slot allocation**

```
// Two equivalent compilations, different proving costs:

// Option A: slots 0,1,2,3 used → 4 carry columns needed
Read(dst=0, t, c, r1)
Read(dst=1, t, c, r2)
Add(dst=2, 0, 1)
Write(t, c, r3, src=2)

// Option B: reuse slots → 2 carry columns needed
Read(dst=0, t, c, r1)
Read(dst=1, t, c, r2)
Add(dst=0, 0, 1)       // Overwrite slot 0 (SSA still holds if Read dst=0 not used after)
Write(t, c, r3, src=0)
```

**2. Template-aware lowering**

Compiler recognizes "transfer" pattern (Read A, Read B, compute, Write A, Write B)
and emits IR in the exact form expected by TransferTemplate chip.

**3. Access-order optimization**

Compiler reorders Read/Write instructions to maximize ShortRun classification:

```
// Before: interleaved access → SortedMemory required
Read(t=0, c=1, r=K1)
Read(t=0, c=2, r=K2)
Write(t=0, c=1, r=K1)

// After: grouped by key → ShortRun(InitReadWrite) possible for K1
Read(t=0, c=1, r=K1)
Write(t=0, c=1, r=K1)
Read(t=0, c=2, r=K2)
```

### Trust Model

The compiler is part of the trusted setup. Compiler optimizations don't affect
soundness — they only reduce proving cost. A malicious compiler could produce
invalid programs, but `Program::register()` validates NF invariants regardless
of compiler behavior.

---

## Optimization 8: Incremental State Transition Proofs

### Principle

Column-oriented state structure enables proving only the delta between batches.
Untouched columns carry forward their commitments without re-proving.

### Architecture

```
Batch N:   columns {A, B, C, D}
           touched: {A, B}
           → Prove: A (SSMC + merge + SMT path), B (SSMC + merge + SMT path)
           → Carry: C commitment, D commitment (from previous batch)
           → SMT root: update paths for A, B only

Batch N+1: columns {A, B, C, D}
           touched: {B, C}
           → Prove: B (SSMC + merge + SMT path), C (SSMC + merge + SMT path)
           → Carry: A commitment (from batch N), D commitment (unchanged)
           → SMT root: update paths for B, C only
```

### Why This Is Impossible in zkVMs

zkVMs commit to the entire memory state as a single Merkle root. Partial
updates require re-computing the full tree. Tabula's two-layer SMT
(table-level → column-level) naturally supports per-column delta proofs.

### Requirements

- Persistent storage for per-column commitments between batches
- SMT delta proof: only update Merkle paths for touched columns
- ColumnMeta chip already tracks `is_touched` per column — this extends naturally

---

## Priority and Dependencies

| # | Optimization | Effort | Impact | Prerequisites |
|---|-------------|--------|--------|---------------|
| 2 | Static-coord sharding | High | Very High | Machine layer (shared PCS) |
| 3 | Late-binding proof strategy | Medium | High | KeyRoute (already designed) |
| 5 | Dual-axis composition | Low | High | Bus architecture (already exists) |
| 6 | Batch amortization | Low | Medium | Already partially implemented |
| 4 | Schema-driven specialization | Medium | Medium | Width-class chips |
| 1 | NF-aware elision | Medium | Medium | Program analysis infrastructure |
| 7 | Compilation-proving co-design | High | Very High | Compiler maturity |
| 8 | Incremental state proofs | High | High | Persistent commitment store |

### Recommended Implementation Order

1. **Machine layer** (prerequisite for all) — see [machine-layer-architecture.md](machine-layer-architecture.md)
2. **Late-binding (#3)** + **dual-axis (#5)** — low-hanging fruit, already designed
3. **Batch amortization (#6)** — already partially in place
4. **Schema specialization (#4)** — width-class chips
5. **Static-coord sharding (#2)** — highest impact, requires shared PCS
6. **NF elision (#1)** + **co-design (#7)** — requires compiler + program analysis
7. **Incremental proofs (#8)** — requires persistent state infrastructure
