# Evaluation Harness

Working note. **Not authoritative for architecture** — canonical
architecture lives in
[`docs/design/architecture.md`](../design/architecture.md). This note
is a **design target** for the `tabula-eval` crate, written to be
precise enough to implement against.

Companions:
- [`evaluation-stage-interfaces.md`](evaluation-stage-interfaces.md) — cross-role stage types this harness consumes.
- [`eurosys-2026-workload.md`](eurosys-2026-workload.md) — locked paper workload that this harness measures.
- [`eurosys-2026-contributions.md`](eurosys-2026-contributions.md) — paper contribution list.
- [`eurosys-2026-section-outline.md`](eurosys-2026-section-outline.md) — figure / table inventory the harness must feed.

## 1. Purpose

A dedicated benchmark harness is a structural need for Tabula
independent of any single paper. Its job is:

- **Control temperature.** Make cold-start and warm-steady-state
  measurements distinguishable at the call-site, not inferred from a
  run index.
- **Control reuse.** Decide per-stage whether prepared state is
  reused across runs, batches, or workloads, and record the decision
  in every output row.
- **Run multiple systems.** Measure Tabula alongside SP1 and RISC0
  through a common `SystemAdapter` trait without either side bending
  to the other's APIs.
- **Separate workload identity from measurement config.** A single
  "workload" (e.g. "StarkEx-class multi-asset spot trading rollup")
  is measured under many variants (N/M/S tuples, ablations). The two
  concerns are not flattened into one name.
- **Produce a canonical schema.** Every row in
  `benchmark_results.jsonl` has the same shape, regardless of
  workload, system, or ablation. Downstream figures and tables are
  pure queries over that schema.
- **Be deterministic.** Same inputs, same system, same config →
  byte-identical intermediate artifacts; latency / memory numbers are
  reported with statistics, not point values.

Non-goals:
- Not a replacement for the CLI. The CLI remains the end-user
  interface to the artifact.
- Not a substitute for integration tests in the individual crates.
- Not a graph / dashboard tool. It produces data; plotting is
  downstream.

## 2. Goals

- **G1 — Stage-level measurement.** Every stage from
  [`evaluation-stage-interfaces.md`](evaluation-stage-interfaces.md)
  can be measured in isolation.
- **G2 — Temperature correctness.** Cold and warm runs differ only in
  whether prepared state is reused; no other source of variance is
  smuggled in.
- **G3 — Explicit reuse state.** The harness never relies on hidden
  caches; all reuse is recorded on the run-level handle.
- **G4 — Cross-system parity.** Tabula, SP1, and RISC0 are three
  implementations of `SystemAdapter<Workload = ...>`; their outputs
  share the same schema.
- **G5 — Two-level workload model.** Workload identity
  (schema + transaction types + L1 behaviour) is one object;
  measurement configuration (N, M, S, ablation toggles, seeds) is a
  separate object.
- **G6 — Canonical output schema.** A single versioned
  `BenchmarkRecord` struct is the only row shape in the output
  stream.
- **G7 — Reproducibility.** Every row carries enough metadata
  (tool versions, machine fingerprint, seed, feature flags) that a
  third party can reproduce it.

## 3. Non-Goals

- Running Tabula / SP1 / RISC0 in the same process. Cross-system
  adapters may fork subprocesses for isolation.
- Building SP1 / RISC0 from source. The harness expects pre-built
  system toolchains at pinned versions.
- Replacing the paper's plotting scripts. The harness emits rows; a
  separate `scripts/figures/` directory (not covered here) consumes
  them.

## 4. Lessons Baked In

Observations from earlier design iterations that shape the current
target:

- Hidden `Mutex<BTreeMap<...>>` caches in the SDK made it impossible
  to distinguish cold from warm without guessing; the stage-interfaces
  note replaces them with explicit prepared handles.
- An aggregate statement type that was sometimes a caller-supplied
  claim and sometimes an attested fact blurred what a "verified
  proof" meant for the harness; the
  `PublicStatement` / `BoundStatement` split removes that confusion.
- Flattening workload names and variants into one string
  (`transfer_N16_M5000_S100k_nf_off`) destroyed filterability; a
  two-level model restores it.
- A single `ReuseState` enum variants into incoherent combinations
  (e.g. "new prover on cached artifact but cold executor"); the
  replacement `ReuseState` struct with one per-stage field per stage
  is what the harness actually wants.
- A CLI-owned borsh `ReceiptBridge` meant the harness could not
  construct an `ExecutionRecord` in-process without linking the CLI;
  the stage-interfaces note moves that codec into `tabula-contract`.

## 5. Consumed Interfaces

The harness is a pure consumer of:

- **`tabula-contract`** — `SealedArtifact`, `ExecutionRecord`,
  `PublicStatement`, `BoundStatement`, `Proof`, `ArtifactId`.
- **`tabula-executor`** — `prepare_executor`, `PreparedExecutor`,
  `ExecuteError` (behind the `execute` feature).
- **`tabula-witness`** — `WitnessEnvelope` (behind the `execute` or
  `prove` feature; opaque to the harness).
- **`tabula-runtime`** — `prepare_prover`, `prepare_verifier`,
  `PreparedProver`, `PreparedVerifier`, `public_statement_from_record`,
  and any orchestration policy the harness calls directly. The
  prepared-handle stages (§10 of the stage-interfaces note) are
  runtime-owned because they carry statement-binding policy.
- **`tabula-machine`** — `BackendProver`, `BackendVerifier` backend
  primitives only. The harness does not call these directly; they sit
  underneath the runtime's prepared handles.
- **`tabula-sdk`** — `Tabula` facade for the "ergonomic" run mode.
  Not used by the per-stage harness path.
- **External system toolchains** — SP1 and RISC0 SDKs, consumed only
  through their `SystemAdapter` implementations.

The harness **does not** depend on `tabula-cli`, `tabula-lang`, or
`tabula-compiler` at runtime. Fixture generation may shell out to the
compiler during a setup phase.

Vocabulary in this note matches
[`evaluation-stage-interfaces.md`](evaluation-stage-interfaces.md) and
[`docs/design/architecture.md`](../design/architecture.md): the
caller-supplied claim is `PublicStatement`; the verifier-issued
attested fact is `BoundStatement`. The earlier names
`ExpectedStatement` / `AttestedStatement` are retired.

## 6. Crate Layout

```
crates/
  eval/
    Cargo.toml
    src/
      lib.rs
      workload/                 # §7
        mod.rs
        spot_trading.rs         # the locked StarkEx-class workload
        variant.rs              # WorkloadVariant
        fixture.rs              # deterministic fixture generation

      adapter/                  # §9
        mod.rs
        tabula.rs               # SystemAdapter impl for Tabula
        sp1.rs                  # behind feature = "sp1"
        risc0.rs                # behind feature = "risc0"

      run/                      # §10 / §11
        mod.rs
        action.rs               # ActionMode
        temperature.rs          # ReuseState
        runner.rs               # stage-by-stage runner
        warmup.rs

      measure/                  # §12
        mod.rs
        clock.rs
        memory.rs
        counters.rs

      cache/                    # §13
        mod.rs
        key.rs
        store.rs                # content-addressed disk cache

      schema/                   # §14
        mod.rs
        record.rs               # BenchmarkRecord (versioned)

      report/                   # §15
        mod.rs
        jsonl.rs
        summary.rs

      cli.rs                    # tabula-eval CLI entry points
      determinism.rs            # §19

    fixtures/                   # checked-in test fixtures, small
      spot_trading_N4_M64_S512/

    benches/                    # sanity microbenches, not the paper runs
```

Rules:
- `crates/eval/src/` lives in the workspace.
- `crates/eval/benches/` is development-only; paper runs use the
  harness CLI.
- Feature flags: `default = []`, `tabula = ...` (default on),
  `sp1 = ...`, `risc0 = ...`. The harness builds with just `tabula`
  for day-to-day development; `sp1` / `risc0` are opt-in.

## 7. Workload Model (Two-Level)

### 7.1 Semantic identity: `Workload`

A `Workload` is the *semantic identity* of a benchmark target. It
names the state schema, the transaction types, the invariants, and
the fixture-generation rules. It does **not** carry numeric sweep
parameters.

```rust
pub trait Workload: Send + Sync + 'static {
    /// Stable identifier — e.g. "starkex_spot_trading".
    fn id(&self) -> WorkloadId;

    /// Human-readable name for logs / figures.
    fn display_name(&self) -> &str;

    /// The workload's parameter schema — what WorkloadVariant must
    /// supply to materialize a concrete program + fixture.
    fn parameter_schema(&self) -> ParameterSchema;

    /// Deterministically generate an initial state and transaction
    /// batch for the given variant + seed. System-agnostic — the
    /// fixture is the same inputs regardless of which prover will
    /// consume it.
    fn generate_fixture(
        &self,
        variant: &WorkloadVariant,
        seed: FixtureSeed,
    ) -> Result<Fixture, WorkloadError>;

    /// Stable semantic digest of the workload-variant pair
    /// (no system binding). Two systems targeting the same variant
    /// share this digest; it is the cross-system join key.
    fn semantic_workload_digest(&self, variant: &WorkloadVariant)
        -> Digest;
}
```

Compilation is *per system*: SP1 and RISC0 do not consume
`tabula-contract::SealedArtifact` directly, so the workload cannot
produce an artifact without knowing the target system. Compilation
lives on the system adapter (§9). The `Workload` trait is the
semantic and fixture-generation side only.

For the paper, exactly one `Workload` is registered:
`StarkExSpotTrading` (see
[`eurosys-2026-workload.md`](eurosys-2026-workload.md)). Additional
workloads can be added later without touching the harness core.

### 7.2 Measurement configuration: `WorkloadVariant`

```rust
pub struct WorkloadVariant {
    pub workload_id: WorkloadId,
    pub parameters: BTreeMap<String, ParameterValue>,
    pub ablations: AblationSet,
    pub seed: FixtureSeed,
}
```

Where `parameters` for the locked workload includes:

- `"N"` — asset count (e.g. `16`).
- `"M"` — batch size (e.g. `5000`).
- `"S"` — state size (e.g. `100000`).
- `"tx_distribution"` — the 60/10/25/5 mix or a sweep override.
- `"asset_mix"` — the 80/20 mix or a sweep override.

`AblationSet`:

```rust
pub struct AblationSet {
    pub nf_elision: bool,             // A1
    pub uniform_width: bool,          // A2
    pub shard_topology: ShardTopology, // A3 (monolithic vs per-column-sealed vs per-column-runtime)
    pub relation_runtime: bool,       // M5 micro-ablation (if used)
}
```

`ShardTopology` is an enum because A3 has three settings
(A3-monolithic, A3-seal, A3-runtime-topology), not a toggle.

### 7.3 Fixture

```rust
pub struct Fixture {
    pub variant: WorkloadVariant,
    pub semantic_workload_digest: Digest,  // cross-system join key
    pub initial_state: StateSnapshot,      // system-agnostic encoding
    pub batch: TxBatch,                    // system-agnostic encoding
    pub oracle: Oracle,
}
```

The fixture carries no `SealedArtifact`: each system compiles its own
program from the same semantic workload (§9). The harness uses
`semantic_workload_digest` to join rows across systems and verifies
post-execute that every system produced the same `BehaviorOracle`
values (§12.5).

### 7.4 Oracle split

Current practice mixes two things into one "expected" blob. The
`Oracle` splits *semantic behaviour* (what any faithful executor of
the workload must produce) from *proof shape* (what a specific
prover's ceremony looks like):

```rust
pub struct Oracle {
    pub behavior: BehaviorOracle,
    pub proof: BTreeMap<SystemId, ProofOracle>,
}

pub struct BehaviorOracle {
    pub expected_post_state_root: Digest,
    pub expected_event_digest: Digest,
    pub expected_applied_tx_digest: Digest,
}

pub struct ProofOracle {
    pub expected_proof_layout: ProofLayoutSummary,
}
```

Rules:
- `BehaviorOracle` is system-agnostic and shared across all systems
  targeting the same variant; the harness compares every system's
  `ExecutionRecord` against it after `execute`. A mismatch is a
  cross-system semantic divergence and fails the row, not a warning.
- `ProofOracle` is system-specific (proof layout is a per-prover
  fact); it is compared after `prove` for that system only.
- `PublicStatement` is *not* in the oracle. Each system derives its
  own `PublicStatement` from its own `ExecutionRecord`; if two
  systems disagree on the `PublicStatement` digest for a variant
  with identical `BehaviorOracle`, that is also a semantic
  divergence bug, logged on the row.
- Oracles are produced by `generate_fixture`; neither is computed
  from a prover.

## 8. Workload Registry

```rust
pub struct WorkloadRegistry {
    by_id: BTreeMap<WorkloadId, Arc<dyn Workload>>,
}
```

The registry is populated in `workload::mod.rs`; the CLI looks up a
workload by id. There is no dynamic registration from outside the
crate — this is research code and the workload list is in-tree.

## 9. SystemAdapter

Cross-system parity is expressed as a trait parameterised by the
`Workload`. Each adapter owns its own `SystemArtifact` type (the
system-specific sealed program), its own `ExecutionRecord`, and its
own `Proof` / claim / attested-fact types. The harness compares
systems only through cross-system projections: the semantic
workload digest, the `BehaviorOracle`, and latency / size scalars.

```rust
pub trait SystemAdapter: Send + Sync + 'static {
    /// The workload family this adapter knows how to compile.
    type Workload: Workload;

    /// System-specific sealed program. For Tabula this is
    /// `tabula_contract::SealedArtifact`; for SP1 / RISC0 this is
    /// the system's own program-image type. `SystemArtifactId` is
    /// the per-system content-addressed identity (distinct from
    /// `semantic_workload_digest`).
    type SystemArtifact:   Send + Sync;
    type SystemArtifactId: Send + Sync + Clone + Eq + Hash;

    type PreparedExecutor: Send + Sync;
    type PreparedProver:   Send + Sync;
    type PreparedVerifier: Send + Sync;

    type ExecutionRecord:  Send + Sync;
    type PublicStatement:  Send + Sync;
    type Proof:            Send + Sync;
    type BoundStatement:   Send + Sync;

    fn id(&self) -> SystemId;
    fn version(&self) -> SystemVersion;

    /// Compile a sealed program for this system from the workload
    /// variant. Two adapters over the same variant produce different
    /// `SystemArtifact`s but must agree on the `BehaviorOracle`
    /// after `execute` (§7.4, §12.5).
    fn compile(
        &self,
        workload: &Self::Workload,
        variant: &WorkloadVariant,
    ) -> Result<Self::SystemArtifact, AdapterError>;

    fn system_artifact_id(&self, a: &Self::SystemArtifact)
        -> Self::SystemArtifactId;

    fn prepare_executor(&self, artifact: &Self::SystemArtifact)
        -> Result<Self::PreparedExecutor, AdapterError>;
    fn prepare_prover(&self, artifact: &Self::SystemArtifact)
        -> Result<Self::PreparedProver, AdapterError>;
    fn prepare_verifier(&self, artifact: &Self::SystemArtifact)
        -> Result<Self::PreparedVerifier, AdapterError>;

    fn execute(
        &self,
        executor: &Self::PreparedExecutor,
        state: &StateSnapshot,
        batch: &TxBatch,
    ) -> Result<Self::ExecutionRecord, AdapterError>;

    /// Project the system-specific record onto the cross-system
    /// behaviour oracle. Used for §12.5 semantic-equivalence
    /// enforcement; return value is compared across adapters.
    fn behaviour_from_record(
        &self,
        artifact: &Self::SystemArtifact,
        record: &Self::ExecutionRecord,
    ) -> BehaviorOracle;

    /// Construct this system's `PublicStatement` from its own
    /// record. For Tabula this delegates to
    /// `tabula_runtime::public_statement_from_record`.
    fn public_statement_from_record(
        &self,
        artifact: &Self::SystemArtifact,
        record: &Self::ExecutionRecord,
    ) -> Self::PublicStatement;

    fn prove(
        &self,
        prover: &Self::PreparedProver,
        record: &Self::ExecutionRecord,
        claim: &Self::PublicStatement,
    ) -> Result<Self::Proof, AdapterError>;

    fn verify(
        &self,
        verifier: &Self::PreparedVerifier,
        proof: &Self::Proof,
        claim: &Self::PublicStatement,
    ) -> Result<Self::BoundStatement, AdapterError>;

    /// Canonical serialization surface for caching. Byte identity
    /// of these is what §13 keys against.
    fn serialize_record(&self, r: &Self::ExecutionRecord) -> Vec<u8>;
    fn deserialize_record(&self, bytes: &[u8]) -> Result<Self::ExecutionRecord, AdapterError>;
    fn serialize_proof(&self, p: &Self::Proof) -> Vec<u8>;
    fn deserialize_proof(&self, bytes: &[u8]) -> Result<Self::Proof, AdapterError>;
}
```

Concrete implementations:

- `TabulaAdapter` — in-process. Associated type mapping:
  - `SystemArtifact       = tabula_contract::SealedArtifact`
  - `SystemArtifactId     = tabula_contract::ArtifactId`
  - `PreparedExecutor     = tabula_executor::PreparedExecutor`
  - `PreparedProver       = tabula_runtime::PreparedProver`
  - `PreparedVerifier     = tabula_runtime::PreparedVerifier`
  - `ExecutionRecord      = tabula_contract::ExecutionRecord`
  - `PublicStatement      = tabula_contract::PublicStatement`
  - `Proof                = tabula_contract::Proof`
  - `BoundStatement       = tabula_contract::BoundStatement`

  `prove` / `verify` delegate directly to the runtime-owned
  prepared handles (§8 of the stage-interfaces note).
- `Sp1Adapter` — subprocess-isolated (§11); associated types are
  SP1's program image / record / proof with an adapter-local claim
  / attested-fact type.
- `Risc0Adapter` — same shape as SP1.

`SystemId` is an enum; `SystemVersion` is a struct carrying toolchain
version, commit hash if available, and feature flags.

### 9.1 Cross-system join semantics

Rows from different adapters are joined by
`semantic_workload_digest`, not by `SystemArtifactId`. Two adapters
that produce identical `BehaviorOracle` values for the same variant
are *semantically equivalent* on that variant; divergence is a
fail-closed error, reported on the row and optionally aborting the
sweep (see §12.5).

## 10. Action Modes

Actions the harness can take against one `(WorkloadVariant, system)`
pair:

```rust
pub enum ActionMode {
    Execute,         // run executor, stop
    Prove,           // execute + prove
    Verify,          // prove + verify
    FullPipeline,    // compile + execute + prove + verify
}
```

Determinism checking is a *cross-cutting mode* on top of an action,
not an action of its own. The knob lives on `RunConfig::determinism`
(§11, `DeterminismMode`). When set to `RecordOnly` or
`ProofEnvelope`, the runner performs each downstream stage twice
with identical inputs and compares outputs byte-wise. This makes
"run the pipeline and also check determinism" expressible as
`ActionMode::FullPipeline + DeterminismMode::ProofEnvelope` rather
than as a fifth action that duplicates `FullPipeline` semantics.

## 11. Temperature And Reuse (`ReuseState`)

Temperature is three independent axes, not one flag. For each stage
the harness records *where the input came from* (source), *whether
the process owns prepared state* (process_state), and *whether the
page cache is likely hot* (page_cache). Collapsing these into one
`Cold/Warm/Rehydrated` enum hides the distinction between "prover
binary is still in memory" and "program image is decoded in the
address space", which matters for SP1 / RISC0 steady-state numbers.

```rust
pub struct ReuseState {
    pub artifact:           StageReuse,
    pub prepared_executor:  StageReuse,
    pub prepared_prover:    StageReuse,
    pub prepared_verifier:  StageReuse,
    pub execution_record:   StageReuse,
    pub public_statement:   StageReuse,
    pub proof:              StageReuse,
}

pub struct StageReuse {
    pub source:        ReuseSource,        // where inputs came from
    pub process_state: ProcessStateReuse,  // prepared object status
    pub page_cache:    PageCacheState,     // OS-level file cache
}

pub enum ReuseSource {
    FreshBuild,          // produced this repetition
    InMemoryFromRun,     // produced earlier in this process
    LoadedFromDisk,      // decoded from the harness cache
    LoadedFromSubprocess,// returned from a subprocess cache-dir handoff
}

pub enum ProcessStateReuse {
    NotApplicable,       // stage has no prepared handle
    Fresh,               // prepared this repetition
    Retained,            // prepared handle held across repetitions
}

pub enum PageCacheState {
    Dropped,             // drop_caches issued before this stage
    Warm,                // caches not dropped
    Unknown,             // adapter could not control page cache
}
```

Runner parameter:

```rust
pub struct RunConfig {
    pub action: ActionMode,
    pub reuse_policy: ReusePolicy,
    pub repetitions: Repetitions,
    pub warmup: WarmupConfig,    // see §12.3
    pub determinism: DeterminismMode,
    pub execution_model: ExecutionModel,
}

pub struct ReusePolicy {
    pub artifact:          ReusePreference,
    pub prepared_executor: ReusePreference,
    pub prepared_prover:   ReusePreference,
    pub prepared_verifier: ReusePreference,
    pub execution_record:  ReusePreference,
    pub public_statement:  ReusePreference,
    pub proof:             ReusePreference,
    pub page_cache:        PageCachePolicy,
}

pub enum ReusePreference {
    NeverReuse,            // cold every repetition
    WithinRun,             // reuse across repetitions of the same variant
    AcrossVariants,        // reuse across compatible variants
    FromDiskIfPresent,
}

pub enum PageCachePolicy {
    LeaveAlone,
    DropBeforeStage(StageKind),  // issue drop_caches before stage
}
```

Rules:
- `NeverReuse` is the default for stages being measured. Everything
  upstream of them defaults to `WithinRun` unless explicitly changed.
- `ReusePreference::AcrossVariants` is keyed by the stage's content
  digest (§13); two variants that yield the same key share the cache
  entry.
- Dropping the page cache requires elevated privileges; if
  unavailable, the harness records `PageCacheState::Unknown` and
  does not fail.

### 11.1 Process model

Cold mode for SP1 / RISC0 may require a subprocess to isolate
toolchain state. The harness supports two execution models:

```rust
pub enum ExecutionModel {
    InProcess,
    Subprocess(SubprocessTransport),
}

pub enum SubprocessTransport {
    /// Worker reads inputs from and writes outputs into a shared
    /// cache directory, keyed by content-addressed digests (§13).
    /// The parent never inherits artifacts on stdout; only a JSON
    /// status line is emitted there.
    CacheDirHandoff {
        cache_dir: PathBuf,
        status_fd: StatusChannel,
    },
}

pub enum StatusChannel {
    Stderr,              // structured JSON lines on stderr
    DescriptorFd(u32),   // dedicated fd for structured status
}
```

Rules:
- `InProcess` is the default for Tabula.
- `Subprocess` is the default for SP1 / RISC0 adapters when cold
  measurements are required.
- Subprocess adapters must use `CacheDirHandoff`: large artifacts
  (records, proofs) flow through the on-disk cache, not through
  stdout pipes. Byte-for-byte verification is performed by the
  parent via content digest; a mismatch fails the row.
- Process exit is awaited via `wait4`, which produces the child's
  `rusage` for cpu-time attribution (§12.1).

## 12. Measurement

### 12.1 Primary metrics

Per stage, per row:

- `wall_ns: u64` — monotonic clock.
- `cpu_ns: u64` — process CPU time attributable to the measured
  stage. In-process: difference of `ru_utime + ru_stime` from
  `getrusage(RUSAGE_SELF)` sampled immediately before and after the
  stage call, summed to nanoseconds. Subprocess: read from the
  `rusage` returned by `wait4` on the worker. Threaded prover
  backends sample `RUSAGE_THREAD` per worker and aggregate.
  `ru_utime + ru_stime` is *not* the same as wall time multiplied
  by core count; under-utilised cores produce lower `cpu_ns` and
  that distinction is the point.
- `peak_rss_bytes: u64` — peak resident set across the stage.
  Sampling strategy:
  - Linux in-process: start a sampler thread that reads
    `/proc/self/statm` at 10 ms intervals, records the maximum, and
    is joined when the stage returns. `VmHWM` from
    `/proc/self/status` is a *process-lifetime* high-water mark and
    does not reset between stages, so reading it post-stage
    over-reports whenever an earlier stage allocated more. If
    `proc(5)` supports it on the measurement kernel, the sampler
    issues `echo 5 > /proc/self/clear_refs` between stages to reset
    the working-set high-water mark; on kernels that do not expose
    `clear_refs` for VmHWM, the sampler value is authoritative.
  - Linux subprocess: the worker writes its own sampled peak to the
    cache-dir handoff payload, and the parent reads it alongside
    `wait4`'s `ru_maxrss` (which for children does reset per child).
  - macOS: `mach_task_basic_info::resident_size_max` sampled by the
    sampler thread.
- `proof_bytes: Option<u64>` — proof stage only.
- `record_bytes: Option<u64>` — execute stage only.

### 12.2 Clock discipline

- Single monotonic clock (`std::time::Instant`) for `wall_ns`. No
  `SystemTime::now()` for measurement.
- Clock is sampled immediately around the stage call; I/O for
  serialization / deserialization is measured in a separate
  sub-stage (`serialize_record`, `serialize_proof`, ...).
- `cpu_ns` samples bracket the stage with the same discipline: no
  stage runs between the pre- and post-sample besides the measured
  call.

### 12.3 Warmup

```rust
pub struct Repetitions {
    pub min: u32,                         // ≥ 30 for headline rows
    pub max: u32,                         // hard cap; default 100
}

pub struct WarmupConfig {
    pub warmup_repetitions: u32,          // not counted
}
```

Rules:
- Warmup rows are recorded in the output but tagged
  `phase: "warmup"` and excluded from summary statistics.
- The paper pipeline does *not* use an adaptive stability-threshold
  early-stop. Variable stopping rules bias bootstrap intervals
  (`mean` and `ci95` both become dependent on the stopping state).
  Early stop is permitted only in developer mode, tagged on the row
  so the reporting layer can refuse to compute headline statistics
  from it.
- The earlier §11 `RunConfig::repetitions: usize` (a single field)
  and the earlier `min_repetitions: ≥ 5` were contradictory.
  `Repetitions::min` is the only knob that governs headline runs;
  §18 sets the floor (n ≥ 30 for any row that feeds a paper
  figure, n ≥ 100 for the headline table).

### 12.4 Statistical reporting

Per `(WorkloadVariant, system, ActionMode, stage)`:

- `median`, `mean`, `stddev`.
- Non-parametric 95% confidence interval via **BCa bootstrap** (bias-
  corrected and accelerated; B = 10 000 resamples). Percentile
  bootstrap is recorded as a fallback only; BCa is the reported CI.
- `min`, `max` for transparency.
- Paired tests across systems at the same variant via paired
  Wilcoxon signed-rank on per-repetition wall-time (§18).

### 12.5 Cross-system semantic equivalence

After `execute` for each `(variant, system)` pair, the harness:

1. Computes `behaviour = adapter.behaviour_from_record(artifact,
   record)`.
2. Compares `behaviour` against `fixture.oracle.behavior`.
3. Compares `behaviour` across *every* system that has run this
   variant (including earlier rows loaded from cache, keyed by
   `semantic_workload_digest`).

Mismatches are hard errors by default (`--on-divergence fail`), not
warnings. The harness records the offending digests on the row and
refuses to emit a summary row that mixes divergent systems. This
closes the loophole where two adapters could silently "pass" while
proving different post-state roots for the same variant.

## 13. Caching

Stage reuse is keyed by content-addressed stage digests.

### 13.1 Keys

- Semantic key (cross-system): `semantic_workload_digest` from
  `Workload::semantic_workload_digest(&variant)`.
- `SystemArtifact` → `SystemArtifactId` (per-system).
- `PreparedExecutor` key: `(system_artifact_id, system_id,
  system_version, feature_set_digest, executor_prepared_version)`.
- `PreparedProver` / `PreparedVerifier`: same shape with their own
  `prepared_version`.
- `ExecutionRecord` key: `(system_artifact_id, initial_state_digest,
  batch_digest, system_id)`.
- `PublicStatement` key: `public_statement.digest()`
  (`Poseidon2(canonical_bytes)` per
  [`evaluation-stage-interfaces.md`](evaluation-stage-interfaces.md)
  §5.3).
- `Proof` key: `(system_artifact_id, public_statement_digest,
  system_id, prover_config_digest)`.
- `BoundStatement` is *not* cached: it is a deterministic function
  of `(PublicStatement, verifier state)` and is re-derived on demand.

`feature_set_digest` covers compiler features, proof-backend features,
and any adapter-specific build options that change output.

### 13.2 Store

On-disk cache lives under `$XDG_CACHE_HOME/tabula-eval/` by default;
overridable via CLI flag and env var. Layout:

```
tabula-eval/
  systems/<system_id>/<system_artifact_id>/
    artifact.sealed
    prepared_executor.bin           # optional
    prepared_prover.bin             # optional
    prepared_verifier.bin           # optional
  records/<record_key>.bin
  statements/<statement_key>.bin
  proofs/<proof_key>.bin
  handoff/<run_id>/                 # subprocess CacheDirHandoff area
    inputs/
    outputs/
    status.jsonl
```

### 13.3 Policy

- Cache writes are atomic: write to `.tmp`, fsync, rename.
- Cache reads verify the file's content digest (Blake3) matches the
  key. Mismatches are hard errors, not silent fallbacks.
- Subprocess adapters in `CacheDirHandoff` mode write their outputs
  into `handoff/<run_id>/outputs/` first; the parent then validates
  the payload digest and moves the file into the shared cache
  directory. The stdout / stderr channels carry only structured
  status lines (§11.1), never artifact bytes.
- The runner emits a `CacheEvent` per stage describing hit / miss /
  rehydrated, which is persisted to the output row.

## 14. Output Schema

A single versioned struct, emitted as JSONL.

```rust
pub struct BenchmarkRecord {
    // record-level identity
    pub schema_version: u16,
    pub record_id: Uuid,
    pub run_id: Uuid,
    pub emitted_at: DateTime<Utc>,
    pub phase: RecordPhase,   // Warmup, Measurement, Debug

    // workload identity
    pub workload_id: WorkloadId,
    pub workload_version: String,
    pub workload_variant: WorkloadVariantFingerprint, // canonical bytes digest
    pub workload_parameters: BTreeMap<String, ParameterValue>,
    pub ablation: AblationSet,
    pub fixture_seed: FixtureSeed,

    // system identity
    pub system_id: SystemId,
    pub system_version: SystemVersion,
    pub feature_set_digest: Digest,

    // stage identity
    pub stage: StageKind,
    pub action: ActionMode,
    pub repetition: u32,
    pub reuse_state: ReuseState,
    pub cache_events: Vec<CacheEvent>,

    // artifact identity
    pub semantic_workload_digest: Digest,     // cross-system key
    pub system_artifact_id: SystemArtifactIdBytes, // per-system
    pub public_statement_digest: Option<Digest>,
    pub bound_statement_digest:  Option<Digest>,
    pub proof_digest: Option<Digest>,

    // timing
    pub wall_ns: u64,
    pub cpu_ns: u64,

    // memory
    pub peak_rss_bytes: u64,

    // sizes
    pub record_bytes: Option<u64>,
    pub proof_bytes: Option<u64>,

    // statistical summary (populated on rolled-up rows only)
    pub confidence: Option<ConfidenceSummary>,

    // machine fingerprint
    pub machine: MachineFingerprint,

    // reproducibility
    pub commit_sha: String,
    pub rustc_version: String,
    pub adapter_toolchain: BTreeMap<String, String>, // e.g. sp1_version, risc0_version
    pub run_env: RunEnv,
}

pub enum StageKind {
    Compile,
    PrepareExecutor,
    PrepareProver,
    PrepareVerifier,
    Execute,
    SerializeRecord,
    DerivePublicStatement,
    Prove,
    SerializeProof,
    Verify,
}

pub struct ConfidenceSummary {
    pub median: f64,
    pub mean: f64,
    pub stddev: f64,
    pub ci95_low: f64,
    pub ci95_high: f64,
    pub n: u32,
}

pub struct MachineFingerprint {
    pub os: String,
    pub kernel: String,
    pub cpu_model: String,
    pub cpu_cores: u16,
    pub cpu_threads: u16,
    pub smt_enabled: Option<bool>,
    pub turbo_enabled: Option<bool>,
    pub disabled_cstates: Vec<String>,
    pub ram_bytes: u64,
    pub numa_node_bound: Option<u16>,
    pub aslr_value: Option<u8>,              // /proc/sys/kernel/randomize_va_space
    pub power_profile: Option<String>,
    pub frequency_policy: Option<String>,    // e.g. "performance", "schedutil"
    pub thermal_peak_c: Option<f32>,         // reporting-only, not a computation input
    pub kernel_tainted: Option<u64>,
    pub preempt_model: Option<String>,
}
```

Rules:
- `schema_version` bumps on any field change; downstream consumers
  pin the version they understand.
- `workload_variant` is a single canonical-bytes digest of
  `WorkloadVariant`, so filtering doesn't need to reconstruct the
  struct.
- One row per stage per repetition. Summary rows (with
  `confidence`) are emitted alongside raw rows, never instead of
  them.

Emission:
- `benchmark_results.jsonl` — primary stream.
- `benchmark_summary.json` — roll-ups grouped by
  `(workload_variant, system_id, action, stage)`.
- `run.manifest.json` — per-invocation metadata (CLI args, env
  snapshot, fixture seeds).

## 15. Reporting

Two-step:

1. **Raw log** — `benchmark_results.jsonl` as produced by the runner.
2. **Roll-ups** — `benchmark_summary.json` computed by the report
   module over a finished run.

Explicit *non-goal*: no embedded plotting. Figure scripts live
outside `crates/eval/` and read the schema.

## 16. Fixture Pipeline

Fixtures are generated deterministically from a `FixtureSeed`. The
generator is part of the workload implementation
(`Workload::generate_fixture`). Rules:

- Same seed + same variant ⇒ byte-identical `(initial_state, batch,
  oracle)`.
- `FixtureSeed` is a 256-bit value; the CLI can either accept a seed
  or derive one from a human-readable name (`"paper_headline_run_1"`
  → Blake3).
- Generators MUST use `rand_chacha::ChaCha20Rng` (or another RNG
  with a documented, cross-platform-stable byte stream). `thread_rng`,
  `StdRng`, and any RNG whose output depends on the target
  `rand` version are forbidden for fixture paths.
- Generators MUST NOT use floating-point arithmetic for any value
  that feeds fixture bytes (counts, distributions, asset mixes,
  etc.). Integer arithmetic with explicit rounding rules is the
  only acceptable path, because `f64` ordering of summations is not
  reproducible across platforms or LLVM versions. Distribution
  sampling that traditionally uses `f64` (e.g. alias-method mixes)
  is reimplemented over fixed-point integers in the fixture layer.
- Small fixtures for unit tests live under
  `crates/eval/fixtures/` and are regenerated by a CI check that
  compares byte-for-byte to the committed file. The CI check runs
  on both Linux x86_64 and Linux aarch64 to catch endianness /
  instruction-set drift.

## 17. CLI

`tabula-eval` subcommands:

```
tabula-eval list-workloads
tabula-eval describe-workload <id>

tabula-eval run \
  --workload starkex_spot_trading \
  --param N=16,M=5000,S=100000 \
  --ablation nf_elision=off,uniform_width=off,shard_topology=per_column_sealed \
  --system tabula,sp1 \
  --action full_pipeline \
  --repetitions 10 \
  --warmup 3 \
  --reuse-policy default \
  --output ./out/

tabula-eval sweep <sweep.toml>         # multi-variant declarative sweep
tabula-eval check-determinism <variant>
tabula-eval verify-cache               # validates on-disk cache integrity
```

Sweep file (TOML) grammar — one variant per row:

```toml
[[sweep]]
workload = "starkex_spot_trading"
parameters = { N = 16, M = 5000, S = 100000 }
ablations  = { nf_elision = "off", shard_topology = "per_column_sealed" }
systems    = ["tabula"]
action     = "full_pipeline"
repetitions = 10

[[sweep]]
workload = "starkex_spot_trading"
parameters = { N = 16, M = 5000, S = 100000 }
ablations  = { nf_elision = "off", shard_topology = "monolithic" }   # A3-seal off
systems    = ["tabula"]
action     = "full_pipeline"
repetitions = 10
```

## 18. Statistical Protocol

Sample size:
- Any row that feeds a paper figure: **n ≥ 30**. This is the floor
  that makes the BCa bootstrap interval stable and lets paired
  Wilcoxon approach its asymptotic distribution for cross-system
  comparisons.
- Headline table rows: **n ≥ 100**. Deep enough to keep the CI
  narrow even at the 99% level, and to give the paired Wilcoxon
  p-value enough resolution to pass a Holm–Bonferroni correction
  across the full paper table.
- Warmup: drop the first `warmup_repetitions` rows from statistics.
  Warmup does not count toward `n`.

Reporting:
- Report **median + 95% BCa bootstrap CI** (B = 10 000 resamples)
  as the headline statistic. Mean / stddev / min / max are also
  emitted for audit. The reporting layer refuses to print a
  headline CI from any row whose `phase != Measurement`, whose
  stopping rule was adaptive, or whose `n < 30`.
- One row = one repetition. The harness never silently collapses
  repetitions into one pre-aggregated row.

Cross-system tests:
- Per-variant paired comparison between systems uses the **paired
  Wilcoxon signed-rank test** on per-repetition wall time,
  matched by repetition index within warmed steady state. The
  harness uses a paired test because per-repetition noise is
  correlated across systems (shared thermal and scheduler state on
  the measurement machine); an unpaired t-test over-estimates
  variance.
- When the same hypothesis is evaluated across k variants, p-values
  are corrected via **Holm–Bonferroni** and the corrected p is the
  one reported. The raw p-value and the family size k are both
  emitted for audit.

Machine discipline fields (also populated from §18.1):
- CPU pinning (`taskset`) is recorded in `run_env.cpu_pinning`.
- Frequency governor is recorded in `machine.frequency_policy`; the
  CLI warns if it is not `performance` on Linux.
- Background-load check: the CLI samples `/proc/loadavg` before and
  after each repetition; load above a threshold is logged and can
  optionally abort the row (`--abort-on-load`).

### 18.1 Measurement Machine Discipline

The statistical protocol assumes that the measurement machine does
not silently change the steady state between repetitions. The
harness records and, where possible, enforces the following before
starting a measurement run. Each field appears on every row via
`machine` or `run_env` so readers can audit the discipline the
numbers were collected under.

- **CPU governor.** Linux `cpufreq` governor must be `performance`;
  the CLI asserts this and records the value.
- **Turbo / boost.** Intel Turbo Boost (`/sys/devices/system/cpu/
  intel_pstate/no_turbo`) and AMD CPB (`cpb` MSR where exposed)
  are disabled for the run; their state is recorded.
- **C-states.** Deep C-states beyond C1 are disabled for pinned
  cores via `cpuidle` sysfs; the CLI records which C-states were
  disabled.
- **SMT.** Hyper-threading / SMT is disabled by writing to
  `/sys/devices/system/cpu/smt/control`, because SMT siblings
  sharing a core inflate CPU-time attributions. If SMT cannot be
  disabled (e.g. non-root), the state is recorded and the
  reporting layer flags the row.
- **NUMA.** The worker is pinned to a single NUMA node (`numactl
  --cpunodebind --membind`); node id is recorded. Cross-node
  measurements are an explicit sweep axis, not an accident.
- **ASLR.** `/proc/sys/kernel/randomize_va_space` is set to 0 for
  the run and restored after. This is the only hygiene setting
  that meaningfully changes fixture-to-fixture numbers through
  heap-layout-sensitive hashing and branch prediction.
- **Thermal.** `thermal_zone*/temp` is sampled per repetition; any
  row where temperature rose above a configurable threshold
  (default: 85 °C) is tagged `phase: "thermal_outlier"` and
  excluded from summary statistics unless the user explicitly
  overrides.
- **Scheduler and background services.** The harness logs the
  output of `systemctl list-units --state=running` at run start.
  Known noisy services (`apt.timer`, `unattended-upgrades`,
  `mlocate.timer`) produce a warning; unknown services are just
  logged.
- **Kernel taint / preemption model.** Recorded from
  `/proc/sys/kernel/tainted` and the kernel build (`CONFIG_PREEMPT*`
  detected via `/proc/config.gz` if available).

All of the above land on `MachineFingerprint` (§14) as concrete
fields; the fingerprint is what downstream figures join on when
comparing machines.

## 19. Determinism Mode

`DeterminismMode` is a cross-cutting knob, not an action (see §10).
Given any `ActionMode`, `DeterminismMode` controls whether the
runner also performs a byte-for-byte replay comparison at the
stages the action already executes.

```rust
pub enum DeterminismMode {
    Off,
    RecordOnly,        // every execute stage is replayed & compared
    ProofEnvelope,     // also compare proof envelope bytes
}
```

Semantics:
- `Off` — the default for throughput rows. No replay overhead.
- `RecordOnly` — for each `Execute` stage in the action, the runner
  executes the same variant twice with identical inputs. If
  `ExecutionRecord::encode(r1) != encode(r2)`, the row fails with a
  diff-locatable error describing which field differs. The
  `cpu_ns` / `wall_ns` of the *first* execution feeds the
  measurement; the second is tagged `phase: "replay"` and
  excluded from headline statistics.
- `ProofEnvelope` — `RecordOnly` plus a replay at the `Prove`
  stage. `serialize_proof(p1) == serialize_proof(p2)` is required
  whenever the adapter claims a deterministic prover. Tabula's
  prover is deterministic; SP1 / RISC0 adapters expose a flag
  describing their guarantee, and the harness refuses
  `ProofEnvelope` for any adapter that does not claim it.

## 20. Implementation Order

Target order for building the crate. The ordering exists to keep each
step independently testable; it is not a timeline.

1. **`schema/`** — `BenchmarkRecord` and JSONL emission first, so
   every subsequent step can emit a row.
2. **`workload/`** — `Workload` trait, `WorkloadVariant`,
   `Fixture`, `Oracle`. Register `StarkExSpotTrading` with a minimal
   `N=4, M=64, S=512` fixture.
3. **`adapter/tabula.rs`** — thinnest possible adapter over
   `tabula-sdk`. Produces a valid `BenchmarkRecord` for the
   minimal fixture through `FullPipeline`.
4. **`run/`** — `RunConfig`, `ReuseState`, the runner loop,
   warmup. Single-system, in-process.
5. **`measure/`** — real clock / memory instrumentation. Replace
   the placeholder metrics used in step 3.
6. **`cache/`** — disk cache, `CacheEvent` plumbing, `verify-cache`
   subcommand.
7. **`run/temperature.rs`** — cross-variant reuse. Exercise all
   `StageReuse` outcomes on contrived variants.
8. **`determinism.rs`** — `CheckDeterminism` action.
9. **`adapter/sp1.rs`** — SP1 adapter behind the `sp1` feature.
   Subprocess-isolated. Add `Sp1Adapter`-specific toolchain fields
   to `MachineFingerprint`.
10. **`adapter/risc0.rs`** — analogous.
11. **CLI polish + sweep file support.**
12. **Paper fixtures** — fill in `N ∈ {4,8,16,32}`, `M ∈ {1k,5k,10k}`,
    `S ∈ {10k,100k,1M}` and confirm end-to-end on at least one point
    for each system.

## 21. Acceptance Criteria

The harness is considered complete when:

1. `cargo check -p tabula-eval` succeeds with just `default`
   features.
2. `cargo check -p tabula-eval --features sp1` and `--features
   risc0` succeed on a host with the matching toolchains installed.
3. `cargo tree -p tabula-eval -e normal,build` does not list
   `tabula-cli`, `tabula-lang`, or `tabula-compiler`. The `execute`
   feature adds `tabula-executor` and `tabula-witness`; the
   `verify`-only slice compiles without either.
4. `tabula-eval run --workload starkex_spot_trading --param
    N=4,M=64,S=512 --system tabula --action full_pipeline
    --repetitions 30` produces a `benchmark_results.jsonl` whose rows
    all validate against `BenchmarkRecord` schema_version 1.
5. `tabula-eval run … --determinism record_only` produces matching
   replay rows for every Execute stage; a deliberately perturbed
   adapter reports a diff-locatable error.
6. A cross-system run on two adapters that produce different
   `BehaviorOracle` values for the same variant is failed closed
   by the runner (§12.5), not silently averaged.
7. `tabula-eval run` with `ReusePreference::WithinRun` for
   `prepared_prover` shows `process_state.prepared_prover =
   Retained` on every repetition after the first, and the first is
   `Fresh`.
8. `tabula-eval verify-cache` rejects a file whose on-disk Blake3
   digest does not match its key.
9. Subprocess adapters (`Sp1Adapter`, `Risc0Adapter`) move records
   and proofs through the cache-dir handoff area; stdout / stderr
   of the worker carry only structured status lines. A test
   injects an artifact-sized blob on worker stdout and the harness
   ignores it.
10. A minimal paper roll-up — one `(N=16, M=5000, S=100000)` row per
    system, `n = 100` — can be regenerated from cache in < 1 minute
    on the measurement machine, with BCa CI and paired-Wilcoxon
    results filled in on the summary row.

## 22. References

- [`evaluation-stage-interfaces.md`](evaluation-stage-interfaces.md)
  — stage types this harness consumes.
- [`evaluation-stage-support.md`](evaluation-stage-support.md) —
  balanced top-level skeletons for cross-stage support types.
- [`eurosys-2026-workload.md`](eurosys-2026-workload.md) — locked
  workload.
- [`eurosys-2026-contributions.md`](eurosys-2026-contributions.md) —
  paper contribution list.
- [`eurosys-2026-section-outline.md`](eurosys-2026-section-outline.md)
  — paper section outline.
- [`docs/design/architecture.md`](../design/architecture.md) —
  canonical dependency direction, layer boundaries, and verification
  vocabulary.
- [`../research/tabula-zkvm-benchmark-spec.md`](../research/tabula-zkvm-benchmark-spec.md)
  — external benchmark-spec research note.
