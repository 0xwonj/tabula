# Custom Type Extensibility

> Describes how Tabula's type system extends to support application-defined types
> with arbitrary encoding widths, while maintaining the constraint that core types
> use the same mechanism as custom types.
>
> Related: [extensibility-architecture.md](extensibility-architecture.md) (framework-level extensibility),
> [commitment-architecture-research.md](commitment-architecture-research.md) (VC strategy),
> [full-sharding-research.md](full-sharding-research.md) (per-column proofs)

---

## Problem: Closed Type Chain

The current type system forms a closed chain from application values down to AIR chips:

```
Value(enum 4) → ValueType(enum 4) → KoalaBearCodec(exhaustive match) → EncodingWidth → chip<W>
                                                                        ↑ already open
```

`Value` and `ValueType` are closed enums with 4 variants each (U64, I64, Bool, Bytes32).
`KoalaBearCodec` uses exhaustive `match` in `encode()`, `decode()`, and `field_elements_per()`.
Adding a new type requires modifying all three.

The chain must be opened so that application developers can define new types (e.g., `FixedPoint128`, `Address20`, `OrderId`) without modifying Tabula's codebase — consistent with the [Zero-Modification Principle](extensibility-architecture.md#11-the-zero-modification-principle).

**Key insight**: `EncodingWidth` is already open (`EncodingWidth(pub usize)`). The closure point is above it — in `ValueType` and the codec dispatch.

---

## Current Implementation

### ValueType (closed enum)

```rust
// crates/core/src/state/value.rs
pub enum ValueType { U64, I64, Bool, Bytes32 }
```

Used in: column schemas (`ColumnDef.value_type`), codec dispatch, zero-value generation, display, serialization. Approximately 91 reference sites across the workspace.

### ValueCodec trait (open interface)

```rust
// crates/core/src/traits/codec.rs
pub trait ValueCodec: Send + Sync {
    type FieldRepr: Clone + Send + Sync;
    fn encode(&self, value: &Value) -> Result<Vec<Self::FieldRepr>, TabulaError>;
    fn decode(&self, fes: &[Self::FieldRepr], target_type: ValueType) -> Result<Value, TabulaError>;
    fn field_elements_per(&self, value_type: ValueType) -> usize;
}
```

The trait is open, but its `decode()` and `field_elements_per()` take `ValueType` — coupling the open interface to the closed enum.

### EncodingWidth (open newtype)

```rust
// crates/stark/src/trace/column_commitment.rs
pub struct EncodingWidth(pub usize);
impl EncodingWidth {
    pub const BOOL: Self = Self(1);      // w(Bool) = 1
    pub const STANDARD: Self = Self(3);  // w(U64) = w(I64) = 3
    pub const WIDE: Self = Self(8);      // w(Bytes32) = w(Digest) = 8
}
```

Already fully open. Application types can use `EncodingWidth(5)` or any width. This is the target state for the entire chain.

### KoalaBearCodec (closed implementation)

```rust
// crates/commitment/src/codec.rs
impl ValueCodec for KoalaBearCodec {
    fn encode(&self, value: &Value) -> Result<Vec<KoalaBear>, TabulaError> {
        match value {
            Value::Bool(b) => ...,
            Value::U64(n) => ...,
            Value::I64(n) => ...,
            Value::Bytes32(b) => ...,
        }
    }
    // Similar exhaustive matches in decode() and field_elements_per()
}
```

Three exhaustive match sites. This is the codec-level closure point.

---

## Design: TypeTag Open Identifier

Replace the closed `ValueType` enum with an open `TypeTag(u16)` newtype, following the same pattern as `BusId(u16)` and `ChipId`.

```rust
/// Open type identifier. Core types use well-known constants.
/// Application types use the app range (1000+).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TypeTag(pub u16);

impl TypeTag {
    pub const U64: Self = Self(0);
    pub const I64: Self = Self(1);
    pub const BOOL: Self = Self(2);
    pub const BYTES32: Self = Self(3);

    /// Create an application-defined type tag.
    pub const fn app(id: u16) -> Self { Self(id + 1000) }
}
```

### Migration Path

`ValueType` → `TypeTag` replacement affects ~91 reference sites. The migration is mechanical:

| Before | After |
|--------|-------|
| `ValueType::U64` | `TypeTag::U64` |
| `fn foo(ty: ValueType)` | `fn foo(ty: TypeTag)` |
| `match ty { ValueType::U64 => ... }` | Registry dispatch (see TypeEncoding) |
| `ColumnDef { value_type: ValueType }` | `ColumnDef { type_tag: TypeTag }` |

The exhaustive `match` sites (9 total) convert to registry lookups, which is the key benefit: new types register without modifying existing code.

### Compatibility

- `ValueType` can be kept as a `pub(crate)` internal enum for core code that needs exhaustive matching on the 4 built-in types.
- `TypeTag` is the public interface in schemas, codec dispatch, and column plans.
- Serde: `TypeTag(u16)` is naturally serializable. Well-known constants serialize as integers.

---

## Design: TypeEncoding Trait

A per-type encoding strategy that replaces the monolithic `KoalaBearCodec` exhaustive matches:

```rust
/// Defines how a type is encoded into field elements for proof circuits.
pub trait TypeEncoding<F: PrimeField32>: Send + Sync {
    /// Which type this encoding handles.
    fn type_tag(&self) -> TypeTag;

    /// Number of field elements per value (the encoding width W).
    fn encoding_width(&self) -> EncodingWidth;

    /// Encode a raw value into field elements.
    fn encode(&self, raw: &[u8]) -> Result<Vec<F>, TabulaError>;

    /// Decode field elements back into raw bytes.
    fn decode(&self, fes: &[F]) -> Result<Vec<u8>, TabulaError>;

    /// The canonical zero encoding (used when val_is_null = true).
    fn zero_encoding(&self) -> Vec<F>;
}
```

### Design Decisions

**Raw bytes interface**: `encode(&[u8])` / `decode() -> Vec<u8>` instead of `Value` — decouples from the `Value` enum. Core types serialize `Value` to bytes first, then encode. Custom types own their byte representation.

**Zero encoding**: Explicit method instead of encoding a "zero value" — custom types define their own null representation without needing a `Value` variant.

**Registry dispatch**: A `TypeEncodingRegistry` maps `TypeTag → Box<dyn TypeEncoding<F>>`. Core types are pre-registered. Applications register custom types at setup time:

```rust
let mut registry = TypeEncodingRegistry::default(); // pre-registers U64, I64, Bool, Bytes32
registry.register(FixedPoint128Encoding::new());    // TypeTag::app(0), EncodingWidth(5)
```

### Relationship to ValueCodec

`ValueCodec` remains for the core `Value` enum (executor-level). `TypeEncoding` is the proof-level encoding — a different layer. The two coexist:

```
Application:  Value (closed enum, executor)  →  ValueCodec (core types only)
Proof:        TypeTag (open) + raw bytes     →  TypeEncoding (any type)
```

When full sharding is adopted, each per-column proof only needs `TypeEncoding` — the `Value` enum and `ValueCodec` become executor-only concerns.

---

## Design: Bus Width Handling

Custom types with encoding width W ≠ 3 create a bus compatibility challenge. The current Memory bus has a fixed `val[3]` field.

### Option A: MAX_W with Zero-Padding

Use a fixed maximum width (e.g., MAX_W = 8) for all bus messages. Narrow types zero-pad.

```
Memory bus message: (t, c, r[3], tau[3], is_write, val[MAX_W], val_is_null)
```

- **Pro**: Single bus, simple implementation, no architectural change.
- **Con**: Waste for narrow types (Bool uses 1/8 of val field). Bus width drives trace width for all chips.

### Option B: Per-Column Proof with Native Width (Full Sharding)

With [full sharding](full-sharding-research.md), each column is an independent proof with its own encoding width. The Memory bus is per-column-proof, so each uses exactly W field elements.

```
Column proof for (t=1, c=2, W=5):
  Memory bus message: (r[3], tau[3], is_write, val[5], val_is_null)
```

- **Pro**: Zero padding waste. Each column-proof is width-optimal.
- **Con**: Requires full sharding architecture (large effort).

### Option C: Per-Width Buses (Rejected)

Separate Memory buses for each width class (e.g., MemoryBus1, MemoryBus3, MemoryBus8).

- **Rejected**: Creates separate pipelines, breaks unified memory consistency. Explicitly incompatible with the LogUp single-bus architecture.

### Decision

The choice depends on whether full sharding is adopted:

- **Without sharding**: Option A is the only viable approach. MAX_W should be configurable via `ProofConfig` with a sensible default (8, matching Bytes32/Digest).
- **With sharding**: Option B is natural — per-column proofs inherently have per-column bus width.

The current architecture supports Option A as a stopgap. The `EncodingWidth` type and `ColumnPlan` metadata are already in place to enable either path.

---

## Design: Value Enum Extension

The `Value` enum is used throughout the executor for runtime computation. Extending it for custom types has significant impact.

### Option A: Value::Custom Variant

```rust
pub enum Value {
    U64(u64),
    I64(i64),
    Bool(bool),
    Bytes32([u8; 32]),
    Custom { tag: TypeTag, data: Vec<u8> },  // NEW
}
```

**Impact**: `Value` loses `Copy` (currently implements `Copy` — ~251 sites depend on this). `Vec<u8>` requires heap allocation for every custom value.

### Option B: Value Unchanged, Custom Types Bypass Executor

Custom types are storage-only — they cannot participate in executor arithmetic (no `Add`, `Cmp`, etc.). The executor sees them as opaque blobs. Only proof-level encoding applies.

- **Pro**: No disruption to `Value` or executor. Most custom types need only Read/Write, not arithmetic.
- **Con**: Cannot compute on custom types in the DSL. Types like `FixedPoint128` that need arithmetic require precompile wiring.

### Current Recommendation

**Option B** for the initial implementation. Rationale:

1. Most custom types are storage/commitment types (addresses, hashes, custom structs) that need Read/Write but not arithmetic.
2. Types requiring computation (FixedPoint128) naturally map to the [precompile pattern](extensibility-architecture.md#10-precompile-system) — the computation is in a custom chip, not in the executor's generic arithmetic.
3. Preserving `Value: Copy` avoids a cascading 251-site migration that provides little value.

Option A remains available as a future extension if strong demand emerges.

---

## Impact Analysis

| Change | Scope | Risk | Depends On |
|--------|-------|------|------------|
| `TypeTag` replacement | ~91 reference sites | Medium — mechanical refactor | None |
| `TypeEncoding` trait | New trait + 4 core impls | Low — additive | TypeTag |
| `TypeEncodingRegistry` | New registry, codec dispatch | Low — additive | TypeEncoding |
| KoalaBearCodec → registry | 3 exhaustive matches | Low — refactor | TypeEncoding |
| Bus width (Option A) | All chip send/receive for Memory bus | High — AIR changes | TypeTag |
| Bus width (Option B) | Full sharding adoption | Very High — architecture change | Sharding |
| `Value::Custom` (if needed) | ~251 creation sites, Copy→Clone | High — cascading | Decision |

---

## Interaction with Other Systems

### Full Sharding

Full sharding ([research](full-sharding-research.md)) eliminates the bus width problem entirely. Each per-column proof uses its column's native `EncodingWidth`. TypeTag and TypeEncoding are prerequisites for sharding — the column plan must know each column's encoding width to configure per-column proof parameters.

### ColumnCommitment

The `ColumnCommitment` trait already receives `ColumnPlan` which includes `EncodingWidth`. Custom types integrate naturally — the commitment scheme sees field elements at the correct width, regardless of the source type.

### Width Specialization

Width specialization (instantiating chips at W=1, W=3, W=8) is a natural consequence of TypeTag + EncodingWidth. The `ProofPlan` groups columns by width, and width-specialized chips (e.g., `MemoryShard<1>`, `MemoryShard<3>`, `MemoryShard<8>`) are instantiated accordingly.

---

## Summary

The custom type extensibility architecture opens the type chain at two points:

1. **TypeTag** (replacing ValueType) — open identifier for schema-level type declaration
2. **TypeEncoding** (complementing ValueCodec) — per-type encoding strategy for proof circuits

The bus width challenge is resolved by either MAX_W padding (immediate) or full sharding (optimal). The `Value` enum remains unchanged initially — custom types are storage-only, with computation via precompiles.

This design ensures that core types (U64, I64, Bool, Bytes32) are pre-registered instances of the same mechanism that custom types use — no special-casing.
