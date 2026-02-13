# Implementation Conventions

> Rules governing all code in the Tabula workspace.

---

## Rust Conventions

- **Edition 2024**, `rustc 1.93`
- **No `unwrap()`/`expect()` in library crates** — always return `Result<T, TabulaError>`
- **`BTreeMap`/`BTreeSet` only** in `tabula-executor` — no `HashMap` (determinism)
- **All public items get doc comments** (`#![warn(missing_docs)]` on lib crates)
- **`#![deny(unused)]`** on all library crates
- **Derive standard traits** on all data types: `Debug, Clone, PartialEq, Eq`
- **Borsh + Serde derives** on types that need serialization: `Serialize, Deserialize, BorshSerialize, BorshDeserialize`
- **`Ord`/`PartialOrd`** on identifier types (`TableId`, `ColId`, `RowKey`, `CellKey`) for BTreeMap keys

## Naming

- Types: `PascalCase` (Rust standard)
- Traits: `PascalCase`, adjective-like where possible (`Hasher`, not `Hash`)
- Modules: `snake_case`, singular (`overlay`, not `overlays`)
- Test functions: `test_<behavior>` (e.g., `test_read_your_writes`)
- Error variants: `PascalCase`, descriptive noun phrase (`ArithmeticOverflow`, not `Overflow`)

## Error Handling

- One error enum per crate boundary: `TabulaError` in `tabula-core`, used everywhere
- Error variants carry enough context to diagnose (include the `CellKey`, `TableId`, etc.)
- No `anyhow` in library crates — only in `tabula-cli`

## Testing

- **TDD**: write test first, verify it fails, then implement
- **One test = one behavior** — descriptive name, single assert focus
- **Property-based tests** (`proptest`) for overlay semantics and consistency
- **No test helpers that hide assertions** — keep tests readable inline
- **Test modules** at bottom of each source file (`#[cfg(test)] mod tests { ... }`)

## Architecture Rules

- **Executor has zero crypto deps** — enforced by Cargo.toml (no `blake3`, no `ark-*`)
- **All crypto via traits** — executor receives `&dyn SigVerifier`, `&dyn Hasher`, etc.
- **Immutable snapshot** — `StateSnapshot` is read-only, overlay handles mutations
- **Stage 1 / Stage 2 boundary** — `ExecutionResult` is the handoff, no leaking

## Null / Absence Semantics

Null is **not** a value type. The `Value` enum has four variants: `U64`, `I64`, `Bool`, `Bytes32`. Absence is represented separately:

- **State layer:** `Option<Value>` — `None` = absent cell.
- **IR Read:** `Read { dst_val, dst_is_null, table, col, row }` — produces two SSA slots. `dst_is_null: Bool` indicates absence.
- **IR Write:** `Write { table, col, row, src_val, src_is_null }` — `src_is_null = true` is a **delete**.
- **Canonical zero:** When `val_is_null = true`, the value slot MUST contain `zero_value(T)` (U64→0, I64→0, Bool→false, Bytes32→[0;32]).

**Guard pattern:**
```
Assert(Eq(Slot(is_null_slot), Literal(Bool(false))))  // ensure key exists before use
```

**Rationale:** No SQL-style three-valued logic. In a ZK constraint system every boolean must resolve to 0 or 1; a separate `is_null` flag is cheaper than a tagged union or `Unknown` truth value.

## Commit Style

- One step = one commit
- Format: `feat: <description>` (conventional commits, no body)
- Each commit must pass: `cargo check --workspace && cargo test --workspace && cargo clippy --workspace`
