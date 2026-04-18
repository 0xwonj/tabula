# Architecture Refactoring — Umbrella Design

> Status: approved design, umbrella for sub-project execution
> Date: 2026-04-18
> Scope: workspace-wide cross-crate refactoring toward the canonical
> architecture defined in [`docs/design/architecture.md`](../../design/architecture.md)

This document is the **umbrella spec** for a multi-sub-project
refactoring of the Tabula workspace. Individual sub-projects (SP-1
through SP-6) get their own design docs and plans when execution
starts; this umbrella captures the final target and the ordering.

The refactoring is **not** harness-driven. The evaluation harness
exposed architectural asymmetries, but every item here is
independently motivated by `docs/design/architecture.md`'s layer
boundaries and dependency-direction rules. Deadlines are not a
constraint for this design; correctness and long-term maintainability
are the priorities.

---

## 1. Motivation

An audit across four dimensions (dependency direction, hidden state,
wire-type ownership, per-crate cohesion) surfaced a cluster of
architectural violations and drift. None are defects of the canonical
design — they are gaps between the canonical design and the current
code. The refactoring closes those gaps.

### 1.1 Findings (condensed)

**High-severity:**
- `PublicStatement` is defined in `tabula-stark::air::statement`, a
  backend crate, but it is a wire type for the compiler →
  runtime → verifier boundary. Consequence: `tabula-contract`
  depends on `tabula-stark`, violating the rule that contract builds
  only on shared meaning.
- `TabulaProof` embeds `public_statement` as a field, blurring the
  "machine = pure backend primitive" boundary.
- `tabula-runtime::TabulaRuntime::prove` is a monolith without a
  prepared prover handle; the verifier side already has
  `Verifier`/`VerifierBuilder`.
- `crates/sdk/src/builder.rs:17` holds a process-global
  `AtomicU64` fingerprint counter; two identical SDKs receive
  different fingerprints, breaking deterministic cache behavior.

**Medium-severity:**
- `tabula-witness` imports concrete chip row types
  (`InstructionRecord`, `RelationTableWitnessRow`, etc.) and
  directly constructs chip-specific rows, violating the spirit of
  "proof crates are split by responsibility" and blocking chip
  composition by third parties.
- `crates/runtime/src/engine.rs` is 2976 LOC, mixing execution
  orchestration, snapshot management, proof assembly, and verifier
  binding in one file.
- `tabula-witness` depends on `tabula-executor`. Because witness is
  a backend crate and executor is above backend, this is a
  reverse-direction dep suspicion that needs resolution.
- `tabula-runtime` has an internal borsh codec for
  `SnapshotCellRecord` whose ownership is ambiguous.

**Low-severity:**
- `tabula-sdk` wraps `Proof`, `ExecutionReceipt`, `State` — wrapper
  value unclear; may be type fragmentation.
- `types`, `profile`, `ir` have no `README.md`.
- `build_public_statement_from_journal` is `pub(crate)` in runtime;
  needed publicly as `public_statement_from_record`.

### 1.2 Non-goals

- Changing proof semantics or verification theorems.
- Touching compiler policy or language syntax.
- Back-compat shims (per `.claude/CLAUDE.md`: early development,
  clean breaks preferred).
- Changing the doc-tree authority model.

---

## 2. Final Target State

### 2.1 `tabula-contract` — sole wire-type authority

- Owns and defines: `SealedArtifact`, `ArtifactId`,
  `ExecutionRecord`, `PublicStatement`, `BoundStatement`,
  `ArtifactContext`, `ProofEnvelope`, `ProgramBinding`, plus their
  canonical-bytes codecs.
- Exposes `public_statement_from_record(artifact, record) ->
  PublicStatement` as stable public API.
- Dependencies: `tabula-core` only, with a possible allowance for
  `tabula-commitment` if commitment is reclassified as a shared
  foundation (open decision in SP-1).
- No dependency on `tabula-stark`, `tabula-ir`, or any other
  backend/authoring crate.

### 2.2 `tabula-machine` — pure backend primitive

- Exposes `BackendProver::prove_envelope(...)` and
  `BackendVerifier::verify_envelope(...)` taking binding digest and
  envelope; no embedded wire types.
- `TabulaProof` = `(ArtifactId, binding_digest, ProofEnvelope)`
  only. No `public_statement` field.
- `PreparedMachineInput` carries no `PublicStatement`.
- No wire-type re-exports; callers import from `tabula-contract`.
- Dev-dependencies pruned: no `tabula-lang` or `tabula-executor`
  in tests.

### 2.3 `tabula-runtime` — statement binding + symmetric prepared handles

- `PreparedProver` / `prepare_prover(Arc<SealedArtifact>) ->
  Result<PreparedProver, PrepareError>`.
- `PreparedVerifier` / `prepare_verifier(Arc<SealedArtifact>)`
  (renamed from the current `Verifier`).
- `VerifierState` (or equivalent) is a named public type (currently
  private inside `verifier.rs`).
- `engine.rs` is decomposed into role-specific modules: `prover.rs`,
  `verifier/` (already exists, extended), `execution.rs`,
  `snapshot.rs`, `statement_materialization.rs`, `state_binding.rs`.
- Internal borsh codec relocated (to contract if proof-visible, to
  a documented runtime-internal module if not).

### 2.4 `tabula-executor` — unchanged in role, verified pure

- No new proof-backend dependencies.
- Spot-check that `witness → executor` direction is genuinely
  consumed by witness for types only, and that those types should
  migrate to contract or profile.

### 2.5 `tabula-witness` + `tabula-chips` — chip-agnostic protocol

- New trait — working name `ChipWitnessKit`, home TBD in SP-3
  (candidates: `tabula-ext` or a new `tabula-witness-core`) —
  describes how a chip contributes to a witness store from
  execution-derived lowering output.
- `tabula-witness` produces chip-agnostic `LoweringOutput`; concrete
  row construction migrates to each chip's `WitnessKit` impl.
- `tabula-machine` builder drives a kit registry, calling each
  chip's `populate` before trace generation.
- `tabula-witness` no longer directly constructs
  `InstructionRecord`, `RelationTableWitnessRow`, etc.
- `tabula-witness → tabula-executor` dep either removed (preferred)
  or justified via a shared type migration to a lower layer.
- This decouples witness from chip composition and cleanly formalizes
  the extension authoring seam.

### 2.6 `tabula-sdk` — thin application facade

- No global counters. `NEXT_ENVIRONMENT_FINGERPRINT` gone; replaced
  by a deterministic fingerprint derived from explicit inputs.
- `Mutex<BTreeMap<...>>` caches either removed (caller holds
  `Arc<PreparedVerifier>` directly) or reshaped into explicit handle
  slots.
- Wrapper types (`sdk::Proof`, `sdk::ExecutionReceipt`, `sdk::State`):
  keep only if they carry application-semantic value-add; otherwise
  drop in favor of direct re-exports of contract/runtime types.
- `CommittedStateSnapshot` interop: either make explicitly public
  in runtime with docs, or remove the SDK re-export.

### 2.7 `tabula-cli` — thin adapter only

- Verify no re-defined semantics after upstream refactors.
- Keep consuming runtime + SDK; no new wire-type ownership.

### 2.8 Documentation

- Crate READMEs for `tabula-types`, `tabula-profile`, `tabula-ir`
  (currently missing).
- All crate READMEs updated to reflect post-refactor boundaries.
- `docs/notes/evaluation-stage-interfaces.md`,
  `docs/notes/evaluation-harness.md`,
  `docs/notes/evaluation-stage-support.md` reconciled against the
  post-refactor public API so the evaluation harness implementation
  can proceed without doc-drift surprises.

---

## 3. Dependency Direction Invariants

After refactoring, these must hold (expressed as forbidden imports):

- `tabula-core` imports nothing from the workspace.
- `tabula-contract` imports only `tabula-core` (and possibly
  `tabula-commitment` per SP-1 open question).
- `tabula-executor` has no `tabula-<backend-crate>` imports.
- `tabula-compiler` has no `tabula-runtime` or backend imports.
- `tabula-machine` has no `tabula-compiler`, `tabula-runtime`,
  `tabula-lang`, or `tabula-executor` imports (incl. dev-deps).
- `tabula-witness` has no `tabula-executor` imports (either removed
  outright, or shared types moved to a lower-layer crate).
- `tabula-witness` holds no concrete `tabula-chips::*` row types.
- `tabula-stark` holds no concrete chip deps.
- Public surfaces (`tabula-sdk`, `tabula-cli`, `tabula-ext`) do not
  redefine wire types owned by `tabula-contract`.

CI — if feasible — gains a `tools/check-layer-boundaries` script
that enforces these rules against `Cargo.toml` dep lists.

---

## 4. Sub-Project Decomposition

Six sub-projects in strict order. Each has its own design doc +
ultraplan + implementation session.

### SP-1 — Contract wire-type consolidation (foundation)

**Goal:** `tabula-contract` becomes the sole wire-type authority with
a minimal dep set.

**Scope:**
- Move `PublicStatement` from `tabula-stark::air::statement` to
  `tabula-contract`. Relocate its canonical field-element codec.
- Flip the dep direction: `tabula-stark` (and chips) import
  `PublicStatement` from contract.
- Introduce `tabula_contract::public_statement_from_record` as
  stable public API.
- Resolve `contract → ir` dep: either remove by moving shared types
  into contract/core, or justify and document.
- Decide on `contract → commitment`: allow (reclassify commitment
  as shared foundation) or move commitment's native primitive part
  to core.
- Remove `contract → stark` dep.

**Completion criteria:**
- `cargo tree -p tabula-contract` shows only core (+ commitment if
  kept) from workspace.
- `pub use tabula_stark::air::statement::PublicStatement` removed
  from `tabula-contract`, `tabula-machine`.
- All existing tests pass unchanged.

### SP-2 — Machine backend primitive split

**Goal:** `tabula-machine` exposes pure backend primitives; wire
types come from contract.

**Scope:**
- Introduce `BackendProver`, `BackendVerifier` with envelope-level
  `prove_envelope` / `verify_envelope`.
- Remove `public_statement` field from `TabulaProof`. `TabulaProof`
  is `(ArtifactId, binding_digest, ProofEnvelope)` only.
- Restructure `PreparedMachineInput` to not carry `PublicStatement`.
- Remove wire-type re-exports from `tabula-machine`.
- Prune dev-deps: remove `tabula-lang`, `tabula-executor` from
  machine dev-dependencies; refactor tests to use lower-level
  fixtures.
- Resolve `witness → executor` dep: move the shared types to
  contract (or another lower-layer home) and remove the Cargo dep.

**Completion criteria:**
- `BackendProver` / `BackendVerifier` are the only public entry
  points for envelope-level prove/verify in machine.
- `TabulaProof` wire format excludes public_statement.
- `cargo tree -p tabula-witness` shows no `tabula-executor`.
- CLI `verify --proof --statement` flow still works end-to-end on
  `basic` and `membership` examples.

### SP-3 — Witness-Chips protocol abstraction

**Goal:** Witness is chip-agnostic; the extension authoring seam is
formal.

**Scope:**
- Introduce `ChipWitnessKit` trait (exact name, shape, and home
  crate — `tabula-ext` vs. a new dedicated crate — finalized in
  SP-3's design doc).
- Refactor `tabula-witness::prepare_execution_store` into a
  chip-agnostic `LoweringOutput` producer.
- Each concrete chip (in `tabula-chips`) implements `ChipWitnessKit`
  — mechanical delegation.
- `tabula-machine` builder drives a kit registry, invoking
  `populate` per chip before trace generation.
- Remove all `tabula_chips::<concrete row type>` imports from
  `tabula-witness`.

**Completion criteria:**
- `grep -r 'use tabula_chips::' crates/witness/` returns at most
  label-level identifiers, no concrete row types.
- Adding a hypothetical new chip requires changes only to
  `tabula-chips` + `tabula-machine` builder registration; no
  witness crate edits.
- Existing end-to-end prove/verify on `basic` and `membership`
  produces byte-identical proofs.

### SP-4 — Runtime symmetric prepared handles

**Goal:** `tabula-runtime` exposes `PreparedProver` and
`PreparedVerifier` with matching shape.

**Scope:**
- Introduce `PreparedProver` + `prepare_prover(artifact)`.
- Rename `Verifier` → `PreparedVerifier`.
  `VerifierBuilder` knobs re-examined: drop knobs that no call
  site needs; promote those that do into explicit params or
  defaults.
- Expose `VerifierState` (or equivalent) as named public type.
- `TabulaRuntime::prove` becomes a thin facade over
  `PreparedProver` or is removed.
- SDK consumes the new prepared handles directly.

**Completion criteria:**
- `prepare_prover` and `prepare_verifier` are public in
  `tabula-runtime`.
- `PreparedVerifier::verify(&self, proof, public_statement) ->
  Result<BoundStatement, VerifyError>` is the single verify entry
  point through the runtime.
- CLI and SDK call sites use the new handles.

### SP-5 — Runtime engine decomposition

**Goal:** `engine.rs` is split into role-focused modules.

**Scope:**
- Extract modules from `engine.rs`: `prover/`, `execution/`,
  `snapshot/`, `statement_materialization.rs`, `state_binding.rs`.
- Relocate internal `SnapshotCellRecord` borsh codec to its
  canonical home (contract or a documented runtime-internal
  module).
- Remove or shrink the `TabulaRuntime` facade per SP-4's outcome.

**Completion criteria:**
- No single file in `crates/runtime/src/` exceeds ~800 LOC.
- Each runtime submodule has a single, documentable
  responsibility.

### SP-6 — SDK thinning, global-state removal, docs polish

**Goal:** Public surface layer is thin and coherent; workspace docs
reflect post-refactor reality.

**Scope:**
- Remove `NEXT_ENVIRONMENT_FINGERPRINT: AtomicU64`; replace with
  deterministic fingerprint.
- Remove or restructure SDK's `Mutex<BTreeMap<...>>` caches per
  SP-4's prepared-handle design.
- Resolve SDK wrapper types (`sdk::Proof`, `sdk::ExecutionReceipt`,
  `sdk::State`): keep with clear value-add docs, or drop.
- Resolve `CommittedStateSnapshot` interop exposure.
- Verify `tabula-cli` is still an adapter with no new semantics.
- Add `README.md` to `tabula-types`, `tabula-profile`, `tabula-ir`.
- Update all crate READMEs to match post-refactor boundaries.
- Reconcile `docs/notes/evaluation-{stage-interfaces,harness,
  stage-support}.md` with the final public API; remove any
  pre-refactor ambiguities.
- Optionally add a `tools/check-layer-boundaries` CI script
  enforcing §3 invariants.

**Completion criteria:**
- `grep -rn 'static NEXT_ENVIRONMENT_FINGERPRINT' crates/` returns
  nothing.
- Every crate under `crates/` has a `README.md`.
- The evaluation-harness note, when re-read alongside current code,
  has no symbol-ownership mismatches.

---

## 5. Ordering Rationale

The sequence is not arbitrary:

1. **SP-1 first** because every downstream sub-project consumes
   `tabula-contract` types. A clean contract surface is foundational.
2. **SP-2 second** because wire-type consolidation (SP-1) unlocks the
   machine primitive split; `TabulaProof` cleanup depends on
   `PublicStatement` being in contract.
3. **SP-3 third** because the chip-agnostic protocol benefits from
   a clean machine primitive boundary (SP-2) but must happen before
   runtime rearranges its prove/verify pathway.
4. **SP-4 fourth** because `PreparedProver` binding statement
   construction uses `public_statement_from_record` (SP-1) and
   composes with `BackendProver` (SP-2) via the chip-agnostic
   witness (SP-3).
5. **SP-5 fifth** because engine decomposition is easiest once the
   new prepared handles (SP-4) are in place — the decomposition
   follows the new module contracts rather than creating them.
6. **SP-6 last** because SDK + docs consume everything above;
   inverting this order would force redundant SDK edits.

Parallelism is minimal at the SP level. Within a single SP, multiple
mechanical tasks (e.g., every chip implementing `ChipWitnessKit`)
can be parallelized via subagent dispatch.

---

## 6. Execution Convention

- **Per-SP design doc** lives in `docs/superpowers/specs/` as
  `YYYY-MM-DD-sp{N}-<topic>-design.md`. Committed before starting.
- **Per-SP implementation plan** uses built-in Claude Code plan
  mode (`EnterPlanMode`) with explicit user approval via
  `ExitPlanMode`. Plans are ephemeral; a short summary of the
  approved plan may be committed alongside the SP's first commit
  as a scope anchor.
- **Per-SP branch** (optional, decided per SP) via git worktrees
  if the change surface is isolable.
- **Per-SP completion gate**: all acceptance criteria hold, test
  suite green, cross-crate invariants (§3) still enforceable.
- **advisor consultation** at minimum twice per SP: once before
  starting (after design doc commit, before ultraplan), once
  before declaring the SP done.

---

## 7. Open Decisions (to be settled in their SP)

- **SP-1**: Does `tabula-contract` retain its dep on
  `tabula-commitment`, or does commitment's native-primitive
  portion migrate to `tabula-core`?
- **SP-1**: Does `contract → ir` get removed, or does the shared
  type live elsewhere?
- **SP-2**: Exact final shape of `PreparedMachineInput` and
  `TabulaProof` after public_statement removal.
- **SP-3**: Final name and shape of `ChipWitnessKit`; whether it
  lives in `tabula-ext` or a new `tabula-witness-core` crate.
- **SP-4**: Does `TabulaRuntime` survive as a facade, or is it
  removed entirely?
- **SP-6**: Do SDK wrapper types (`sdk::Proof` etc.) survive?

Each open decision is logged in its SP's design doc with the
selected resolution.

---

## 8. References

- [`docs/design/architecture.md`](../../design/architecture.md) —
  canonical layer boundaries, dependency direction rules,
  verification vocabulary.
- [`ARTIFACT.md`](../../../ARTIFACT.md) — current reproducible
  artifact flow that must remain green post-refactor.
- [`docs/notes/evaluation-stage-interfaces.md`](../../notes/evaluation-stage-interfaces.md)
  — describes the *target* public API shape post-refactor; this
  refactoring is what makes those doc claims implementable.
- `.claude/CLAUDE.md` — collaboration posture and clean-break
  development posture.
