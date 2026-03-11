# Type Foundation

> Status: ✅ Complete
> Design: [docs/design/custom-type-extensibility.md](../docs/design/custom-type-extensibility.md)

## Goal

Establish a sound, minimal type system for Tabula's state layer.

**Decision**: Closed type system with `ValueType` enum. No custom type extensibility.

**Rationale**:
- U64/I64/Bool/Bytes32 covers all practical state needs
- `bytes32` serves as escape hatch for complex data (Ethereum-proven pattern)
- Each type requires AIR constraints — open extensibility risks soundness
- Adding a new built-in type is ~100 lines; extensibility infrastructure was 425+ lines with zero callers
- Exhaustive matching at compile time is safer than runtime registry dispatch

## Completed

### ValueType as Sole Identifier

- [x] `ValueType` enum (U64, I64, Bool, Bytes32) — closed, with Hash/Ord
- [x] `ColumnDef.value_type: ValueType`
- [x] `ParamDef.value_type: ValueType`
- [x] `ColumnWitness.value_type: ValueType`
- [x] `ValueCodec` trait: `decode()` and `field_elements_per()` take `ValueType`
- [x] `zero_value(ValueType)` — infallible, no panic path
- [x] `BabyBearCodec`: direct exhaustive matching, no indirection

### Soundness Fixes

- [x] Boolean input constraints added to NOT, AND, OR in `chips/execution/ops/logic.rs`
  - NOT: `src1_val[0] * (src1_val[0] - 1) = 0` (1 constraint)
  - AND: `src1_val[0], src2_val[0] ∈ {0,1}` (2 constraints)
  - OR: `src1_val[0], src2_val[0] ∈ {0,1}` (2 constraints)
- [x] `const { assert!(W >= 3) }` in Add, Sub, Mul, Cmp, DivMod
- [x] Null encoding: literal zeros consistently (encode_trace + witness pipeline)
- [x] `SSMC_MAX_VALUE_FES=5` const extracted in `witness/encoding.rs`

### Removed (over-engineered without custom types)

- [x] `TypeTag(u16)` — open newtype, removed entirely
- [x] `TypeEncoding` trait + `TypeEncodingRegistry` — deleted (425 lines, zero callers)
- [x] `type_encoding.rs` — deleted from `commitment/src/`

## Verification

```
cargo check --workspace --all-targets  ✅
cargo test --workspace                 ✅ 857 tests, 0 failures
cargo clippy --workspace --all-targets ✅ 0 warnings
```
