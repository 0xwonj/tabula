# SP-5 — Runtime Decomposition + Executor Symmetry

> Status: proposed design (rewrite 2026-04-19)
> Parent: [architecture refactoring umbrella](./2026-04-18-architecture-refactoring-design.md) §2.3, §2.5, §4 SP-5
> Predecessors: SP-4 landed (`334a1f7`); SP-1.5 landed (`48bb08a`)
> Audience: SP-5 implementer + reviewers

## 1. Goal

Finish the runtime prepared-handle story begun in SP-4. Promote the
residual `TabulaRuntime` facade into a third symmetric handle
(`PreparedExecutor`), decompose the 3,139-LOC `engine.rs` into
role-focused modules, narrow the error surface per handle, formalize
the "runtime pre-stuff" seam as a typed API, seal `ChipWitnessKit`,
and mark public prepared-handle types `#[non_exhaustive]`.

SP-5 is a pure refactor: byte-identical proofs + public statements
across the SP-4 → SP-5 transition.

## 2. Prerequisite state (already landed)

Asymmetric prepared-handle signatures, locked by SP-1.5:

```rust
fn prepare_prover(Arc<RegisteredProgram>, …) -> Result<PreparedProver, _>;
fn prepare_verifier(Arc<SealedArtifact>,   …) -> Result<PreparedVerifier, _>;
fn prepare_executor(Arc<RegisteredProgram>, …) -> Result<PreparedExecutor, _>;   // new in SP-5
```

Verifier is IR-free once `relation_policy` and `uses_ir_hash` are
sealed at registration. Prover and executor must execute IR. Forcing
symmetric `Arc<SealedArtifact>` would require either a `contract → ir`
dep (forbidden by umbrella §3) or `SealedArtifact` carrying
`ValidatedProgram` (weakens the name). The asymmetry is structural.

## 3. Feature matrix (authoritative)

From `crates/runtime/Cargo.toml`:

- `default = []` — nothing compiles; present only so `cargo build`
  succeeds as a topology check.
- `verify` — adds `tabula-machine`, `tabula-chips`, `tabula-stark`.
  Baseline practical feature. Enables `PreparedVerifier`,
  `PreparedExecutor`, and the shared setup path.
- `prove` — adds `tabula-witness`, `rayon`, and STARK proving inputs.
  Implies `verify`. Enables `PreparedProver`.

Error types are feature-gated to match the surface that compiles at
each level (§7).

## 4. Scope

### 4.1 In scope

1. `PreparedExecutor` + `prepare_executor`. `TabulaRuntime` and
   `RuntimeBuilder` removed (not deprecated — clean break per
   `.claude/CLAUDE.md`).
2. `engine.rs` decomposed into role-focused modules (§6).
3. Narrowed per-handle errors: `ProveError`, `VerifyError`,
   `ExecuteError`, `SetupError`. `RuntimeError` is the
   `#[non_exhaustive]` umbrella with `#[from]` from each narrowed
   type (§7).
4. **Single API entry point per handle**: `prepare_*(artifact, &opts)`
   free functions. **Per-handle builders removed.** `PreparedOptions`
   is the knob surface with `::standard()` + `.with_*()` chaining
   (§5).
5. Typed pre-stuff API: chip-specific row identifiers
   (`InstructionRecord`, `RelationTableWitnessRow`, …) absent from
   `crates/runtime/src/**`. Replaced by logical row types owned in
   `tabula-stark::witness_kit` and installed through
   `PreStuffInstaller` (§8).
6. `ChipWitnessKit` sealed via convention seal + trybuild compile-fail
   and compile-pass probes (§9).
7. `VerifierState`, `PreparedOptions`, and all prepared-handle public
   types marked `#[non_exhaustive]`.
8. `SnapshotCellRecord` codec disposition resolved (§10).
9. Byte-identity gate reintroduced as a shell script (§11).
10. Guardrail tests (§12).

### 4.2 Out of scope

- SDK thinning, `NEXT_ENVIRONMENT_FINGERPRINT` removal → SP-6.
- `tabula-types` / `tabula-profile` / `tabula-ir` READMEs → SP-6.
- 9 → 15 bus doc drift → SP-6.
- Feature matrix unification across the workspace → SP-7.
- NF-1/2/3/4 validation + `--nf-elision` → SP-8.

## 5. API shape

### 5.1 Single entry point

```rust
// crates/runtime/src/options.rs

#[non_exhaustive]
pub struct PreparedOptions {
    pub host_environment: HostEnvironment,
    pub machine_stark_config: TabulaStarkConfig,
    #[cfg(feature = "prove")]
    pub root_backend_bundle: RootBackendBundle,
    #[cfg(not(feature = "prove"))]
    pub root_proof_backend: Arc<dyn RootProofBackend>,
}

impl PreparedOptions {
    /// Infallible default shape. Errors that used to come out of
    /// `HostEnvironment::standard()` are re-exposed via dedicated
    /// constructors; `standard()` itself does not fail.
    pub fn standard() -> Self { … }

    pub fn with_host_environment(mut self, env: HostEnvironment) -> Self { … }
    pub fn with_machine_stark_config(mut self, cfg: TabulaStarkConfig) -> Self { … }
    #[cfg(feature = "prove")]
    pub fn with_root_backend_bundle(mut self, bundle: RootBackendBundle) -> Self { … }
    #[cfg(not(feature = "prove"))]
    pub fn with_root_proof_backend(mut self, backend: Arc<dyn RootProofBackend>) -> Self { … }
}
```

```rust
#[cfg(feature = "prove")]
pub fn prepare_prover(
    registered: Arc<RegisteredProgram>,
    opts: &PreparedOptions,
) -> Result<PreparedProver, ProveError>;

#[cfg(feature = "verify")]
pub fn prepare_verifier(
    sealed: Arc<SealedArtifact>,
    opts: &PreparedOptions,
) -> Result<PreparedVerifier, VerifyError>;

#[cfg(feature = "verify")]
pub fn prepare_executor(
    registered: Arc<RegisteredProgram>,
    opts: &PreparedOptions,
) -> Result<PreparedExecutor, ExecuteError>;
```

No `PreparedProverBuilder` / `PreparedVerifierBuilder` types. The
builder surface SP-4 introduced is dropped — DX equivalent is
`PreparedOptions::standard().with_*()` chained into the free
function.

Rationale: three constructor shapes (free fn + builder + options
chain) is API sprawl. One shape that scales to three handles is
simpler and matches clean-break posture.

### 5.2 Handle shape

Every handle exposes `&self` operations. Per-call mutable state
(scratch buffers, column artifacts, execution journals) lives on
the stack for the duration of one call. Same handle + same input
→ byte-identical output. All three are `Send + Sync + 'static`.

```rust
impl PreparedProver {
    pub fn prove(&self, input: &ProveInput<'_>) -> Result<ProveResult, ProveError>;
    pub fn prove_and_verify(
        &self,
        verifier: &PreparedVerifier,
        input: &ProveInput<'_>,
    ) -> Result<VerifiedResult, ProveError>;
    // + accessors (binding, machine, etc.)
}

impl PreparedVerifier {
    pub fn verify(
        &self,
        proof: &TabulaProof,
        expected: &PublicStatement,
    ) -> Result<BoundStatement, VerifyError>;
    // + accessors
}

impl PreparedExecutor {
    pub fn execute_batch(
        &self,
        snapshot: &CommittedStateSnapshot,
        batch: &ir::EntryBatch,
        context: &ir::ContextInput,
    ) -> Result<ExecutionJournal, ExecuteError>;
    pub fn execute_query(&self, …) -> Result<…, ExecuteError>;
    pub fn materialize_logical_state(&self, …) -> Result<CommittedStateSnapshot, ExecuteError>;
    // + accessors
}
```

### 5.3 `validate_core_first_program` placement

The "reject capability calls outside the native proving subset" check
requires `ir::Program`, so it runs in `prepare_executor` and
`prepare_prover` at handle-build time (not per call).
`prepare_verifier` skips it — binding-digest check at verify time
gates mismatched programs.

## 6. Target module layout (`crates/runtime/src/`)

```text
lib.rs               re-exports only
error.rs             RuntimeError umbrella + narrowed errors (§7)
options.rs           PreparedOptions (§5.1)
prover.rs            PreparedProver + prepare_prover
verifier.rs          PreparedVerifier + prepare_verifier
executor.rs          PreparedExecutor + prepare_executor (new)
execution.rs         execute_batch impl + query exec + ExecutionJournal, ExecutionReceipt
snapshot.rs          CommittedStateSnapshot + SnapshotCellRecord + borsh codec
statement.rs         PublicStatement materialization helpers
binding.rs           post-state binding-digest wiring
prelude.rs           ContextPreludeSlot / ParamPreludeSlot loaders
pre_stuff.rs         PreStuffInstaller typed API (cfg prove)
semantics.rs         (extant — not touched by SP-5 beyond re-export hygiene)
state_runtime.rs     (extant; already has from_sealed_artifact + from_registered_program)
proof_summary.rs     (extant)
host/                (extant)
bootstrap/           (extant — keep as-is; resolve_*_setup + machine builder shared by all handles)
```

**Hard budget:** no file under `src/**` exceeds 800 LOC. Target mean
300–500. If a file approaches 800 during execution, split further
before calling SP-5 done.

**`semantics.rs` note:** `semantics.rs` is ~51 KB today and sits
outside SP-5's refactor. It is left intact; its only SP-5 obligation
is updating type paths when other modules change. If `semantics.rs`
exceeds 800 LOC (it does — ~1,800 lines after comments), that is
**pre-existing tech debt** tracked separately; splitting it is not
SP-5's job.

## 7. Error narrowing

### 7.1 Shape

```rust
// crates/runtime/src/error.rs

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RuntimeError {
    #[error(transparent)] Setup(#[from] SetupError),
    #[cfg(feature = "prove")]
    #[error(transparent)] Prove(#[from] ProveError),
    #[cfg(feature = "verify")]
    #[error(transparent)] Verify(#[from] VerifyError),
    #[cfg(feature = "verify")]
    #[error(transparent)] Execute(#[from] ExecuteError),
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SetupError {
    #[error("validation: {detail}")]          Validation { detail: String },
    #[cfg(feature = "verify")]
    #[error("machine setup: {0}")]            MachineSetup(#[source] tabula_machine::SetupError),
    #[error("compiler validation: {0}")]      CompilerValidation(#[source] tabula_compiler::CompilerError),
    #[error("extension setup: {0}")]          Extension(#[source] tabula_ext::ExtError),
}

#[cfg(feature = "prove")]
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProveError {
    #[error("witness generation: {detail}")]  WitnessGeneration { detail: String },
    #[error("trace build: {0}")]              TraceBuild(#[source] tabula_core::error::TabulaError),
    #[error("commitment state: {detail}")]    CommitmentState { detail: String },
    #[error("proving: {0}")]                  Proving(#[source] tabula_machine::ProveError),
    #[error(transparent)]                     Execute(#[from] ExecuteError),   // prove subsumes execute
    #[error(transparent)]                     Setup(#[from] SetupError),
}

#[cfg(feature = "verify")]
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum VerifyError {
    #[error("verification: {0}")]             Verification(#[source] tabula_machine::VerificationError),
    #[error("statement build: {detail}")]     StatementBuild { detail: String },
    #[error("validation: {detail}")]          Validation { detail: String },
    #[error(transparent)]                     Setup(#[from] SetupError),
}

#[cfg(feature = "verify")]
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ExecuteError {
    #[error("execution failed: {source}")]
    Execution {
        #[source] source: tabula_core::error::TabulaError,
        instruction_index: Option<usize>,
        tx_index: Option<u32>,
    },
    #[error("validation: {detail}")]          Validation { detail: String },
    #[error(transparent)]                     Setup(#[from] SetupError),
}
```

### 7.2 Feature-gate rationale

`SetupError` is **not** `verify`-gated: `SetupError::Validation` and
`SetupError::CompilerValidation` apply to any runtime-level preparation
step that might run without proof backends (artifact validation, host
registration). `MachineSetup` is the only `verify`-gated variant
inside `SetupError`.

`ExecuteError`, `VerifyError`, and `PreparedExecutor` are `verify`-gated
because they depend on `tabula-machine` chips for transcript
construction. Pure-execution-without-any-machine-types is not a
supported build shape today.

### 7.3 Migration table (SP-4 `RuntimeError` → SP-5)

| Old variant (`RuntimeError::`) | New home                         |
|--------------------------------|----------------------------------|
| `Execution { … }`              | `ExecuteError::Execution`        |
| `ValidationFailed { detail }`  | split: `SetupError::Validation` or `VerifyError::Validation` or `ExecuteError::Validation` depending on call site |
| `CompilerValidation(e)`        | `SetupError::CompilerValidation` |
| `MachineSetup(e)`              | `SetupError::MachineSetup`       |
| `CommitmentState { detail }`   | `ProveError::CommitmentState`    |
| `WitnessGeneration { detail }` | `ProveError::WitnessGeneration`  |
| `TraceBuild(e)`                | `ProveError::TraceBuild`         |
| `StatementBuild { detail }`    | `VerifyError::StatementBuild`    |
| `Proving(e)`                   | `ProveError::Proving`            |
| `Verification(e)`              | `VerifyError::Verification`      |

`RuntimeError::from_extension_setup` / `from_extension_proof` helpers
deleted; routing is via `SetupError::Extension` + per-handle `#[from]
SetupError`.

### 7.4 SDK migration impact

Six SDK call sites match on `RuntimeError::ValidationFailed`:

- `crates/sdk/src/program/runner.rs:293`
- `crates/sdk/src/sdk.rs:328, 332, 407, 411, 431, 435`

Each becomes a match on
`RuntimeError::Verify(VerifyError::Validation { .. })` or the analogous
`Setup`/`Execute` variant, depending on the operation that produced
the error. The migration is mechanical; SDK pattern-match audit is
part of the SP-5 close-out checklist.

## 8. Typed pre-stuff API (`cfg prove`)

### 8.1 The boundary

Today `engine.rs` constructs chip-internal row types directly:

```rust
|row| tabula_chips::relation_table::RelationTableWitnessRow { … }
records.push(InstructionRecord { … });
```

Post-SP-5, these identifiers **do not appear under
`crates/runtime/src/**`**. Guardrail test enforces this (§12).

### 8.2 Logical row types (in `tabula-stark::witness_kit`)

```rust
#[non_exhaustive]
pub struct LogicalRelationTableRow {
    pub label: RelationLabel,
    pub fields: Vec<TypedValue>,
    // …
}

#[non_exhaustive]
pub struct LogicalExecutionPrelude {
    pub opcode: OpcodeTag,
    pub inputs: Vec<TypedValue>,
    pub outputs: Vec<TypedValue>,
    // …
}
```

**Type sourcing.** `TypedValue` and `OpcodeTag` live in `tabula-core`
(or `tabula-types`, verify on execution). `RelationLabel` lives in
`tabula-contract`. `tabula-stark` already depends on core + contract;
no new forbidden deps are introduced. **Under no circumstances does
`tabula-stark` pick up a `tabula-ir` dep** — if a field needs IR,
translate to a core/contract-level type at the runtime boundary.

### 8.3 Installer

```rust
// crates/runtime/src/pre_stuff.rs  (cfg prove)

pub(crate) struct PreStuffInstaller<'a> {
    kits: &'a ChipKitRegistry,
}

impl<'a> PreStuffInstaller<'a> {
    pub fn new(kits: &'a ChipKitRegistry) -> Self { Self { kits } }

    pub fn install_relation_table_rows(
        &self,
        scratch: &mut KitScratch,
        rows: impl IntoIterator<Item = LogicalRelationTableRow>,
    ) -> Result<(), ProveError> { … }

    pub fn install_execution_prelude(
        &self,
        scratch: &mut KitScratch,
        prelude: LogicalExecutionPrelude,
    ) -> Result<(), ProveError> { … }
}
```

Two methods, matching the two current chip-sourced row types. Adding
a new chip adds a new method. The trait-based `InstallableRow`
abstraction is deferred; two call sites is not enough to justify the
indirection. Revisit if SP-5 follow-ups add a third chip.

Each blessed chip in `tabula-chips` gains `install_rows` /
`install_prelude` on its `ChipWitnessKit` impl, translating logical
rows into the chip's private `*WitnessRow` representation.

## 9. `ChipWitnessKit` sealing

### 9.1 Nature of the seal

`ChipWitnessKit` impls live in `tabula-chips`, separate from the
`tabula-stark` crate where the trait is defined. A true type-level
seal (private-supertrait pattern) requires the `Sealed` supertrait to
be unreachable from external crates; but blessed chips must reach it.
Cargo does not distinguish "blessed workspace member" from
"arbitrary downstream" at the type-system level, so the two constraints
cannot both hold.

The seal is therefore **convention + trybuild**:

- `ChipWitnessKit: sealed::Sealed` with `pub mod sealed { pub trait Sealed {} }`
  in `tabula-stark::witness_kit`. Module name signals intent.
- Each blessed chip adds `impl …::sealed::Sealed for MyKit {}` next to
  its existing `impl ChipWitnessKit`.
- Two trybuild probes:
  - **Compile-fail**: external-crate-like fixture without `Sealed`
    impl must not compile. Error text pinned via `.stderr`.
  - **Compile-pass companion**: same fixture with `Sealed` added must
    compile. Guards against trivially-unreachable seals.

### 9.2 Scope note

Third-party chip authoring is out of scope. If it becomes a goal,
the seal upgrades to a runtime registration check or a macro-mediated
handshake — separate design discussion.

## 10. `SnapshotCellRecord` disposition

Grep result:

```
crates/runtime/src/engine.rs:73  struct SnapshotCellRecord { … }
crates/runtime/src/engine.rs:275 SnapshotCellRecord { … }
```

`SnapshotCellRecord` is runtime-internal. It is not embedded in
`ProofEnvelope` or `PublicStatement`; it serves only as the on-wire
form that `materialize_logical_state` emits and `CommittedStateSnapshot`
consumes. It stays in `runtime::snapshot` with a rationale comment:

```rust
// Runtime-internal codec. Never crosses the proof-visible wire
// boundary; therefore not a tabula-contract concern. See SP-5 §10.
```

## 11. Byte-identity gate

SP-4's byte-identity script was retired in `334a1f7`. SP-5
reintroduces one at `scripts/sp5_byte_identity.sh`:

```bash
#!/usr/bin/env bash
# Captures envelope + public-statement bytes on the current HEAD
# and diffs against a captured baseline.
set -euo pipefail
for example in basic membership; do
    cargo run --quiet -p tabula-cli --features prove -- prove "examples/$example"
    sha256sum "target/proofs/$example/proof.bin" "target/proofs/$example/public_statement.json"
done
```

Sequence:

1. Baseline capture on the SP-1.5 HEAD (`main`, commit `48bb08a`):
   run script, save hashes.
2. SP-5 work proceeds on `sp5-runtime-decomposition` branch.
3. Close-out: rerun script on SP-5 HEAD; hashes must match.
4. Any divergence is a bug — SP-5 is a pure refactor.

The script lives under `scripts/`, is kept after SP-5 as a
regression probe for future refactors, and is documented in
`crates/runtime/README.md`.

## 12. Guardrail tests

Each guardrail lives in its own file so CI output points directly at
the violated invariant.

| Guardrail                                          | File                                                  |
|----------------------------------------------------|-------------------------------------------------------|
| No `tabula_chips::*Row` in `runtime/src/**`        | `crates/runtime/tests/no_chip_rows_in_runtime.rs`     |
| All 3 handles `Send + Sync + 'static`              | `crates/runtime/tests/prepared_handle_bounds.rs`      |
| `From<{Prove,Verify,Execute,Setup}Error>` present  | `crates/runtime/tests/error_conversions.rs`           |
| `PreparedExecutor` / `prepare_executor` public     | `crates/runtime/tests/prepared_executor_symmetry.rs`  |
| External `impl ChipWitnessKit` fails, blessed ok   | `crates/stark/tests/sealing.rs` (trybuild)            |

## 13. Concurrency and determinism

All three handles are `Send + Sync + 'static`. `prove`, `verify`,
`execute_*` all take `&self`. Per-call mutable state (`KitScratch`,
column artifacts, execution journals, column workspaces) is
stack-local. Safe for concurrent driving: multiple threads may call
any combination of `prove` / `verify` / `execute_*` on the same
handle. Determinism contract: same handle + same input produce
byte-identical output. Existing `prove_twice_on_same_handle_is_byte_identical`
test extends to `execute_twice_*` and `verify_twice_*`.

## 14. Risks and mitigations

| Risk | Mitigation |
|------|------------|
| Byte-identity drift during decomposition | Script baseline on SP-1.5 HEAD; hash-compare at close-out; any diff blocks merge. |
| Dropping per-handle builders breaks DX expectations | `PreparedOptions::standard().with_*()` chaining covers the ergonomic case; single entry point is clearer long-term. |
| Error narrowing breaks SDK match arms | Six call sites pre-enumerated in §7.4; mechanical rewrite. |
| Convention seal slips (external impl compiles) | Trybuild compile-fail + compile-pass probes in CI. |
| `semantics.rs` pre-existing size drags into SP-5 scope | Out of scope; not touched beyond type-path updates. Tracked separately. |
| `PreStuffInstaller` method count grows as chips are added | Accept linear growth for now; revisit `InstallableRow` trait if a 3rd chip appears. |

## 15. Open decisions

- **`TypedValue` / `OpcodeTag` exact crate**: likely `tabula-core` or
  `tabula-types`. Verify on Task 13 (`tabula-stark` logical row
  extraction); update this doc's §8.2 if choice diverges.
- **`SetupError` naming collision with `tabula_machine::SetupError`**:
  expect no collision at use sites (callers fully-qualify the machine
  one); if it bites during implementation, rename to
  `RuntimeSetupError`.

## 16. Ordering of implementation tasks

Tasks land as separate commits. Byte-identity script runs after Tasks
with proof-path changes.

0. **Baseline capture.** Write `scripts/sp5_byte_identity.sh`; capture
   baseline hashes on SP-1.5 HEAD; save to
   `docs/superpowers/specs/2026-04-19-sp5-byte-identity-baseline.txt`.
1. **Extract `snapshot.rs`**: pull `CommittedStateSnapshot`,
   `SnapshotCellRecord` + codec out of `engine.rs`. Build + `cargo
   check`.
2. **Extract `statement.rs`**: PublicStatement materialization.
3. **Extract `binding.rs`**: post-state binding-digest wiring.
4. **Extract `prelude.rs`**: context/param prelude slot loaders.
5. **Extract `execution.rs`**: `execute_batch` + query + receipt
   types. Free functions over `PreparedRuntimeState` borrows.
6. **Introduce `options.rs` (`PreparedOptions`)**, migrate
   `prepare_prover` / `prepare_verifier` to
   `prepare_*(artifact, &opts)`. **Drop `PreparedProverBuilder` /
   `PreparedVerifierBuilder`**. Migrate call sites (SDK, CLI, tests).
   Run byte-identity script — must match baseline.
7. **Introduce `PreparedExecutor` + `prepare_executor`.** Move
   `TabulaRuntime::execute_batch` semantics into `PreparedExecutor`.
   Guardrail test (symmetry).
8. **Delete `TabulaRuntime` / `RuntimeBuilder`.** Migrate
   `crates/compiler/tests/cutover.rs`, `crates/runtime/src/prover.rs`
   test scaffolding, `crates/cli` receipt_bridge references (`interop`
   one is a different symbol — verify).
9. **Narrow errors.** Introduce `error.rs` with `RuntimeError`
   umbrella + `SetupError` / `ProveError` / `VerifyError` /
   `ExecuteError`. Migrate all 6 SDK call sites and any internal
   callers. Guardrail test (`error_conversions.rs`).
10. **Typed pre-stuff.** Introduce
    `LogicalRelationTableRow` / `LogicalExecutionPrelude` in
    `tabula-stark::witness_kit`. Migrate runtime pre-stuff sites.
    Chip-row guardrail test.
11. **Seal `ChipWitnessKit`.** Add `sealed::Sealed`; each blessed chip
    adds one impl line. Trybuild compile-fail + compile-pass probes.
12. **`#[non_exhaustive]` sweep** on `VerifierState`,
    `PreparedOptions`, any remaining public prepared-handle types.
13. **Final audit.** `wc -l` check (< 800 LOC per file); module doc
    headers present; module purpose in one sentence each. Full
    workspace `cargo build --no-default-features`,
    `--features verify`, `--features prove`; `cargo test --workspace
    --all-features`; clippy `-D warnings`.
14. **Byte-identity close-out.** Rerun script; compare to baseline.
    Any diff blocks the SP.
15. Update `crates/runtime/README.md` for the three-handle shape.
    Mark umbrella SP-5 Landed; close §7 Open Decisions.

Tasks 1–5 are mechanical extractions and can be serialized quickly.
Task 6 (options + builder drop) and Task 9 (error narrowing) are
the substantive surface changes. Task 7 (executor introduction) is
the scope-completing step.

## 17. Completion criteria

- `crates/runtime/src/engine.rs` does not exist.
- No file under `crates/runtime/src/**` exceeds 800 LOC
  (exception: `semantics.rs` — pre-existing, out of scope).
- Three prepared handles public: `PreparedProver`, `PreparedVerifier`,
  `PreparedExecutor`. All `Send + Sync + 'static`. Built by
  `prepare_*(artifact, &opts)` free functions. No per-handle builder
  types exported.
- `PreparedOptions` exists and is consumed by all three `prepare_*`.
- `TabulaRuntime` / `RuntimeBuilder` symbols removed. CLI, SDK,
  examples, tests migrated.
- `RuntimeError` is `#[non_exhaustive]` umbrella; narrowed errors
  (`ProveError`, `VerifyError`, `ExecuteError`, `SetupError`) are the
  per-handle surface; all `From` impls compile; guardrail test green.
- Zero `tabula_chips::*Row` identifiers in `crates/runtime/src/**`
  (guardrail test).
- `ChipWitnessKit` sealed; trybuild compile-fail + compile-pass
  probes green.
- `VerifierState`, `PreparedOptions`, remaining prepared-handle types
  marked `#[non_exhaustive]`.
- `SnapshotCellRecord` stays in `runtime::snapshot` with documented
  rationale.
- `cargo build --workspace --no-default-features` green.
- `cargo build --workspace --features verify` green.
- `cargo build --workspace --features prove` green.
- `cargo test --workspace --all-features` green.
- `cargo clippy --workspace --all-features --all-targets -- -D warnings`
  green.
- `scripts/sp5_byte_identity.sh` produces hashes matching the
  SP-1.5-HEAD baseline (`examples/basic`, `examples/membership`).
- Dependency-direction invariants from umbrella §3 still hold.
- `crates/runtime/README.md` updated.
- Umbrella doc marks SP-5 Landed.

## 18. References

- Umbrella: [`2026-04-18-architecture-refactoring-design.md`](./2026-04-18-architecture-refactoring-design.md)
- SP-1.5: [`2026-04-19-sp1.5-sealed-artifact-design.md`](./2026-04-19-sp1.5-sealed-artifact-design.md)
- SP-4: [`2026-04-19-sp4-runtime-prepared-handles-design.md`](./2026-04-19-sp4-runtime-prepared-handles-design.md)
- SP-3: [`2026-04-19-sp3-witness-chip-kit-design.md`](./2026-04-19-sp3-witness-chip-kit-design.md)
- Canonical architecture: [`docs/design/architecture.md`](../../design/architecture.md)
- Project posture: `.claude/CLAUDE.md`
