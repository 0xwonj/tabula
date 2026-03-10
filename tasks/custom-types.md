# Type Foundation

> Status: 🔵 Ready (CT-1 and CT-2 have no blockers)
> Design: [docs/design/custom-type-extensibility.md](../docs/design/custom-type-extensibility.md)
> Related: [docs/design/extensibility-architecture.md](../docs/design/extensibility-architecture.md)

## Goal

Open the closed type chain (ValueType → BabyBearCodec → exhaustive match) so custom types can register without modifying Tabula.

**With full sharding as base architecture**: Bus width (CT-5) is solved automatically — each column proof uses its own W. No MAX_W padding needed. CT-1 and CT-2 are still valuable as foundational infrastructure.

## Tasks

### CT-1: TypeTag Open Identifier (~1 day)

> No blockers. Independent of sharding.

- [ ] `ValueType` → `TypeTag(u16)` replacement
- [ ] Well-known constants (U64=0, I64=1, BOOL=2, BYTES32=3)
- [ ] `TypeTag::app(id)` for application-defined types
- [ ] Update ~91 reference sites (mechanical)
- [ ] Update `ColumnDef` schema type

### CT-2: TypeEncoding Trait (~1 day)

> Depends: CT-1

- [ ] Trait definition in `commitment/` or `stark/`
  ```rust
  pub trait TypeEncoding<F: PrimeField32>: Send + Sync {
      fn type_tag(&self) -> TypeTag;
      fn encoding_width(&self) -> EncodingWidth;
      fn encode(&self, raw: &[u8]) -> Result<Vec<F>, TabulaError>;
      fn decode(&self, fes: &[F]) -> Result<Vec<u8>, TabulaError>;
      fn zero_encoding(&self) -> Vec<F>;
  }
  ```
- [ ] Core type implementations (U64Encoding, I64Encoding, BoolEncoding, Bytes32Encoding)
- [ ] `TypeEncodingRegistry` with pre-registered core types

### CT-3: BabyBearCodec Registry Dispatch

> Depends: CT-2

- [ ] Replace 3 exhaustive matches with registry lookup
- [ ] Backward compatible (core types pre-registered)

### CT-4: Value::Custom Variant (deferred)

Deferred — custom types are storage-only. Types requiring computation use precompile pattern.

### ~~CT-5: Bus Width Unification~~ (resolved by sharding)

~~Implement MAX_W padding or sharding.~~ Full sharding resolves this: each column proof uses its column's native W. No global bus width coordination needed.

## Verification

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
```
