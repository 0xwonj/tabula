# SP-1 — Contract Wire-Type Consolidation

> Status: design, ready for ultraplan
> Date: 2026-04-18
> Umbrella: [2026-04-18-architecture-refactoring-design.md](2026-04-18-architecture-refactoring-design.md)

Sub-project 1 of the architecture refactoring. Makes `tabula-contract`
the sole wire-type authority with a minimal dep set, and introduces
the canonical `public_statement_from_record` entry point.

---

## 1. Goal

After this sub-project:

- `PublicStatement` (struct + error) is defined in `tabula-contract`.
- `tabula-contract`'s Cargo dependencies are `tabula-core` and
  `tabula-commitment` only; no `tabula-stark`, no `tabula-ir`.
- `tabula-stark` (and any other consumer) imports `PublicStatement`
  from `tabula-contract`.

The canonical `public_statement_from_record(artifact, record)` entry
point is deferred: `ExecutionRecord` does not yet exist, and the
current runtime materializer depends on `ProofJournal` /
`PublicStatementMaterialization` (runtime-internal `pub(crate)`)
plus `tabula-types` registries. Both would have to move to satisfy
SP-1's dep-set goal, which is out of scope here. The API is
reassigned to whichever later SP introduces `ExecutionRecord` —
likely SP-4.

Nothing else in the code is allowed to change behaviour. Byte-identical
proofs before and after SP-1 on `basic` and `membership` examples.

---

## 2. Resolved Open Decisions

Both open items from the umbrella spec (§7) are resolved by pre-SP
investigation:

### 2.1 `contract → commitment` — **KEEP**

Contract's use of `tabula-commitment` is exclusively native primitives:
`PoseidonHasher`, `NativeDigest`, `FieldHasher`. Commitment's
`primitives/` module is already cleanly separated from proof-side
schemes. Extracting those primitives into `tabula-core` would force
`p3-koala-bear` and related crates into core, which is a larger and
riskier move for no architectural gain.

**Action:** keep the dep; document commitment's role as a shared
foundation in the crate README.

### 2.2 `contract → ir` — **REMOVE**

Contract imports only four IR vocabulary types (`ProgramId`,
`ContextFieldId`, `EntryId`, `EventId`) — all `u32`-wrapper
identifiers with no logic. They are infrastructure, not IR semantics.

**Action:** move those four IDs into `tabula-core`. `tabula-ir`
re-exports them from core for source-compatibility; contract imports
from core. Drop `tabula-ir` from contract's Cargo dependencies.

---

## 3. Migration Sequence

Ordered so each step keeps `cargo build --workspace` green. Each step
is one reviewable unit (could be one commit or one PR; decided at
ultraplan).

### Step 0 — Capture pre-SP-1 reference proof bytes

- Run `example → execute → prove → verify` for both `basic` and
  `membership` on the current tree, recording the produced
  `proof.bin` under a scratch path (not committed).
- These bytes are the fixed point that steps 1–3 must match on each
  completion check. Without them, "byte-identical proofs" is
  unverifiable.
- Note: the `tabula-commitment` feature-flag question
  (`features = ["stark"]`) is **out of scope for SP-1** and deferred
  to SP-6's dep audit.

### Step 1 — Move IR vocabulary IDs into `tabula-core`

- Move type definitions `ProgramId`, `ContextFieldId`, `EntryId`,
  `EventId` from `crates/ir/src/model/ids.rs` to
  `crates/core/src/ids.rs` (or the existing ID module if one exists).
- Preserve derive set (Debug, Clone, Copy, Eq, Hash, Serialize,
  Deserialize — whatever is current), canonical encodings, and
  `Display` impls.
- `tabula-ir` re-exports from core:
  `pub use tabula_core::{ProgramId, ContextFieldId, EntryId, EventId};`
  so every existing `tabula_ir::ProgramId` call site still compiles.
- No contract change yet.
- **Verify:** `cargo build --workspace` and the full test suite
  remain green.

### Step 2 — Contract consumes IDs from core

- Rewrite contract's two import sites
  (`crates/contract/src/verification.rs:8` and
  `crates/contract/src/format/public_statement_transcript.rs:9`) to
  import the IDs from `tabula_core` instead of
  `tabula_ir as ir`.
- Drop `tabula-ir` from `crates/contract/Cargo.toml` dependencies.
- **Verify:** contract tests green; workspace green.

### Step 3 — Move `PublicStatement` + `PublicStatementError` into contract

- Create `crates/contract/src/public_statement.rs` (name TBD, could
  fit into existing `verification.rs` or a new module; ultraplan
  decides). Move the struct, the error enum, `to_field_elements`,
  `from_field_elements` verbatim from
  `crates/stark/src/air/statement.rs` into contract.
- `crates/contract/src/lib.rs` replaces
  `pub use tabula_stark::air::statement::PublicStatement;` with a
  `pub use` from the new contract module.
- `tabula-stark` adds `tabula-contract = { workspace = true }` to
  its Cargo dependencies.
- `crates/stark/src/air/statement.rs` is **deleted** (not kept as a
  re-export). Per `.claude/CLAUDE.md` clean-break posture, every
  caller migrates to `tabula_contract::PublicStatement`; no compat
  shim remains in stark. If any non-`PublicStatement` item lives in
  that module today, move it inline at the single caller before
  deleting the file.
- Every downstream crate that does
  `use tabula_stark::air::statement::PublicStatement` is rewritten
  to `use tabula_contract::PublicStatement`. (Callers enumerated
  below in §4.)
- Drop `tabula-stark` from `crates/contract/Cargo.toml`.
- **Verify:** workspace green; `basic` / `membership` CLI flow
  produces byte-identical proof bytes.

### Step 4 — Documentation & invariant declaration

- Update `crates/contract/README.md`: declare that contract is the
  wire-type authority; document the allowed dep set (`tabula-core`
  + `tabula-commitment`); document `public_statement_from_record`
  as the canonical derivation function.
- Update `crates/commitment/README.md` (if present): note the
  shared-foundation status for contract consumption.
- Optional: add a `tools/check-layer-boundaries` script seed. Full
  script is deferred to SP-6; we just prepare the invariant
  statement.

No production code changes in step 4.

---

## 4. Ripple — Call Sites to Rewrite

Expected edits in step 3:

| Crate | File | Current import | New import |
|-------|------|----------------|------------|
| machine | `src/lib.rs:39` | `pub use tabula_stark::air::statement::PublicStatement` | `pub use tabula_contract::PublicStatement` |
| machine | `src/proof/codec.rs:17` | `use tabula_stark::air::statement::PublicStatement` | `use tabula_contract::PublicStatement` |
| machine | `src/proof/model.rs:8` | same | same |
| machine | `src/proof/transcript.rs:11` | same | same |
| runtime | `src/semantics.rs:9` | same | same |
| runtime | `src/verifier.rs:12` | same | same |
| runtime | `src/engine.rs:19` | same | same |
| runtime | `src/lib.rs:65` | re-export | re-export (no path change needed if it already re-exports from contract) |
| sdk | (inspect during step 3) | likely re-export | re-export from contract |
| contract | `src/lib.rs:33` | re-export from stark | local `pub use` |
| contract | `src/verification.rs:9` | from stark | local `use` |

All call sites are in-workspace; external consumers do not exist yet.

---

## 5. Completion Criteria

Hard gates (all must pass):

1. `cargo tree -p tabula-contract` shows only `tabula-core` and
   `tabula-commitment` from the workspace.
2. `grep -r 'use tabula_ir' crates/contract/src/` returns no matches.
3. `grep -r 'use tabula_stark' crates/contract/src/` returns no
   matches.
4. `grep -r 'tabula_stark::air::statement' crates/` returns no
   matches (the module is deleted, no re-export shim remains).
5. `tabula-stark` appears in `crates/stark/Cargo.toml` as having
   `tabula-contract` as a dependency (inversion confirmed).
6. `cargo build --workspace` succeeds.
7. `cargo test --workspace` passes without changes to test logic.
8. End-to-end CLI flow on both `basic` and `membership`:
   `example → execute → prove → verify` succeeds, and the resulting
   `proof.bin` bytes are identical to the step-0 reference capture.
9. `crates/contract/README.md` documents the post-SP-1 dep set and
   wire-type authority.

No soft gates. `public_statement_from_record` is explicitly
out of scope (see §1).

---

## 6. Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Moving `PublicStatement` changes its byte layout inadvertently (derives, field order) | Low | High — proofs diverge | Verbatim move; diff-check the moved file vs. original; proof byte equality test in step 3's PR |
| IR ID move breaks an `unsafe` cast or serde rename assumption | Low | Medium | Check IR tests that round-trip IDs; preserve derives exactly |
| A call site outside our audit list still does `tabula_stark::air::statement::PublicStatement` | Low | Low — build error surfaces it | Step 3's completion criteria include the grep check |
| Removing `tabula-ir` from contract breaks feature-flag matrix (ir has features) | Low | Low | Check `crates/contract/Cargo.toml` before drop; if a feature was previously enabling ir, inspect it |

---

## 7. Out of Scope

- Introducing `public_statement_from_record` or defining
  `ExecutionRecord`. `ExecutionRecord` does not exist in code today;
  the current materializer depends on runtime-internal `ProofJournal`
  / `PublicStatementMaterialization` and `tabula-types` registries.
  Moving that into contract would require either pulling those
  types/deps along (breaking the "core + commitment only" goal) or
  designing `ExecutionRecord` here — both out of SP-1's scope.
  Reassigned to whichever later SP introduces `ExecutionRecord`
  (likely SP-4).
- Re-examining `tabula-commitment`'s `features = ["stark"]` usage
  from contract — deferred to SP-6's dep audit.
- Any machine-side work: `TabulaProof` cleanup, `BackendProver`,
  `PreparedMachineInput` restructuring — all SP-2.
- Any runtime-side restructure: `PreparedProver`, engine.rs split —
  SP-4 / SP-5.
- SDK / CLI changes beyond the import-rewrite mechanics.

---

## 8. References

- Umbrella design:
  [`2026-04-18-architecture-refactoring-design.md`](2026-04-18-architecture-refactoring-design.md)
- Canonical architecture:
  [`docs/design/architecture.md`](../../design/architecture.md)
- Current `PublicStatement` definition:
  `crates/stark/src/air/statement.rs:14-25`
- Current contract import sites:
  `crates/contract/src/lib.rs:33`,
  `crates/contract/src/verification.rs:9`,
  `crates/contract/src/format/public_statement_transcript.rs:7-9`
- Current runtime statement builder (stays runtime-internal in SP-1):
  `crates/runtime/src/semantics.rs::build_public_statement_from_journal`
