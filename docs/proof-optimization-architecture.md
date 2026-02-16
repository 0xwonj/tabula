# Proof Optimization Architecture

> Status: Design
> Date: 2026-02-14
> Depends on: research.md, air-chip-architecture.md, proof-spec.md §4-§8
> Scope: Phases 1-4 (excludes Phase 5 full circuit compiler)
> Supersedes: key-routing-design.md (merged into this document)

---

## 1. Problem & Cost Model

The baseline proof system (M7-M9) uses a generic interpreter model:

| Component | Rows | Width | % of total |
|-----------|------|-------|-----------|
| Execution trace | B × I | ~30 | **~86%** |
| GlobalSortedMem | U + A | ~11+w | ~11% |
| GlobalSSMC + GlobalMerge | Σ m_g | ~10-14 | ~2.5% |
| ColumnMeta | G | ~25 | ~0.5% |

(B=txs per batch, I=instructions per tx, U=unique keys, A=total accesses, G=column groups)

Two categories of waste:

1. **Execution layer** (~86%): Generic instruction trace — opcode dispatch, SSA carry, slot forwarding — despite the program being known at compile time.
2. **Memory layer** (~11%): All keys traverse GlobalSortedMem regardless of access pattern — read-only keys get full sorted-memory treatment when only a VC opening is needed.

These are **independent** problems with independent solutions.

### 1.1 Two Orthogonal Axes

```
                    Execution Layer
                    ┌─────────────────┬──────────────────┐
                    │  Interpreter    │  Template Chips   │
                    │  (generic)      │  (fused per-type) │
  ┌─────────────────┼─────────────────┼──────────────────┤
  │ Full            │                 │                  │
  │ GlobalSortedMem │  BASELINE       │  Phase 3 only    │
  │                 │  (M7-M9)        │                  │
  ├─────────────────┼─────────────────┼──────────────────┤
  │ §2 + §3         │                 │                  │
  │ KeyRoute        │  Phase 2 only   │  Phase 2 + 3     │
  │ optimized       │                 │  (best)          │
  └─────────────────┴─────────────────┴──────────────────┘
```

Each axis can be developed and tested independently. The optimizations compose without interference because template chips emit the same LogUp bus interactions as the generic execution chip — only more efficiently.

### 1.2 Target Reductions

For a program with 20 instructions, 5 accesses, 1000 txs, 500 unique keys (300 read-only):

| Optimization | Cells | Reduction |
|-------------|-------|-----------|
| Baseline | ~693,000 | — |
| + §2 read-only (memory axis) | ~655,000 | 5% |
| + §3 short-run (memory axis) | ~642,000 | 7% |
| + Template chips (execution axis) | ~108,000 | 84% |
| + Template + §2 + §3 | ~70,000 | 90% |
| + §5 literal carry (template internal) | ~58,000 | 92% |

The execution axis dominates. Memory-layer optimizations add incremental gains on top.

---

## 2. Axis 1: Memory-Layer Optimization

### 2.1 KeyRoute and `route_keys()`

A single function classifies every accessed `CellKey` into the cheapest valid proof path for the memory consistency layer.

```rust
// route.rs (renamed from classify.rs)

/// Access pattern for short-run specialization (§3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AccessPattern {
    /// Init → Read → Write (single-tx read-then-write).
    InitReadWrite,
    /// Init → Write (single-tx blind write).
    InitWrite,
}

/// Memory-layer proof path for a cell key.
///
/// Classification priority: ReadOnlyOpening > ShortRun > SortedMemory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyRoute {
    /// §2: Read-only key — proven via VC opening only.
    ReadOnlyOpening,
    /// §3: Single-tx access with fixed pattern — specialized chip.
    ShortRun(AccessPattern),
    /// Fallback: full GlobalSortedMem path.
    SortedMemory,
}

/// Classify all accessed keys into memory-layer proof paths.
pub fn route_keys(result: &ExecutionResult) -> BTreeMap<CellKey, KeyRoute> {
    let written: BTreeSet<CellKey> = result.write_set_final
        .iter().map(|(k, _)| *k).collect();

    // Build per-key access summary for ShortRun detection.
    let mut access_summary: BTreeMap<CellKey, Vec<(u32, bool)>> = BTreeMap::new();
    for event in &result.events {
        access_summary.entry(event.key)
            .or_default()
            .push((event.tx_index, event.op == OpKind::Write));
    }

    let mut routes = BTreeMap::new();

    for key in access_summary.keys().chain(
        result.write_set_final.iter().map(|(k, _)| k)
    ) {
        if routes.contains_key(key) { continue; }

        // Priority 1: ReadOnly — not in write_set_final
        if !written.contains(key) {
            routes.insert(*key, KeyRoute::ReadOnlyOpening);
            continue;
        }

        // Priority 2: ShortRun — single-tx, fixed pattern
        if let Some(accesses) = access_summary.get(key) {
            let tx_ids: BTreeSet<u32> = accesses.iter().map(|(tx, _)| *tx).collect();
            if tx_ids.len() == 1 {
                let has_read = accesses.iter().any(|(_, w)| !w);
                let has_write = accesses.iter().any(|(_, w)| *w);
                match (has_read, has_write) {
                    (true, true) => {
                        routes.insert(*key, KeyRoute::ShortRun(AccessPattern::InitReadWrite));
                        continue;
                    }
                    (false, true) => {
                        routes.insert(*key, KeyRoute::ShortRun(AccessPattern::InitWrite));
                        continue;
                    }
                    _ => {} // read-only caught above
                }
            }
        }

        // Priority 3: SortedMemory — fallback
        routes.insert(*key, KeyRoute::SortedMemory);
    }

    routes
}
```

Note: `LiteralWire` (§5) is intentionally **not** a KeyRoute variant. It is an internal mechanism of template chips (§3.4) that does not interact with the memory-layer routing.

### 2.2 Soundness

Classification is a prover hint. Soundness comes from LogUp completeness:

- Every execution access event must appear on exactly one bus: `Memory`, `ReadOnlyOpening`, or `ShortRunAccess`.
- Multiplicity is gated by witness bits on the execution trace (or template chip).
- **Misclassification attacks**:
  - Written key → ReadOnlyOpening: Write fingerprint includes the new value, which differs from base state → LogUp imbalance → proof failure. Edge case (write value = base state): `Com_new = Com_old`, harmless.
  - ShortRun key → SortedMemory: Valid but wasteful. No soundness issue.
  - SortedMemory key → ShortRun: Pattern mismatch (multi-tx access won't fit ShortRunChip's fixed-width row) → LogUp imbalance → proof failure.

### 2.3 Design Decisions

**D1. Per-batch dynamic classification** (not static program analysis).
- More precise: catches batch-specific opportunities (program CAN write but no tx in this batch does).
- No prover cost: soundness is enforced by LogUp, not by proving the classification.

**D2. Single ReadOnlyOpeningChip with tag selector** (one chip for both SSMC and SMT columns).
- Tag-gated LogUp multiplicity routes to appropriate VC bus.
- SSMC: `is_real * (1 - tag)` on SsmcMembership bus.
- SMT: `is_real * tag` on SmtOpening bus.

**D3. Soundness via witness bit + LogUp completeness** (no separate classification proof).
- Execution trace adds per-access `route_selector` column (2-bit: 0=SortedMemory, 1=ReadOnly, 2=ShortRun).
- Gated multiplicity per bus. Constraint: exactly one bus receives each access.

### 2.4 ReadOnlyOpeningChip (§2)

One row per unique read-only key. Proves value matches base-state commitment.

**Columns** (~14 for Standard width):

```rust
ReadOnlyOpeningCols<T> {
    is_real: T,
    table_id: T,
    col_id: T,
    row_key: [T; 3],       // 30+30+4 BabyBear limbs
    tag: T,                 // 0=SSMC, 1=SMT
    val: [T; MAX_W],        // Tier 1 ComEnc
    val_is_null: T,
}
```

**Constraints**: boolean, is_real prefix. VC opening delegated via LogUp.

**LogUp interactions**:

| Bus | Direction | Multiplicity | Fingerprint |
|-----|-----------|-------------|-------------|
| ReadOnlyOpening | Receive | `is_real` | `(t, c, r, val, val_is_null)` |
| SsmcMembership | Send | `is_real * (1 - tag)` | `(t, c, r, val)` |
| SmtOpening | Send | `is_real * tag` | `(t, c, r, val, merkle_root)` |

Code: `src/air/chips/read_only_opening/` (~200L).

### 2.5 ShortRunChip (§3)

One row per key with a fixed single-tx access pattern. Handles written keys that don't need the full sorted-memory machinery.

**Columns (InitReadWrite)** (~22 for Standard):

```rust
ShortRunIrwCols<T> {
    is_real: T,
    table_id: T,
    col_id: T,
    row_key: [T; 3],
    tag: T,

    init_val: [T; MAX_W],
    init_null: T,

    tau_read: T,           // single FE (bounded by batch size)
    tau_write: T,

    write_val: [T; MAX_W],
    write_null: T,
}
```

**Constraints**:
1. Boolean: `is_real`, `tag`, `init_null`, `write_null`
2. `is_real` prefix
3. `0 < tau_read < tau_write` (temporal ordering)
4. LogUp: receive read + write events from execution (ShortRunAccess bus)
5. LogUp: send to VC opening (SsmcMembership/SmtOpening, tag-gated) for `init_val`
6. LogUp: send `(t, c, r, write_val)` to **MergeCompleteness** bus for write-set contribution

**Write-set path**: ShortRunChip contributes its written values to GlobalMerge via the same MergeCompleteness bus that GlobalSortedMem uses. GlobalMerge receives from both chips — LogUp sums multiplicities from all senders. No new bus needed.

**InitWrite variant**: Same struct without `tau_read` and with constraint that init value is not read by execution.

Code: `src/air/chips/short_run/` (~300L).

---

## 3. Axis 2: Execution-Layer Optimization

### 3.1 ExecutionMode and ProgramInfo

Execution-layer optimization replaces the generic instruction trace with fused per-tx-type chips. This is orthogonal to memory-layer routing.

```rust
// template_dispatch.rs

/// How execution is proven for this batch.
pub enum ExecutionMode {
    /// Generic execution chip (baseline). Works for any program.
    Interpreter,
    /// Template chip replaces execution chip for matching tx types.
    /// Non-matching tx types fall back to interpreter.
    Template,
}
```

```rust
// program_info.rs

/// Static properties extracted from a program's IR.
/// Computed once per program, not per batch.
pub struct ProgramInfo {
    /// Per-tx-type template matching.
    /// Key: TxTypeId. Value: matched template, or None (use interpreter).
    pub tx_type_templates: BTreeMap<TxTypeId, Option<TemplateId>>,

    /// Cell addresses with all-literal (t, c, r). Used by §5 carry columns.
    pub literal_cells: BTreeSet<LiteralCell>,

    /// Max distinct keys accessed per tx (budget validation).
    pub max_keys_per_tx: usize,
}

/// Identifies a pre-built template chip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TemplateId {
    /// Read-compute-write on a single key.
    ReadComputeWrite,
    /// Two-party transfer (2 reads, compute, 2 writes).
    Transfer,
}
```

**Per-tx-type matching**: A `Program` has `BTreeMap<TxTypeId, TxTypeDef>`. Each `TxTypeDef` has its own instruction body. Template matching operates per tx type, not per program. A batch with mixed tx types can use TransferTemplate for type A and interpreter for type B.

### 3.2 How Template Chips Work

A template chip is a hand-written AIR that fuses one tx type's entire execution into a single row per tx. It:

1. **Replaces** the generic execution chip for that tx type's rows
2. **Emits the same LogUp interactions** as the execution chip would (Memory, ReadOnlyOpening, or ShortRunAccess buses)
3. **Adds fused compute constraints** inline (no opcode dispatch, no SSA carry)

Critical insight: **template chips don't change the memory-layer architecture.** They produce the same access-event fingerprints, just from fewer rows with less overhead. GlobalSortedMem, ReadOnlyOpeningChip, and ShortRunChip work identically regardless of whether events come from interpreter or template.

```
Interpreter path:
  ExecutionChip (I rows/tx × 30 cols) → Memory/ReadOnly/ShortRun buses → ...

Template path:
  TransferTemplateChip (1 row/tx × 28 cols) → same buses → ...
```

### 3.3 TransferTemplate

Pattern:

```
Read(s0, s1, t, c, Param(0))     // sender balance
Read(s2, s3, t, c, Param(1))     // receiver balance
Sub(s4, s0, Param(2))            // deduct amount
Add(s5, s2, Param(2))            // credit amount
Write(t, c, Param(0), s4, s1)   // write sender
Write(t, c, Param(1), s5, s3)   // write receiver
```

**Columns** (~28):

```rust
TransferTemplateCols<T> {
    is_real: T,

    // Tx parameters
    sender_key: [T; 3],
    recv_key: [T; 3],
    amount: [T; 3],

    // Sender
    sender_old_val: [T; 3],
    sender_old_null: T,
    sender_new_val: [T; 3],     // constrained: = sender_old - amount

    // Receiver
    recv_old_val: [T; 3],
    recv_old_null: T,
    recv_new_val: [T; 3],       // constrained: = recv_old + amount

    // Routing (per-key, per-access)
    sender_route: T,             // 0=SortedMem, 1=ReadOnly, 2=ShortRun
    recv_route: T,

    // Auxiliary
    underflow_check: T,
}
```

Width: ~28 cols. One row per tx.
vs interpreter: 6 instruction rows × ~30 cols = ~180 cols per tx.

**LogUp interactions**: For each of the 4 access events (2 reads, 2 writes), the template chip sends a fingerprint to the bus determined by `sender_route`/`recv_route`. Same gating mechanism as the generic execution chip's `route_selector`.

### 3.4 Literal-Key Carry Columns (§5)

For keys with all-literal `(t, c, r)`, the template chip embeds the value as **carry columns** across tx rows, eliminating memory-layer handling entirely.

**Mechanism**:
1. `ProgramInfo.literal_cells` identifies these keys at compile time
2. Template chip adds per-literal-key columns: `lit_val[w], lit_null`
3. Row 0: VC opening proves value (once)
4. Row i: `lit_val_i = lit_val_{i-1}` (carry constraint) unless written
5. Row N: If changed, VC update proves new commitment

**No LogUp needed** — column identity = structural proof. No Memory/ReadOnly/ShortRun bus interaction.

This optimization is **internal to template chips**. It does not appear in `KeyRoute` because:
- Without a template chip, there's no carry column mechanism
- `KeyRoute` answers "which memory chip handles this key?" — literal carry means "no memory chip needed"
- Keeping it template-internal avoids coupling the two axes

Code: added to template chip columns/air/trace, not a separate chip.

---

## 4. Composition: How the Axes Interact

### 4.1 Independence Property

Both axes emit/consume the same LogUp buses:

```
                   ┌─────────────────────────┐
                   │  Execution Layer         │
                   │  (Interpreter OR         │
                   │   TemplateChip)          │
                   └──┬───────┬────────┬──────┘
                      │       │        │
               route=0│ route=1│  route=2│
                      │       │        │
              ┌───────▼──┐ ┌──▼──────┐ ┌▼──────────┐
              │Memory bus│ │ReadOnly │ │ShortRun   │
              │          │ │Opening  │ │Access bus  │
              │          │ │bus      │ │           │
              └─────┬────┘ └────┬────┘ └─────┬─────┘
                    │           │             │
              ┌─────▼────┐ ┌───▼──────────┐ ┌▼─────────┐
              │GlobalSort│ │ReadOnlyOpen  │ │ShortRun  │
              │edMem     │ │ingChip       │ │Chip      │
              └──┬────┬──┘ └──┬───────┬───┘ └┬────┬────┘
                 │    │       │       │      │    │
      ┌──────────▼─┐ │  ┌────▼───┐   │   ┌──▼────▼──────┐
      │SsmcMember  │ │  │SsmcMem │   │   │SsmcMembership│
      │ship bus    │ │  │bership │   │   │+ MergeCompl  │
      └─────┬──────┘ │  └───┬────┘   │   └──────┬───────┘
            │        │      │        │          │
      ┌─────▼────────▼──────▼────────▼──────────▼──┐
      │                VC Layer                     │
      │  GlobalSSMC, MerkleVerifier, GlobalMerge    │
      └──────────────────┬─────────────────────────┘
                         │
                   ┌─────▼──────┐
                   │ColumnMeta  │  ← state root transition
                   └────────────┘
```

The execution layer is a **pluggable source** of access-event fingerprints. Swapping interpreter for template chip changes the source, not the memory-layer topology.

### 4.2 InteractionKind Enum (Complete)

```rust
pub enum InteractionKind {
    // Memory layer
    Memory,              // Execution ↔ GlobalSortedMem
    ReadOnlyOpening,     // Execution ↔ ReadOnlyOpeningChip
    ShortRunAccess,      // Execution ↔ ShortRunChip

    // VC layer
    SsmcMembership,      // SortedMem/ReadOnly/ShortRun ↔ GlobalSSMC
    SmtOpening,          // SortedMem/ReadOnly/ShortRun ↔ MerkleVerifier
    MergeCompleteness,   // GlobalMerge ↔ {GlobalSortedMem, ShortRunChip}

    // Global
    ColumnMetaJoin,      // Any chip ↔ ColumnMeta
    RangeCheck,          // Any chip ↔ RangeCheck table
}
```

Bus count is bounded (8 total). Adding new memory-layer chips or template chips does not add buses — they connect to existing ones.

### 4.3 Chip Instantiation

The prover assembles the chip set per batch:

```rust
fn assemble_chips(
    program_info: &ProgramInfo,
    key_routes: &BTreeMap<CellKey, KeyRoute>,
    batch_tx_types: &[TxTypeId],
) -> Vec<TabulaAir> {
    let mut chips = Vec::new();

    // Execution layer: per tx type
    let unique_types: BTreeSet<_> = batch_tx_types.iter().collect();
    let mut needs_interpreter = false;
    for &tx_type in &unique_types {
        match program_info.tx_type_templates.get(tx_type) {
            Some(Some(TemplateId::Transfer)) =>
                chips.push(TabulaAir::Transfer(TransferTemplateChip)),
            Some(Some(TemplateId::ReadComputeWrite)) =>
                chips.push(TabulaAir::ReadComputeWrite(RcwTemplateChip)),
            _ => needs_interpreter = true,
        }
    }
    if needs_interpreter {
        chips.push(TabulaAir::Execution(ExecutionChip));
    }

    // Memory layer: only chips that have keys routed to them
    let has = |r: KeyRoute| key_routes.values().any(|&v| v == r);
    if has(KeyRoute::SortedMemory) {
        chips.push(TabulaAir::SortedMem(SortedMemChip));
    }
    if has(KeyRoute::ReadOnlyOpening) {
        chips.push(TabulaAir::ReadOnlyOpening(ReadOnlyOpeningChip));
    }
    if key_routes.values().any(|r| matches!(r, KeyRoute::ShortRun(_))) {
        chips.push(TabulaAir::ShortRun(ShortRunChip));
    }

    // VC + global: always present
    chips.push(TabulaAir::Ssmc(SsmcChip));
    chips.push(TabulaAir::Merge(MergeChip));
    chips.push(TabulaAir::ColumnMeta(ColumnMetaChip));
    chips.push(TabulaAir::RangeCheck(RangeCheckChip));
    // SmtPath only if SMT columns touched
    chips
}
```

Key insight: execution-layer and memory-layer chips are selected independently. A batch can use `TransferTemplate` + `ReadOnlyOpeningChip` + `SortedMem` simultaneously.

---

## 5. End-to-End Walkthrough

**Program**: Fee-transfer — reads fee config, transfers between sender/receiver.

```
Read(s0, s1, t=0, c=1, r=Lit(0))    // read fee rate (literal key)
Read(s2, s3, t=0, c=0, r=Param(0))  // read sender balance
Read(s4, s5, t=0, c=0, r=Param(1))  // read receiver balance
Mul(s6, Param(2), s0)               // fee = amount × rate
Sub(s7, s2, Param(2))               // deduct amount from sender
Sub(s8, s7, s6)                     // deduct fee from sender
Add(s9, s4, Param(2))               // credit amount to receiver
Write(t=0, c=0, r=Param(0), s8, s3) // write sender
Write(t=0, c=0, r=Param(1), s9, s5) // write receiver
```

**Batch**: 1000 txs. 800 unique sender/receiver keys, 1 fee config key.

### Step 1: Program Analysis (once)

```
ProgramInfo {
    tx_type_templates: { TxType(0) → None },  // doesn't match Transfer exactly (has fee)
    literal_cells: { (t=0, c=1, r=0) },
    max_keys_per_tx: 3,
}
```

No template match (7 instructions, 3 keys — not a standard Transfer pattern). Uses interpreter.

### Step 2: Execution (per batch)

Executor runs 1000 txs, produces `ExecutionResult`:
- `events`: 3000 read events + 2000 write events = 5000 events
- `write_set_final`: 800 entries (sender/receiver keys, coalesced across txs)
- `read_set_old`: 801 entries (800 sender/receiver + 1 fee config)

### Step 3: Key Routing

```
route_keys(result) →
  (0,1,0)             → ReadOnlyOpening   // fee config: read by all, never written
  sender-only keys    → ShortRun(IRW)      // touched by 1 tx: init-read-write
  multi-tx keys       → SortedMemory       // e.g. Alice sends in tx 3, receives in tx 7
  receiver-only keys  → ShortRun(IRW)      // touched by 1 tx: init-read-write
```

Suppose: 1 ReadOnly, 600 ShortRun, 200 SortedMemory.

### Step 4: Witness Generation

`WitnessGenerator.generate()` builds `BatchWitness`:
- `key_routes`: the map from Step 3
- `columns[]`: per-(t,c) witness data
  - init_rows: only for SortedMemory keys (200 init rows, not 801)
  - access_rows: all 5000 events (each tagged with route for bus gating)
- `column_metas`: all columns including untouched

### Step 5: Trace Generation

Each chip builds its trace from `BatchWitness`:

| Chip | Rows | Source |
|------|------|--------|
| ExecutionChip | 7 × 1000 = 7000 | instructions × txs |
| GlobalSortedMem | 200 init + ~1600 access = ~1800 | SortedMemory keys only |
| ReadOnlyOpeningChip | 1 | fee config key |
| ShortRunChip | 600 | ShortRun keys (1 row each) |
| GlobalSSMC | ~801 | all base-state entries |
| GlobalMerge | ~801 + 800 writes | merge trace |
| ColumnMeta | 2 | (t=0,c=0) and (t=0,c=1) |

### Step 6: Cost Comparison

| | Baseline (no routing) | With §2+§3 routing |
|-|----------------------|-------------------|
| GlobalSortedMem rows | 801 init + 5000 access = 5801 | 200 init + 1600 access = 1800 |
| Savings | — | **69% reduction** in sorted-memory trace |

If the program matched a template (hypothetical), the 7000 execution rows would become 1000 template rows — an additional **86% reduction** in execution trace.

---

## 6. Code Architecture

### 6.1 Current State (post-M6)

```
tabula-proof/src/
├── lib.rs
├── statement.rs
├── classify.rs          ← v1 (AccessClass, classify_keys)
├── trace.rs             ← BatchWitness with key_classification
├── witness.rs           ← WitnessGenerator
└── air/
    ├── mod.rs
    ├── bus.rs           ← InteractionKind (6 variants)
    ├── columns.rs
    ├── debug.rs
    ├── gadgets/
    │   ├── mod.rs
    │   └── boolean.rs
    └── chips/
        ├── mod.rs       ← TabulaAir, ChipMeta
        └── column_meta/
```

### 6.2 Target Structure

```
tabula-proof/src/
├── lib.rs
├── statement.rs
├── route.rs                # KeyRoute + route_keys()
├── program_info.rs         # ProgramInfo + analyze()
├── trace.rs                # BatchWitness (key_routes field)
├── witness.rs              # WitnessGenerator (route-aware)
│
└── air/
    ├── mod.rs
    ├── bus.rs              # InteractionKind (8 variants)
    ├── columns.rs
    ├── debug.rs
    │
    ├── gadgets/
    │   ├── mod.rs
    │   ├── boolean.rs
    │   ├── integer.rs
    │   └── lex_order.rs
    │
    └── chips/
        ├── mod.rs              # TabulaAir enum, ChipMeta
        │
        │  # Baseline (M7-M8)
        ├── column_meta/
        ├── sorted_mem/
        ├── ssmc/
        ├── merge/
        ├── execution/
        ├── smt_path/
        ├── range_check.rs
        │
        │  # Memory-layer optimization (Phase 2)
        ├── read_only_opening/
        ├── short_run/
        │
        │  # Execution-layer optimization (Phase 3)
        └── templates/
            ├── mod.rs
            ├── transfer/
            └── read_compute_write/
```

### 6.3 Interface Contracts

**route_keys → WitnessGenerator**:

```rust
// witness.rs
impl WitnessGenerator<H> {
    pub fn generate(
        &self,
        result: &ExecutionResult,
        schemas: &BTreeMap<TableId, TableSchema>,
        old_column_states: &BTreeMap<(TableId, ColId), ColumnState<H>>,
    ) -> Result<BatchWitness<H>, TabulaError>
    // Calls route_keys() internally. Stores routes in BatchWitness.key_routes.
}
```

**BatchWitness → trace generators**:

Each `generate_*_trace()` function reads `witness.key_routes` to select its rows:

```rust
// sorted_mem/trace.rs — only SortedMemory keys
fn generate_sorted_mem_trace(witness: &BatchWitness<H>) -> RowMajorMatrix<BabyBear> {
    for col_w in &witness.columns {
        for init in &col_w.init_rows {
            if witness.key_routes[&init.key] == KeyRoute::SortedMemory { /* add row */ }
        }
    }
}

// read_only_opening/trace.rs — only ReadOnlyOpening keys
fn generate_read_only_opening_trace(witness: &BatchWitness<H>) -> RowMajorMatrix<BabyBear> {
    for (key, &route) in &witness.key_routes {
        if route == KeyRoute::ReadOnlyOpening { /* add row from read_set_old */ }
    }
}
```

**ProgramInfo → chip assembly** (Phase 3+):

```rust
// Prover decides execution mode per tx type
for tx_type in batch.unique_tx_types() {
    match program_info.tx_type_templates[&tx_type] {
        Some(TemplateId::Transfer) => use TransferTemplateChip for those txs,
        None => use ExecutionChip for those txs,
    }
}
```

---

## 7. Implementation Phases

### Phase 1: Rename + Scaffold (now, before M7)

Establish naming conventions. No functional change.

| Action | Detail |
|--------|--------|
| `classify.rs` → `route.rs` | Rename file, `AccessClass` → `KeyRoute`, `classify_keys` → `route_keys` |
| `trace.rs` | `key_classification` → `key_routes` |
| `witness.rs`, `lib.rs` | Update imports and re-exports |
| `program_info.rs` (new) | Types only: `ProgramInfo`, `LiteralCell`, `TemplateId`. No `analyze()` yet |

~50 lines new, ~30 lines changed. All existing tests pass with renamed types.

### Phase 2: §2 + §3 Chips (after M7-M8)

**Prerequisite**: GlobalSortedMem and ExecutionChip exist (nothing to bypass otherwise).

| Action | Lines |
|--------|-------|
| `read_only_opening/` (columns, air, trace, mod) | ~200 |
| `short_run/` (columns, air, trace, mod) | ~300 |
| `chips/mod.rs` — +2 TabulaAir variants, ChipMeta impls | ~30 |
| `bus.rs` — +ShortRunAccess | ~3 |
| `route.rs` — ShortRun classification logic | ~40 |
| `witness.rs` — route-aware init/access row building | ~30 |
| Execution chip — `route_selector` column + gated multiplicity | ~20 |

~620 new lines.

### Phase 3: Template Chips (after Phase 2)

| Action | Lines |
|--------|-------|
| `program_info.rs` — `analyze()` implementation | ~100 |
| `templates/mod.rs` — TemplateChip enum | ~30 |
| `templates/transfer/` (columns, air, trace, mod) | ~250 |
| `templates/read_compute_write/` | ~200 |
| `chips/mod.rs` — +2 TabulaAir variants | ~20 |
| `witness.rs` — template-aware execution mode | ~40 |

~640 new lines.

### Phase 4: §5 Literal-Key Carry (after Phase 3)

| Action | Lines |
|--------|-------|
| `program_info.rs` — `literal_cells` extraction | ~30 |
| `templates/transfer/columns.rs` — carry columns | ~20 |
| `templates/transfer/air.rs` — carry constraints | ~30 |
| `templates/transfer/trace.rs` — carry propagation | ~25 |
| `templates/read_compute_write/` — same | ~60 |

~165 new lines.

### Dependency Graph

```
Phase 1 (rename)    ←── no deps, do now
    │
    ├──── M7 (SortedMem, Execution) ──── M8 (SSMC, Merge, SMT)
    │                                         │
    └─────────────────────────────────────────┤
                                              │
                                        Phase 2 (§2 + §3)
                                              │
                                        Phase 3 (templates)
                                              │
                                        Phase 4 (§5 carry)
```

Phase 1 and M7-M8 are independent parallel tracks. Phase 2+ waits for baseline chips.

---

## 8. Open Items

1. **Width-class variants for ReadOnlyOpeningChip/ShortRunChip**: Single MAX_W=8 chip with gating, or per-width-class chips? Decide at implementation time based on measured waste.
2. **SmtOpening bus**: Not yet defined in `InteractionKind`. Add when MerkleVerifier chip is designed (M8).
3. **Template matching strictness**: Current design requires exact structural match. Partial matching (e.g., Transfer + extra reads → Transfer template + interpreter for extras) is a Phase 5 topic.
4. **Multi-tx-type batches**: When a batch has tx type A (template) and type B (interpreter), both execution chips run on their respective tx subsets. Need to define how tx indices partition into chip rows.
5. **ShortRun timestamp encoding**: Single FE for `tau` assumes batch size < p (~2 billion). Safe for all practical batches but should be documented as a constraint.
