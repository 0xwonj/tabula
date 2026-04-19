# SP-5 — Runtime Decomposition + Executor Symmetry

> Status: proposed design (rewrite 2 — 2026-04-19, incorporates 4-reviewer feedback)
> Parent: [architecture refactoring umbrella](./2026-04-18-architecture-refactoring-design.md) §2.3, §2.5, §4 SP-5
> Predecessors: SP-4 landed (`334a1f7`); SP-1.5 landed (`48bb08a`)
> Audience: SP-5 implementer + reviewers

## 1. Goal

Finish the runtime prepared-handle story begun in SP-4. Promote the
residual `TabulaRuntime` facade into a third symmetric handle
(`PreparedExecutor`), decompose the 3,139-LOC `engine.rs` into
role-focused modules, narrow the error surface per handle, formalize
the "runtime pre-stuff" seam as a typed API, tighten the
`ChipWitnessKit` authoring surface, and mark public prepared-handle
types `#[non_exhaustive]`.

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
`ValidatedProgram` (weakens the name). The asymmetry is structural and
stays.

No unifying `AsPreparedInput` trait is introduced: that either hides
IR-requirements behind a generic bound (poor diagnostics) or forces a
`registered.sealed()` projection at the wrong layer. `RegisteredProgram`
already exposes `sealed()` so verifier-shaped helpers can be fed from
prover sites without ceremony.

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
each level (§7). All three feature shapes are exercised in CI as
guardrails (§12), not only in the close-out checklist.

## 4. Scope

### 4.1 In scope

1. `PreparedExecutor` + `prepare_executor`. `TabulaRuntime` and
   `RuntimeBuilder` removed (not deprecated — clean break per
   `.claude/CLAUDE.md`).
2. `engine.rs` decomposed into role-focused modules (§6).
3. Narrowed per-handle errors: `ProveError`, `VerifyError`,
   `ExecuteError`, `SetupError`. `RuntimeError` is the
   `#[non_exhaustive]` umbrella with `#[from]` from each narrowed
   type (§7). **No `#[from]` conversions between narrowed enums**
   (avoids two-path `SetupError → ProveError` ambiguity).
4. **Single API entry point per handle**: `prepare_*(artifact, &opts)`
   free functions. **Per-handle builders removed** after a
   wrappers-then-delete migration (§16 ordering).
   `PreparedOptions` is the knob surface with `try_standard()` +
   `.with_*()` chaining (§5).
5. Typed pre-stuff API: chip-specific row identifiers
   (`InstructionRecord`, `RelationTableWitnessRow`, …) absent from
   `crates/runtime/src/**`. Replaced by logical row types owned in
   `tabula-stark::witness_kit` and installed through
   `PreStuffInstaller` (§8).
6. `ChipWitnessKit` authoring convention tightened (§9): `sealed::Sealed`
   supertrait + trybuild compile-fail **and** compile-pass probes.
   Explicitly **not** a type-level seal (documented).
7. `VerifierState`, `PreparedOptions`, and all prepared-handle public
   types marked `#[non_exhaustive]`. Public structs expose
   fields through accessors, not `pub` struct literals (§5).
8. `SnapshotCellRecord` codec disposition resolved (§10).
9. Byte-identity gate reintroduced as a shell script that matches the
   real two-step CLI flow (§11).
10. Guardrail tests (§12), including feature-matrix smoke tests.

### 4.2 Out of scope

- SDK thinning, `NEXT_ENVIRONMENT_FINGERPRINT` removal → SP-6.
- `tabula-types` / `tabula-profile` / `tabula-ir` READMEs → SP-6.
- 9 → 15 bus doc drift → SP-6.
- Feature matrix unification across the workspace → SP-7.
- NF-1/2/3/4 validation + `--nf-elision` → SP-8.
- `crates/runtime/src/semantics.rs` splitting. The file is ~1,370
  LOC and sits outside SP-5's refactor; only its import paths update.
  Treating it here would muddy the byte-identity gate. Tracked as a
  follow-up in the umbrella §7 open decisions.
- Infallible rewrite of `TypeRuntimeRegistry::seeded()` /
  `EncodingRuntimeRegistry::seeded()`. The seeded paths are
  never-fails-in-practice but the type says `Result`; `PreparedOptions`
  honors the type rather than aspirating over it (§5.1).

## 5. API shape

### 5.1 Single entry point

```rust
// crates/runtime/src/options.rs

#[non_exhaustive]
pub struct PreparedOptions {
    host_environment: HostEnvironment,
    machine_stark_config: TabulaStarkConfig,
    root_backend: RootBackend,
}

impl PreparedOptions {
    /// Standard built-in options. Fallible today because
    /// `HostEnvironment::standard()` recurses into seeded runtime
    /// registries that return `Result`. An infallible refactor of the
    /// seeds is tracked as a follow-up and deliberately out of SP-5
    /// scope.
    pub fn try_standard() -> Result<Self, SetupError>;

    pub fn with_host_environment(self, env: HostEnvironment) -> Self;
    pub fn with_machine_stark_config(self, cfg: TabulaStarkConfig) -> Self;
    pub fn with_root_backend(self, backend: RootBackend) -> Self;

    pub fn host_environment(&self) -> &HostEnvironment;
    pub fn machine_stark_config(&self) -> &TabulaStarkConfig;
    pub fn root_backend(&self) -> &RootBackend;
}
```

`RootBackend` is one name across feature shapes:

```rust
// crates/runtime/src/options.rs

#[cfg(feature = "prove")]
#[derive(Clone)]
pub struct RootBackend(pub(crate) tabula_ext::root::RootBackendBundle);

#[cfg(all(feature = "verify", not(feature = "prove")))]
#[derive(Clone)]
pub struct RootBackend(pub(crate) Arc<dyn tabula_ext::root::RootProofBackend>);

impl RootBackend {
    #[cfg(feature = "prove")]
    pub fn from_bundle(bundle: tabula_ext::root::RootBackendBundle) -> Self { Self(bundle) }
    #[cfg(all(feature = "verify", not(feature = "prove")))]
    pub fn from_proof_backend(backend: Arc<dyn tabula_ext::root::RootProofBackend>) -> Self { Self(backend) }
}
```

Call sites `PreparedOptions::try_standard()?.with_root_backend(…)` work
identically on any feature shape; only the value supplied differs.

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

No `PreparedProverBuilder` / `PreparedVerifierBuilder` types post-SP-5.
The migration lands in two steps (§16): free functions are introduced
as thin sugar over the existing builders and all SDK/CLI/testing call
sites are migrated first; only then are the builder types deleted.

Rationale: three constructor shapes (free fn + builder + options
chain) is API sprawl. One shape scaling to three handles is simpler
and matches clean-break posture.

### 5.2 Handle shape

Every handle exposes `&self` operations. Per-call mutable state
(scratch buffers, column artifacts, execution journals) lives on
the stack for the duration of one call. Same handle + same input
→ byte-identical output. All three are `Send + Sync + 'static`.

Public struct fields are private; access is via accessors. Construction
and extension go through `#[non_exhaustive]` + `with_*` / constructor
functions.

```rust
impl PreparedProver {
    pub fn prove(&self, input: &ProveInput<'_>) -> Result<ProveResult, ProveError>;
    pub fn prove_and_verify(
        &self,
        verifier: &PreparedVerifier,
        input: &ProveInput<'_>,
    ) -> Result<VerifiedResult, ProveError>;
}

impl PreparedVerifier {
    pub fn verify(
        &self,
        proof: &TabulaProof,
        expected: &PublicStatement,
    ) -> Result<BoundStatement, VerifyError>;
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
}
```

`PreparedExecutor` is stateless w.r.t. snapshots; caller owns snapshot
mutation. Concurrent driving of any combination of `prove` / `verify` /
`execute_*` on the same handle is safe; interleaving those against a
caller-held mutable snapshot is the caller's concurrency problem, not
the handle's (§13).

### 5.3 `validate_core_first_program` placement

The "reject capability calls outside the native proving subset" check
requires `ir::Program`. It lives in `bootstrap::program` and runs once
per handle build: inside `prepare_executor` and `prepare_prover`.
`prepare_verifier` skips it — binding-digest check at verify time
gates mismatched programs.

When a single call site builds both prover and executor handles over
the same `Arc<RegisteredProgram>`, the validation runs twice. This is
acceptable (one-shot per handle build, not per call); memoization on
`RegisteredProgram` is a follow-up if it ever measures hot. Documented
here so no reader "optimizes" it silently.

## 6. Target module layout (`crates/runtime/src/`)

```text
lib.rs               re-exports only
error.rs             RuntimeError umbrella + narrowed errors (§7)
options.rs           PreparedOptions + RootBackend (§5.1)
prover.rs            PreparedProver + prepare_prover
verifier.rs          PreparedVerifier + prepare_verifier
executor.rs          PreparedExecutor + prepare_executor (new)
execution.rs         execute_batch impl + query exec + ExecutionJournal, ExecutionReceipt
snapshot.rs          CommittedStateSnapshot + SnapshotCellRecord + borsh codec
statement.rs         PublicStatement materialization helpers + post-state binding-digest wiring
prelude.rs           ContextPreludeSlot / ParamPreludeSlot loaders
prepared_state.rs    PreparedRuntimeState + PreparedRuntimeBuild (the central type every handle holds)
proof_artifacts.rs   (cfg prove) prepare_proof_artifacts, synthesize_missing_init_cells,
                     prepare_column_slot, prepare_proof_machine_input, PreparedColumnSlot,
                     PreparedColumnArtifacts, PreparedArtifacts
pre_stuff.rs         (cfg prove) PreStuffInstaller typed API
state_runtime.rs     ResolvedStateRuntime (extant; preserved)
proof_summary.rs     (extant)
semantics.rs         (extant; not touched by SP-5 beyond import paths — see §4.2)
host/                (extant)
bootstrap/           (shared setup nucleus — see §6.1)
```

**Hard budget:** no file under `src/**` exceeds 800 LOC, with one
documented exception: `semantics.rs` (~1,370 LOC, out of scope per
§4.2). Target mean 300–500 LOC. If a file approaches 800 during
execution, split further before calling SP-5 done.

**Why `proof_artifacts.rs` is called out separately.** The
`prepare_proof_artifacts` cluster (`engine.rs:1115-1940`) is ~550 LOC
and prove-gated. Placing it in `prover.rs` blows the budget; placing
it in `execution.rs` conflates verify-gated execute code with
prove-gated assembly. It earns its own file.

**Why `prepared_state.rs` is called out separately.** Every extracted
module touches `PreparedRuntimeState` (10 fields, half `#[cfg]`-gated,
`pub(crate)` today). Without naming its home, the first extraction PR
picks arbitrarily and the rest cascade. `prepared_state.rs` owns
`PreparedRuntimeState` + `PreparedRuntimeBuild` so every other module
imports from one place.

**Why `binding.rs` is absent.** Post-state binding-digest wiring is
~40 LOC of helper; a standalone file is noise. Folded into
`statement.rs` alongside public-statement construction.

**Whole-module `#[cfg]` convention.** Modules that are entirely
prove-gated (`proof_artifacts.rs`, `pre_stuff.rs`) use
`#![cfg(feature = "prove")]` at the top; likewise
`#![cfg(feature = "verify")]` for whole verify-gated modules. Internal
items do not repeat the gate. Prevents cfg-attribute proliferation
inside modules.

### 6.1 `bootstrap/` is the shared setup nucleus

`bootstrap/` already exports the authoritative shared setup functions:
`resolve_sealed_artifact_setup`, `resolve_program_setup`,
`build_registered_program_machine`, `execution_backends_for`,
`validate_core_first_program`. It is 192 LOC across three files and is
the right shape today; SP-5 grows it rather than dismantling it.

SP-5 adds to `bootstrap/`:

- No new modules (pre-stuff and options live in top-level
  `runtime::src/` per §6).
- Continued role as the single seam where sealed-artifact vs
  registered-program dispatch resolves.

`bootstrap/` stays a directory. Flattening to a single file loses the
(small) organizational benefit of separating machine-builder wiring
from program-setup resolution. 192 LOC is small, but the separation
remains useful for cfg-gating hygiene.

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
    #[error("verification (post-prove): {0}")] PostVerify(#[source] tabula_machine::VerificationError),
}

#[cfg(feature = "verify")]
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum VerifyError {
    #[error("verification: {0}")]             Verification(#[source] tabula_machine::VerificationError),
    #[error("statement build: {detail}")]     StatementBuild { detail: String },
    #[error("validation: {detail}")]          Validation { detail: String },
}

#[cfg(feature = "verify")]
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ExecuteError {
    #[error("execution failed")]
    Execution {
        #[source] source: tabula_core::error::TabulaError,
        instruction_index: Option<usize>,
        tx_index: Option<u32>,
    },
    #[error("validation: {detail}")]          Validation { detail: String },
}
```

**No `#[from]` conversions between narrowed enums.** Setup failures
inside `prepare_prover` / `prepare_verifier` / `prepare_executor` do
**not** produce `ProveError::Setup(SetupError)` or
`VerifyError::Setup(SetupError)` — there is no such variant. Instead,
the `prepare_*` functions return `Result<_, ProveError>` /
`VerifyError` / `ExecuteError` and explicitly convert setup errors
into the per-handle narrow form at the call site that observes them
(e.g., `SetupError::Validation` observed during prover assembly maps
to `ProveError::WitnessGeneration` or `CommitmentState` with
appropriate detail string). One path from each source type to
`RuntimeError`. Downstream `matches!(e, RuntimeError::Setup(_))` is
unambiguous.

`ExecuteError::Execution`'s `#[error]` message does not interpolate
`{source}` — `thiserror` renders the source via the error chain; a
second inline `{source}` produces duplicated Display output. Fixed
in the shape above.

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

| Old variant (`RuntimeError::`) | New home |
|---|---|
| `Execution { … }` | `ExecuteError::Execution` |
| `ValidationFailed { detail }` | split: `SetupError::Validation` (in `prepare_*`) / `VerifyError::Validation` (in `verify`) / `ExecuteError::Validation` (in `execute_*`) |
| `CompilerValidation(e)` | `SetupError::CompilerValidation` |
| `MachineSetup(e)` | `SetupError::MachineSetup` |
| `CommitmentState { detail }` | `ProveError::CommitmentState` |
| `WitnessGeneration { detail }` | `ProveError::WitnessGeneration` |
| `TraceBuild(e)` | `ProveError::TraceBuild` |
| `StatementBuild { detail }` | `VerifyError::StatementBuild` |
| `Proving(e)` | `ProveError::Proving` |
| `Verification(e)` | `VerifyError::Verification` |

`RuntimeError::from_extension_setup` / `from_extension_proof` helpers
deleted. Routing of `ExtError` is via `SetupError::Extension` +
per-site explicit conversion.

### 7.4 SDK migration (two-pass)

Current SDK call sites match on `RuntimeError::ValidationFailed`:

- `crates/sdk/src/program/runner.rs:293` (encode error synthesized as
  `ValidationFailed` — anti-pattern; see note below)
- `crates/sdk/src/sdk.rs:328, 332, 407, 411, 431, 435`

Each one asserts *the failure was of the validation family*, not a
specific narrow variant. A direct rename would silently approve
mis-routed errors.

**Migration is two-pass:**

1. **Pass 1 (widen):** rewrite each `matches!(e, RuntimeError::ValidationFailed { .. })`
   to `matches!(e, RuntimeError::Setup(_) | RuntimeError::Verify(_) | RuntimeError::Execute(_))`.
   Tests should still pass.
2. **Pass 2 (tighten):** for each site, determine the expected narrow
   variant from context and tighten to it. Any site where the tightened
   match fails is flagged as a routing bug and fixed upstream (not
   papered over).

The `sdk/src/program/runner.rs:293` site constructs a runtime error
from an encode failure. This is SDK-synthesizing-runtime — an
anti-pattern. Fix: route encode errors through `SdkError::Encoding`
(or equivalent SDK-native variant), not through `RuntimeError`. SP-5
removes this synthesis; SP-6 (SDK thinning) finishes the SDK-side
error taxonomy.

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
(or `tabula-types`, verified on implementation). `RelationLabel` lives
in `tabula-contract`. `tabula-stark` already depends on core + contract;
no new forbidden deps. Under no circumstances does `tabula-stark`
pick up a `tabula-ir` dep — if a field needs IR, translate to a
core/contract-level type at the runtime boundary.

### 8.3 Installer

```rust
// crates/runtime/src/pre_stuff.rs  (#![cfg(feature = "prove")])

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

## 9. `ChipWitnessKit` authoring convention

### 9.1 Not a type-level seal — honest framing

Third-party chip authoring is not a goal. We want workspace chips to
be clearly enumerated and external `impl ChipWitnessKit` to raise a
visible signal in review + CI. Cargo does not distinguish "blessed
workspace member" from "external downstream" at the type-system level,
so **a true private-supertrait seal is not achievable** for
`ChipWitnessKit`. The design does not pretend otherwise.

The mechanism SP-5 ships is an **authoring convention** enforced in
CI:

- `ChipWitnessKit: sealed::Sealed` with `pub mod sealed { pub trait Sealed {} }`
  in `tabula-stark::witness_kit`. The `sealed` module name signals
  intent.
- Each blessed chip adds `impl …::sealed::Sealed for MyKit {}` next to
  its existing `impl ChipWitnessKit`.
- Two trybuild probes:
  - **Compile-fail**: fixture that impls `ChipWitnessKit` without
    `Sealed` must not compile. Error text pinned via `.stderr`.
  - **Compile-pass companion**: same fixture with `Sealed` added must
    compile. Guards against trivially-unreachable seals (the seal
    being enforced because `Sealed` itself is unreachable).

This catches the accidental case (forgot to add `Sealed`) and the
review-signal case (a new `impl Sealed` line in a non-blessed crate
stands out). It does not block a determined implementer. If
third-party chip authoring becomes a goal, this upgrades to runtime
registration validation — separate design discussion.

§12 encodes the two probes as guardrail tests.

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

SP-4's byte-identity script was retired in `334a1f7`. SP-5 reintroduces
one at `scripts/sp5_byte_identity.sh` that matches the real CLI
two-step flow (`execute` → `prove`, see `crates/cli/README.md:44`):

```bash
#!/usr/bin/env bash
set -euo pipefail

EXAMPLES=(basic membership)   # expand as examples/ grows
WORK="${WORK:-$(mktemp -d)}"

cargo build --quiet -p tabula-cli --features prove

for ex in "${EXAMPLES[@]}"; do
    dir="$WORK/$ex"
    rm -rf "$dir" && mkdir -p "$dir"
    target/debug/tabula-cli example "$ex" --dir "$dir"
    target/debug/tabula-cli execute \
        --program "$dir/program.tab" \
        --state "$dir/state.json" \
        --batch "$dir/batch.json" \
        --context "$dir/context.json" \
        --receipt-out "$dir/receipt.bin"
    target/debug/tabula-cli prove \
        --program "$dir/program.tab" \
        --receipt "$dir/receipt.bin" \
        --proof-out "$dir/proof.bin" \
        --public-statement-out "$dir/public_statement.json" \
        --summary-out "$dir/proof_summary.json"
    sha256sum "$dir/proof.bin" "$dir/public_statement.json"
done
```

Sequence:

1. **Baseline capture** on SP-1.5 HEAD (`main`, commit `48bb08a`):
   run script, save sha256 hashes to
   `docs/superpowers/specs/2026-04-19-sp5-byte-identity-baseline.txt`.
2. SP-5 work proceeds on `sp5-runtime-decomposition` branch.
3. Close-out: rerun script on SP-5 HEAD; hashes must match.
4. Any divergence is a bug — SP-5 is a pure refactor.

**Coverage.** Two examples is the starting corpus. Any example added
to `examples/` during SP-5 execution is added to `EXAMPLES=`.
Parameterized-program, multi-chip, and query-only coverage are SP-8
concerns (adequate execution coverage gates there); SP-5 targets the
examples that already exist.

**Baseline update path.** If SP-5 produces a legitimate hash delta
(e.g., a `tabula-stark` config bump lands concurrently), the baseline
file is regenerated in the same commit as the delta, with the commit
message stating the externally-caused reason. Routine refactor commits
do not update the baseline.

The script lives under `scripts/`, is kept after SP-5 as a regression
probe, and is referenced from `crates/runtime/README.md`.

## 12. Guardrail tests

Each guardrail lives in its own file so CI output points directly at
the violated invariant.

| Guardrail | File |
|---|---|
| No `tabula_chips::*Row` in `runtime/src/**` (source grep, not re-exports) | `crates/runtime/tests/no_chip_rows_in_runtime.rs` |
| All 3 handles `Send + Sync + 'static` | `crates/runtime/tests/prepared_handle_bounds.rs` |
| `From<narrow> for RuntimeError` present; **no `From` between narrowed enums** (negative probe) | `crates/runtime/tests/error_conversions.rs` |
| `PreparedExecutor` / `prepare_executor` public symbols | `crates/runtime/tests/prepared_executor_symmetry.rs` |
| External `impl ChipWitnessKit` fails (compile-fail); blessed companion compiles (compile-pass) | `crates/stark/tests/sealing.rs` (trybuild) |
| `cargo build --workspace --no-default-features` compiles | `scripts/sp5_feature_matrix.sh` + CI job |
| `cargo build --workspace --features verify` compiles | `scripts/sp5_feature_matrix.sh` + CI job |
| `cargo build --workspace --features prove` compiles | `scripts/sp5_feature_matrix.sh` + CI job |

The feature-matrix script is part of Task 0 and runs in CI on every
PR that touches `crates/runtime/**` or `crates/sdk/**`. It is not a
close-out-only check.

## 13. Concurrency and determinism

All three handles are `Send + Sync + 'static`. `prove`, `verify`,
`execute_*` all take `&self`. Per-call mutable state (`KitScratch`,
column artifacts, execution journals, column workspaces) is
stack-local. Safe for concurrent driving: multiple threads may call
any combination of `prove` / `verify` / `execute_*` on the same
handle.

Determinism contract: same handle + same input produce byte-identical
output. Existing `prove_twice_on_same_handle_is_byte_identical` test
extends to `execute_twice_*` and `verify_twice_*`.

Snapshot mutation is caller-owned. Interleaving `execute_*` calls
against a shared-mutable `CommittedStateSnapshot` across threads is
the caller's synchronization problem, not the handle's.

## 14. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Byte-identity drift during decomposition | Baseline captured on SP-1.5 HEAD; hash-compare at close-out blocks merge on any diff. |
| Dropping per-handle builders breaks DX expectations | `PreparedOptions::try_standard()?.with_*()` chaining covers the ergonomic case; single entry point is clearer long-term. Migration is wrappers-then-delete (§16) so all call sites move before the builder types disappear. |
| Error narrowing breaks SDK match arms | Six call sites pre-enumerated (§7.4); two-pass migration (widen then tighten) prevents silent routing bugs. |
| Convention seal slips (external impl compiles) | Trybuild compile-fail + compile-pass probes in CI (§9.1). |
| Feature-matrix regression lands outside SP-5 close-out | `scripts/sp5_feature_matrix.sh` wired into CI on PR (§12), not just final audit. |
| `semantics.rs` pre-existing size drags into SP-5 scope | Explicit §4.2 exclusion; follow-up tracked in umbrella §7 open decisions. |
| `PreStuffInstaller` method count grows as chips are added | Accept linear growth for 2 call sites; revisit `InstallableRow` trait if a 3rd chip appears. |
| `PreparedExecutor` being `verify`-gated precludes execute-only slim build | Accept: an execute-only path is not a current goal. If it becomes one, narrow the gate via a new `execute` feature; do not retrofit under SP-5. |
| Duplicate `validate_core_first_program` cost when one site builds both prover and executor | Accept: one-shot per handle build, not hot. Memoize on `RegisteredProgram` only if measurement shows it matters. |

## 15. Open decisions

- **`TypedValue` / `OpcodeTag` exact crate**: likely `tabula-core` or
  `tabula-types`. Verify on Task 11 (`tabula-stark` logical row
  extraction); update §8.2 if choice diverges.
- **`SetupError` naming collision with `tabula_machine::SetupError`**:
  expect no collision at use sites (callers fully-qualify the machine
  one); if it bites during implementation, rename to
  `RuntimeSetupError`. Guardrail test in `error_conversions.rs` covers
  the decision either way.
- **Infallible `HostEnvironment::standard()` follow-up**: the seeded
  registry paths are `Result` but never fail in practice. A refactor
  making them `const`-constructed is desirable but out of SP-5 scope.
  Track as a follow-up umbrella open-decision entry.

## 16. Ordering of implementation tasks

Tasks land as separate commits. Byte-identity script runs after any
task that could affect proof bytes.

0. **Baseline capture + CI wiring.** Write
   `scripts/sp5_byte_identity.sh` matching the real CLI flow; capture
   baseline hashes on SP-1.5 HEAD; save to
   `docs/superpowers/specs/2026-04-19-sp5-byte-identity-baseline.txt`.
   Write `scripts/sp5_feature_matrix.sh` and wire it + the byte-identity
   script into CI for PRs touching `crates/runtime/**` or
   `crates/sdk/**`.
1. **Extract `snapshot.rs`**: pull `CommittedStateSnapshot`,
   `SnapshotCellRecord` + codec out of `engine.rs`. `cargo check` green
   on all three feature shapes.
2. **Error narrowing** (lifted early). Introduce `error.rs` with
   `RuntimeError` umbrella + `SetupError` / `ProveError` /
   `VerifyError` / `ExecuteError`. Wire all current runtime call sites
   to new variants. **Pass 1** of SDK migration (widen to family-level
   matches). Guardrail test (`error_conversions.rs`). Byte-identity
   re-run. Landing this early means subsequent extractions already use
   the new taxonomy; no double-touch.
3. **Extract `statement.rs`**: PublicStatement materialization +
   post-state binding-digest wiring (formerly planned as separate
   `binding.rs`).
4. **Extract `prelude.rs`**: context/param prelude slot loaders.
5. **Extract `prepared_state.rs`**: `PreparedRuntimeState` +
   `PreparedRuntimeBuild`. All downstream modules now import from this
   one home.
6. **Extract `execution.rs`**: `execute_batch` + query + receipt types
   as free functions over `&PreparedRuntimeState`.
7. **Extract `proof_artifacts.rs`** (cfg prove): the
   `prepare_proof_artifacts` cluster (`synthesize_missing_init_cells`,
   `prepare_column_slot`, `prepare_proof_machine_input`,
   `PreparedColumnSlot`, `PreparedColumnArtifacts`, `PreparedArtifacts`).
   Byte-identity re-run.
8. **Introduce `options.rs`** (`PreparedOptions` + `RootBackend`).
   Introduce free `prepare_prover` / `prepare_verifier` as **sugar
   over existing builders** (no behavior change). Migrate all SDK
   (`sdk/src/sdk.rs:164, 180, 229`), CLI
   (`crates/cli/src/commands/prove.rs`, `crates/cli/src/handoff/receipt_bridge.rs`),
   and testing (`crates/testing/src/runtime.rs`,
   `crates/testing/src/exec.rs`) call sites to the free functions.
   Byte-identity re-run.
9. **Introduce `PreparedExecutor` + `prepare_executor`** (executor.rs).
   Move `TabulaRuntime::execute_batch` semantics into
   `PreparedExecutor`. Guardrail test (symmetry).
10. **Delete `TabulaRuntime` / `RuntimeBuilder` / `PreparedProverBuilder` /
    `PreparedVerifierBuilder`**. Migrate final holdouts
    (`crates/compiler/tests/cutover.rs` and any remaining
    `interop.rs` re-exports). Byte-identity re-run.
11. **Typed pre-stuff.** Introduce `LogicalRelationTableRow` /
    `LogicalExecutionPrelude` in `tabula-stark::witness_kit`. Migrate
    runtime pre-stuff sites (`engine.rs:1509-1680` or their post-
    extraction home). Chip-row guardrail test. Byte-identity re-run.
12. **Tighten `ChipWitnessKit` authoring convention.** Add
    `sealed::Sealed`; each blessed chip adds one impl line. Trybuild
    compile-fail + compile-pass probes.
13. **`#[non_exhaustive]` + accessor sweep** on `VerifierState`,
    `PreparedOptions`, and any remaining public prepared-handle types.
    Remove `pub` from struct fields where setters exist; add accessors.
14. **SDK migration Pass 2** (tighten). Each `matches!` widened in
    Task 2 is tightened to the specific narrow variant. Any routing
    bug surfaced by tightening is fixed, not papered over.
15. **Final audit.** `wc -l` check (< 800 LOC per file, `semantics.rs`
    excepted); module doc headers present; module purpose in one
    sentence each. Full workspace
    `cargo build --no-default-features`, `--features verify`,
    `--features prove`; `cargo test --workspace --all-features`;
    clippy `-D warnings`.
16. **Byte-identity close-out.** Rerun `sp5_byte_identity.sh`; compare
    to baseline. Any diff blocks the SP.
17. Update `crates/runtime/README.md` for the three-handle shape.
    Mark umbrella SP-5 Landed; close §7 Open Decisions items that SP-5
    resolved.

Tasks 1, 3–7 are mechanical extractions and serialize quickly. Task 2
(error narrowing) lands early to avoid downstream double-touch. Task 8
(options + free-fn wrappers) and Task 10 (builder deletion) bracket
the API-shape change with a safe wrappers-first migration. Task 9
(executor) is the scope-completing surface addition.

## 17. Completion criteria

- `crates/runtime/src/engine.rs` does not exist.
- No file under `crates/runtime/src/**` exceeds 800 LOC except the
  documented `semantics.rs` exception (§4.2).
- Three prepared handles public: `PreparedProver`, `PreparedVerifier`,
  `PreparedExecutor`. All `Send + Sync + 'static`. Built by
  `prepare_*(artifact, &opts)` free functions. No per-handle builder
  types exported.
- `PreparedOptions` + `RootBackend` exist; consumed by all three
  `prepare_*`. `try_standard()` fallible; no aspirational infallible
  claim.
- `TabulaRuntime` / `RuntimeBuilder` / `PreparedProverBuilder` /
  `PreparedVerifierBuilder` removed. CLI, SDK, examples, tests
  migrated.
- `RuntimeError` is `#[non_exhaustive]` umbrella; narrowed errors
  (`ProveError`, `VerifyError`, `ExecuteError`, `SetupError`) are the
  per-handle surface; **no `From` conversions between narrowed
  enums** (guardrail test negative probe).
- Zero `tabula_chips::*Row` identifiers in `crates/runtime/src/**`
  (source grep guardrail).
- `ChipWitnessKit` authoring-convention trybuild compile-fail +
  compile-pass probes green. `sealed::Sealed` is documented as
  convention, not a seal.
- `VerifierState`, `PreparedOptions`, remaining prepared-handle types
  marked `#[non_exhaustive]`; `pub` fields replaced by accessors where
  setters exist.
- `SnapshotCellRecord` stays in `runtime::snapshot` with documented
  rationale.
- `cargo build --workspace --no-default-features` green.
- `cargo build --workspace --features verify` green.
- `cargo build --workspace --features prove` green.
- `cargo test --workspace --all-features` green.
- `cargo clippy --workspace --all-features --all-targets -- -D warnings`
  green.
- `scripts/sp5_byte_identity.sh` (execute→prove flow) produces hashes
  matching the SP-1.5-HEAD baseline for `examples/basic` and
  `examples/membership`.
- `scripts/sp5_feature_matrix.sh` wired into CI on `crates/runtime/**`
  and `crates/sdk/**` PRs (not only close-out).
- Dependency-direction invariants from umbrella §3 still hold.
- `crates/runtime/README.md` updated.
- Umbrella doc marks SP-5 Landed; SP-5-resolved §7 open decisions
  closed; follow-ups (semantics.rs split, infallible seed refactor)
  filed.

## 18. References

- Umbrella: [`2026-04-18-architecture-refactoring-design.md`](./2026-04-18-architecture-refactoring-design.md)
- SP-1.5: [`2026-04-19-sp1.5-sealed-artifact-design.md`](./2026-04-19-sp1.5-sealed-artifact-design.md)
- SP-4: [`2026-04-19-sp4-runtime-prepared-handles-design.md`](./2026-04-19-sp4-runtime-prepared-handles-design.md)
- SP-3: [`2026-04-19-sp3-witness-chip-kit-design.md`](./2026-04-19-sp3-witness-chip-kit-design.md)
- Canonical architecture: [`docs/design/architecture.md`](../../design/architecture.md)
- Project posture: `.claude/CLAUDE.md`
- CLI flow (for byte-identity script): `crates/cli/README.md:44`
