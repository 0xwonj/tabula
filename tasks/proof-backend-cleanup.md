# Proof Backend Boundary Cleanup

> Status: ✅ Complete
> Design: [docs/design/proof-backend-contract.md](../docs/design/proof-backend-contract.md)
> Related: [proving-layer.md](proving-layer.md), [docs/design/proving-layer-architecture.md](../docs/design/proving-layer-architecture.md), [docs/design/zkvm-library-architecture.md](../docs/design/zkvm-library-architecture.md)

## Goal

Make the proof stack boundary explicit and enforce it in code:

1. `tabula-machine` becomes a pure backend
2. `tabula-runtime` owns proof-plan resolution and per-column proof-input assembly
3. `tabula-witness` separates generic trace infrastructure from builtin lowering helpers
4. `tabula-stark` stops exposing a competing public gadget namespace

---

## Tasks

### PB-0: Lock the Design Contract ✅

- [x] Add canonical boundary contract in `docs/design/proof-backend-contract.md`
- [x] Link existing architecture docs back to the canonical contract
- [x] Record this cleanup in `tasks/todo.md`

### PB-1: Purify `tabula-machine` ✅

- [x] Narrow `ProofColumn` to backend metadata and chip construction only
- [x] Remove `Prover::build_column_stores()` from `tabula-machine`
- [x] Stop retaining proof-column objects in `MachineSetup`
- [x] Remove machine-level public re-exports of property-query facade types
- [x] Verify `tabula-machine` public API no longer exposes `BatchWitness`, `ColumnWitness`, or `PropertyReadRecord`

### PB-2: Move Proof-Input Assembly to `tabula-runtime` ✅

- [x] Add runtime-side `ProofInputBuilder`
- [x] Extend `ColumnViews` to carry runtime, machine, and proof-input views
- [x] Extend `ResolvedColumnViews` to store proof columns and proof-input builders per `(table_id, col_id)`
- [x] Update builtin schemes to construct both backend and proof-input views
- [x] Ensure `runtime::proving` assembles per-column witness stores without calling into `machine`

### PB-3: Reorganize `tabula-witness` ✅

- [x] Keep generic orchestration in `tabula_witness::trace`
- [x] Move builtin lowering under `tabula_witness::trace::builtin`
- [x] Rename `TraceBuilder` to `BuiltinTraceBuilder`
- [x] Return property-read extraction as explicit builtin output, not shared-store label coupling
- [x] Stop root re-exporting builtin lowering helpers as generic proof infrastructure

### PB-4: Unify the Gadget Boundary ✅

- [x] Keep `tabula-gadgets` as the only public gadget namespace
- [x] Move protocol-facing gadget internals in `tabula-stark` under `air::primitives`
- [x] Update bus macros and gadget re-exports to use the internal path
- [x] Remove `pub mod gadgets;` from `tabula-stark`

### PB-5: Tighten Docs and Public Surfaces ✅

- [x] Make `tabula-runtime` the obvious default prove/verify path in docs/examples
- [x] Document `tabula-machine` as advanced/backend usage only
- [x] Mark standalone verifier work as explicitly deferred until after this cleanup

### PB-6: Add Guardrails ✅

- [x] Add architecture test for forbidden crate dependencies using `cargo metadata`
- [x] Add runtime-only prove/verify smoke test
- [x] Add backend smoke test from prepared proof inputs
- [x] Add regression tests for `ColumnViews` / `ProofInputBuilder` resolution and store assembly

### PB-7: Verification ✅

- [x] `cargo test -p tabula-machine --tests`
- [x] `cargo test -p tabula-witness --tests`
- [x] `cargo test -p tabula-runtime --features prove`
- [x] `cargo test -p tabula-daemon --features stark`
- [x] `cargo test --workspace`
- [x] `cargo clippy --workspace --all-targets`

---

## Completion Criteria

- [x] `tabula-machine` is backend-only and no longer assembles proof inputs
- [x] `tabula-runtime` is the sole owner of proof planning and per-column proof-input assembly
- [x] generic witness APIs no longer imply builtin chip semantics
- [x] `tabula-stark` no longer exposes a public gadget module
- [x] architecture guardrails fail on forbidden dependency regressions

---

## Deferred Follow-Up

- [ ] Standalone verifier surface (`verify-stark` or equivalent) after PB-0 through PB-7 are complete and stable

## Verification

```bash
cargo test -p tabula-machine --tests
cargo test -p tabula-witness --tests
cargo test -p tabula-runtime --features prove
cargo test -p tabula-daemon --features stark
cargo test --workspace
cargo clippy --workspace --all-targets
```
