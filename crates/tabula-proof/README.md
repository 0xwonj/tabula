# tabula-proof

STARK proof generation and verification for the Tabula kernel.

Given an `ExecutionResult` and column state snapshots, generates structured
witness data and verifies AIR constraints. Uses Plonky3 over BabyBear.

## Current Status: M7 (GlobalSortedMem)

- **M5** (done): Witness generation — `ExecutionResult` to `BatchWitness`
- **M6** (done): AIR constraint infrastructure — columns, gadgets, debug checker, ColumnMetaChip
- **M7** (done): GlobalSortedMem chip — memory consistency AIR + trace generation

## Architecture

```
ExecutionResult ──→ WitnessGenerator ──→ BatchWitness ──→ per-chip traces
   (executor)      (witness/generator)   (witness/types)   (chips/**/trace.rs)
```

### Module Layout

```
src/
├── lib.rs                       Crate root, feature gates
├── statement.rs                 Public inputs (ApplyBatchStatement)
│
├── witness/                     Witness generation (behind `stark`)
│   ├── mod.rs                   Module re-exports
│   ├── types.rs                 InitRow, AccessRow, BatchWitness, ColumnWitness
│   ├── generator.rs             WitnessGenerator: ExecutionResult → BatchWitness
│   ├── route.rs                 Key routing (route_keys, KeyRoute, AccessPattern)
│   └── program_info.rs          ProgramInfo, TemplateId, LiteralCell
│
└── air/                         AIR constraint system (behind `stark`)
    ├── columns.rs               Zero-copy borrow utilities (#[repr(C)] pattern)
    ├── bus.rs                   LogUp interaction types (InteractionKind)
    ├── debug.rs                 Debug constraint checker (debug_check, debug_check_all)
    │
    ├── gadgets/                 Reusable constraint building blocks
    │   ├── mod.rs               Re-exports, bool_fe() helper
    │   ├── boolean.rs           is_real prefix constraint
    │   ├── integer.rs           U64Limbs, IsZero, StrictIneq gadgets
    │   └── mem.rs               Null canonicality, mem read/write constraints
    │
    └── chips/                   Per-chip AIR implementations
        ├── mod.rs               TabulaAir enum, ChipMeta trait, dispatch
        ├── column_meta/         ColumnMeta chip (columns/air/trace)
        ├── sorted_mem/          GlobalSortedMem chip (columns/air/trace)
        └── range_check.rs       RangeCheck preprocessed table
```

### Chip Organization

Each chip follows a consistent 3-file pattern:

| File | Concern | Changes when... |
|------|---------|-----------------|
| `columns.rs` | `#[repr(C)]` column struct + width | Column layout changes |
| `air.rs` | `BaseAir` + `Air` impl (constraints) | Constraint logic changes |
| `trace.rs` | Witness → `RowMajorMatrix<BabyBear>` | Witness format changes |

Gadgets in `gadgets/` bundle column structs + `populate()` + `constrain_*()` for
reuse across chips (SP1 Operations pattern).

## Public API

```rust
// Always available
pub struct ApplyBatchStatement { ... }

// Behind `stark` feature
pub struct WitnessGenerator<H> { ... }
pub struct BatchWitness<H> { ... }
pub struct InitRow { ... }
pub struct AccessRow { ... }
pub struct ColumnWitness<H> { ... }
pub struct ProgramInfo { ... }
pub fn debug_check(air, trace) -> Result<(), ConstraintError>;
pub fn debug_check_all(air, trace) -> Vec<ConstraintError>;
```

## Feature Flags

| Feature | Dependencies | Purpose |
|---------|-------------|---------|
| `stark` | p3-field, p3-baby-bear, p3-air, p3-matrix, tabula-commitment | AIR constraints, witness gen, trace gen |

Without `stark`: only `ApplyBatchStatement` is available.

## Dependencies

- `tabula-core`: types, events, errors, tx definitions
- `tabula-commitment` (optional, with `stark`): field hasher, column state, HybridVC, ColumnMeta
- `p3-*` (optional, with `stark`): Plonky3 AIR traits and BabyBear field

## Upcoming

| Milestone | Chips | Spec ref |
|-----------|-------|----------|
| M8 | GlobalSSMC, GlobalMerge | §4.2, §8.5-8.8 |
| M9 | LogUp wiring, prover integration | §8.4, §4.5 |
