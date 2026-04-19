# SP-5 — Runtime Decomposition + Executor Symmetry

> Status: proposed design
> Date: 2026-04-19
> Parent: [architecture refactoring umbrella](./2026-04-18-architecture-refactoring-design.md) §2.3, §2.5, §4 SP-5
> Predecessors: SP-4 (runtime prepared handles) landed 2026-04-19; SP-1.5 (SealedArtifact introduction) is a hard prerequisite — see §1.5.
> Audience: SP-5 implementer + reviewers

## 1. Goal

Finish the runtime prepared-handle story begun in SP-4. Promote the
residual `TabulaRuntime` facade into a third symmetric handle
(`PreparedExecutor`), decompose the 3,150-LOC `engine.rs` into
role-focused modules, narrow the error surface per handle, formalize
the "runtime pre-stuff" pattern as a typed API, seal `ChipWitnessKit`,
and mark public prepared-handle types `#[non_exhaustive]`.

## 1.5 Prerequisite: SP-1.5 (SealedArtifact)

SP-5 inherits a structurally asymmetric signature set for the three
prepared handles, landed by SP-1.5:

- `prepare_prover(Arc<RegisteredProgram>) -> Result<PreparedProver, ProveError>`
- `prepare_verifier(Arc<SealedArtifact>) -> Result<PreparedVerifier, VerifyError>`
- `prepare_executor(Arc<RegisteredProgram>) -> Result<PreparedExecutor, ExecuteError>` (new in SP-5)

The asymmetry reflects the layering: verifier is a pure binding / static
artifact check (IR-free, once `relation_policy` and `uses_ir_hash` are
sealed at compile time — SP-1.5's contribution), while prover and
executor must lower or execute `ir::ValidatedProgram`. Forcing a
symmetric `Arc<SealedArtifact>` for all three would require either a
`contract → ir` dep (violates umbrella §3) or `SealedArtifact` carrying
`ValidatedProgram` (weakens the name).

SP-5 does **not** introduce `SealedArtifact`, the verifier-signature
flip, or the `RelationPolicy` relocation — those are SP-1.5. SP-5
builds on the post-SP-1.5 state.

## 2. Locked decisions (carried from umbrella + SP-4 directional review)

- **TabulaRuntime → PreparedExecutor promotion.** The execute surface
  becomes a third prepare-once / drive-many handle symmetric with
  `PreparedProver` and `PreparedVerifier`. Not deleted — promoted.
- **ChipWitnessKit sealed.** Only workspace-internal crates may
  implement; third-party chip authoring is deferred. The seal is a
  convention + trybuild compile-fail probe, not a type-level
  unforgeability proof — see §9 for the cargo-topology reason.

## 3. Scope

### 3.1 In-scope

1. `PreparedExecutor` + `prepare_executor` symmetric with the other
   two handles. `TabulaRuntime` / `RuntimeBuilder` removed.
2. `engine.rs` decomposition into role-focused modules
   (§5 lays out the concrete layout).
3. Narrowed per-handle errors: `ProveError`, `VerifyError`,
   `ExecuteError`. `RuntimeError` survives as a `#[non_exhaustive]`
   umbrella with transparent `From` impls for composing callers.
4. Typed runtime pre-stuff API. Chip-specific row type names
   (`InstructionRecord`, `RelationTableWitnessRow`, ...) absent from
   `crates/runtime/src/**`. Replaced by logical-row types owned in
   `tabula-stark::witness_kit` and installed through a typed seam.
5. `ChipWitnessKit` sealed via private-supertrait pattern in
   `tabula-stark`. Trybuild compile-fail probe for external impls.
6. `VerifierState` and public prepared-handle builder/option types
   marked `#[non_exhaustive]`.
7. `SnapshotCellRecord` borsh codec relocated to its canonical home
   (see §6 for the disposition call).
8. Three guardrail tests: no-chip-row-in-runtime, all three handles
   `Send + Sync + 'static`, `From<ProveError|VerifyError|ExecuteError>
   for RuntimeError`.

### 3.2 Out of scope (belongs to other SPs)

- SDK thinning, `NEXT_ENVIRONMENT_FINGERPRINT` removal → SP-6.
- `tabula-types` / `tabula-profile` / `tabula-ir` READMEs → SP-6.
- `docs/design/architecture.md` commitment-tier amendment → SP-6.
- 9 → 15 bus doc drift → SP-6.
- Feature matrix unification → SP-7.
- NF-1/2/3/4 validation + `--nf-elision` → SP-8.
- `SealedArtifact` introduction and verifier-signature flip → SP-1.5
  (prerequisite; see §1.5).

## 4. Improvements beyond the umbrella (proposed here, not yet locked)

The umbrella is a strategic document and occasionally under-specifies
tactics. One refinement surfaces naturally once you read SP-4's landed
code. I propose adopting it in SP-5; it is not a hard requirement to
close the SP.

(An earlier draft also proposed a shared `PreparedRuntimeCore` held
behind `Arc` so callers building all three handles for the same
program would amortize preparation. Retracted on YAGNI review: no
current caller builds all three for the same program, and the
4-symbol public surface would exist only for a hypothetical future
optimization. Per `.claude/CLAUDE.md` clean-break posture, it can land
as its own SP if a real caller needs it.)

### 4.2 Consolidated `PreparedOptions`

**Observation.** SP-4 has three knobs duplicated across builders:
`with_host_environment`, `with_machine_stark_config`,
`with_root_backend_bundle` (+ `with_root_proof_backend[_arc]` on the
verify-only build). Each new handle (executor here) would add its
own clone. Adding a knob means editing N builders.

**Proposal.** One `PreparedOptions` struct; each `prepare_*` function
takes `&PreparedOptions`. Handle-specific builders become optional
sugar.

```rust
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
    pub fn standard() -> Result<Self, SetupError> { ... }
    pub fn with_host_environment(self, ...) -> Self { ... }
    // ... other with_*
}

pub fn prepare_prover(
    registered_program: Arc<RegisteredProgram>,
    opts: &PreparedOptions,
) -> Result<PreparedProver, ProveError> { ... }

pub fn prepare_verifier(
    sealed_artifact: Arc<SealedArtifact>,
    opts: &PreparedOptions,
) -> Result<PreparedVerifier, VerifyError> { ... }

pub fn prepare_executor(
    registered_program: Arc<RegisteredProgram>,
    opts: &PreparedOptions,
) -> Result<PreparedExecutor, ExecuteError> { ... }

// Sugar for the common case:
pub fn prepare_prover_standard(
    registered_program: Arc<RegisteredProgram>,
) -> Result<PreparedProver, ProveError> {
    prepare_prover(registered_program, &PreparedOptions::standard()?)
}
```

**Benefits.** One place to add a knob. Options value can be cheaply
reused across `prepare_{prover,verifier,executor}` even though the
three handles take different artifact types (per §1.5 structural
asymmetry).

**Risks.** SDK and CLI call sites need a migration. The migration is
mechanical and small — SP-4 already landed with per-handle builders,
so the existing call sites are a good proxy for how many edits.

**Default position.** Adopt §4.2 in SP-5.

### 4.3 Explicit non-goals for this design

- Dropping `PreparedProverBuilder` / `PreparedVerifierBuilder`. The
  fluent builder style is a nice DX; we keep it as thin sugar over
  `prepare_*(reg, opts.with_*(…))`.
- `TabulaRuntime` façade deprecation path. No deprecation path —
  `.claude/CLAUDE.md` says clean breaks; remove the symbol.
- Cross-process proof caching. Out of scope; would belong to SDK.

## 5. Target module layout (`crates/runtime/src/`)

```text
lib.rs                          # re-exports only
error.rs                        # RuntimeError umbrella + narrowed errors
options.rs                      # PreparedOptions (§4.2)
prover.rs                       # PreparedProver + prepare_prover (+ builder sugar)
verifier/
  mod.rs                        # PreparedVerifier + prepare_verifier (Arc<SealedArtifact>)
  state.rs                      # VerifierState (public, #[non_exhaustive])
  check.rs                      # verify-path helpers
executor.rs                     # PreparedExecutor + prepare_executor (new)
execution.rs                    # batch exec / query exec / receipt
snapshot.rs                     # CommittedStateSnapshot + borsh codec (disposition in §6)
statement_materialization.rs    # PublicStatement construction helpers
state_binding.rs                # post-state materialization, binding-digest wiring
prelude.rs                      # ContextPreludeSlot / ParamPreludeSlot + loaders
pre_stuff.rs                    # PreStuffInstaller typed API
bootstrap/                      # (extant)
host.rs                         # (extant — HostEnvironment etc.)
proof_summary.rs                # (extant)
semantics.rs                    # (extant)
state_runtime.rs                # (extant; post-SP-1.5 has from_sealed_artifact + from_registered_program)
```

**Hard budget:** no file exceeds 800 LOC. If one does, split further
before calling SP-5 done. Target mean is around 300–400 LOC per file.

**Deliberate deviation from umbrella §2.3.** Umbrella lists
`executor.rs`, `execution.rs`, `snapshot.rs`, `statement_materialization.rs`,
`state_binding.rs` as the new modules. I add three more:
`options.rs` (§4.2 consolidated options), `pre_stuff.rs` (typed install
API), and `prelude.rs` (the context/param slot loaders). All three are
natural seams once the file is read top to bottom.

## 6. `SnapshotCellRecord` borsh codec disposition

**Call sites to inspect before deciding:**

```bash
grep -rn "SnapshotCellRecord\|snapshot_cell_record" crates/
```

**Decision rule.** If the bytes cross a proof-visible boundary
(embedded in `ProofEnvelope` or `PublicStatement`), move the codec to
`tabula-contract`. Otherwise keep it in `runtime::snapshot` with a
rationale comment at the definition.

**Tentative call.** I expect this to be runtime-internal — snapshots
are an execute-time convenience for round-tripping state between
`CommittedStateSnapshot` and canonical bytes, not a proof-visible
artifact. If the grep confirms this, keep it in `runtime::snapshot`
with:

```rust
// Runtime-internal on-disk/in-memory codec for SnapshotCellRecord.
// Never crosses the proof-visible wire boundary; therefore not a
// tabula-contract concern. See SP-5 design §6 + umbrella §2.3.
```

If the grep surprises us, we move to `tabula-contract` and revise.

## 7. Error narrowing

### 7.1 Shape

```rust
// error.rs

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RuntimeError {
    #[error(transparent)] Prove(#[from] ProveError),
    #[error(transparent)] Verify(#[from] VerifyError),
    #[error(transparent)] Execute(#[from] ExecuteError),
    #[error(transparent)] Setup(#[from] SetupError),
}

#[cfg(feature = "prove")]
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ProveError {
    #[error("witness generation: {detail}")]  WitnessGeneration { detail: String },
    #[error("trace build: {0}")]              TraceBuild(#[source] tabula_core::error::TabulaError),
    #[error("commitment state: {detail}")]    CommitmentState { detail: String },
    #[error("proving: {0}")]                  Proving(#[source] tabula_machine::ProveError),
    #[error(transparent)]                     Execute(#[from] ExecuteError),  // prove implies execute
    #[error(transparent)]                     Setup(#[from] SetupError),
}

#[cfg(feature = "verify")]
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum VerifyError {
    #[error("verification: {0}")]             Verification(#[source] tabula_machine::VerificationError),
    #[error("statement build: {detail}")]     StatementBuild { detail: String },
    #[error(transparent)]                     Setup(#[from] SetupError),
}

#[cfg(feature = "verify")]
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ExecuteError {
    #[error("execution failed: {source}")]
    Execution {
        #[source] source: tabula_core::error::TabulaError,
        instruction_index: Option<usize>,
        tx_index: Option<u32>,
    },
    #[error(transparent)]                     Setup(#[from] SetupError),
}

#[cfg(feature = "verify")]
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SetupError {
    #[error("validation: {detail}")]          Validation { detail: String },
    #[error("machine setup: {0}")]            MachineSetup(#[source] tabula_machine::SetupError),
    #[error("compiler validation: {0}")]      CompilerValidation(#[source] tabula_compiler::CompilerError),
    #[error("extension setup: {0}")]          Extension(#[source] tabula_ext::ExtError),
}
```

### 7.2 Notes

- `SetupError` is the shared preparation-time error. It flows
  naturally into each handle's error via `#[from]`, which keeps the
  call sites ergonomic (`?` works everywhere).
- `ProveError::Execute(#[from] ExecuteError)` captures the truth that
  proving subsumes executing. The other direction does not hold;
  execute does not surface `ProveError`.
- Narrower than the umbrella bullet suggested — I drop `SetupCommon`
  as a name (plain `SetupError` reads better) and fold
  `tabula_ext::ExtError` into `SetupError::Extension` rather than
  spreading ad-hoc conversions.

### 7.3 Migration of the existing `RuntimeError` variants

| Old variant                     | New home                                 |
|---------------------------------|------------------------------------------|
| `Execution { ... }`             | `ExecuteError::Execution`                |
| `ValidationFailed { detail }`   | `SetupError::Validation`                 |
| `CompilerValidation(e)`         | `SetupError::CompilerValidation`         |
| `MachineSetup(e)`               | `SetupError::MachineSetup`               |
| `CommitmentState { detail }`    | `ProveError::CommitmentState`            |
| `WitnessGeneration { detail }`  | `ProveError::WitnessGeneration`          |
| `TraceBuild(e)`                 | `ProveError::TraceBuild`                 |
| `StatementBuild { detail }`     | `VerifyError::StatementBuild`            |
| `Proving(e)`                    | `ProveError::Proving`                    |
| `Verification(e)`               | `VerifyError::Verification`              |

`RuntimeError::from_extension_setup` / `from_extension_proof` are
deleted; the routing is now via `SetupError::Extension` plus
per-handle `#[from] SetupError`.

## 8. Typed pre-stuff API

### 8.1 The boundary

Today:

```rust
// engine.rs:1690 (current landed)
|row| tabula_chips::relation_table::RelationTableWitnessRow { ... }

// engine.rs:1272
records.push(InstructionRecord { ... });
```

Target: these identifiers do not appear under `crates/runtime/src/**`.

### 8.2 Logical row types (in `tabula-stark::witness_kit`)

```rust
/// Chip-agnostic relation-table row from the runtime's view.
/// The concrete RelationTableWitnessRow construction is performed
/// inside the relation-table ChipWitnessKit's `install_rows`.
#[non_exhaustive]
pub struct LogicalRelationTableRow {
    pub label: RelationLabel,
    pub fields: Vec<TypedValue>,
    // ... whatever the concrete row currently carries, minus chip wiring
}

/// Chip-agnostic execution prelude record.
#[non_exhaustive]
pub struct LogicalExecutionPrelude {
    pub opcode: OpcodeTag,
    pub inputs: Vec<TypedValue>,
    pub outputs: Vec<TypedValue>,
    // ...
}
```

### 8.3 Installer seam (in `runtime::pre_stuff`)

```rust
pub(crate) struct PreStuffInstaller<'a> {
    kits: &'a ChipKitRegistry,
}

impl<'a> PreStuffInstaller<'a> {
    pub fn new(kits: &'a ChipKitRegistry) -> Self { Self { kits } }

    pub fn install_relation_table_rows(
        &self,
        scratch: &mut KitScratch,
        rows: impl IntoIterator<Item = LogicalRelationTableRow>,
    ) -> Result<(), SetupError> {
        self.kits.relation_table().install_rows(scratch, rows)
            .map_err(|e| SetupError::Validation { detail: e.to_string() })
    }

    pub fn install_execution_prelude(
        &self,
        scratch: &mut KitScratch,
        prelude: LogicalExecutionPrelude,
    ) -> Result<(), SetupError> { ... }
}
```

Called by `PreparedProver::prove` through its own borrow of the kit
registry held on the prover handle.

The relation-table and execution-lane `ChipWitnessKit` impls in
`tabula-chips` gain `install_rows` / `install_prelude` methods that
translate logical rows into their private `*WitnessRow` representations
and push to `KitScratch`. Runtime calls only the logical seam.

### 8.4 Guardrail

```rust
// crates/runtime/tests/no_chip_rows_in_runtime.rs
const FORBIDDEN: &[&str] = &[
    "RelationTableWitnessRow",
    "InstructionRecord",
    // extend as new runtime-sourced chip rows appear.
];
```

Any file under `crates/runtime/src/**` that contains one of the
forbidden patterns (excluding `// intentional: ...` escape-hatch
comments) fails the test.

## 9. `ChipWitnessKit` sealing

### 9.1 Nature of the seal

Chip `ChipWitnessKit` impls live in `tabula-chips`, a crate separate
from `tabula-stark` where the trait is defined. A true type-level seal
(private-supertrait pattern) requires the `Sealed` supertrait to live
in a module that external crates cannot reach, but blessed chips in
`tabula-chips` must reach it to write their impls. Those two
constraints cannot both be satisfied simultaneously with Rust's
visibility rules — any route that lets `tabula-chips` impl `Sealed`
is also reachable to any other downstream crate, because cargo does
not distinguish "blessed workspace member" from "arbitrary downstream
consumer" at the type-system level.

The seal is therefore a **convention seal + trybuild compile-fail
probe**, not a type-level unforgeability proof:

- `ChipWitnessKit` gains a `Sealed` supertrait in a `pub mod sealed`
  module of `tabula-stark::witness_kit`. The module name signals
  intent.
- Blessed chips in `tabula-chips` write `impl ChipWitnessKit::sealed::Sealed
  for InstructionKit {}` alongside their `impl ChipWitnessKit`.
- A trybuild compile-fail probe pins the seal: CI fails if an
  external-crate-like fixture omits the `Sealed` impl and compiles,
  or if it trivially succeeds by accident.

The research-prototype scope accepts this. If third-party chip
authoring becomes a goal later, the seal upgrades to a runtime
registration check or a macro-mediated handshake — design discussion
not in SP-5's scope.

### 9.2 Pattern

```rust
// crates/stark/src/witness_kit.rs

pub mod sealed {
    pub trait Sealed {}
}

pub trait ChipWitnessKit: sealed::Sealed + Send + Sync {
    // existing surface unchanged
}
```

Each blessed chip in `tabula-chips` adds one line next to its existing
`impl ChipWitnessKit`:

```rust
impl tabula_stark::witness_kit::sealed::Sealed for InstructionKit {}
impl ChipWitnessKit for InstructionKit { ... }
```

### 9.3 Trybuild probe

```rust
// crates/stark/tests/compile_fail/external_chip_witness_kit_impl.rs
//
// Compile-fail probe. Without the `Sealed` impl, this fixture must
// not compile. The error text is pinned in the sibling .stderr file.

use tabula_stark::witness_kit::ChipWitnessKit;

struct External;
impl ChipWitnessKit for External {}

fn main() {}
```

The probe's expected error is "the trait bound `External:
tabula_stark::witness_kit::sealed::Sealed` is not satisfied". Pinned
via trybuild.

A companion compile-pass probe (with the `Sealed` impl added)
guarantees the seal is not trivially unreachable.

## 10. Guardrails summary

| Guardrail                                        | Location                                          |
|--------------------------------------------------|---------------------------------------------------|
| No `tabula_chips::*Row` names in runtime         | `crates/runtime/tests/no_chip_rows_in_runtime.rs` |
| All three handles `Send + Sync + 'static`        | `crates/runtime/tests/prepared_handle_bounds.rs`  |
| `From<{Prove,Verify,Execute,Setup}Error>` → `RuntimeError` | `crates/runtime/tests/error_conversions.rs` |
| External `impl ChipWitnessKit` fails             | `crates/stark/tests/sealing.rs` (trybuild)        |
| `PreparedExecutor` / `prepare_executor` public   | `crates/runtime/tests/prepared_executor_symmetry.rs` |

Each guardrail lives in its own `tests/*.rs` so CI log output points
at the exact invariant that broke.

## 11. Completion criteria

- `crates/runtime/src/engine.rs` does not exist.
- No file under `crates/runtime/src/` exceeds 800 LOC. Mean ≤ 400.
- Three symmetric prepared handles public: `PreparedProver`,
  `PreparedVerifier`, `PreparedExecutor`. All `Send + Sync`. Built by
  `prepare_*(reg, &opts)` free functions plus optional fluent
  builders.
- `PreparedOptions` exists and is consumed by all three `prepare_*`.
- `TabulaRuntime` / `RuntimeBuilder` symbols removed from the public
  API. CLI, SDK, examples, xtask migrated.
- `RuntimeError` is the `#[non_exhaustive]` umbrella; narrowed errors
  are the per-handle surface. All `From` impls compile; guardrail
  test green.
- Zero concrete `tabula_chips::*Row` identifiers in
  `crates/runtime/src/**`. Guardrail test green.
- `ChipWitnessKit` sealed; trybuild compile-fail probe green.
- `VerifierState`, `PreparedOptions`, prepared-handle builder types
  marked `#[non_exhaustive]`.
- `SnapshotCellRecord` codec has a documented home with a rationale
  comment.
- `cargo test --workspace --all-features` green.
  `cargo clippy --workspace --all-features --all-targets -- -D warnings` green.
  `cargo build --workspace --no-default-features{,--features verify,--features prove}` green.
- Byte-identity: `examples/basic` and `examples/membership` proof
  bytes identical across SP-4 → SP-5 transition. SP-5 is a pure
  refactor.
- Dependency-direction invariants from umbrella §3 still hold.
- `crates/runtime/README.md` updated to describe the three-handle
  shape.
- Umbrella doc marks SP-5 Landed; §7 Open Decisions updates (SP-5
  codec disposition resolved; all other SP-5 resolutions confirmed).
- advisor consultation pre-flight (before any code change) and
  close-out (before declaring done).

## 12. Risks & mitigations

| Risk | Mitigation |
|------|------------|
| Byte-identity drift during decomposition | Capture post-SP-1.5 baseline proof bytes first; `cmp` at the end; fail SP-5 if bytes differ. |
| Error narrowing breaks downstream match arms | CLI / SDK are the only matchers; audit before declaring done. SDK migration is in-scope of SP-5 for the error types it already matches on. |
| Convention seal for `ChipWitnessKit` slips (external impl lands without `Sealed`) | Trybuild compile-fail probe in CI catches regressions; a companion compile-pass probe catches trivially-unreachable seals. |
| `PreparedOptions` adds one more knob layer | Migration is mechanical (SP-4 call sites are the proxy). Single consolidation is worth it long term; if it pushes back in practice, keep per-handle builders as the primary surface. |
| Module count jumps from 8 to ~13 | Each module gets a one-sentence doc header stating its single responsibility; if you can't write that sentence without a compound, split further. |

## 13. Open decisions (resolve before or during implementation)

- **§6 `SnapshotCellRecord` disposition.** Runtime-internal vs.
  `tabula-contract`. Decide on inspection of call sites before
  starting module-level moves (otherwise moves churn).
- Error `SetupError` naming. Alternative: `RuntimeSetupError`. Pick
  whichever doesn't collide with existing `tabula_machine::SetupError`
  at the use site. Expect `SetupError` to be fine because callers
  usually fully-qualify the machine one.
- **§4.2 adoption gate.** Default position is to adopt. If the
  migration pressure on SDK/CLI is materially disruptive mid-SP-5,
  defer `PreparedOptions` to a follow-up and keep SP-4's per-handle
  `with_*` builders.

## 14. Ordering of implementation tasks

Proposed task order (plan doc will turn these into bite-sized steps):

1. Byte-identity baseline capture for `examples/basic` + `membership`
   on the post-SP-1.5 head.
2. advisor pre-flight consultation.
3. SP-5 design doc commit (this file) + branch.
4. Extract `snapshot.rs`. Build + test.
5. Extract `prelude.rs`. Build + test.
6. Extract `statement_materialization.rs`. Build + test.
7. Extract `state_binding.rs`. Build + test.
8. Introduce `options.rs` (`PreparedOptions`) and migrate `prepare_prover`
   / `prepare_verifier` to `prepare_*(artifact, &opts)` signatures.
9. Extract `execution.rs` as free functions over the prepared-state
   borrows needed (no shared core).
10. Introduce `PreparedExecutor` + `prepare_executor(Arc<RegisteredProgram>,
    &opts)`. Guardrail test.
11. Delete `TabulaRuntime` / `RuntimeBuilder`. Migrate CLI, SDK,
    examples, xtask. Full suite green.
12. Narrow errors (`ProveError` / `VerifyError` / `ExecuteError` /
    `SetupError`). Guardrail + caller migration.
13. Introduce `LogicalRelationTableRow` / `LogicalExecutionPrelude`
    in `tabula-stark`. Migrate runtime pre-stuff sites. Chip-row
    guardrail test.
14. Seal `ChipWitnessKit`. Convention seal + trybuild probes (compile-
    fail + compile-pass companion). Each blessed chip adds its
    `Sealed` impl line.
15. `#[non_exhaustive]` sweep over `VerifierState`, `PreparedOptions`,
    builder types.
16. `SnapshotCellRecord` codec disposition (per §6).
17. Final audit: wc -l, doc headers, module purpose sentences.
18. Byte-identity gate: compare SP-5 vs post-SP-1.5 baselines.
19. Update `crates/runtime/README.md` + umbrella (SP-5 Landed).
20. advisor close-out consultation.

Tasks 4–7 are mechanical and can be serialized; tasks 8, 10, 12, 13,
14 are the substantive ones and benefit from fresh-subagent dispatch.
The plan doc will bite-size them further.

## 15. References

- Umbrella: [`docs/superpowers/specs/2026-04-18-architecture-refactoring-design.md`](./2026-04-18-architecture-refactoring-design.md)
- Canonical architecture: [`docs/design/architecture.md`](../../design/architecture.md)
- SP-4 design: [`2026-04-19-sp4-runtime-prepared-handles-design.md`](./2026-04-19-sp4-runtime-prepared-handles-design.md)
- SP-3 design (chip-kit): [`2026-04-19-sp3-witness-chip-kit-design.md`](./2026-04-19-sp3-witness-chip-kit-design.md)
- `.claude/CLAUDE.md` (collaboration posture, clean-break policy)
