# Machine Layer Architecture

> Architectural decision for Tabula's multi-chip STARK orchestration layer.

## The Layer Stack

```
L0: Field arithmetic              → p3-field, p3-baby-bear     (commodity, external)
L1: PCS / FRI                     → p3-fri, p3-commit          (commodity, external)
L2: Single-chip STARK             → p3-uni-stark               (commodity, external)
────────────────────────────────────────────────────────────────────────────────────
L3: Multi-chip orchestration      → THIS DOCUMENT              (critical path, own)
L4: LogUp / permutation           → THIS DOCUMENT              (critical path, own)
────────────────────────────────────────────────────────────────────────────────────
L5: AIR framework / gadgets       → tabula-proof/air, gadgets  (differentiator, own)
L6: Chips                         → tabula-proof/chips         (differentiator, own)
L7: Trace / Witness               → tabula-proof/trace,witness (differentiator, own)
```

**Principle**: Own the critical path (L3-L4), outsource commodities (L0-L2).

Every successful large-scale STARK project (SP1, OpenVM, Risc0, Polygon Miden) owns
its machine layer. None build field arithmetic from scratch. Tabula should follow
the same pattern.

## Decision: Option C — Pattern Borrowing + Tabula-Owned Machine Layer

### Options Considered

| Option | Description | Verdict |
|--------|-------------|---------|
| A: openvm dependency | Use openvm-stark-backend as Cargo dep | Rejected: v2.0 risk, zkVM API mismatch |
| B: Status quo | Keep current ~2,600 LOC, fix C1 manually | Rejected: per-chip PCS inefficiency remains |
| **C: Pattern borrowing** | **Build Tabula machine layer, borrow openvm patterns** | **Selected** |
| D: Fork openvm | Fork and customize stark-backend | Rejected: maintenance burden, divergence |

### Rationale for Option C

1. **Same p3 version** (vanilla 0.4) — patterns translate without compatibility issues
2. **C1 solved by design** — PCS-committed LogUp from day one
3. **Shared PCS** — single opening proof vs 9 per-chip proofs
4. **Full control** — no external roadmap dependency (v2.0 SWIRL)
5. **Domain-specific optimizations** — Tabula's NF/static-coord properties are exploitable
6. **Net code reduction** — ~2,600 LOC removed, ~1,200 LOC added = -1,400 LOC

## Target Architecture

### Module Layout

```
tabula-proof/src/
├── machine/              ← NEW: Tabula machine layer (~1,200 LOC)
│   ├── mod.rs            ← Public API
│   ├── config.rs         ← STARK config (BabyBear + Poseidon2 + FRI)
│   ├── prover.rs         ← Multi-chip prover with shared PCS
│   ├── verifier.rs       ← Multi-chip verifier
│   ├── logup.rs          ← PCS-committed LogUp (two-round protocol)
│   ├── proof.rs          ← TabulaProof structure
│   └── traits.rs         ← TabulaMachine trait boundary
│
├── air/                  ← MODIFIED: RAP pattern adoption
│   ├── builder.rs        ← InteractionAirBuilder (retained, simplified)
│   ├── rap.rs            ← NEW: RAP blanket impl (replaces EmptyMessageBuilder)
│   ├── bus_macro.rs      ← Retained (typed buses are Tabula's advantage)
│   ├── bus.rs            ← Retained
│   ├── chip_set.rs       ← Simplified (builder pattern optional)
│   ├── chip_instance.rs  ← Adapted for shared PCS
│   ├── interaction.rs    ← Retained
│   ├── columns.rs        ← Retained
│   └── [extractor.rs]    ← REMOVED (RAP symbolic eval replaces column-scanning)
│
├── chips/                ← UNCHANGED (domain logic)
├── gadgets/              ← UNCHANGED
├── trace/                ← UNCHANGED
├── witness/              ← UNCHANGED
└── debug/                ← ADAPTED (use RAP-based interaction collection)
```

### What Gets Replaced (~1,600 LOC removed)

| Current File | LOC | Replacement |
|-------------|-----|-------------|
| `stark/prover.rs` | 181 | `machine/prover.rs` (shared PCS) |
| `stark/verifier.rs` | 120 | `machine/verifier.rs` |
| `stark/permutation.rs` | 319 | `machine/logup.rs` (PCS-committed) |
| `stark/bridge.rs` | 26 | `air/rap.rs` (RAP blanket impl) |
| `stark/config.rs` | 91 | `machine/config.rs` |
| `stark/proof.rs` | 97 | `machine/proof.rs` |
| `air/extractor.rs` | 214 | Removed (RAP replaces column-scanning) |
| Debug LogUp subset | ~550 | Adapted to new LogUp |

### What Gets Written (~1,200 LOC new)

| New File | ~LOC | Purpose | Reference |
|----------|------|---------|-----------|
| `machine/prover.rs` | 300 | Shared PCS across all chips | openvm Coordinator pattern |
| `machine/verifier.rs` | 200 | Multi-chip verification | openvm MultiTraceStarkVerifier |
| `machine/logup.rs` | 400 | PCS-committed cumsums, two-round | openvm FriLogUpPhase |
| `machine/config.rs` | 100 | STARK config wiring | Current config.rs adapted |
| `machine/proof.rs` | 100 | New proof structure | — |
| `air/rap.rs` | 100 | RAP blanket impl | openvm Rap trait |

## Key Design Decisions

### 1. Two-Round Proving Protocol (from openvm FriLogUp)

```
Round 1: Commit main traces
  For each chip: generate main trace matrix
  Batch-commit all main traces via single PCS

Round 2: Commit permutation traces (LogUp)
  Fiat-Shamir squeeze → (α, β) challenges in EF4
  For each chip: generate permutation trace from interactions
  Batch-commit all permutation traces via single PCS

Round 3: Quotient + opening
  Deep ALI quotient computation
  Single PCS opening proof across all committed polynomials
```

**Benefit**: LogUp cumsums are PCS-committed (C1 solved). Single PCS opening
(vs current 9 separate proofs).

### 2. RAP Blanket Impl (from openvm)

Replace `EmptyMessageBuilder` bridge with `Rap` trait:

```rust
// Current (awkward):
impl EmptyMessageBuilder for SymbolicAirBuilder<F, EF> {}
impl EmptyMessageBuilder for ProverConstraintFolder<'_, SC> {}
// ... one impl per p3 builder type

// New (clean):
pub trait Rap<AB: InteractionAirBuilder>: Air<AB> {
    fn eval_interactions(&self, builder: &mut AB) {
        // Default: interactions already emitted in Air::eval()
    }
}
// Blanket impl: any Air<AB> that uses InteractionAirBuilder gets Rap for free
```

**Benefit**: Chips implement `Air<AB>` as before. The `Rap` layer automatically
handles interaction collection. No bridge impls needed.

### 3. Typed Buses Retained

openvm uses raw `bus_index: u16`. Tabula keeps `define_bus!` macro:

```rust
define_bus! {
    pub MemoryBusAirBuilder(InteractionKind::Memory, ...) {
        table_id: expr,
        col_id: expr,
        row_key: u64limbs,
        timestamp: u64limbs,
        is_write: expr,
        value: var_arr<W>,
        val_is_null: expr,
    }
}
```

**Rationale**: Type safety across 9+ buses prevents subtle fingerprint mismatches.
This is a Tabula advantage, not overhead.

### 4. Static Chip Set (vs Dynamic Builder)

openvm uses dynamic `builder.add_air()`. Tabula keeps static enum dispatch:

```rust
define_chip_set! {
    pub enum TabulaAir {
        Execution(ExecutionChip<3>),
        StateColumn(StateColumnChip<3>),
        // ...
    }
}
```

**Rationale**: Tabula's chip set is known at compile time. Static dispatch enables
exhaustive matching, zero runtime overhead, and compile-time verification.

## Proof Structure

### Current

```rust
pub struct TabulaProof {
    pub chip_proofs: Vec<ChipProofEntry>,  // 9 independent STARK proofs
    pub logup_challenges: [EF4; 2],
}
```

### Target

```rust
pub struct TabulaProof {
    pub commitments: BatchCommitments,      // Shared PCS commitments
    pub opening: BatchOpening,              // Single opening proof
    pub per_chip: Vec<ChipData>,            // Per-chip metadata + public values
    pub logup_challenges: [EF4; 2],         // Fiat-Shamir derived
    pub logup_cumsums: Vec<EF4>,            // PCS-committed (C1 solved)
}
```

## Migration Path

### Phase 1: RAP Adoption
- Add `air/rap.rs` with blanket impl
- Migrate chips from `EmptyMessageBuilder` to RAP pattern
- Remove `air/extractor.rs`
- Tests: all existing 339+ tests pass

### Phase 2: Machine Layer (Prover)
- Implement `machine/prover.rs` with shared PCS batching
- Implement `machine/logup.rs` with PCS-committed cumsums
- Implement `machine/config.rs`
- Remove old `stark/prover.rs`, `stark/permutation.rs`
- Tests: E2E prover tests pass with new architecture

### Phase 3: Machine Layer (Verifier)
- Implement `machine/verifier.rs`
- Implement `machine/proof.rs`
- Remove old `stark/verifier.rs`, `stark/bridge.rs`
- Tests: E2E verify tests pass

### Phase 4: Debug Tooling Adaptation
- Adapt `debug/` module to use RAP-based interaction collection
- Remove old `debug/logup.rs` column-scanning path
- Tests: all debug checker tests pass

## Future Extension Points

The machine layer is designed to accommodate future Tabula-native optimizations
(see [Tabula-Native Optimizations](tabula-native-optimizations.md)):

- **Static-coordinate proof sharding**: Per-(t,c) parallel proving
- **Schema-driven chip specialization**: Width-class instantiation
- **Witness-driven late binding**: Dynamic KeyRoute → chip selection
- **Batch amortization**: Fixed/variable cost separation
- **Incremental state proofs**: Delta proving for untouched columns

These are impossible with a generic backend and require Tabula-owned L3-L4.
