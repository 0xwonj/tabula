# tabula-proof

STARK proof system for the Tabula kernel.

## Role

Given execution results and column state snapshots, generates structured
witness data and constrains it via AIR chips over Plonky3/BabyBear.

Depends only on `tabula-core` (no IR, no executor). Cryptographic
primitives come from `tabula-commitment` behind the `stark` feature.

## Key Design

**Chip 3-file pattern.** Each AIR chip is a directory with three files:

- `columns.rs` — `#[repr(C)]` column struct (data shape)
- `air.rs` — `BaseAir` + `Air` impl (constraints)
- `trace.rs` — witness → `RowMajorMatrix<BabyBear>` (trace generation)

Changes to one concern (e.g. adding a column) affect exactly the
files responsible for that concern.

**Gadgets are reusable building blocks.** Each gadget bundles column
structs + `populate()` + `constrain_*()` for cross-chip reuse
(boolean prefix, integer limbs, lexicographic ordering, etc.).

**Debug checker.** `debug_check()` runs AIR constraints over a trace
with concrete field values, giving row-level error messages. This is
the primary testing tool — every chip has tests that build a trace
and run it through the debug checker.

## Feature Flags

| Feature | Effect |
|---------|--------|
| `stark` | Enables AIR constraints, witness generation, Plonky3 dependencies |
