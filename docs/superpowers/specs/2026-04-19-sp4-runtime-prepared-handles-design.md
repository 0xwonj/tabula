# SP-4 — Runtime Symmetric Prepared Handles (Design)

**Status:** Design.
**Umbrella:** `docs/superpowers/specs/2026-04-18-architecture-refactoring-design.md`
§4 SP-4.
**Depends on:** SP-1 (wire types in contract, landed in
constrained form), SP-2 (machine backend primitive split, designed),
SP-3 (witness chip-kit, landed).

---

## 1. Goal

`tabula-runtime` exposes two symmetric prepared handles —
`PreparedProver` and `PreparedVerifier` — with matching construction
shape and matching "prepare once, drive many" semantics.

After SP-4:

- `prepare_prover(registered_program) -> Result<PreparedProver,
  RuntimeError>` is the canonical way to get a prove-capable handle
  in the runtime.
- `prepare_verifier(registered_program) -> Result<PreparedVerifier,
  RuntimeError>` (replacing today's `Verifier::builder().build()`
  path) is the canonical way to get a verify-capable handle.
- `PreparedVerifier::verify(&self, proof, expected_public_statement)
  -> Result<BoundStatement, VerifyError>` is the single verify entry
  point through the runtime.
- `PreparedProver::prove(&self, ProveInput) -> Result<ProveResult,
  ProveError>` is the single prove entry point; it threads a fresh
  `KitScratch` per call (see §2.5).
- Both handles are `Send + Sync` and cheap to share via `Arc`.
- `VerifierState` is a named public type; there is no longer a
  privately-scoped "prepared shape" hidden behind a builder.
- `TabulaRuntime` survives *only* as a thin execute-only facade for
  the verify-feature surface it uniquely provides; its `prove`
  entry is removed. See §2.1.

SP-4 does **not** decompose `engine.rs` (SP-5) and does **not**
rename `RegisteredProgram` to `SealedArtifact` (SP-1 follow-up).

---

## 2. Resolved Open Decisions

SP-4 closes the following decisions up front so implementation
stages don't re-litigate them.

### 2.1 Execute-only surface fate — `TabulaRuntime` survives as execute-only facade

`TabulaRuntime` today carries a mixed surface: `prove` (prove
feature) plus a bundle of verify-feature execute and snapshot
helpers — `execute_batch`, `execute_query`, `execute_batch_receipt`,
`materialize_logical_state`, `decode_committed_snapshot`,
`project_logical_state`, `empty_state_snapshot`.

These execute-surface methods **cannot** move onto `PreparedProver`,
which is a prove-only handle. They also do not belong on
`PreparedVerifier`, whose single responsibility is verify. Rather
than invent a third `PreparedExecutor` handle with no expressive
payoff, SP-4:

- Removes `TabulaRuntime::prove`. Prove callers migrate to
  `PreparedProver::prove`.
- Keeps `TabulaRuntime` as a verify-feature execute-only facade
  that owns the same prepared state `PreparedVerifier` owns
  (conceptually: a `PreparedExecutor`-shaped thing spelled
  `TabulaRuntime`).
- Leaves the final disposition of this facade to SP-5, which may
  fold it into a cleaner execute module once `engine.rs` is
  decomposed.

**Rationale.** The execute surface is real and has call sites;
deleting it in SP-4 would exceed scope and collide with SP-5. A
two-handle + execute-facade shape lets SP-4 land the prove/verify
symmetry without touching the execute surface.

### 2.2 Input type stays `RegisteredProgram`

The umbrella's forward-looking "`SealedArtifact`" naming is an
SP-1 follow-up. Today's compiler output is `RegisteredProgram` and
that is what the runtime consumes. SP-4 passes
`RegisteredProgram` (by value) into both `prepare_prover` and
`prepare_verifier`. Renaming to `SealedArtifact` is explicitly out
of scope.

### 2.3 Builder knobs retained, not pruned

`VerifierBuilder` today exposes four knobs:

- `with_host_environment`
- `with_machine_stark_config`
- `with_root_backend_bundle` (prove feature)
- `with_root_proof_backend` / `with_root_proof_backend_arc`
  (verify-only feature)

Grepping call sites shows every knob has real consumers across
`crates/sdk`, `crates/runtime/src/bootstrap`, `crates/machine/tests`,
and `crates/ext`. SP-4 therefore preserves all four knobs on the
renamed builders. Knob pruning is deferred to SP-6 when the SDK's
construction path is restructured end-to-end.

### 2.4 Handle sharing: `Arc`-friendly, `Send + Sync` from day one

Both `PreparedProver` and `PreparedVerifier` are
`Send + Sync`. The SDK and any future concurrent drivers can hold
them behind `Arc<PreparedProver>` / `Arc<PreparedVerifier>` without
wrapping them in their own synchronization. This is cheaper to
commit to now than to retrofit in SP-6 when the SDK cache layer is
removed.

Practically: no interior mutability, no `RefCell`, no
non-`Sync` runtime types on the prepared-state struct. Today's
`VerifierState` already satisfies this; the invariant is restated
here so S1/S2 implementers don't break it.

### 2.5 SP-3 boundary is load-bearing

Quoting the SP-3 Landed amendment verbatim (see
`2026-04-19-sp3-witness-chip-kit-design.md` "SP-4 boundary left by
SP-3"):

> *Prepared-once, SP-4-hoistable.* Backend selection,
> `ChipKitRegistry` construction, and AIR wiring. The registry
> builds from each configured `ExecutionBackend`'s `witness_kits()`
> and does not depend on batch inputs, so it can move onto a
> `PreparedProver` handle without witness-crate changes.
>
> *Still eager per-batch.* `KitScratch` allocation, runtime
> pre-stuff of relation-table and transcript rows, and
> `prepare_execution_store` itself. These depend on the batch's
> execution trace and relation-proof output and cannot be
> preallocated. SP-4's `PreparedProver` must therefore surface the
> prepared registry/backends but still thread a fresh `KitScratch`
> per prove call.

**Per-prove contract.** `PreparedProver::prove(&self, input)` must:

1. Allocate a fresh `KitScratch` owned by the prove call (not by
   the handle).
2. Let opcode lowering push rows into that `KitScratch` as it
   executes (unchanged from today).
3. Runtime pre-stuffs relation-table / transcript-family rows for
   this batch into the `KitScratch` before `prepare_execution_store`.
4. Call `prepare_execution_store(&mut lowering, &self.kit_registry)`
   with the prepared-once registry reference, draining the
   per-batch scratch.

The `&self` signature on `prove` is therefore load-bearing: the
prepared state is shared-read, and all per-batch mutable state
lives in locals. This keeps `Send + Sync` (§2.4) honest.

### 2.6 `VerifierState` promoted to public type

`VerifierState` becomes `pub struct` with documented fields
(context, relation policy, machine). `PreparedVerifier` wraps it;
`PreparedProver` wraps the same prepared state plus the
`ChipKitRegistry` and prove-only backend bundle. Exposing
`VerifierState` directly gives downstream consumers (SDK, tests)
a type to name without going through a builder.

---

## 3. Shape of Change

### 3.1 Types

```rust
// tabula-runtime, verify feature
pub struct VerifierState {
    pub context: ArtifactContext,
    pub relation_policy: RelationPolicy,
    pub machine: TabulaMachine,
}

// verify feature
pub struct PreparedVerifier {
    state: VerifierState,
    // verify-only root backend (Arc<dyn ...> under !prove)
    root_proof_backend: RootProofBackend,
}

pub struct PreparedVerifierBuilder { /* same knobs as today */ }

pub fn prepare_verifier(
    registered: RegisteredProgram,
) -> Result<PreparedVerifier, RuntimeError>;

impl PreparedVerifier {
    pub fn verify(
        &self,
        proof: &TabulaProof,
        expected: &PublicStatement,
    ) -> Result<BoundStatement, VerifyError>;
    pub fn state(&self) -> &VerifierState;
}

// prove feature only
pub struct PreparedProver {
    state: VerifierState,
    kit_registry: ChipKitRegistry,
    root_backend_bundle: RootBackendBundle,
    // everything else RuntimeBuilder::build resolves prepared-once
}

pub struct PreparedProverBuilder { /* mirrors VerifierBuilder */ }

pub fn prepare_prover(
    registered: RegisteredProgram,
) -> Result<PreparedProver, RuntimeError>;

impl PreparedProver {
    pub fn prove(
        &self,
        input: ProveInput,
    ) -> Result<ProveResult, ProveError>;
    pub fn state(&self) -> &VerifierState;
}
```

Both prepared handles are `Send + Sync` (§2.4).

### 3.2 `TabulaRuntime` after SP-4

- `TabulaRuntime::prove` is removed.
- `TabulaRuntime` remains a verify-feature struct holding the
  execute surface: `execute_batch`, `execute_query`,
  `execute_batch_receipt`, `materialize_logical_state`,
  `decode_committed_snapshot`, `project_logical_state`,
  `empty_state_snapshot`.
- `RuntimeBuilder` keeps building `TabulaRuntime` for execute
  callers; it no longer produces a prove-capable handle.
- Final disposition of `TabulaRuntime` — keep, rename to
  `PreparedExecutor`, or fold into a runtime execute module — is
  deferred to SP-5.

### 3.3 SDK / CLI migration

- SDK prove path switches from `TabulaRuntime::prove` to
  `prepare_prover(...)` + `PreparedProver::prove(...)`.
- SDK verify path switches from `Verifier::builder().build()` to
  `prepare_verifier(...)`.
- CLI follows. No new semantics; only call-site renames.
- SDK internal caches (`Mutex<BTreeMap<...>>`) untouched — that
  rework is SP-6.

### 3.4 Error types

- `prepare_prover` and `prepare_verifier` return
  `Result<_, RuntimeError>` (existing runtime error enum). No new
  error hierarchy.
- `PreparedProver::prove` keeps today's `ProveResult` /
  `ProveError` shape unchanged.

---

## 4. Stages

Each stage compiles, tests, and proves the `basic` + `membership`
examples byte-identically before the next stage begins (see §5).

### S1 — Expose `VerifierState`; rename `Verifier` → `PreparedVerifier`

- Promote `VerifierState` to a public struct with documented fields.
- Rename `Verifier` → `PreparedVerifier`, `VerifierBuilder` →
  `PreparedVerifierBuilder`, `Verifier::verify_public_statement` →
  `PreparedVerifier::verify`.
- Add free-function constructor `prepare_verifier(registered)` as
  sugar over `PreparedVerifierBuilder::new(registered).build()`.
- Update `crates/runtime/src/lib.rs` public re-exports.
- Propagate renames through SDK/CLI/tests mechanically.

No semantic change.

### S2 — Introduce `PreparedProver` + `prepare_prover`

- Factor `RuntimeBuilder::build`'s prepared-once portion
  (program resolution, machine construction, context, relation
  policy, backend bundle, chip-kit registry) into a shared
  helper that both `TabulaRuntime::builder().build()` and
  `prepare_prover(...)` call.
- Construct `PreparedProver` from that helper's output.
- Move the body of today's `TabulaRuntime::prove` onto
  `PreparedProver::prove(&self, ProveInput)`. Ensure per-call
  `KitScratch` allocation + runtime pre-stuff happens inside
  `prove`, not at handle construction (§2.5).
- Have `TabulaRuntime::prove` temporarily delegate to
  `self.as_prepared_prover().prove(input)` to keep S2 free of
  call-site churn.

### S3 — Migrate SDK/CLI call sites

- SDK prove path → `prepare_prover` + `PreparedProver::prove`.
- SDK verify path → `prepare_verifier` + `PreparedVerifier::verify`.
- CLI updates its constructors to match.
- Confirm every external consumer goes through the prepared
  handles.

### S4 — Remove `TabulaRuntime::prove`; shrink facade

- Delete `TabulaRuntime::prove` (all callers migrated in S3).
- Narrow `TabulaRuntime` to the execute-only surface (§3.2).
- `RuntimeBuilder` keeps building it; prove knobs stay available
  on `PreparedProverBuilder` instead.

### S5 — Documentation

- Update `crates/runtime/README.md` with the two-handle shape.
- Update `docs/design/architecture.md` runtime section.
- Cross-link SP-4 from umbrella; mark SP-4 Landed.
- Update or remove stale references in `docs/notes/` that assume
  `Verifier` / `TabulaRuntime::prove`.

---

## 5. Verification

At every stage boundary (S1–S4):

- `cargo fmt --check`
- `cargo clippy --workspace --all-features --all-targets -- -D warnings`
- `cargo test --workspace --all-features`
- End-to-end prove + verify on `basic` and `membership` examples
  produces **byte-identical** proofs against the pre-SP-4
  reference. Determinism is the primary guard that renaming and
  handle restructuring did not perturb any hash input, witness
  column, or transcript ordering.
- For S2 specifically: confirm `PreparedProver::prove` called
  twice on the same handle with the same `ProveInput` produces
  byte-identical proofs (tests the `&self` + per-call
  `KitScratch` contract from §2.5).

S5 additionally: `cargo doc --workspace --no-deps` builds without
new warnings; `missing_docs` respected on new public items.

---

## 6. What This Does Not Do

- **No `engine.rs` decomposition.** The file stays at ~3kLOC; SP-5
  owns splitting it.
- **No `RegisteredProgram` rename.** The `SealedArtifact` naming
  is an SP-1 follow-up.
- **No SDK cache removal.** `Mutex<BTreeMap<...>>` and
  `NEXT_ENVIRONMENT_FINGERPRINT` stay; SP-6 owns that.
- **No wire-format changes.** `TabulaProof`, `PublicStatement`,
  and all committed artifacts stay bit-compatible.
- **No builder-knob pruning.** All four knobs survive per §2.3.
- **No new error hierarchy.** Existing `RuntimeError` /
  `ProveError` / `VerifyError` are reused.
- **No execute-surface rework.** `TabulaRuntime`'s execute methods
  are preserved verbatim and relocated only if trivially required.

---

## 7. Open Decisions (Deferred)

- **TabulaRuntime facade disposition.** SP-4 keeps it as an
  execute-only facade; SP-5 decides whether to rename it
  `PreparedExecutor`, fold it into an execute module, or leave it
  in place.
- **`prepare_verifier` vs `PreparedVerifier::builder`.** SP-4
  ships both (free-fn as sugar, builder for knob-carrying
  construction). Whether the builder is eventually deleted in
  favor of an options struct is an SP-6 call.
- **Prepared-handle lifetime in SDK.** Whether the SDK caches
  `Arc<PreparedProver>` per-program, constructs per-call, or does
  something in between is SP-6 scope. SP-4 only guarantees the
  handles are `Send + Sync` and cheap to share.

---

## 8. Risks

- **Silent perturbation of the prove hot path.** Factoring
  `RuntimeBuilder::build` into a shared helper risks moving work
  between prepared-once and per-prove phases. The byte-identical
  determinism gate (§5) is the primary tripwire; S2 should also
  add a micro-benchmark run on `basic` to confirm prove time has
  not regressed more than noise.
- **`Send + Sync` regression.** Adding a non-`Sync` field to
  `PreparedProver` later would quietly break SDK sharing. Add a
  `static_assertions::assert_impl_all!(PreparedProver: Send, Sync)`
  alongside the struct definition.
- **Call-site churn.** S1 renames touch many files. Mechanical;
  mitigated by doing the rename in a single commit with no
  semantic edits.
- **Drift from SP-3 contract.** If S2 accidentally hoists
  `KitScratch` or relation pre-stuff onto the handle, per-batch
  determinism breaks. The S2 determinism check (two proves on
  the same handle) is the tripwire.

---

## SP-4 Landed Notes (2026-04-19)

Implementation landed per stages S0–S5. Summary of material deviations
from the original spec §3/§4:

- `PreparedVerifier::verify` returns `Result<BoundStatement, RuntimeError>`
  rather than spec §1's `Result<BoundStatement, VerifyError>`; there
  is no separate `VerifyError` type — `RuntimeError` continues to
  cover both prove and verify errors per spec §3.4's "No new error
  hierarchy." This matches `PreparedProver::prove`'s
  `Result<ProveResult, RuntimeError>`.
- The SDK's internal method is spelled
  `Sdk::prepare_prepared_prover` / `prepare_prepared_verifier` to
  avoid collision with the runtime free functions
  `tabula_runtime::prepare_prover` / `prepare_verifier` at every
  import site. External SDK interop surface exposes both under the
  natural names `interop::prepare_prover` / `interop::prepare_verifier`.
- `TabulaRuntime` lost its `root_backend_bundle` field entirely in S4
  (not just its `prove` method). The execute surface never consumed
  this field, so carrying it would have been dead state.
- `PreparedRuntimeState` was introduced as a `pub(crate)` newtype
  factored from the old private `RuntimeProgramState`. Both
  `TabulaRuntime` and `PreparedProver` embed it by value.

### Known follow-ups

- `prepare_verifier` vs the verifier builder — SP-4 ships both. SP-6
  call on whether to drop the builder in favor of an options struct.
- `TabulaRuntime` facade disposition (rename / fold into execute
  module / leave) — SP-5 owns.
- Stale `docs/notes/*.md` references that pre-date SP-4 may still use
  `Verifier` / `TabulaRuntime::prove`; authority lives in `docs/design/`
  and crate READMEs now.
