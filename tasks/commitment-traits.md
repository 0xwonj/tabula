# Goal 6: Extensibility API

> Status: ✅ Complete (Phase 1-3 ✅, E9 deferred to Goal 7)
> Design: [docs/design/extensibility-architecture.md](../docs/design/extensibility-architecture.md)
> Depends: Goals 1-5 (✅ complete)
> Unblocks: Goal 7 (Precompile), Goal 8 (Templates)

## Goal

Enable the Zero-Modification Principle: applications customize Tabula purely in their own crate, consuming Tabula as an immutable Cargo dependency. Foundation for future SDK.

## Already Implemented (from prior goals)

- [x] `ChipRegistry` + `AnyRap` — runtime chip registration via `Box<dyn AnyRap>`
- [x] `BusId(u16)` + `define_bus!` — open bus IDs (core 0-99, app 100+)
- [x] `TraceContributor` + `WitnessStore` — phase-ordered trace generation
- [x] `DynChip` + `BusConsumer` — object-safe chip + bus consumption
- [x] `RootProof` trait — pluggable root proof (default: SmtRootProof)
- [x] Per-tier setup — `TierSetup` with registry + keys + dyn_chips
- [x] `EncodingWidth` + `ColumnPlan` — per-column width polymorphism

## Phase 1: Builder API + ChipExtension (F2, F12)

### E1: `ChipExtension` trait ✅

- [x] Trait defined in `machine/src/extension.rs`
  - `name()`, `airs()`, `dyn_chips()`, `bus_consumers()`, `populate_witness()`
  - Follows `RootProof` pattern: returns owned `Box<dyn AnyRap>` / `Box<dyn DynChip>`
- [x] `ExtensionContext` — minimal Phase 1 struct, expanded in Phase 2+
- [x] Tests: register extension, verify chips appear in registry (10 tests in `builder.rs`)

### E2: `TabulaMachine::builder()` API ✅

- [x] `MachineBuilder` struct with fluent API in `machine/src/builder.rs`
  - `with_columns()`, `with_config()`, `with_root_proof()`, `with_extension()`, `build()`
- [x] `TabulaMachine::new()` and `with_config()` delegate to builder
- [x] Duplicate ChipId detection via `registry.validate()`
- [x] `register_boxed()` widened from `pub(crate)` to `pub`
- [x] `ColumnSetupConfig` derives Clone + Copy + Debug
- [x] Tests: core-only, with extension, with config, duplicate rejection, legacy parity

### E3: `tabula-machine::prelude` ✅

- [x] Module re-exporting p3 types + Tabula extension traits in `machine/src/prelude.rs`
  - p3: `Air`, `AirBuilder`, `BaseAir`, `BabyBear`, `PrimeCharacteristicRing`, `Matrix`, `RowMajorMatrix`
  - Tabula: `ChipSpec`, `ChipId`, `ChipIdAllocator`, `BusId`, `InteractionAirBuilder`,
    `TraceContributor`, `TracePhase`, `WitnessStore`, `DynChip`, `BusConsumer`
  - Machine: `AnyRap`, `ChipRegistry`, `ChipExtension`, `ExtensionContext`
- [x] Verified: DummyChip test uses only prelude imports

## Phase 2: ColumnCommitment Impls (F5b)

> `ColumnCommitment` trait is already defined in `stark/src/trace/column_commitment.rs`.
> This phase wraps the existing SSMC/SMT logic into trait impls.

### E4: `SsmcCommitment` impl ✅ (pre-existing)

- [x] `ColumnCommitment` trait impl in `chips/src/shards/ssmc.rs` (pre-existing, tested)
- [x] Bus integration preserved (all 7 core buses)

### E5: `SmtCommitment` impl ✅ (pre-existing)

- [x] `ColumnCommitment` trait impl in `chips/src/shards/smt.rs` (pre-existing, tested)
- [x] Bus integration preserved

### E6: Per-column commitment selection ✅

- [x] `ColumnScheme` trait in `machine/src/column_scheme.rs`
  - `name()`, `create_chips()` — per-column chip instantiation
- [x] `SsmcScheme<W>` — 3 shard chips (Memory, State, Meta)
- [x] `SmtScheme<W>` — 2 shard chips (Memory, Meta)
- [x] `MachineBuilder::with_column_scheme(tag, scheme)` — register per-tag schemes
- [x] `column_tier_setup_with_scheme()` — setup dispatches to registered scheme
- [x] Default: `SsmcScheme<3>` pre-registered for `scheme_tags::SSMC`
- [x] Unregistered scheme_tag → `SetupError::SetupFailed`
- [x] Tests: SSMC default, SMT registration, mixed schemes, error on unknown tag

## Phase 3: PropertyOpening Trait (F6)

### E8: `PropertyOpening` trait ✅

- [x] Trait defined in `machine/src/property.rs`
  - `PropertyOpening` trait: `name()`, `compatible_scheme_tag()`, `supported_queries()`, `prove()`, `column_verifier()`
  - `PropertyQuery` enum: Minimum, Maximum, Successor, Predecessor, NonExistenceRange, Aggregate (keys use `RowKey`)
  - `PropertyQueryKind` enum: capability declaration for implementations
  - `AggregateKind` enum: Sum, Count
  - `PropertyWitness` trait: `value()`, `key()`, `is_null()`, `as_any()` (opaque, downcastable)
  - `PropertyError` enum: UnsupportedQuery, IncompatibleSchemeTag, NoOpeningRegistered, ProofFailed
- [x] Types exported from `machine/src/lib.rs` and `machine/src/prelude.rs`
- [x] `MachineBuilder::with_property_opening()` for registration
- [x] `TabulaMachine::property_openings()` accessor
- [x] Tests: registration, multiple openings, prove/verify, unsupported query, empty state, query-kind mapping (7 tests in `builder.rs`)

### E9: `PropertyRead` IR variant (⏳ deferred to Goal 7)

> Deferred: Adding a new `Instruction` variant requires updating 4+ exhaustive match sites
> across crates. This is best done in Goal 7 (Precompile) when concrete property opening
> implementations exist to test against.

- [ ] Add to `Instruction` enum (one-time)
- [ ] Executor dispatch to `PropertyOpening.prove()`
- [ ] Wire PropertyWitness into WitnessStore → chip trace

## Completion Criteria

- [x] App crate can define custom chips using only `tabula-machine::prelude`
- [x] App crate can package chips via `ChipExtension` trait
- [x] `TabulaMachine::builder()` API for composition
- [x] SSMC/SMT wrapped as `ColumnScheme` impls with per-column selection
- [x] `PropertyOpening` trait for structural queries
- [x] All 882 tests pass (25 builder tests including H1/L5/L6/M6 validation tests)
- [x] Integration test demonstrating custom extension (DummyChip in `builder.rs`)

## Verification

```bash
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets
```
