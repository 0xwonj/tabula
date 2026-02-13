# tabula-core

Core types, trait definitions, and IR for the Tabula kernel.

This crate defines **what exists** — every other crate depends on it,
and it depends on nothing internal.

## Modules

| Module | Contents |
|--------|----------|
| `types` | `Value` (U64, I64, Bool, Bytes32), `CellKey`, `TableId`, `ColId`, `RowKey`, `ValueType` |
| `ir` | `Instruction` enum (Read, Write, Lookup, Add, Sub, Mul, DivMod, Assert, Hash, Select, Emit), `RowExpr`, `ValueExpr`, `Predicate`, `Slot` |
| `traits` | Pluggable abstractions: `Hasher`, `StateSnapshot`, `SigVerifier`, `NoncePolicy`, `ValueCodec`, `MembershipScheme`, `BatchDigester`, `StaticTableProvider` |
| `mock` | (feature-gated) Blake3-based mock implementations: `MockHasher`, `InMemoryState`, `MockSigVerifier`, `SequentialNonce`, `InMemoryStaticTables`, `MockValueCodec`, `FlatHashMembership`, `SimpleBatchDigester` |
| `schema` | `TableSchema`, `ColumnDef` |
| `tx` | `Transaction`, `TxTypeDef`, `Batch`, `ProgramBudgets` |
| `event` | `ExecutionEvent`, `ExecutionResult`, `TxOutcome` |
| `state` | `StateRoot`, `Digest`, `TableCommitmentId`, `ColumnCommitmentId` |
| `error` | `TabulaError` — all error variants including NF-1~NF-4 violations |

## Design Principles

- **Zero crypto dependencies.** All cryptographic operations are behind traits.
  The crate only depends on `borsh`, `serde`, and `thiserror`.
- **Trait-driven pluggability.** The executor and commitment layers are parameterized,
  not hardcoded. Different deployments can inject different implementations.
- **Two-slot IR.** `Read` produces `(dst_val, dst_is_null)`; `Write` takes
  `(src_val, src_is_null)`. Null is a boolean flag, not a value type.

## Features

| Feature | Effect |
|---------|--------|
| `mock` | Enables `mock` module with Blake3-based test implementations of all traits |
