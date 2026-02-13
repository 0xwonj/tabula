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

## Null Value Semantics

`Value::Null` represents the absence of a value (e.g. an uninitialized cell).

**Comparison on Null → `NullValue` error.** `Value::compare()` returns
`Err(TabulaError::NullValue)` when either operand is `Null`. There is no
SQL-style three-valued logic (3VL) — no `Unknown` truth value.

**Arithmetic on Null → `NullValue` error.** `checked_add`, `checked_sub`,
`checked_mul`, and `checked_divmod` all fail immediately on `Null` operands.

**Guard pattern:**
```
Assert(NotNull(Slot(s)))   // ensure the slot is non-Null before comparison
Assert(Gte(Slot(s), ...))  // safe — we know s is not Null
```

**Rationale:** 3VL introduces an `Unknown` truth value that must propagate
through `And`/`Or`/`Not`. In a ZK constraint system every boolean must
resolve to 0 or 1; modelling `Unknown` as a third state doubles the
constraint cost of every predicate. Failing fast on `Null` keeps the
constraint system simple and moves responsibility to the program author.

## Commit Style

- One step = one commit
- Format: `feat: <description>` (conventional commits, no body)
- Each commit must pass: `cargo check --workspace && cargo test --workspace && cargo clippy --workspace`
