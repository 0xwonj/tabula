# Composition Framework

> Status: 🔵 Ready
> Depends: None
> Design: [docs/design/extensibility-architecture.md](../docs/design/extensibility-architecture.md) §4 (Chip Composition), §5 (Trace Pipeline)

## Goal

App developers can package chip + bus + witness as one distributable unit and register it with MachineBuilder. No Tabula code modification required.

## Tasks

### BusId newtype (~50 LOC)

> Ready — no dependencies

Replace closed `InteractionKind` enum with open `BusId(u16)` newtype.

- [ ] Define `BusId(u16)` in `stark/src/air/interaction.rs`
  ```rust
  pub struct BusId(pub u16);
  pub mod core_buses {
      pub const MEMORY: BusId = BusId(1);
      pub const SSMC_MEMBERSHIP: BusId = BusId(2);
      // ... 11 core buses
      pub const ALL: [BusId; 11] = [...];
  }
  impl BusId {
      pub const fn app(id: u16) -> Self { Self(id + 1000) }
  }
  ```
- [ ] Replace `InteractionKind` → `BusId` in all chip `send()`/`receive()` calls
- [ ] Update `define_bus!` macro
- [ ] Update `stark/src/debug/logup.rs` bus balance verification

**Impact**: 9 chip air.rs files, builder.rs, debug/logup.rs

### ChipExtension trait (~150 LOC)

> Blocked on: BusId

Interface for app-provided chip packages.

- [ ] Define trait in `machine/src/extension.rs`
  ```rust
  pub trait ChipExtension: Send + Sync {
      fn name(&self) -> &str;
      fn chip_ids(&self) -> Vec<ChipId>;
      fn airs(&self) -> Vec<Box<dyn AnyRap>>;
      fn dyn_chips(&self) -> Vec<Box<dyn DynChip>>;
      fn buses(&self) -> Vec<BusId>;
      fn bus_consumers(&self) -> Vec<Box<dyn BusConsumer>>;
  }
  ```
- [ ] `MachineBuilder::with_extension()` method
- [ ] Test: register custom extension + build machine

### WitnessStore typed KV (~100 LOC)

> Blocked on: ChipExtension

Chip-to-chip witness data passing via typed key-value store.

- [ ] Extension-aware API on existing WitnessStore
- [ ] `ChipExtension::populate_witness()` method
- [ ] Test: extension populates + reads witness data

### Prelude module (~50 LOC)

> Ready — no dependencies

Re-export p3 types so app developers don't import p3 directly.

- [ ] Create `machine/src/prelude.rs`
  ```rust
  pub use p3_baby_bear::BabyBear;
  pub use p3_field::{PrimeCharacteristicRing, PrimeField32};
  pub use p3_matrix::dense::RowMajorMatrix;
  pub use tabula_stark::chips::{ChipId, ChipSpec};
  pub use crate::{AnyRap, TabulaMachine, MachineBuilder};
  ```
- [ ] Add `pub mod prelude;` to `lib.rs`

## Completion Criteria

- BusId is an open newtype (app-defined buses via `BusId::app()`)
- ChipExtension trait packages chips + buses + witness
- Prelude re-exports all types app developers need
- All existing tests pass

## Verification

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
```
