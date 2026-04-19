# Architecture Refactoring — Umbrella Design

> Status: approved design, umbrella for sub-project execution
> Date: 2026-04-18
> Scope: workspace-wide cross-crate refactoring toward the canonical
> architecture defined in [`docs/design/architecture.md`](../../design/architecture.md)

This document is the **umbrella spec** for a multi-sub-project
refactoring of the Tabula workspace. Individual sub-projects (SP-1
through SP-8) get their own design docs and plans when execution
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
- Dependencies: `tabula-core` plus `tabula-commitment`.
  `tabula-commitment` is reclassified as **Shared Meaning** rather
  than Proof Backend (decision taken during SP-4 review; formalized
  by SP-6's `docs/design/architecture.md` amendment). No code move —
  the fix is conceptual: commitment carries cryptographic substance
  shared across non-backend crates (contract, sdk, cli already use it),
  and fracturing its native-primitive portion into `tabula-core`
  would pour crypto into the "imports-nothing" foundation and
  accelerate drift as new commitment schemes land.
- No dependency on `tabula-stark`, `tabula-ir`, or any other
  backend/authoring crate.

### 2.2 `tabula-machine` — pure backend primitive

- Exposes `BackendProver::prove_envelope(...)` and
  `BackendVerifier::verify_envelope(...)` taking binding digest and
  envelope; no embedded wire types.
- Wire form is `ProofEnvelope` (owned by `tabula-contract`).
  In-memory decoded form is `TabulaProof { sub_proof_envelopes,
  binding_digest }` — no `public_statement` field, no `ArtifactId`
  (superseded in SP-2: `binding_digest` alone commits to the
  `(artifact, public_statement)` pair).
- `PreparedMachineInput` carries no `PublicStatement`.
- No wire-type re-exports; callers import from `tabula-contract`.
- Dev-dependencies pruned: no `tabula-lang` or `tabula-executor`
  in tests.

### 2.3 `tabula-runtime` — statement binding + three symmetric prepared handles

Runtime exposes three prepare-once / drive-many handles with matching
shape. The input-type asymmetry is structural, not accidental: the
verifier is IR-free (a pure binding / static-artifact check), while
prover and executor must lower or execute IR. `SealedArtifact` cannot
carry `ir::ValidatedProgram` without forcing a `contract → ir` dep
(violating §3), so the verifier alone consumes the pure sealed
artifact and the execution-side handles consume the fuller
`RegisteredProgram`:

- `PreparedProver` / `prepare_prover(Arc<RegisteredProgram>) ->
  Result<PreparedProver, ProveError>`. (SP-4 landed against
  `RegisteredProgram`; stays there.)
- `PreparedVerifier` / `prepare_verifier(Arc<SealedArtifact>) ->
  Result<PreparedVerifier, VerifyError>` (SP-4 landed against
  `RegisteredProgram`; flipped to `SealedArtifact` in SP-1.5).
- `PreparedExecutor` / `prepare_executor(Arc<RegisteredProgram>) ->
  Result<PreparedExecutor, ExecuteError>` — new in SP-5. Replaces
  the residual `TabulaRuntime` execute-only facade. Owns batch
  execution, query execution, and logical-state projection. Symmetric
  `Send + Sync`.
- `VerifierState` (or equivalent) is a named public type and marked
  `#[non_exhaustive]` to avoid pre-1.0 breaking-change footguns.
- `engine.rs` is decomposed into role-specific modules: `prover.rs`
  (extant), `verifier/` (extant), `executor.rs`, `execution.rs`,
  `snapshot.rs`, `statement_materialization.rs`, `state_binding.rs`.
- Internal borsh codec relocated (to contract if proof-visible, to
  a documented runtime-internal module if not).
- Error surface is narrowed per handle: `VerifyError`, `ProveError`,
  `ExecuteError` as dedicated types with `From` into a top-level
  `RuntimeError` for callers that span multiple handles.
- "Runtime pre-stuff" for runtime-sourced chip rows (context,
  tx-batch, event, relation-table) is exposed as a typed API
  (e.g., `install_relation_table_rows(...)`) rather than opaque
  pushes into the `KitScratch`. `tabula-chips::*Row` names do not
  appear in `tabula-runtime/src/**` — enforced by guardrail test.

### 2.4 `tabula-executor` — unchanged in role, verified pure

- No new proof-backend dependencies.
- Spot-check that `witness → executor` direction is genuinely
  consumed by witness for types only, and that those types should
  migrate to contract or profile.

### 2.5 `tabula-witness` + `tabula-chips` — chip-agnostic protocol

- `ChipWitnessKit` trait lives in `tabula-stark::witness_kit` (SP-3
  landed). It describes how a chip contributes to a witness store
  from execution-derived lowering output.
- **Sealed trait** — only workspace-internal crates may implement
  `ChipWitnessKit` (locked decision, applied in SP-5). Third-party
  chip authoring is deferred; today's implementers are blessed chips
  in `tabula-chips`. The seal uses the standard "private supertrait
  in a private module" pattern; no proc-macro.
- `tabula-witness` produces chip-agnostic `LoweringOutput`; concrete
  row construction lives inside each chip's `ChipWitnessKit` impl
  (SP-3 landed).
- `tabula-machine` builder drives a kit registry, calling each
  chip's `populate` before trace generation.
- `tabula-witness` no longer directly constructs
  `InstructionRecord`, `RelationTableWitnessRow`, etc.
- `tabula-witness → tabula-executor` dep either removed (preferred)
  or justified via a shared type migration to a lower layer.
- This decouples witness from chip composition and formalizes the
  extension authoring seam as a closed-but-typed interface.

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

Nine sub-projects. SP-1 through SP-4 are landed; SP-1.5 closes SP-1's
structural gap (SealedArtifact introduction) and is a hard prerequisite
for SP-5; the remaining five execute in the order described in §5
(SP-5 and SP-8 may run in parallel). Each has its own design doc +
ultraplan + implementation session.

### SP-1 — Contract wire-type consolidation (foundation)

**Goal:** `tabula-contract` becomes the sole wire-type authority with
a minimal dep set.

**Scope:**
- Move `PublicStatement` from `tabula-stark::air::statement` to
  `tabula-contract`. Relocate its canonical field-element codec.
- Flip the dep direction: `tabula-stark` (and chips) import
  `PublicStatement` from contract.
- ~~Introduce `tabula_contract::public_statement_from_record` as
  stable public API.~~ **Reassigned out of SP-1** (see SP-1 §1):
  `ExecutionRecord` does not yet exist, and the current materializer
  depends on runtime-internal `ProofJournal` /
  `PublicStatementMaterialization` plus `tabula-types` registries.
  The API moves to whichever later SP defines `ExecutionRecord` —
  likely SP-4.
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
  retains only the decoded sub-proof envelopes plus `binding_digest`;
  the 32-byte digest alone commits to the `(artifact,
  public_statement)` pair, so no `ArtifactId` is carried on the proof.
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

> **Status: shipped (2026-04-19).** Landed on `refactor/witness-chip-kit` as
> commits `dc2690e` (S1 trait infra), `7ddf374` (S2 IrHashKit pilot),
> `8066d3f` / `6360ade` / `218f6af` (S3.1–S3.3 remaining execution-tier
> chips), `6c216e8` (S4 guardrail assertive). See the SP-3 design doc's
> "Landed" amendment for deviations from the original shape.

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

> **Status: Landed 2026-04-19.** See
> [SP-4 design spec](2026-04-19-sp4-runtime-prepared-handles-design.md)
> and its Landed Notes section for details.

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

### SP-1.5 — SealedArtifact introduction (SP-1 continuation, SP-5 prerequisite)

**Goal:** Introduce `tabula-contract::SealedArtifact` as the canonical
"what the compiler sealed" type, flip `prepare_verifier` to
`Arc<SealedArtifact>`, and lift the two IR-derived setup quantities
(`RelationPolicy`, `uses_ir_hash`) into the sealing step so the verifier
becomes IR-free. Closes the structural gap left open at the end of SP-1.

**Motivation:**
- Umbrella §2.1 names `SealedArtifact` as a contract-owned type; it does
  not yet exist. The sealed fields currently live on
  `tabula-compiler::RegisteredProgram`, commingled with
  `ir::ValidatedProgram` and `capability_manifest`.
- SP-5 rewrites the three builder surfaces and migrates all call sites
  (CLI, SDK, examples, tests). If SP-5 proceeds against
  `RegisteredProgram` for the verifier and `SealedArtifact` is
  introduced later, the same surface is rewritten twice. Landing the
  split before SP-5 avoids the duplicate migration and keeps SP-5's
  byte-identity gate clean (pure module surgery, no wire-type reshuffle).
- Making the verifier IR-free matches the dependency-direction rule that
  verification is a pure binding check; it also simplifies SP-5's
  verifier decomposition.

**Scope:**
- Introduce `pub struct SealedArtifact` in `tabula-contract` carrying the
  contract-level tier of what `RegisteredProgram` holds today:
  `artifact_schema_version`, `execution_contract`, `profile_catalog`,
  `tuple_encoding_defaults`, `static_table_artifact`,
  `metadata_envelope`, `binding`.
- Add two seal-time-computed fields to `SealedArtifact`:
  `relation_policy: SealedRelationPolicy` (enum `{ Disabled,
  RequireArtifactRoot }`, relocated from `tabula-runtime::bootstrap`)
  and `uses_ir_hash: bool`. The compiler computes both at
  `registration::register` time by scanning IR ops, so verifier setup
  never re-derives them.
- Refactor `RegisteredProgram` in `tabula-compiler` to hold
  `{ sealed: SealedArtifact, validated: ir::ValidatedProgram,
  capability_manifest: Vec<CapabilityDescriptor> }` with accessors that
  proxy to `sealed`.
- Move contract-level validation (`validate_sealed_artifact`) into
  `SealedArtifact::validate`; `RegisteredProgram::validate_sealed_artifact`
  delegates.
- Bump `RegisteredProgram`'s canonical schema from v1 to v2 to reflect
  the new struct layout (no on-disk compat needed — research prototype,
  clean break).
- Flip `PreparedVerifier::builder` and `prepare_verifier` to
  `Arc<SealedArtifact>`. Split `resolve_program_setup` into a
  sealed-facing variant (verifier path, IR-free) and the existing
  registered-facing variant (prover/executor paths). Refactor
  `ResolvedStateRuntime::from_registered_program` to share an impl with
  a new `from_sealed_artifact` constructor.
- Migrate CLI, SDK, and testing helpers to pass `SealedArtifact` to
  verifier construction sites. SDK `Artifact` gains a
  `.sealed_artifact()` accessor; wraps `RegisteredProgram` unchanged.

**Completion criteria:**
- `tabula-contract::SealedArtifact` is the public verifier-facing artifact
  type; `PreparedVerifier::builder(Arc<SealedArtifact>)` is the only
  verifier construction entry point.
- `grep -rn 'RegisteredProgram' crates/runtime/src/verifier.rs` returns
  zero matches.
- `prepare_prover` and future `prepare_executor` keep
  `Arc<RegisteredProgram>` (structural asymmetry per §2.3).
- `cargo tree -p tabula-contract` shows only the allowed deps
  (`tabula-core`, `tabula-commitment`, and `tabula-profile` if the audit
  permits — else `ProfileCatalog` handling is scoped to a non-contract
  layer).
- `RelationPolicy::from_program` and `program_uses_hash` are no longer
  called in `tabula-runtime`; their equivalents live in the compiler
  seal step.
- `examples/basic` and `examples/membership` produce byte-identical
  `ProofEnvelope` and `public_statement.json` pre- and post-SP-1.5.
- New tests: `crates/contract/tests/sealed_artifact.rs` (round-trip +
  validate matrix) and `crates/compiler/tests/sealed_artifact_seal.rs`
  (seal-time correctness of `relation_policy` and `uses_ir_hash`).

### SP-5 — Runtime decomposition + executor symmetry

**Goal:** Finish the runtime prepared-handle story begun in SP-4.
Promote the residual `TabulaRuntime` into a third symmetric handle
(`PreparedExecutor`), decompose `engine.rs` into single-responsibility
modules, narrow the error surface per handle, formalize the "runtime
pre-stuff" pattern as a typed API, seal `ChipWitnessKit`, and mark
public prepared-handle types `#[non_exhaustive]`.

**Scope:**
- **TabulaRuntime → PreparedExecutor** (locked decision). Promote the
  execute-only facade into a third prepare-once / drive-many handle
  symmetric with `PreparedProver` / `PreparedVerifier`. Public surface:
  `prepare_executor(Arc<SealedArtifact>) -> Result<PreparedExecutor,
  ExecuteError>`. `Send + Sync`. `TabulaRuntime` / `RuntimeBuilder`
  symbols removed; CLI + SDK migrated.
- Decompose `engine.rs` into role-focused modules: `executor.rs`,
  `execution.rs`, `snapshot.rs`, `statement_materialization.rs`,
  `state_binding.rs`, `prepared_state.rs`, `prelude.rs`, `pre_stuff.rs`.
  Existing `prover.rs` and `verifier/` retained.
- Relocate internal `SnapshotCellRecord` borsh codec to its canonical
  home (contract if proof-visible, a documented runtime-internal
  module otherwise). Record the disposition rationale inline.
- Narrow errors per handle: introduce `ProveError`, `VerifyError`,
  `ExecuteError` plus a shared `SetupCommon`. `RuntimeError` survives
  as a `#[non_exhaustive]` umbrella with `From` conversions for each.
- Typed "runtime pre-stuff" API. Introduce a `PreStuffInstaller`
  seam (or equivalent) with methods like `install_relation_table_rows`
  that accept chip-agnostic logical-row types owned by
  `tabula-stark::witness_kit`. Concrete `tabula_chips::*Row` identifiers
  do not appear in `crates/runtime/src/**`.
- **Seal `ChipWitnessKit`** (locked decision). Apply the standard
  private-supertrait seal in `tabula-stark::witness_kit`. Add a
  trybuild compile-fail probe for external impls.
- Mark `VerifierState` and its sibling public prepared-handle types
  `#[non_exhaustive]` to avoid pre-1.0 breaking-change footguns.
- Guardrail test: `tabula_chips::*Row` identifier names absent from
  `crates/runtime/src/**`.
- Guardrail test: `PreparedProver`, `PreparedVerifier`, `PreparedExecutor`
  are all `Send + Sync + 'static` (compile-time assertion).

**Completion criteria:**
- `crates/runtime/src/engine.rs` no longer exists; no single file
  under `crates/runtime/src/` exceeds ~800 LOC.
- Three symmetric prepared handles publicly available, all
  `Send + Sync`, built by matching `prepare_*` free functions.
- `TabulaRuntime` / `RuntimeBuilder` removed from the public API;
  CLI, SDK, examples migrated.
- `ProveError` / `VerifyError` / `ExecuteError` are the per-handle
  error types; `RuntimeError` is a `#[non_exhaustive]` umbrella.
- `ChipWitnessKit` sealed; external impl fails to compile.
- Byte-identity on `examples/basic` and `examples/membership` proofs
  across the SP-4 → SP-5 transition (pure refactor).

### SP-6 — SDK thinning, architecture.md amendment, docs polish

**Goal:** Public surface layer is thin and coherent; workspace docs
reflect post-refactor reality, including the conceptual relocation of
`tabula-commitment` to Shared Meaning.

**Scope:**
- Remove `NEXT_ENVIRONMENT_FINGERPRINT: AtomicU64`; replace with a
  deterministic fingerprint derived from explicit inputs.
- Remove or restructure SDK's `Mutex<BTreeMap<...>>` caches per
  SP-4's prepared-handle design (callers hold `Arc<PreparedVerifier>`
  or `Arc<PreparedExecutor>` directly where possible).
- Resolve SDK wrapper types (`sdk::Proof`, `sdk::ExecutionReceipt`,
  `sdk::State`): keep with clear value-add docs, or drop.
- Resolve `CommittedStateSnapshot` interop exposure.
- Verify `tabula-cli` is still an adapter with no new semantics.
- Add `README.md` to `tabula-types`, `tabula-profile`, `tabula-ir`.
- Update all crate READMEs to match post-refactor boundaries.
- **`docs/design/architecture.md` amendment**: relocate
  `tabula-commitment` from "Proof Backend" to "Shared Meaning" (or a
  "Shared Cryptographic Foundation" sub-tier) to reflect that
  non-backend crates already depend on it. No code move; the diagram
  and dependency-direction text update to match landed reality.
- **9 → 15 bus doc drift**: reconcile any lingering "9 bus" references
  in design notes and crate docs against the current 15-bus machine
  topology.
- Optionally add a `tools/check-layer-boundaries` CI script enforcing
  §3 invariants.
- Guardrail test: all three prepared handles (`PreparedProver`,
  `PreparedVerifier`, `PreparedExecutor`) and their inner
  `VerifierState` / equivalent carry `Send + Sync` (compile-time
  assertion; may subsume the SP-5 version if not yet ergonomic).

**Completion criteria:**
- `grep -rn 'static NEXT_ENVIRONMENT_FINGERPRINT' crates/` returns
  nothing.
- Every crate under `crates/` has a `README.md`.
- `docs/design/architecture.md` places `tabula-commitment` in the
  Shared Meaning tier; the layer diagram and §Dependency Direction
  reflect the change.
- No "9 bus" stale references remain in design docs or crate docs.
- Send+Sync guardrail test is green on the published prepared-handle
  types.

### SP-7 — Feature matrix unification

**Goal:** Workspace feature flags form a single coherent, monotone
axis that matches the post-refactor layer boundaries. New
contributors and EuroSys artifact reviewers can predict
`cargo build --features …` behavior from a one-page table, not from
reading seven `Cargo.toml`s.

**Motivation:**
- Axes are mixed: function (`compile`/`execute`/`verify`/`prove`),
  role (`authoring`/`runtime`/`backend` in `tabula-ext`), and
  implementation (`stark`, `test-utils`) are entangled across crates.
- Monotonicity is uneven: `runtime` has clean `prove ⊃ verify`, but
  `ext`'s five-level chain (`backend ⊃ prove ⊃ verify ⊃ runtime ⊃
  authoring`) is hard to reason about, and `sdk::advanced` is an
  orphan axis.
- Workspace-level comment in root `Cargo.toml` still advertises a
  three-flag world (`default`/`stark`/`test-utils`) that no longer
  matches reality.
- Reproducibility risk for the EuroSys 2027 artifact: if the feature
  graph is inconsistent, the artifact-evaluation `cargo build`
  incantations become accidentally fragile.

**Scope:**
- Map the current feature graph across all crates; identify the true
  set of end-user configurations worth supporting.
- Unify on a single primary axis (function ladder:
  `compile ⊂ execute ⊂ verify ⊂ prove`).
- Demote implementation-choice axes to a separate namespace
  (`impl-stark`, `impl-mock`) so users can reason about them
  orthogonally.
- Reconcile `tabula-ext`'s role-based axis with the function axis
  (collapse, rename, or keep with explicit justification).
- Replace the root `Cargo.toml` comment with a canonical
  `docs/design/feature-matrix.md` that lists every supported
  configuration and what it links in.
- Optionally: CI job that builds each documented configuration to
  prevent silent breakage.
- Guardrail test: a feature-flag monotonicity check that, for every
  pair on the primary axis, asserts the superset relation programmatically
  (not just documentarily).

**Completion criteria:**
- Every feature flag in the workspace appears in
  `docs/design/feature-matrix.md` with a one-line purpose.
- No crate exposes a feature not referenced by the matrix doc.
- `cargo build --features <X>` for every documented configuration
  succeeds from a clean build.
- At least one monotone primary axis: for every pair of features on
  it, one is a superset of the other — enforced by a test, not
  just documentation.

### SP-8 — NF completeness + `--nf-elision` compiler mode

**Goal:** Close two entangled gaps: (1) `tabula-ir::validate` does not
yet fully catch Normal-Form violations, and (2) the runtime has no
mode for skipping the RAM-consistency fragment of the proof in trusted
NF contexts. Together these pin down the compiler → sealed-artifact →
verifier contract around NF.

**Motivation:**
- Even today, handwritten or externally-sourced IR can violate NF-1/2/3/4
  without the validator catching it, shifting the implicit "NF holds"
  guarantee off the compiler and onto programmer discipline.
- An NF-elision mode is repeatedly useful for experiments and for
  higher-throughput batching when the IR source is trusted. It is not
  a simple flag — it changes the codegen variant, the sealed-artifact
  metadata, and the verifier binding calculation. Wiring it as a
  one-shot SP prevents the mode from accreting ad-hoc across the
  stack.

**Scope:**
- **NF-1/2/3/4 + True SSA validation** tightening in
  `tabula-ir::validate`. Exhaustive test matrix per rule; validator
  rejects every documented violation.
- `--nf-elision` compiler flag. A codegen variant that suppresses the
  RAM-consistency proof subsystem where NF holds.
- Sealed-artifact metadata records the NF mode (present / elided)
  under the compiler-sealed `metadata_hash` so the verifier sees the
  mode via artifact binding, not a side channel.
- Verifier binding logic is mode-aware: the binding digest and
  `PreparedVerifier::verify` path differ for the elided mode, matching
  the proof shape the prover emitted.
- Documentation: a short `docs/design/nf-modes.md` describing the two
  modes, their sealed-artifact encoding, and their trust surface.
- Guardrail test: round-trip every example under both modes;
  cross-mode proofs must fail verification (wrong mode caught by
  the binding, not by silent success).

**Completion criteria:**
- Every NF rule has at least one positive and one negative test in
  `tabula-ir::validate`.
- `tabula-compiler` accepts `--nf-elision`; produced sealed artifact
  carries the mode tag under `metadata_hash`.
- `PreparedVerifier::verify` honors the mode tag and rejects
  cross-mode proofs.
- `examples/basic` and `examples/membership` run in both modes end to
  end; cross-mode proof → cross-mode verifier is a verification
  failure, not a silent accept.

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
5. **SP-1.5 after SP-4** because it closes SP-1's contract-authority
   gap (introducing `SealedArtifact`) and because it touches the
   `prepare_verifier` signature that SP-4 just landed. Running it here
   means SP-5 inherits the asymmetric signature shape (`Arc<SealedArtifact>`
   for verifier, `Arc<RegisteredProgram>` for prover/executor) cleanly.
   SP-5's byte-identity gate stays pure-module surgery — no wire-type
   reshuffling mixed in.
6. **SP-5 and SP-8 run in parallel** once SP-1.5 has landed. They are
   independent:
   - SP-5 touches only `tabula-runtime` (decomposition, executor
     symmetry, error narrowing, pre-stuff API) and the
     `ChipWitnessKit` seal in `tabula-stark`.
   - SP-8 touches `tabula-ir` (validate tightening), `tabula-compiler`
     (codegen variant + sealed-artifact metadata), and the verifier
     binding path inside `tabula-runtime::verifier`.
   The only shared surface is the verifier module; coordinate there
   but do not serialize the SPs.
7. **SP-6 after SP-5 and SP-8** because SDK + docs consume everything
   above. SP-6 also carries the `docs/design/architecture.md`
   amendment (commitment tier) and the 9→15 bus doc drift — both
   cleanest to write once SP-5's runtime shape has landed.
8. **SP-7 last** because feature flags cross-cut every crate the
   earlier SPs restructure. Unifying the matrix before those
   boundaries settle would lock in a shape that SP-1…SP-6/SP-8 then
   invalidate. Done last, it reconciles the final layer reality
   into one coherent user-facing surface.

Within a single SP, multiple mechanical tasks (e.g., every chip
implementing `ChipWitnessKit`, every NF rule getting its positive /
negative test pair) can be parallelized via subagent dispatch.

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

**Resolved (logged for traceability):**

- **SP-1** *(resolved)*: `tabula-contract` keeps its dep on
  `tabula-commitment`; commitment is reclassified as Shared Meaning
  rather than moving primitives into `tabula-core`. Architecture.md
  amendment lives in SP-6.
- **SP-3** *(resolved)*: `ChipWitnessKit` lives in
  `tabula-stark::witness_kit` (not `tabula-ext` or a new crate).
- **SP-4** *(resolved)*: `TabulaRuntime` is **promoted** to a third
  symmetric handle `PreparedExecutor` rather than removed; landed in
  SP-5.
- **SP-5** *(resolved ahead of execution)*: `ChipWitnessKit` is
  **sealed**. Third-party chip authoring is deferred; workspace
  chips remain the only implementers.
- **SealedArtifact introduction timing** *(resolved)*: introduced in
  SP-1.5, **before** SP-5. Bundling it into SP-5 would muddy the
  byte-identity gate; deferring it past SP-5 would force a second
  migration of the same builder surfaces. SP-1.5 lives between SP-4
  and SP-5 in §5's sequencing.
- **§2.3 prepared-handle input asymmetry** *(resolved)*: verifier
  takes `Arc<SealedArtifact>`; prover and executor take
  `Arc<RegisteredProgram>`. Symmetric `Arc<SealedArtifact>` would
  require `contract → ir` dep (violates §3) or `SealedArtifact`
  carrying `ValidatedProgram` (weakens the name). The asymmetry
  reflects the underlying layering.
- **tabula-ext split** *(resolved)*: `tabula-ext` is **not** split.
  Empirical dep graph (runtime and sdk are the only importers;
  backend crates do not import ext) shows the crate already sits
  cleanly above the backend boundary.

**Still open (to be settled in the named SP):**

- **SP-1**: Does `contract → ir` get removed, or does the shared
  type live elsewhere?
- **SP-2**: Exact final shape of `PreparedMachineInput` and
  `TabulaProof` after public_statement removal.
- **SP-5**: Final disposition of `SnapshotCellRecord` borsh codec
  (tabula-contract vs. documented runtime-internal module).
- **SP-6**: Do SDK wrapper types (`sdk::Proof` etc.) survive?
- **SP-7**: Does `tabula-ext`'s `authoring`/`runtime`/`backend`
  vocabulary collapse into the function axis, or survive with
  explicit justification?
- **SP-7**: Does `sdk::advanced` become a first-class flag or get
  renamed to reflect what it actually gates?
- **SP-7**: Does CI gain a feature-matrix build job, or is the
  matrix doc alone sufficient guard?
- **SP-8**: Exact on-disk encoding of the NF mode tag under
  `metadata_hash`.
- **SP-8**: Whether the elided-mode proof shape requires a new
  `ProofEnvelope` variant or reuses the existing envelope with a
  mode-gated interpretation.

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
