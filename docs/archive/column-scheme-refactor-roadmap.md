# Column Scheme Refactor Roadmap

> **Date**: 2026-03-19
> **Status**: Draft
> **Scope**: Native-field, hash-based column schemes only (`SSMC`, `SMT`, indexed Merkle variants, orderbook trees, Merkle/FRI-like backends)
> **Out of scope**: `KZG`, `IPA`, `Verkle`, pairing-based or foreign-field verifier backends

---

## 1. Recommendation

Do **not** fully plan and implement Phase 1 and Phase 2 as one undifferentiated refactor.

Use this sequence instead:

1. **Phase 1**: Introduce the new shared seam and keep compatibility with the current witness pipeline.
2. **Phase 2**: Remove generic concrete-state handling and move to canonical `ColumnProofInput`.

This keeps the first step small enough to validate architecture without forcing a full witness/root rewrite in the same change set.

---

## 2. Target Architecture

The public column-scheme seam should remain coarse-grained:

- `ColumnSchemeFactory`
- `ColumnViews`
- `RuntimeColumn`
- `ColumnStateBackend`
- `ProofColumn`

`ColumnViews` remains the assembly-time bundle that ties runtime and machine views together for one `(table, col)` pair.

Internally, `ColumnStateBackend` may be composed from:

- `StateModel`
- `CommitmentBackend`

That internal split is for backend authors and framework internals. It should not become the main public entrypoint.

---

## 3. Phase 1

### 3.1 Goal

Add the missing scheme-owned transition seam without rewriting the generic witness model yet.

### 3.2 Changes

- Extend `ColumnViews` so it can carry:
  - `RuntimeColumn`
  - `ColumnStateBackend`
  - `ProofInputBuilder`
  - `ProofColumn`
- Add `ColumnStateBackend` as the owner of:
  - base-state materialization
  - write application
  - `ColumnMeta` creation
  - scheme transition artifact production
- Keep `ProofInputBuilder` temporarily as a compatibility adapter.
- Open `ColumnChipSet` so column schemes can provide scheme-owned bus consumers:
  - `airs`
  - `dyn_chips`
  - `bus_consumers`
- Update runtime materialization/program state so the runtime stores per-column backends directly.

### 3.3 Why stop here first

Phase 1 validates the architecture boundary with low blast radius:

- runtime/machine separation stays intact
- built-in `SSMC` and `SMT` can be migrated incrementally
- future native hash-based schemes have a correct insertion point
- the old witness path still works while the new seam is proven out

### 3.4 Exit Criteria

- `ColumnViews` carries the new backend seam.
- Runtime keeps per-column `ColumnStateBackend` instances.
- Column-tier setup accepts per-scheme bus consumers.
- `SSMC` and `SMT` both implement the new seam.
- No new hardcoded `if scheme == ...` branches appear in shared orchestration.

---

## 4. Phase 2

### 4.1 Goal

Replace the generic concrete-state witness model with canonical per-column proof input.

### 4.2 Required Refactor

This is the large refactor that should be recorded now, but planned in detail only after Phase 1 lands.

The following must be removed from the shared witness/root path:

- `ColumnWitness.old_state`
- `ColumnWitness.new_state`
- `ColumnWitness.merge_trace`
- closed shared `ColumnState`
- any generic witness/root code that directly depends on concrete built-in state layouts

The replacement center is:

```rust
struct ColumnProofInput {
    meta: ColumnMeta,
    witness_store: WitnessStore,
}
```

### 4.3 Affected Areas

- `crates/witness/src/witness/types.rs`
- `crates/commitment/src/column_meta.rs`
- `crates/witness/src/witness/generator.rs`
- `crates/witness/src/witness/encoding.rs`
- `crates/witness/src/trace/memory/state.rs`
- `crates/witness/src/trace/smt.rs`

### 4.4 Why not fully plan Phase 2 now

Because Phase 2 should be planned against the actual Phase 1 seam that lands in code, not against an assumed interface that may still shift.

What should be recorded now:

- the target canonical input shape
- the concrete files and leak points
- the non-goals
- the migration gate for starting Phase 2

What should be deferred until after Phase 1:

- exact type signatures
- exact migration order
- temporary compatibility shims
- final test matrix

### 4.5 Phase 2 Start Gate

Do not start Phase 2 until all of the following are true:

- `ColumnStateBackend` is live in runtime materialization
- `SSMC` and `SMT` are both using the seam
- per-column bus consumers are supported
- there is no ambiguity about where `ColumnMeta` is produced

---

## 5. Native Scheme Coverage

The architecture above is intended to cleanly support:

- `SSMC`
- `SMT`
- indexed Merkle trees
- orderbook / augmented trees
- Merkleized polynomial state
- FRI-like hash-based column backends

It is intentionally **not** trying to fully solve:

- `KZG`
- `IPA`
- `Verkle`
- pairing-based verifier gadgets
- foreign-field heavy proof verification

Those require a separate architecture discussion around proof-service extensions and non-digest commitment objects.

---

## 6. Working Rule

Use this roadmap as the process rule:

- **Phase 1** gets a detailed implementation plan and immediate execution.
- **Phase 2** stays as a recorded design memo until Phase 1 is complete.
- After Phase 1, write a fresh Phase 2 implementation plan based on the landed seam, not on pre-refactor assumptions.

