# Custom Type Extensibility

> Status: 🔬 Research (design decisions needed)
> Depends: [composition.md](composition.md), design decision on bus width (see [sharding.md](sharding.md) or [optimization.md](optimization.md))
> Design: [docs/design/custom-type-extensibility.md](../docs/design/custom-type-extensibility.md)
> Related: [docs/design/extensibility-architecture.md](../docs/design/extensibility-architecture.md)

## Goal

App developers can add custom types with arbitrary encoding width W without modifying Tabula. Core types (U64, I64, Bool, Bytes32) use the same mechanism — they are pre-registered instances, not special-cased.

## Open Design Decisions

Detailed analysis of each option: [custom-type-extensibility.md](../docs/design/custom-type-extensibility.md)

1. **Bus width strategy**: MAX_W padding (immediate, wasteful) vs full sharding (optimal, large effort). Depends on sharding adoption decision.
2. **Value::Custom**: Needed only if custom types require executor arithmetic. Current recommendation: defer (Option B — storage-only custom types).

## Implementation Steps

### CT-1: TypeTag open identifier

- [ ] `ValueType` → `TypeTag(u16)` replacement
- [ ] Well-known constants (U64=0, I64=1, BOOL=2, BYTES32=3)
- [ ] Update ~91 reference sites (mechanical)
- [ ] Update `ColumnDef` schema type

### CT-2: TypeEncoding trait

- [ ] Trait definition in `commitment/` or `stark/`
- [ ] Core type implementations (U64Encoding, I64Encoding, BoolEncoding, Bytes32Encoding)
- [ ] `TypeEncodingRegistry` with pre-registered core types

### CT-3: BabyBearCodec registry dispatch

- [ ] Replace 3 exhaustive matches with registry lookup
- [ ] Backward compatible (core types pre-registered)

### CT-4: Value::Custom variant (if needed)

- [ ] Value enum extension
- [ ] Copy → Clone migration (~251 sites)

### CT-5: Bus width unification

- [ ] Implement chosen approach (MAX_W padding or sharding)

## Impact Summary

| Change | Scope | Risk |
|--------|-------|------|
| TypeTag replacement | ~91 reference sites | Medium |
| TypeEncoding trait + registry | New code, additive | Low |
| BabyBearCodec refactor | 3 exhaustive matches | Low |
| Value::Custom | ~251 sites, Copy lost | High |
| Bus width | All chip send/receive | High |

## Verification

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
```
