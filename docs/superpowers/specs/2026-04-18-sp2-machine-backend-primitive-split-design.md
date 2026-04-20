# SP-2 — Machine Backend Primitive Split

> Status: Landed 2026-04-19 (polish follow-up 2026-04-20)
> Date: 2026-04-18
> Umbrella: [2026-04-18-architecture-refactoring-design.md](2026-04-18-architecture-refactoring-design.md)
> Predecessor: [2026-04-18-sp1-contract-wire-type-consolidation-design.md](2026-04-18-sp1-contract-wire-type-consolidation-design.md)

Sub-project 2 of the architecture refactoring. Turns `tabula-machine`
into a pure backend primitive whose public surface is the envelope-level
`BackendProver` / `BackendVerifier` pair, strips the embedded wire types
from machine-owned structs, and resolves the `witness → executor`
dependency that keeps the witness layer below its consumers.

---

## 1. Goal

After this sub-project:

- `tabula-machine` exposes `BackendProver::prove_envelope` and
  `BackendVerifier::verify_envelope` as the canonical envelope-level
  entry points. The existing `TabulaMachine::{prove, verify}` pair
  either becomes a thin internal helper or is removed in favour of the
  new primitives.
- `TabulaProof` no longer carries `PublicStatement` as a field.
- `PreparedMachineInput` no longer carries `PublicStatement` as a field.
- `tabula-machine` re-exports no wire types. Callers import
  `PublicStatement`, `BoundStatement`, `ProofEnvelope`, etc. directly
  from `tabula-contract`.
- Machine's Fiat-Shamir transcript binds the statement by absorbing the
  32-byte `binding_digest` only; the redundant public-statement felt
  absorption is gone.
- `tabula-machine` dev-dependencies on `tabula-lang` and
  `tabula-executor` are removed; machine tests run against
  lower-level chip / witness fixtures.
- `tabula-witness` no longer depends on `tabula-executor`. The journal
  value-types that previously bridged the two crates live in
  `tabula-types`, which is below both.

SP-2 is the first **byte-breaking** sub-project: the on-disk
`proof.bin` layout changes (no more `AirStatementDto`; transcript
binding changes). Determinism — same inputs still produce same bytes —
is required, but the absolute bytes will differ from the SP-1
reference. A new reference is captured after SP-2 lands.

End-to-end `basic` and `membership` flows (`example → execute → prove
→ verify`) must remain green end-to-end.

---

## 2. Resolved Open Decisions

### 2.1 Umbrella §2.2 says `TabulaProof = (ArtifactId, binding_digest, ProofEnvelope)`. Resolve.

**Problem.** `tabula-contract` does not define `ArtifactId` today. The
umbrella target shape cannot be adopted literally without first
introducing a new wire type.

**Options considered:**

1. Introduce a new `ArtifactId` newtype in `tabula-contract` as part of
   SP-2, derived deterministically from the sealed artifact's
   binding hash. Carry it inside `TabulaProof` and the envelope.
2. Drop `ArtifactId` from the target shape. `binding_digest` already
   covers artifact-identity at the verification boundary: the verifier
   recomputes `binding_digest` locally from its own
   `Arc<SealedArtifact>` and its expected `PublicStatement`, and
   rejects the proof if the two digests disagree. An explicit artifact
   identifier adds a second, less-strict gate on top of the same
   property.

**Decision: option 2.** `ArtifactId` is not introduced in SP-2. The
target shape after SP-2 is:

- Wire form: `ProofEnvelope` (transport metadata + opaque `proof_bytes`).
- In-memory form: `TabulaProof { envelopes, binding_digest }` where
  `envelopes` covers execution / per-column / root sub-proofs (the
  existing `SubProofEnvelope` structure, with its chip openings
  retained so runtime introspection survives).

Rationale:

- `binding_digest` is already a collision-resistant commitment to
  `(artifact_context, public_statement)`. A separate `ArtifactId` is
  information-redundant for the live verification theorem.
- Adding a new wire type costs a full round of fail-closed versioning,
  compat tests, SDK / CLI plumbing, and docs — without earning any
  verification strength the binding digest does not already provide.
- The umbrella can be amended, not a binding contract. It is a target
  *sketch*; SP-level designs are where the shape gets decided against
  current code.

If a later SP (e.g. SP-4 or SP-6) discovers an application-level need
for a short, user-facing artifact identifier (wallet UX, log keys,
cache keys), that SP introduces `ArtifactId` in contract with its own
design. SP-2 does not prejudge that.

**Umbrella amendment.** Umbrella §2.2 is updated to say
`TabulaProof = (binding_digest, ProofEnvelope)` (plus the decoded
sub-proof envelopes needed for runtime introspection). The umbrella
edit is a same-commit doc touch-up in the SP-2 design commit.

### 2.2 Is `TabulaProof` a wire type or an in-memory type?

**Problem.** Umbrella §2.2 describes `TabulaProof` as wire-flat
`(ArtifactId, binding_digest, ProofEnvelope)`. But `tabula-runtime`'s
verifier reads per-chip public values from the decoded proof to
cross-check the `PublicStatement`:

```
crates/runtime/src/verifier.rs:197-234
    proof.execution_chip_public_values(chip_id) -> &[KoalaBear]
```

That introspection requires a decoded, type-safe view, not opaque
bytes.

**Decision.** Split the two roles explicitly:

- `ProofEnvelope` (already in contract) is the **wire form**. Transport
  metadata (`ProofSystemId`, `ProofEncodingId`, version) plus opaque
  `proof_bytes`. This is what `proof.bin` serializes to. Verifier
  clients that do not need introspection can stop here.
- `TabulaProof` (machine-owned, renamed or kept) is the **in-memory
  decoded form**. Carries `binding_digest` plus the decoded
  `SubProofEnvelope`s (execution, columns, root) with their chip
  openings, cumsum maps, etc. This is what the machine's verifier
  produces and returns so runtime-level checks can inspect chip public
  values.

Consequently:

- `BackendProver::prove_envelope(input, binding_digest) ->
  Result<ProofEnvelope, ProveError>` — returns the wire form.
- `BackendVerifier::verify_envelope(envelope, binding_digest) ->
  Result<TabulaProof, VerificationError>` — decodes, verifies, and
  returns the in-memory handle. Callers that need introspection use
  the returned `TabulaProof`; callers that only need "valid or not"
  drop it.

> **Amendment (2026-04-19):** During SP-2 implementation two small
> shape adjustments shipped:
>
> 1. `prove_envelope` takes only `input: PreparedMachineInput` (no
>    separate `binding_digest` argument). `PreparedMachineInput`
>    already carries the binding digest internally — the executor
>    computed it when preparing the input — so a second parameter would
>    be redundant and introduce a "which one wins?" footgun.
> 2. `prove_envelope` returns `(TabulaProof, ProofEnvelope)` rather
>    than just `ProofEnvelope`. The prover has the decoded proof in
>    hand after proving, and the runtime needs both the wire envelope
>    (for persistence and transport) and the decoded form (to
>    introspect chip openings during statement-level verification).
>    Returning the tuple avoids a decode round-trip on the hot path
>    without widening the verifier's API surface.

The existing lower-level `TabulaMachine::prove(input) -> TabulaProof`
and `TabulaMachine::verify(&TabulaProof)` remain as crate-internal
(or narrowly pub) helpers that `BackendProver` / `BackendVerifier`
delegate to. SP-2 does not remove them; SP-5 may re-examine.

### 2.3 Witness → Executor dep: where do the shared types land?

**Problem.** `tabula-witness/Cargo.toml:16` has `tabula-executor =
{ workspace = true }`. Umbrella §2.5 lists three options:
remove outright, or migrate shared types to contract, or migrate to a
new lower-layer home.

Concrete shared types today (pulled from
`crates/executor/src/surface/journal.rs` and
`crates/executor/src/host/property_read.rs`):

- Value-types: `TxCall`, `RelationEffect`, `RelationEffectKind`,
  `TypedStateEffect`, `StateEffectKind`, `TypedEventEffect`,
  `StatePropertyEffect`, `ContextValues`.
- Trait: `StateRuntimeView`.

These are typed execution-output types — conceptually "the decoded
value-level summary of one tx's execution". They are consumed by
witness and produced by executor, but they reference only typed
values, table IDs, column IDs, event IDs, context field IDs, typed
state keys — all of which already live in `tabula-core` and
`tabula-types`.

**Decision.** Move the shared types to **`tabula-types`**. It is the
existing home for runtime type/encoding semantics and already owns
`TypedValue`, `TypedCommittedPropertyQueryResult`, and related
primitives. This keeps witness → types (already allowed) and
executor → types (already allowed), eliminates witness → executor
entirely, and does not require creating a new crate.

Out of scope for SP-2: broader re-examination of
`tabula-executor::surface::*` module layout. We move only what
witness consumes. Anything that turns out to be executor-private after
the move stays in executor; anything that turns out to be shared but
is currently unused by witness stays where it is and gets promoted
only if a future consumer needs it.

`StateRuntimeView` is a trait that defines how the runtime exposes
state to executor hosts. Its only implementer is inside runtime; it is
consumed by executor via trait-object. It is not on the witness path.

> **Amendment (2026-04-19):** During SP-2 implementation we moved
> `StateRuntimeView` into `tabula-types` alongside the other shared
> execution-output types, rather than leaving it in executor as the
> original plan implied. Rationale: the trait composes directly with
> `TypedValue`, `CommittedKey`, and `TypedCommittedPropertyQueryResult`
> (all already in `tabula-types`) and with `ExecContext`, which needed
> a home that did not force runtime callers to depend on `tabula-executor`
> just to name the view. Putting all execution-output value types in one
> crate simplified the executor-re-export removal (S3) and eliminated the
> residual `use tabula_executor::StateRuntimeView` imports from
> `crates/runtime/src/state_runtime.rs` and `crates/compiler/**` tests.
> Witness still does not depend on `StateRuntimeView`; the move is
> motivated by the runtime/compiler/executor boundary, not witness.

---

## 3. Shape Of The Change

```
BEFORE (post-SP-1)
                                         ┌──────────────────────────────┐
                                         │ tabula-machine               │
                                         │   prove(input) -> TabulaProof│
                                         │   verify(&TabulaProof)       │
                                         │   TabulaProof {              │
                                         │     execution: SubProof…     │
                                         │     columns:   Vec<…>        │
                                         │     root:      SubProof…     │
                                         │     public_statement:        │
                                         │       PublicStatement  ◀─────┤ embedded wire type
                                         │     binding_digest:[u8;32]   │
                                         │   }                          │
                                         │   PreparedMachineInput{      │
                                         │     …, public_statement,     │
                                         │     binding_digest           │
                                         │   }                          │
                                         │   pub use PublicStatement  ──┼── re-export (SP-1 redirected to contract)
                                         │   dev-deps: lang, executor   │
                                         └──────────────────────────────┘

                                         ┌───────────────┐
                                         │ tabula-witness│  ─── depends on ───▶ ┌───────────┐
                                         │               │                      │ tabula-   │
                                         │               │                      │ executor  │
                                         └───────────────┘                      └───────────┘
                                           (witness is below executor ─ this dep is reversed)

AFTER SP-2
                                         ┌──────────────────────────────┐
                                         │ tabula-machine               │
                                         │   BackendProver::            │
                                         │     prove_envelope(          │
                                         │       input, binding_digest) │
                                         │     -> Result<ProofEnvelope> │
                                         │   BackendVerifier::          │
                                         │     verify_envelope(         │
                                         │       envelope,              │
                                         │       binding_digest)        │
                                         │     -> Result<TabulaProof>   │
                                         │   TabulaProof {              │
                                         │     execution: SubProof…     │
                                         │     columns:   Vec<…>        │
                                         │     root:      SubProof…     │
                                         │     binding_digest:[u8;32]   │
                                         │   }     (no public_statement)│
                                         │   PreparedMachineInput{      │
                                         │     …, binding_digest        │
                                         │   }     (no public_statement)│
                                         │   (no wire-type re-exports)  │
                                         │   dev-deps: chips, witness,  │
                                         │             core/test-utils  │
                                         └──────────────────────────────┘

                                         ┌───────────────┐                      ┌───────────┐
                                         │ tabula-witness│     (no dep)         │ tabula-   │
                                         │               │◀┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈┈▶ │ executor  │
                                         │               │          │           │           │
                                         └──────┬────────┘          │           └─────┬─────┘
                                                │                   ▼                 │
                                                │           ┌────────────────┐        │
                                                └─────────▶ │ tabula-types   │ ◀──────┘
                                                            │   TxCall,      │
                                                            │   RelationEffect,
                                                            │   TypedStateEffect,
                                                            │   StateEffect, …│
                                                            └────────────────┘
```

Two structural transformations, one byte-format transformation:

1. **Structural (types):** `PublicStatement` leaves
   `PreparedMachineInput` and `TabulaProof`. The statement identity is
   communicated into machine exclusively through `binding_digest`.
2. **Structural (deps):** witness → executor dep removed; shared value
   types move down to `tabula-types`.
3. **Format (wire):** `ProofDto` borsh layout drops
   `AirStatementDto`. `proof.bin` becomes shorter by exactly the
   public-statement felts' worth of bytes plus its length prefix.
   `binding_digest` stays as a top-level 32-byte field.

---

## 4. Migration Sequence

Ordered so each step keeps `cargo build --workspace` green. Each step
is a reviewable unit. Whether this lands as one commit or several is
an ultraplan decision; the SP-1 precedent (single commit) is a
reasonable default.

### Step 0 — Capture pre-SP-2 reference artifacts (not bytes)

SP-2 is byte-breaking, so SHA-256 of `proof.bin` cannot be the
equality gate this time. What is captured instead:

- SHA-256 of `proof.bin` for `basic` and `membership` *after* step 5,
  then re-run and confirm the same bytes (determinism).
- `public_statement.json` equality before and after (this stays
  stable: the runtime statement materialization path does not change).
- The `example → execute → prove → verify` end-to-end path passes on
  both examples.

Recorded paths:
- `/tmp/tabula-sp2-ref-basic/{proof.bin, public_statement.json}`
- `/tmp/tabula-sp2-ref-membership/{proof.bin, public_statement.json}`

### Step 1 — Move witness → executor shared types into `tabula-types`

- In `crates/types/src/journal.rs` (new file), define:
  `TxCall`, `RelationEffect`, `RelationEffectKind`,
  `TypedStateEffect`, `StateEffectKind`, `TypedEventEffect`,
  `StatePropertyEffect`, `ContextValues`.
  Verbatim move from `crates/executor/src/surface/journal.rs`; keep
  derives identical.
- Add `pub mod journal;` and `pub use journal::{...};` to
  `crates/types/src/lib.rs`.
- In `tabula-executor`: delete the moved definitions; replace with
  `pub use tabula_types::{...};` at the original call-site modules so
  in-crate `crate::surface::TxCall` etc. keep resolving.
- In `tabula-witness`: rewrite `use tabula_executor::...` for these
  types to `use tabula_types::...`.
- Delete `tabula-executor` from `crates/witness/Cargo.toml`
  dependencies.
- **Verify:** `cargo build --workspace` green; `cargo test
  --workspace` green; `cargo tree -p tabula-witness` shows no
  `tabula-executor`; the workspace's statement-first verification flow
  (`cargo test -p tabula-runtime`) still passes without test edits.

### Step 2 — `PreparedMachineInput` and `MachineTranscript` absorb `binding_digest` only

- `crates/machine/src/input/mod.rs`: remove `public_statement` field
  from `PreparedMachineInput`.
- `crates/machine/src/proof/transcript.rs`: rename
  `observe_public_statement_binding(&PublicStatement, &[u8; 32])`
  to `observe_binding_digest(&[u8; 32])`. The new body absorbs only
  the 32 digest bytes (as 32 felts, matching the existing digest-felt
  conversion). The public-statement felt absorption is deleted.
- `crates/machine/src/proof/prover.rs`: destructure
  `PreparedMachineInput` without `public_statement`; pass
  `binding_digest` alone to the transcript.
- `crates/machine/src/proof/verifier.rs`: call the renamed transcript
  method with `binding_digest` alone.
- **Upstream adapters** (runtime's assembly path that builds
  `PreparedMachineInput`): stop populating `public_statement`.
- **Verify:** workspace green. This step *changes* proof bytes; the
  comparison against SP-1's reference is expected to fail — that is
  intended.

### Step 3 — `TabulaProof` drops `public_statement`; verifier cross-check simplified

- `crates/machine/src/proof/model.rs`: remove `public_statement` field
  from `TabulaProof`. Constructors in prover.rs updated.
- `crates/machine/src/proof/codec.rs`: remove `AirStatementDto`
  entirely; remove its field from `ProofDto`. Serialized proof now
  contains sub-proofs + `binding_digest` only.
- `crates/runtime/src/verifier.rs`:
  - Drop the redundant
    `if proof.public_statement != *expected_public_statement` check
    (lines ~332–337). Binding-digest equality already guarantees
    statement equality by collision-resistance.
  - `verify_proof(&TabulaProof)` (which currently uses
    `proof.public_statement` as the authoritative statement) is
    removed or re-shaped. Its callers are inspected; statement-first
    verification already routes through `verify_public_statement`, so
    the self-attesting variant has no live consumer.
  - `verify_public_statement` continues to call
    `verify_proved_public_statement_digests` — those per-chip public
    value cross-checks still use `proof.execution_chip_public_values`
    against the externally supplied `PublicStatement`. Nothing there
    depends on `proof.public_statement`.
- **Verify:** workspace green; statement-first verification path still
  catches digest/statement mismatches (existing negative tests cover
  this).

### Step 4 — Wrap machine public surface as `BackendProver` / `BackendVerifier`

- New module `crates/machine/src/backend.rs` (or promoted to
  `pub mod backend;` from `lib.rs`). Defines:
  ```rust
  pub struct BackendProver { machine: TabulaMachine, … }
  impl BackendProver {
      pub fn new(config: TabulaStarkConfig, …) -> Result<Self, …>;
      pub fn prove_envelope(
          &self,
          input: PreparedMachineInput,
          binding_digest: [u8; 32],
      ) -> Result<ProofEnvelope, ProveError>;
  }
  pub struct BackendVerifier { machine: TabulaMachine, … }
  impl BackendVerifier {
      pub fn new(config: TabulaStarkConfig, …) -> Result<Self, …>;
      pub fn verify_envelope(
          &self,
          envelope: &ProofEnvelope,
          binding_digest: [u8; 32],
      ) -> Result<TabulaProof, VerificationError>;
  }
  ```
- `prove_envelope` delegates to `TabulaMachine::prove`, then encodes
  `TabulaProof` via the existing machine codec, then packages the
  resulting bytes into a `ProofEnvelope { proof_system:
  TABULA_STARK, proof_encoding: TABULA_MACHINE_BINARY_V1,
  proof_bytes }`.
- `verify_envelope` first calls `envelope.validate()` (fail-closed on
  unknown IDs), decodes the machine-owned bytes into `TabulaProof`,
  calls `TabulaMachine::verify`, then returns the decoded
  `TabulaProof` so callers can introspect chip openings.
- `binding_digest` is the only statement-carrying parameter. The
  envelope itself does not carry `binding_digest` — it remains a
  separate value plumbed alongside the envelope by the runtime
  prepared-prover / prepared-verifier. (SP-4 will own that plumbing.)
- `crates/machine/src/lib.rs`: `pub use backend::{BackendProver,
  BackendVerifier};`. Remove `pub use tabula_contract::PublicStatement;`
  — callers must import from contract directly.

  > *Note.* Some in-crate modules will still `use
  > tabula_contract::PublicStatement;` internally (transcript binding
  > still needs the type name, etc., once the upstream caller feeds
  > the binding digest). What SP-2 removes is the **re-export** that
  > makes `tabula_machine::PublicStatement` resolvable to external
  > crates.
- **Verify:** workspace green; the new envelope-level API is callable
  from runtime and sdk; old direct `TabulaMachine::prove/verify`
  callers are either migrated or left as an internal delegation
  path.

### Step 5 — Purge `tabula-lang` and `tabula-executor` from machine dev-deps

- `crates/machine/Cargo.toml`: remove `tabula-lang` and
  `tabula-executor` from `[dev-dependencies]`.
- Rewrite affected tests under `crates/machine/tests/` and
  `crates/machine/benches/` to use lower-level primitives:
  - Replace lang-level program compilation with hand-built chip /
    witness fixtures from `tabula-witness`, `tabula-chips`,
    `tabula-core` test-utils.
  - If a test truly needs a full compiled program, that test moves
    *out* of machine — either to `tabula-runtime` tests (where
    `tabula-lang` is already a reasonable dev-dep) or to a workspace
    integration-tests crate. SP-2 prefers the move over keeping
    lang in machine dev-deps.
- **Verify:** `cargo test -p tabula-machine` passes; `cargo tree -p
  tabula-machine --depth 1` shows no `tabula-lang` or
  `tabula-executor`.

### Step 6 — Reference bytes, determinism, and end-to-end

- Re-run `example → execute → prove → verify` on `basic` and
  `membership`. Record new `proof.bin` SHA-256 for both.
- Re-run once more to confirm determinism (same inputs → same bytes).
- Confirm `public_statement.json` is unchanged vs. the pre-SP-2
  capture. (If it changed, the statement-materialization path has
  shifted — a regression, not an SP-2 goal.)
- Confirm `verify` exits 0 on both examples.

### Step 7 — Documentation

- `crates/machine/README.md`: update the "Role" / "Owns" /
  "Does Not Own" sections to reflect the envelope-level API and the
  absence of embedded wire types.
- `crates/contract/README.md`: cross-reference — contract owns
  `ProofEnvelope`, machine emits it.
- `crates/witness/README.md` (if present) / `crates/types/README.md`
  (if present): note the new journal-type home.
- Amend `docs/superpowers/specs/2026-04-18-architecture-refactoring-design.md`
  §2.2 to drop `ArtifactId` from the `TabulaProof` target shape (see
  §2.1 of this doc). This is a same-SP doc touch-up.

No production code changes in step 7.

---

## 5. Ripple — Call Sites

Consumers that reference `PublicStatement` *through machine*:
- `crates/machine/src/lib.rs:39` — re-export removed.
- No external callers rely on this re-export today; every consumer
  (`runtime`, `sdk`, `cli`) already imports from `tabula-contract`
  after SP-1.

Consumers of `TabulaProof.public_statement`:
- `crates/runtime/src/verifier.rs` — redundant equality check
  deleted.
- `crates/runtime/src/engine.rs` — inspected during implementation;
  any reliance migrated to use the externally supplied
  `PublicStatement` (the runtime already has it at the call site).

Consumers of `PreparedMachineInput.public_statement`:
- `crates/runtime/src/engine.rs` (the assembly path that builds the
  prepared input from the journal) — stop populating the field.

Consumers of `MachineTranscript::observe_public_statement_binding`:
- `crates/machine/src/proof/prover.rs` and
  `crates/machine/src/proof/verifier.rs` — both switch to
  `observe_binding_digest`.

Consumers of journal types (witness → executor migration):
- `crates/witness/src/**/*.rs` — rewrite `use tabula_executor::…` to
  `use tabula_types::…` for the moved types.
- `crates/executor/src/surface/journal.rs` — keep a narrow
  `pub use tabula_types::{…};` (or inline the rewrites at consuming
  modules; ultraplan picks). Executor-internal types that are
  **not** on the witness boundary (`ExecutionJournal`,
  `TxExecutionOutcome`, `SuccessfulTxExecution`, `FailedTxExecution`,
  `StatePropertyEffect` if executor-only, etc.) stay put.

`BackendProver` / `BackendVerifier` consumers:
- `crates/runtime/src/engine.rs` prove path: rewire to
  `prove_envelope` (receiving `ProofEnvelope`).
- `crates/runtime/src/verifier.rs` verify path: rewire to
  `verify_envelope` (receiving `TabulaProof` for introspection).
- `crates/sdk/src/**` prove/verify adapters: follow the runtime
  rewire; no new wire types appear in SDK.

---

## 6. Completion Criteria

Hard gates (all must pass):

1. `grep -rn 'public_statement' crates/machine/src/` returns only
   internal parameter names — no struct fields, no `TabulaProof`
   field, no `PreparedMachineInput` field.
2. `grep -rn 'pub use tabula_contract::PublicStatement'
   crates/machine/src/lib.rs` returns no matches.
3. `grep -rn '\bAirStatementDto\b' crates/machine/` returns no
   matches.
4. `cargo tree -p tabula-witness` shows no `tabula-executor`.
5. `cargo tree -p tabula-machine --depth 1 -e normal` shows no
   `tabula-lang`.
6. `cargo tree -p tabula-machine --depth 1 -e normal,dev` shows no
   `tabula-lang` and no `tabula-executor`.
7. `cargo build --workspace` succeeds.
8. `cargo test --workspace` passes.
9. `BackendProver::prove_envelope` and
   `BackendVerifier::verify_envelope` are the sole public
   envelope-level entry points on `tabula-machine`.
10. End-to-end CLI flow on both `basic` and `membership`:
    `example → execute → prove → verify` succeeds; `verify` exits 0.
11. Running the prove step twice on identical inputs produces
    byte-identical `proof.bin` (determinism).
12. `public_statement.json` bytes are identical pre-SP-2 and
    post-SP-2 on both examples.
13. `crates/machine/README.md` documents the envelope-level API and
    the absence of embedded wire types.
14. Umbrella §2.2 is updated to drop `ArtifactId` from the target
    `TabulaProof` shape.

The SP-1 SHA-256 reference for `proof.bin` is explicitly *not* a
gate — SP-2 is byte-breaking by design.

---

## 7. Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Dropping the `proof.public_statement != expected` check masks a runtime bug where binding digests match but statements differ (binding-digest collision) | Very low | High | Collision-resistance of the underlying hash is already the security assumption everywhere. Per-chip digest cross-checks in `verify_proved_public_statement_digests` remain an independent second gate. |
| Witness tests (or other witness consumers) implicitly relied on re-exports from `tabula-executor` for types we moved | Low | Medium | Grep before the move; any such re-exports follow the types into `tabula-types` via `pub use`. |
| Machine-side tests using `tabula-lang`/`tabula-executor` for compiled-program fixtures are hard to rewrite against lower layers | Medium | Medium | Tests that truly require a full compiled pipeline move to `tabula-runtime` tests (which already has these dev-deps) or to a workspace integration-tests crate. SP-2 prefers relocation over keeping the deps. |
| `verify_envelope` returning `TabulaProof` surfaces an internal type in the envelope-level public API, slightly widening what machine publicly exposes | Low | Low | Accepted: runtime introspection of chip public values is a live requirement. Alternative (opaque verifier returning `()`) breaks runtime and is out of scope here. |
| `StateRuntimeView` trait turns out to be on the witness path after all | Low | Low | Step 1 grep decides: if witness imports `StateRuntimeView`, move it with the value-types to `tabula-types`; if not, leave in executor. |
| Amending the umbrella spec mid-sub-project creates doc drift | Low | Low | Amendment lands in the same commit as the SP-2 design doc, so SP-2 never proceeds against a stale target. |

---

## 8. Out of Scope

- Introducing `ArtifactId` as a wire type. (See §2.1.)
- Defining `ExecutionRecord` or
  `public_statement_from_record(artifact, record)`. Those land in
  SP-4.
- Adding `PreparedProver` / `PreparedVerifier` to `tabula-runtime`.
  SP-4.
- Introducing `ChipWitnessKit` or refactoring witness into a
  chip-agnostic lowering producer. SP-3.
- Runtime engine decomposition (`engine.rs` splitting). SP-5.
- SDK global-state removal (`NEXT_ENVIRONMENT_FINGERPRINT`). SP-6.
- Broader `tabula-executor::surface::*` module cleanup beyond the
  types that witness actively consumes.
- Any creation of a new workspace crate. If SP-2 implementation
  uncovers a need for one, the SP pauses and re-opens design with
  user approval first.

---

## 9. References

- Umbrella design:
  [`2026-04-18-architecture-refactoring-design.md`](2026-04-18-architecture-refactoring-design.md)
- SP-1 design:
  [`2026-04-18-sp1-contract-wire-type-consolidation-design.md`](2026-04-18-sp1-contract-wire-type-consolidation-design.md)
- Canonical architecture:
  [`docs/design/architecture.md`](../../design/architecture.md)
- Current `TabulaProof` definition:
  `crates/machine/src/proof/model.rs:67-79`
- Current `PreparedMachineInput` definition:
  `crates/machine/src/input/mod.rs:39-50`
- Current transcript binding:
  `crates/machine/src/proof/transcript.rs:35-48`
- Current envelope wire format:
  `crates/contract/src/proof_envelope.rs:77-108`
- Current runtime verify path:
  `crates/runtime/src/verifier.rs:315-338`
- Current witness→executor dep:
  `crates/witness/Cargo.toml:16`
- Current journal value-types:
  `crates/executor/src/surface/journal.rs`

---

## 10. Landed Notes (2026-04-19)

SP-2 landed in two commits on `main`:

- `a5294f3 SP-2: split machine into an envelope-level backend primitive`
  — primary refactor covering §4's seven migration steps.
- `d16fe82 Address SP-1/SP-2 deep-review findings` — follow-up fixups
  from the deep-review pass.

All 14 completion gates from §6 hold on `main`. Two deviations from the
original spec shipped as in-flight amendments (already documented
inline under §2.2 and §2.3):

- `BackendProver::prove_envelope(input: PreparedMachineInput) ->
  Result<(TabulaProof, ProofEnvelope), ProveError>` — no separate
  `binding_digest` argument (the input already carries it), and the
  return is a tuple so the runtime can skip a decode round-trip.
- `StateRuntimeView` moved into `tabula-types` alongside the journal
  value types, simplifying the `tabula-executor` surface.

### Polish follow-up (2026-04-20)

A small surface-cleanup pass landed on top of SP-2 once SP-5 had
stabilized:

- `BackendVerifier::verify_proof(&self, proof: &TabulaProof,
  expected_binding_digest: [u8; 32])` now performs the binding-digest
  check before delegating to the STARK verifier. Previously this entry
  point omitted the check and relied on upstream discipline (with a
  "binding-digest responsibility" warning in its rustdoc). Unifying the
  discipline removes the footgun mode: there is now exactly one rule
  across both `verify_envelope` and `verify_proof`. `verify_envelope`
  internally delegates to `verify_proof` so the binding check is
  authored in one place.
- The runtime verifier (`crates/runtime/src/verifier.rs`) keeps its
  upstream binding check as a fail-fast guard before the expensive
  chip-digest traversal; the backend's in-`verify_proof` check is a
  defense-in-depth second layer.

Byte-identity against the SP-5 baseline
(`docs/superpowers/specs/2026-04-19-sp5-byte-identity-baseline.txt`)
held after the polish pass.

### Deferred polish (not urgent, candidates for a future SP)

Three further surface refinements were identified during the SP-2
post-landing analysis but deliberately deferred. None is required for
correctness; each trades a one-day edit for incremental ergonomics.

1. **`ProvedBundle` named pair.** Replace the `(TabulaProof,
   ProofEnvelope)` tuple returned by `prove_envelope` with a named
   struct:
   ```rust
   pub struct ProvedBundle { pub envelope: ProofEnvelope, pub decoded: TabulaProof }
   ```
   Motivation: two-field tuples where both fields are "proof-shaped"
   are historically where argument order silently swaps in a future
   refactor. Today the tuple is documented at
   `crates/machine/src/backend/primitive.rs:30-37`; that documentation
   is load-bearing.

2. **Backend error taxonomy split.** `VerificationError` is a single
   flat enum mixing envelope-level concerns (`UnsupportedProofEnvelope`,
   `BackendMismatch`, `BindingDigestMismatch`, `ProofCodec`) with
   STARK-level constraint failures (`ChipVerificationFailed`,
   `CrossProofBusImbalance`, `InternalBusImbalance`, etc.). An ideal
   post-SP-5 shape splits into `BackendVerifyError` (surface / envelope
   / binding / decoding) and `StarkVerifyError` (internal constraint
   failures), analogous to SP-5's runtime error narrowing. Today's
   shape is workable because `thiserror::#[source]` chaining plus the
   runtime's `route_to_verify` pattern already hide most of the
   flatness from callers.

3. **`BindingDigest` newtype.** `binding_digest: [u8; 32]` appears on
   `PreparedMachineInput`, `TabulaProof`, `BackendVerifier::verify_*`,
   and `BoundStatement::binding_digest()`. A newtype
   `BindingDigest([u8; 32])` would prevent accidental mixing with
   other 32-byte digests (contract metadata, artifact context, etc.)
   and give a single authoritative name across the stack. Low value
   relative to other polish; purely hygiene.

These items are candidates for bundling into SP-6 (wire-type and
surface cleanup) rather than a dedicated SP. The `TabulaMachine`
visibility concern flagged in the post-landing analysis was found to
be already addressed: `TabulaMachine::{prove, verify}` are `pub(crate)`
(`crates/machine/src/machine.rs:88, 98`); the type itself is public
only because the runtime crate needs to name it as a field type.
