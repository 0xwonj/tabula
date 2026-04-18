# Evaluation Stage Interfaces

Working note. **Not authoritative for architecture** — canonical
architecture lives in
[`docs/design/architecture.md`](../design/architecture.md) and the
crate-level `README.md` files. This note is a **design target** for the
stage-level data types and their owning crates, written to be precise
enough that the harness
([`evaluation-harness.md`](evaluation-harness.md)) can consume them
without reverse-engineering the codebase.

Companions:
- [`evaluation-harness.md`](evaluation-harness.md) — benchmark harness crate that consumes these stages.
- [`eurosys-2026-contributions.md`](eurosys-2026-contributions.md) — paper contribution list; C3 depends on this spec.
- [`eurosys-2026-workload.md`](eurosys-2026-workload.md) — locked workload that the harness measures.

## 1. Purpose

Tabula has three roles pipelined through the same code (CLI, SDK, soon
harness) but the hand-off objects between roles are currently
informal: the SDK keeps them inside `Mutex<BTreeMap<...>>` caches, the
CLI reconstructs them via a borsh `ReceiptBridge` that lives inside the
CLI crate, and the public boundary between *stating a claim* and
*attesting a verified fact* is not role-typed. This makes harness
instrumentation brittle: the harness needs deterministic control over
where cache hits happen, which types cross process boundaries, and
which statements are caller-supplied versus verifier-issued.

This note defines:

1. The stage vocabulary — the named objects that cross role boundaries.
2. The owning crate for each object (contract / runtime / machine / sdk).
3. The materialization API that turns one stage into the next, and the
   cache keys that identify each stage output.
4. The feature matrix that governs which crates / APIs a given build
   pulls in.
5. The clean-break migration from the current shape to this target.

Scope is **cross-role object design**. Not scope:

- Algorithmic changes inside the machine or proof backend.
- Harness crate internals (see the harness note).
- Paper writing.

Concrete type skeletons for everything named in this note — `Digest`,
`ArtifactId`, `PublicContext`, `EventLog`, `Schema`, `ProofEnvelope`,
the error types, and the harness support types — live in
[`evaluation-stage-support.md`](evaluation-stage-support.md). That
companion is intentionally a top-level skeleton (struct/enum shapes
and invariants), not a full field-level spec; implementation fills in
the internal shape.

**Vocabulary (canonical alignment).** This note uses the canonical
verification vocabulary from
[`docs/design/architecture.md`](../design/architecture.md) (§Verification
Vocabulary):

- `PublicStatement` — the proved public object. The *role* is
  caller-supplied claim: the statement the caller asks the verifier
  to attest.
- `BoundStatement` — the verifier-side binding object that ties a
  `PublicStatement` to one `SealedArtifact`. The *role* is
  verifier-issued attested fact: what `verify` returns on success.
- `ArtifactContext` — artifact-derived binding context recomputed
  from the sealed artifact, mixed into the transcript.
- `VerifierState` — prepared verifier state (artifact binding context
  + relation policy + machine verifier state).
- `public_statement.json` — stable external verification file, a
  versioned `PublicStatement` encoding.

Earlier drafts of this note used `ExpectedStatement` /
`AttestedStatement`. Those names are gone. The role distinction
(caller-supplied claim vs verifier-issued fact) is preserved — it is
expressed through the type pair `(PublicStatement, BoundStatement)`,
not through a second vocabulary.

## 2. Design Principles

- **Role typing.** A caller-supplied statement (claim) and a
  verifier-issued statement (attested fact) are different types, not
  the same type in different contexts.
- **Layer ownership.** Each stage object lives in the crate that owns
  its wire format. SDK wraps; it does not own.
- **No hidden process-local state.** Cache-like storage is explicit and
  carried on a handle the caller owns, never a module-level
  `Mutex<BTreeMap<...>>`.
- **Materialization is total.** Every stage has a pure constructor
  `from_<inputs>(...)` that makes no assumption about whether the
  previous stage is in-memory, on disk, or in another process.
- **Feature independence is enforced.** `verify` never requires
  `execute` / `prove` / the lang / compiler crates to link.
  `execute` never requires the lang / compiler crates to link at
  runtime (only at program-authoring time).
- **Determinism is observable.** Every stage output has a stable
  canonical serialization and a content-addressed digest; two runs on
  the same inputs produce byte-identical outputs.
- **Clean break.** Tabula has no external users; renames / relocations
  are free. Do not add deprecation shims.

## 3. Stage Vocabulary

Roles, stages, and the transitions between them:

```
SourceProgram ──compile──▶ SealedArtifact
                              │
                              ├── prepare(executor) ──▶ PreparedExecutor
                              │                             │
                              │              (state, batch) │
                              │                             ▼
                              │             (ExecutionRecord, WitnessEnvelope)
                              │                             │
                              │                             ▼
                              │              PublicStatement::from_record
                              │                             │
                              │                             ▼
                              │                    PublicStatement  (role: claim)
                              │
                              ├── prepare(prover) ────▶ PreparedProver
                              │                             │
                              │            (record, witness, claim)
                              │                             ▼
                              │                           Proof
                              │
                              └── prepare(verifier) ──▶ PreparedVerifier
                                                            │
                                                            │  (proof, claim)
                                                            ▼
                                                      BoundStatement (role: attested fact)
```

Stage objects:

| Stage | Role | Crate | Produced by | Consumed by |
|-------|------|-------|-------------|-------------|
| `SourceProgram` | authoring input | `tabula-lang` / `tabula-compiler` | user | compile |
| `SealedArtifact` | sealed binding | `tabula-contract` | compile | prepare(\*) |
| `PreparedExecutor` | handle | `tabula-executor` | `prepare_executor` | `execute` |
| `PreparedProver` | handle | `tabula-runtime` | `prepare_prover` | `prove` |
| `PreparedVerifier` | handle | `tabula-runtime` | `prepare_verifier` | `verify` |
| `ExecutionRecord` | verifier-observable record | `tabula-contract` | `execute` | `prove`, `PublicStatement::from_record` |
| `WitnessEnvelope` | prover-only witness | `tabula-witness` | `execute` | `prove` |
| `PublicStatement` | claim | `tabula-contract` | caller or `from_record` | `prove`, `verify` |
| `Proof` | proof envelope | `tabula-contract` | `prove` | `verify` |
| `BoundStatement` | attested fact | `tabula-contract` | `verify` | caller / integrator |

Design rules:

- No stage object is re-derived from another when it is the carrier of
  an input commitment. In particular, `verify` never rebuilds
  `PublicStatement` from `Proof`; the caller must supply the claim
  that `verify` then attests or rejects.
- `ExecutionRecord` is a hand-off contract, not a CLI-only handoff
  file. The CLI `receipt.bin` is one serialization of it; the harness
  passes it in memory. Nothing CLI-specific is allowed in the type.
- `ExecutionRecord` contains only verifier-observable fields. The
  prover-side witness data lives in `WitnessEnvelope` (owned by
  `tabula-witness`) and is passed to `prove` as a separate argument.
  Verifier-only builds therefore do not link witness machinery.
- `OpenedProgram` (the previous "sealed artifact + in-memory program
  loaded once" aggregate) does not exist as a stage type. Its two
  contents live separately: `SealedArtifact` is the wire object;
  `PreparedExecutor` / `PreparedProver` / `PreparedVerifier` are the
  prepared handles.

## 4. Layer Ownership

Canonical architecture
([`docs/design/architecture.md`](../design/architecture.md) §Layer
Boundaries) is the authority. This section scopes which of those
layers own which stage objects defined in this note. Crates the
canonical architecture lists that are not mentioned here (e.g.
`tabula-ir`, `tabula-core`, `tabula-chips`, `tabula-commitment`,
`tabula-gadgets`, `tabula-profile`, `tabula-stark`, `tabula-ext`,
`tabula-testing`) still exist and still sit where the canonical
architecture puts them; this note simply has no stage-interface
concern over them.

Stage ownership. Applications depend on the layers below; the
foundation is at the bottom. Arrows (`▼`) point from dependent to
depended-upon:

```
tabula-cli, tabula-eval               ← applications (top)
         │ depend on
         ▼
tabula-sdk                            ← process-scoped handles
         │
         ▼
tabula-runtime                        ← policy, statement binding,
                                         PreparedExecutor wiring,
                                         PreparedProver / PreparedVerifier
         │
         ▼
tabula-executor, tabula-machine       ← deterministic execution (executor),
tabula-witness                          primitive proof construction +
                                         verification (machine),
                                         prover witness inputs (witness)
         │
         ▼
tabula-lang, tabula-ir, tabula-compiler   ← authoring chain (separate
                                            branch; terminates in
                                            core/contract)
         │
         ▼
tabula-core, tabula-contract          ← shared meaning + wire formats
                                        (foundation)
```

Rules:

- `tabula-core` and `tabula-contract` are the foundation. Contract
  owns every cross-role wire type in §5 below — including
  `PublicStatement`, `BoundStatement`, `ArtifactContext`,
  `SealedArtifact`, `ExecutionRecord`, and `Proof` — *except*
  `WitnessEnvelope` which lives in `tabula-witness`. Contract must
  not link witness, machine, runtime, or any layer above it.
- `tabula-executor` owns `PreparedExecutor`, `prepare_executor`, and
  the `execute` implementation. Canonical architecture puts
  deterministic execution in the executor and *policy* in runtime;
  we follow that split.
- `tabula-witness` owns `WitnessEnvelope` and the prover-side witness
  assembly. `execute` returns both `ExecutionRecord` (contract) and
  `WitnessEnvelope` (witness); only the first is verifier-observable.
- `tabula-machine` owns the proof-backend primitives: trace
  construction, `prove_envelope`, `verify_envelope`, and the low-level
  proof-system verifier. It does **not** own the stage-level
  `PreparedProver` / `PreparedVerifier` types, because those carry
  statement-binding policy, which per canonical belongs to runtime.
  Machine's primitive verifier accepts a precomputed `binding_digest`
  plus a proof envelope and returns accept/reject; it does not
  recompute statements.
- `tabula-runtime` owns:
  - `PublicStatement::from_record` / `public_statement_from_record`
    (statement construction from execution output is runtime
    policy);
  - `PreparedProver` / `prepare_prover`, `PreparedVerifier` /
    `prepare_verifier`, and the high-level `prove` / `verify`
    entry points that perform the statement-binding dance (see §8
    verification sequence);
  - `VerifierState` (runtime-internal, as named in
    [`docs/design/architecture.md`](../design/architecture.md)
    §Verification Vocabulary).
  Runtime wraps machine primitives; runtime does not own wire
  types.
- `tabula-sdk` wraps the runtime entry points with ergonomic APIs.
  It does not own any wire type, does not hide stage caches in
  module-level state, and does not re-export types under SDK-
  specific names. See §9.
- `tabula-cli` is a consumer. The current CLI-owned
  `ReceiptBridge` (borsh serialization for `receipt.bin`) is
  **replaced**: the serialization belongs to `tabula-contract` as
  the canonical `ExecutionRecord` codec; the CLI is only
  responsible for file I/O.
- `tabula-eval` is a new consumer (see
  [`evaluation-harness.md`](evaluation-harness.md)). It imports
  `tabula-contract`, `tabula-executor`, `tabula-witness`,
  `tabula-runtime`, `tabula-machine`, and `tabula-sdk` — never
  `tabula-cli`.

## 5. Contract Types (`tabula-contract`)

Fields shown are private (accessed via read-only methods) unless
annotated `pub`. All wire types derive `serde::Serialize + Deserialize`
and `borsh::BorshSerialize + BorshDeserialize` with a stable canonical
form (sorted maps, length-prefixed sequences). See
[`evaluation-stage-support.md`](evaluation-stage-support.md) for the
concrete shape of each type listed here.

### 5.1 `SealedArtifact`

```rust
pub struct SealedArtifact {
    program_hash: Digest,
    metadata_hash: Digest,
    static_table_root: Digest,
    schema: Schema,
    static_tables: StaticTables,
    proof_layout: ProofLayout,
    verifying_keys: Vec<VerifyingKey>,
    // additional sealed material
}

impl SealedArtifact {
    pub fn program_hash(&self)       -> &Digest;
    pub fn metadata_hash(&self)      -> &Digest;
    pub fn static_table_root(&self)  -> &Digest;
    pub fn artifact_id(&self)        -> ArtifactId;
    pub fn schema(&self)             -> &Schema;
    pub fn static_tables(&self)      -> &StaticTables;
    pub fn proof_layout(&self)       -> &ProofLayout;
    pub fn verifying_keys(&self)     -> &[VerifyingKey];
}
```

Invariants:
- `artifact_id()` returns the `(program_hash, metadata_hash,
  static_table_root)` triple that `BoundStatement` binds to.
- `metadata_hash` covers `verifying_keys` so a proof compiled for a
  different proof-backend version cannot be paired with a matching
  program + static-table.
- Canonical serialization is byte-reproducible.

### 5.2 `ExecutionRecord` and `WitnessEnvelope`

```rust
// tabula-contract
pub struct ExecutionRecord {
    artifact_id: ArtifactId,
    pre_state_root: Digest,
    post_state_root: Digest,
    applied_tx_digest: Digest,
    public_context: PublicContext,
    events: EventLog,
}

// tabula-witness
pub struct WitnessEnvelope {
    // prover-only trace inputs; fields intentionally unspecified here
}
```

Semantics:
- `ExecutionRecord` contains only verifier-observable fields: exactly
  what a verifier needs to compare the resulting proof against a
  claim. It never contains witness / trace data.
- `WitnessEnvelope` carries the runtime-generated trace inputs to the
  machine. It is owned by `tabula-witness`; only `execute` and `prove`
  link it. Verifier-only builds do not link `tabula-witness` at all.
- `execute` returns `(ExecutionRecord, WitnessEnvelope)`. Call sites
  that only need the verifier-observable projection (CLI handoff,
  harness record cache) keep the first and drop the second.

Codec (contract, record only):
- `ExecutionRecord::encode(&self) -> Vec<u8>` and
  `ExecutionRecord::decode(&[u8]) -> Result<Self, CodecError>` are the
  canonical serializer/deserializer.
- CLI `receipt.bin` is exactly this encoding. The CLI performs file
  I/O only.
- `WitnessEnvelope` has its own codec in `tabula-witness` used by the
  harness when caching prover input.

### 5.3 `PublicStatement` (role: claim)

```rust
pub struct PublicStatement {
    artifact_id: ArtifactId,
    pre_state_root: Digest,
    post_state_root: Digest,
    public_context_digest: Digest,
    applied_tx_digest: Digest,
    event_digest: Digest,
}

impl PublicStatement {
    pub fn canonical_bytes(&self) -> Vec<u8>;
    pub fn digest(&self) -> Digest;  // Poseidon2 over domain-tagged canonical_bytes
    pub fn decode(bytes: &[u8]) -> Result<Self, CodecError>;
    pub fn encode(&self) -> Vec<u8>;
}
```

This is the *claim*: "against this sealed artifact, the batch moves
state from `pre_state_root` to `post_state_root` with these public
inputs."

Construction:
- In-process, `PublicStatement` is built by
  `tabula_runtime::public_statement_from_record(&artifact, &record)`
  (see §7, runtime ownership). The harness and CLI both call this.
- Externally supplied claims (chain-fetched, bundle-manifest) come in
  through `PublicStatement::decode`, i.e. the wire-only path. There
  is **no** `from_parts(...)` free constructor — role-typing is only
  meaningful if there is no open escape hatch.
- A pure-verifier build decodes `PublicStatement` from bytes; it does
  not need `ExecutionRecord` or the runtime policy crate to be in its
  link.

Canonicalization:
- `canonical_bytes` is a deterministic, field-ordered encoding with
  the first bytes reserved for a domain tag
  (`b"tabula.contract.public_statement.v1"`). The tag is mandatory; a
  cross-type collision attack (e.g. reinterpreting a `Proof` byte
  string as a `PublicStatement`) fails on the domain check.
- `digest()` is `Poseidon2(canonical_bytes)`. This is the value
  `Proof.public_statement_digest` (§5.4) commits to.

### 5.4 `Proof`

```rust
pub struct Proof {
    artifact_id: ArtifactId,
    public_statement_digest: Digest,   // Poseidon2(PublicStatement.canonical_bytes)
    envelope: ProofEnvelope,
}
```

Rules:
- `public_statement_digest` is `PublicStatement::digest` (§5.3), i.e.
  Poseidon2 over domain-tagged canonical bytes. It is **not** a copy
  of the statement; the caller supplies the statement and the
  verifier compares.
- `public_statement_digest` is transcript-bound by the prover: the
  same digest that `Proof` carries is the digest the AIR transcript
  commits to (mixed in alongside `ArtifactContext`). A prover cannot
  staple a different digest onto a proof of a different statement —
  that would fail transcript verification.
- `envelope` is the opaque, version-tagged payload consumed by the
  proof backend.

### 5.5 `BoundStatement` (role: attested fact)

`BoundStatement` is the verifier-side binding object from the
canonical vocabulary (architecture.md §Verification Vocabulary),
repurposed here as the typed return value of `verify`. It ties a
`PublicStatement` to one `SealedArtifact` via `ArtifactContext` and
carries the binding digest that was checked.

```rust
pub struct BoundStatement {
    claim: PublicStatement,
    artifact_context: ArtifactContext,  // canonical: contract-owned
    binding_digest: Digest,             // transcript-binding digest the verifier checked
    verifier_version: VerifierVersion,  // mixed into artifact_context derivation
}

impl BoundStatement {
    pub fn claim(&self) -> &PublicStatement;
    pub fn artifact_context(&self) -> &ArtifactContext;
    pub fn binding_digest(&self) -> &Digest;
    pub fn verifier_version(&self) -> &VerifierVersion;
}
```

`ArtifactContext` lives in `tabula-contract` (it is a proof-visible
binding object referenced by every layer, so it cannot depend on
runtime or machine). `VerifierState` — the runtime-internal prepared
verifier state — lives in `tabula-runtime` and is what
`PreparedVerifier` (§8) holds.

Rules:
- Produced only by `verify`. The only way to construct one is through
  the verify path; there is no free constructor.
- `BoundStatement` is itself deterministic: given the same
  `(SealedArtifact, PublicStatement, Proof, VerifierVersion)` inputs,
  two successful `verify` calls return byte-identical
  `BoundStatement` values. No wall-clock, no monotonic counter, no
  per-run nonce is baked in.
- `verifier_version` is part of the transcript binding. A v2 verifier
  cannot attest a proof produced against v1 binding rules; that
  divergence surfaces as a verify failure, not as a silently-different
  `BoundStatement`.
- Integrators that want to persist a proof outcome store a
  `BoundStatement`; they do not re-persist `PublicStatement` alone
  and treat it as verified.

`verify -> Result<BoundStatement, VerifyError>` is mandatory in the
API. Callers that care only about success/failure bind with `let _ =
...` or drop the value explicitly.

### 5.6 Support types

`ArtifactId`, `PublicContext`, `EventLog`, `ProofEnvelope`,
`VerifyingKey`, `ProofLayout`, `ArtifactContext`, and `Schema` are
contract-owned wire types. `WitnessEnvelope` is witness-owned.
`VerifierState` is runtime-owned (it is canonical's "runtime-internal
prepared verifier state" and is not a wire type).

Hash discipline:
- Digests that enter the verification pipeline (`PublicStatement`,
  `ArtifactContext`, `BoundStatement.binding_digest`,
  `Proof.public_statement_digest`, `SealedArtifact.artifact_id`
  components) are Poseidon2 over domain-tagged canonical bytes.
- Digests used only for engineering identity (cache keys, log
  correlation, `ExecutionRecord.events` content hashes) are Blake3.
  Blake3 digests must never be mixed into a transcript or substituted
  for a Poseidon2 digest.

Support types carry stable canonical serialization and a `digest()`
accessor whose hash is fixed by this discipline. Full skeletons live
in
[`evaluation-stage-support.md`](evaluation-stage-support.md).

## 6. Executor Types (`tabula-executor`)

### 6.1 `PreparedExecutor`

```rust
pub struct PreparedExecutor {
    artifact: Arc<SealedArtifact>,
    // schema-dependent warm state (JIT, arenas, static-table hydration)
}

impl PreparedExecutor {
    pub fn artifact(&self) -> &SealedArtifact;

    pub fn execute(
        &self,
        initial_state: &StateSnapshot,
        batch: &TxBatch,
    ) -> Result<(ExecutionRecord, WitnessEnvelope), ExecuteError>;
}

pub fn prepare_executor(artifact: Arc<SealedArtifact>)
    -> Result<PreparedExecutor, PrepareError>;
```

Rules:
- `prepare_executor` may perform any amount of schema-dependent
  warmup.
- Two `PreparedExecutor` instances for the same artifact share no
  warm state; reuse across call sites is explicit via the handle.
- `execute` is `&self`, safe to call repeatedly on independent
  batches, and returns both the verifier-observable record and the
  prover-side witness. Callers that only need the verifier projection
  drop the witness.

## 7. Runtime Policy (`tabula-runtime`)

Runtime owns statement construction and the assembly of machine-facing
inputs — it does not own wire types.

```rust
/// Convert an executor's record into the claim a prover commits to
/// and a verifier checks against. Pure function of `(artifact, record)`.
pub fn public_statement_from_record(
    artifact: &SealedArtifact,
    record: &ExecutionRecord,
) -> PublicStatement;
```

Rules:
- This is the only sanctioned in-process constructor of
  `PublicStatement` from an execution record. CLI, harness, and SDK
  all call it.
- Runtime also wires prover inputs (assembling `ExecutionRecord` +
  `WitnessEnvelope` + `PublicStatement` for `PreparedProver::prove`)
  and prepares verifier inputs. Those wiring helpers are policy, not
  wire types.

## 8. Runtime-Prepared Stages (`tabula-runtime`)

The stage-level prover and verifier handles are owned by
`tabula-runtime`: they carry statement-binding policy (`ArtifactContext`
rebuild, `VerifierVersion`, domain-tag derivation) and delegate the
proof-system primitives to `tabula-machine` (see §8.3). Canonical
architecture puts "statement binding" and "preparation of
backend-ready inputs" in runtime, and "verification" as a
proof-backend primitive in machine; this section realises that split.

### 8.1 `PreparedProver`

```rust
pub struct PreparedProver {
    artifact: Arc<SealedArtifact>,
    context:  ArtifactContext,              // rebuilt from artifact
    machine:  tabula_machine::BackendProver, // primitive prover state
    // + any prover-only warm state (witness tables, prover config)
}

impl PreparedProver {
    pub fn prove(
        &self,
        record:   &ExecutionRecord,
        witness:  &WitnessEnvelope,
        claim:    &PublicStatement,
    ) -> Result<Proof, ProveError>;
}

pub fn prepare_prover(artifact: Arc<SealedArtifact>)
    -> Result<PreparedProver, PrepareError>;
```

Rules:
- `prove` consumes all three inputs by reference. None are mutated.
- `claim` is **required**, not re-derived. The prover pipeline
  (runtime side):
  1. Reject if `claim.artifact_id != self.artifact.artifact_id()`.
  2. Compute `claim_digest = claim.digest()`.
  3. Derive `binding_digest` via the same formula as §8.2 step 6.
  4. Hand `(record, witness, binding_digest)` to
     `tabula_machine::BackendProver` to build the envelope.
  5. Wrap the machine output in `Proof { artifact_id,
     public_statement_digest: claim_digest, envelope }`.

### 8.2 `PreparedVerifier`

```rust
pub struct PreparedVerifier {
    artifact: Arc<SealedArtifact>,
    state:    VerifierState,                  // runtime-owned; holds
                                              // artifact_context +
                                              // relation policy +
                                              // machine verifier state
}

impl PreparedVerifier {
    pub fn verify(
        &self,
        proof: &Proof,
        claim: &PublicStatement,
    ) -> Result<BoundStatement, VerifyError>;
}

pub fn prepare_verifier(artifact: Arc<SealedArtifact>)
    -> Result<PreparedVerifier, PrepareError>;
```

**Verification sequence.** `verify` performs these checks in order
and short-circuits on the first failure:

1. Recompute the verifier-side `artifact_id` from
   `self.artifact.artifact_id()`. This is the authoritative triple.
2. Reject if `proof.artifact_id != verifier_artifact_id`.
3. Reject if `claim.artifact_id  != verifier_artifact_id`. Neither
   side is allowed to substitute a different `ArtifactId`.
4. Compute `claim_digest = claim.digest()`.
5. Reject if `claim_digest != proof.public_statement_digest`.
6. Derive the transcript-binding digest:
   ```
   binding_digest = Poseidon2(
       DOMAIN_TAG("tabula.binding.v1")
       || canonical_bytes(self.state.artifact_context)
       || claim_digest
       || verifier_version.to_bytes()
   )
   ```
   The domain tag is *inside* the hash, not prefixed to its
   output. The canonical bytes of `ArtifactContext` (contract-
   owned) are the same bytes every honest prover produces.
7. Hand `(binding_digest, proof.envelope)` to the machine primitive
   `tabula_machine::BackendVerifier::verify_envelope`.
8. On machine-primitive success, bind the result:
   `BoundStatement { claim: claim.clone(),
   artifact_context: self.state.artifact_context.clone(),
   binding_digest, verifier_version: self.state.verifier_version }`.

Rules:
- `verify` is feature-independent: it does not require `execute`,
  `prove`, or `tabula-witness` to be enabled in the build. It does
  require `tabula-machine` (primitive verifier) and
  `tabula-contract` (wire types).
- `verify` never rebuilds `claim`.
- The sequence is the theorem object. Any change to the order, to
  the domain tag, or to which values are hashed into
  `binding_digest` is a verification-semantics change and must
  bump `VerifierVersion`.

### 8.3 Machine primitives (`tabula-machine`)

Machine is the backend. It does not know about statement binding.
Its verify-side API accepts a pre-derived `binding_digest` and a
proof envelope; its prove-side API accepts the analogous inputs
plus the witness:

```rust
// tabula-machine
pub struct BackendVerifier { /* verifying keys, precomputation */ }
pub struct BackendProver   { /* trace config, prover keys */ }

impl BackendVerifier {
    pub fn verify_envelope(
        &self,
        binding_digest: &Digest,
        envelope: &ProofEnvelope,
    ) -> Result<(), BackendError>;
}

impl BackendProver {
    pub fn prove_envelope(
        &self,
        binding_digest: &Digest,
        record:  &ExecutionRecord,
        witness: &WitnessEnvelope,
    ) -> Result<ProofEnvelope, BackendError>;
}
```

Rules:
- Machine's two primitives are the **only** public entry points
  the runtime's `PreparedProver` / `PreparedVerifier` call.
- Machine never reads `PublicStatement`, `BoundStatement`, or
  `ArtifactContext` — it only consumes `binding_digest` as the
  single scalar input representing all of them.
- This keeps machine free of statement-binding policy and makes
  `VerifierVersion` bumps a pure runtime concern.

## 9. SDK (`tabula-sdk`)

The SDK is the ergonomic entry point for *applications* that want
one handle that (a) holds prepared stages, (b) lazily prepares what
is needed, (c) makes reuse across calls explicit, and (d) exposes a
single artifact-loading path (file, bytes, chain).

It is deliberately more than a forwarding shim — applications that
want a single object to pass around should use it; the harness and
low-level consumers can reach for the per-crate APIs directly.

```rust
pub struct Tabula { /* executor / prover / verifier slots, lazy */ }

impl Tabula {
    pub fn open(artifact: SealedArtifact, config: OpenConfig)
        -> Result<Self, SdkError>;

    pub fn load_artifact_from_bytes(bytes: &[u8])
        -> Result<SealedArtifact, SdkError>;
    pub fn load_artifact_from_path(path: &Path)
        -> Result<SealedArtifact, SdkError>;

    pub fn artifact(&self) -> &SealedArtifact;

    pub fn execute(&self, initial: &StateSnapshot, batch: &TxBatch)
        -> Result<(ExecutionRecord, WitnessEnvelope), SdkError>;

    pub fn prove(&self, record: &ExecutionRecord, witness: &WitnessEnvelope,
                 claim: &PublicStatement)
        -> Result<Proof, SdkError>;

    pub fn verify(&self, proof: &Proof, claim: &PublicStatement)
        -> Result<BoundStatement, SdkError>;

    /// Sanctioned statement construction at the SDK boundary —
    /// delegates to tabula_runtime::public_statement_from_record.
    pub fn public_statement_from_record(
        &self, record: &ExecutionRecord,
    ) -> PublicStatement;
}
```

Rules:
- `OpenConfig` selects which of `{executor, prover, verifier}` to
  prepare. A verifier-only build cannot prepare the others.
- Stage preparation is lazy: calling `verify` on a handle opened for
  `{verifier}` is fine; calling `prove` on the same handle is an
  error — the SDK does not silently upgrade.
- The SDK does **not** own any module-level or static cache. All
  reuse is through the `Tabula` handle itself; dropping the handle
  drops the prepared state.
- The SDK does not define wire types; callers serializing stages use
  the contract-owned codec directly.

## 10. Materialization API

Every stage has an explicit materializer:

| From | To | Function | Crate |
|------|-----|----------|-------|
| `SourceProgram` | `SealedArtifact` | `tabula_compiler::compile(&SourceProgram)` | compiler |
| `SealedArtifact` | `PreparedExecutor` | `prepare_executor(Arc<SealedArtifact>)` | executor |
| `SealedArtifact` | `PreparedProver`   | `prepare_prover(Arc<SealedArtifact>)`   | runtime |
| `SealedArtifact` | `PreparedVerifier` | `prepare_verifier(Arc<SealedArtifact>)` | runtime |
| `(PreparedExecutor, state, batch)` | `(ExecutionRecord, WitnessEnvelope)` | `PreparedExecutor::execute` | executor |
| `(SealedArtifact, ExecutionRecord)` | `PublicStatement` | `tabula_runtime::public_statement_from_record` | runtime |
| `(PreparedProver, ExecutionRecord, WitnessEnvelope, PublicStatement)` | `Proof` | `PreparedProver::prove` | runtime |
| `(PreparedVerifier, Proof, PublicStatement)` | `BoundStatement` | `PreparedVerifier::verify` | runtime |

Cache keys (stable content-addressed identifiers, used by the harness
to detect stage reuse):

- `SealedArtifact` → `ArtifactId`.
- `PreparedExecutor` / `PreparedProver` / `PreparedVerifier` →
  `(artifact_id, prepared_version, feature_set_digest)`.
- `ExecutionRecord` → `(artifact_id, initial_state_digest, batch_digest)`.
- `WitnessEnvelope` → `(artifact_id, record_digest)`.
- `PublicStatement` → `PublicStatement::digest()`.
- `Proof` → `(artifact_id, public_statement_digest)` plus envelope
  hash if the backend introduces nondeterminism.

## 11. Feature Matrix

Enforced in `Cargo.toml` at the workspace root and in each crate's
manifest. This matrix is **stricter** than the current shape;
implementations that currently require `execute` to be present in the
verifier path must be refactored.

| Feature | Enables | Extra deps beyond default |
|---------|---------|---------------------------|
| `verify`  | `PreparedVerifier`, `PublicStatement` decode, `Proof` decode, `BoundStatement` | `tabula-runtime` (verify-slice), `tabula-machine` (verifier primitive) |
| `execute` | `PreparedExecutor`, `ExecutionRecord` encode/decode, `WitnessEnvelope` | `tabula-executor`, `tabula-witness` |
| `prove`   | `PreparedProver` | `tabula-runtime` (prove-slice), `tabula-machine` (prover primitive), `tabula-witness` |
| `author`  | `tabula-lang`, `tabula-compiler` | authoring crates |
| `full`    | all of the above | — |

Explicit non-dependencies:

- `verify` must not pull in `tabula-executor`, `tabula-witness`, or
  `tabula-machine`'s prover primitives (`BackendProver`). The
  `tabula-runtime` and `tabula-machine` crates internally gate
  prover-only modules behind their own `prove` feature so a
  verify-only workspace build does not compile them.
- `prove` does not depend on `execute` at runtime. The harness can
  legitimately prove a cached `(ExecutionRecord, WitnessEnvelope)`
  pair without running the executor in this process.
- Neither `execute` nor `verify` may pull in `tabula-lang` or
  `tabula-compiler`. Programs are sealed artifacts; runtime does not
  need the authoring toolchain.
- Feature independence is enforced by CI commands under §16
  (acceptance criteria), not by prose.

## 12. Serialization

All wire types:

- Derive `serde::Serialize + Deserialize` and
  `borsh::BorshSerialize + BorshDeserialize`.
- Provide a canonical byte form (sorted maps, length-prefixed
  sequences, fixed endianness).
- Provide `digest(&self) -> Digest` returning
  `Poseidon2(canonical_bytes)` when the digest is used in the
  verification pipeline; otherwise Blake3 for engineering-only
  digests.

Versioning:

- A `wire_version: u16` is the first field of every serialized wire
  type. Decoders reject unknown versions.
- Version bumps are a clean break: the old version is removed. There
  is no dual-write period.

## 13. Errors

Every crate exposes a single top-level error type. Cross-crate
conversions are via `#[from]`. Errors carry structured context
(artifact id, stage, input digest) and never bare `&'static str`.

Concrete types:

- `CompileError` in `tabula-compiler`.
- `PrepareError` in `tabula-executor` (executor-side) and
  `tabula-runtime` (prover/verifier-side, since prepared handles
  live in runtime). Each is its own type.
- `ExecuteError` in `tabula-executor`.
- `ProveError`, `VerifyError` in `tabula-runtime` (they wrap both
  statement-binding errors and machine `BackendError`).
- `BackendError` in `tabula-machine` (primitive prove/verify
  failures — transcript mismatch, FRI failure, decode error).
- `CodecError` in `tabula-contract`.
- `SdkError` in `tabula-sdk` wraps the above via `#[from]`.

Top-level types listed in the support appendix
([`evaluation-stage-support.md`](evaluation-stage-support.md)); the
authoritative definitions remain in the owning crates.

## 14. File Layout

```
crates/
  contract/
    src/
      artifact.rs                # SealedArtifact
      execution_record.rs        # ExecutionRecord + codec
      statement.rs               # PublicStatement, BoundStatement,
                                 # ArtifactContext
      proof.rs                   # Proof envelope
      support/                   # Schema, PublicContext, EventLog, ...
      error.rs                   # CodecError

  executor/
    src/
      prepared.rs                # PreparedExecutor, prepare_executor
      execute.rs                 # execute(): (Record, Witness)
      error.rs                   # PrepareError, ExecuteError

  witness/
    src/
      envelope.rs                # WitnessEnvelope (opaque)

  runtime/
    src/
      policy.rs                  # public_statement_from_record, other
                                 # orchestration policy
      prover/prepared.rs         # PreparedProver, prepare_prover
      verifier/prepared.rs       # PreparedVerifier, prepare_verifier
      verifier_state.rs          # VerifierState (runtime-internal)
      error.rs                   # PrepareError (runtime),
                                 # ProveError, VerifyError

  machine/
    src/
      backend_prover.rs          # BackendProver::prove_envelope
      backend_verifier.rs        # BackendVerifier::verify_envelope
      error.rs                   # BackendError

  sdk/
    src/
      tabula.rs                  # Tabula facade
      loader.rs                  # load_artifact_from_{path,bytes}

  cli/
    src/
      handoff/                   # file I/O only; no borsh types live here
```

## 15. Clean-Break Migration

Each bullet below is a concrete relocation / replacement. Tabula has
no external users, so the migration prefers an ideal shape over a
layered deprecation path. Items are ordered so that each step can be
performed without requiring a later step to already be done (no
hidden cycles).

1. **Delete `tabula-cli/src/handoff/receipt_bridge.rs`.** Move its
   borsh derivation to the matching fields on `ExecutionRecord` and
   `WitnessEnvelope` in `tabula-contract` / `tabula-witness`. The
   CLI's `receipt.bin` reader/writer becomes a thin wrapper around
   `ExecutionRecord::encode` / `decode` plus `WitnessEnvelope`
   encode/decode.

2. **Delete the aggregate `OpenedProgram` type.** Its two roles split
   into `SealedArtifact` (wire) and `PreparedExecutor / PreparedProver
   / PreparedVerifier` (handles). Every call site takes one of the
   latter three explicitly.

3. **Replace `artifact_digest: String` fields with `ArtifactId`.**
   Strings lose structure and silently accept the wrong hash length.
   Every field that identified an artifact by a stringified digest
   becomes `ArtifactId`.

4. **Adopt canonical verification vocabulary.** Rename any residual
   `ExpectedStatement` to `PublicStatement` and any residual
   `AttestedStatement` to `BoundStatement`. Contract-side constructors
   follow: `PublicStatement::from_record` is the sole in-process
   constructor from `(SealedArtifact, ExecutionRecord)`; the
   `from_parts` escape hatch is removed.

5. **Hoist statement construction to `tabula-runtime`.** The policy
   function `public_statement_from_record(artifact, record) ->
   PublicStatement` lives in runtime and delegates to
   `PublicStatement::from_record`. Prover and verifier never rebuild
   claims; they accept `&PublicStatement` from the caller.

6. **Change `verify`'s return type.** `verify` currently returns
   `Result<(), VerifyError>`; make it
   `Result<BoundStatement, VerifyError>`. Every call site either binds
   the return value or explicitly `let _ = ...`. `BoundStatement` is
   deterministic (no `verified_at_cycle`).

7. **Remove hidden SDK caches.** Drop any module-level
   `Mutex<BTreeMap<ArtifactId, _>>` or `OnceLock<HashMap<...>>` inside
   `tabula-sdk`. All reuse flows through the `Tabula` handle. The
   harness owns its own cache (see harness note); the SDK does not.

8. **Stop re-exporting contract types under SDK names.** If code
   reads `tabula_sdk::PublicStatement`, rewrite it to
   `tabula_contract::PublicStatement`. Same for every other wire
   type.

9. **Tighten the feature matrix.** Audit every
   `#[cfg(feature = "...")]` block in the workspace and ensure
   `verify`-only builds compile cleanly without `execute` / `prove` /
   `author`. Remove accidental imports. The enforcement checks in
   §16 are authoritative.

10. **Introduce an explicit `PreparedMachine` boundary if needed.**
    The harness benchmarks compose-time separately from shard-level
    prover work. If the existing prover does not expose a
    prepared-machine handle separable from the `PreparedProver`
    facade, add one in `tabula-machine` (`PreparedMachine`), surfacing
    the per-shard prover entry points the harness measures. If
    `PreparedProver` already exposes this cleanly, this item is a
    no-op.

11. **Delete placeholder workload families.** The three names
    `transfer`, `membership`, `compute_control` that appeared in the
    previous version of this document are gone. The workload is
    defined by [`eurosys-2026-workload.md`](eurosys-2026-workload.md).

## 16. Acceptance Criteria

The target shape in this note is achieved when the following checks
pass. Each check is a build or lint command so reviewers cannot
satisfy it by renaming tokens to evade a text match.

1. **Contract isolation.**
   `cargo tree -p tabula-contract -e normal,build` lists no crate
   from `{runtime, machine, executor, witness, sdk, cli, lang,
   compiler}`.
2. **Verifier-only feature slice.**
   `cargo check -p tabula-machine --features verify
    --no-default-features` succeeds, and
   `cargo tree -p tabula-machine --features verify
    --no-default-features -e normal,build` lists no crate from
   `{executor, witness, runtime-prover-parts, compiler, lang}`.
3. **SDK lazy preparation compiles.**
   `cargo check -p tabula-sdk --features verify
    --no-default-features` succeeds; the same build with
   `--features prove` also succeeds independently.
4. **No hidden SDK cache.** A workspace clippy lint deny list forbids
   `std::sync::Mutex` and `std::sync::OnceLock` at crate scope in
   `tabula-sdk`; the lint is wired into CI and is enforced by
   `cargo clippy -p tabula-sdk -- -D warnings`.
5. **Stringly-typed artifact digests are gone.** A custom clippy lint
   (or a compile-time sanity test) asserts there is no field of type
   `String` whose identifier matches `artifact_digest`, `program_hash`,
   or `artifact_id` across the workspace. Replacement is `ArtifactId`
   (for the triple) or `Digest` (for a single hash).
6. **Verify returns `BoundStatement`.** A type-level test in
   `tabula-runtime` (e.g. `fn _assert_verify_sig(_: &PreparedVerifier,
   _: &Proof, _: &PublicStatement) -> Result<BoundStatement,
   VerifyError> { unimplemented!() }`) pins the signature; any
   regression breaks the build.
7. **Single statement constructor.** `PublicStatement::from_record`
   is `pub(crate)` to `tabula-contract` internals plus a re-export
   through `tabula-runtime::public_statement_from_record`. The
   machine crate's API surface exposes no alternate constructor. A
   public-API test enforces this.
8. **CLI receipt wrapper is thin.** `tabula-cli`'s receipt module has
   no borsh derive and no wire-type definitions; it only calls
   `ExecutionRecord::encode/decode` and `WitnessEnvelope::encode/decode`.
9. **Harness dependency direction.** `cargo tree -p tabula-eval -e
    normal,build` lists `tabula-contract`, `tabula-runtime`,
    `tabula-executor` (optional under `execute`), `tabula-machine`,
    and `tabula-sdk`; it does not list `tabula-cli`.

## 17. Out Of Scope

- The harness crate itself — see
  [`evaluation-harness.md`](evaluation-harness.md).
- Algorithm changes inside the compiler, executor, runtime, or
  machine.
- External system adapters (SP1, RISC0). Those consume the harness's
  `SystemAdapter` trait, which is a harness concern.
- Any change to the proof-backend wire format beyond the transcript
  binding described in §5.4 and §8.

## 18. References

- [`docs/design/architecture.md`](../design/architecture.md) —
  canonical dependency direction, layer boundaries, and verification
  vocabulary.
- [`evaluation-stage-support.md`](evaluation-stage-support.md) —
  balanced top-level skeletons for support types referenced in this
  note.
- [`evaluation-harness.md`](evaluation-harness.md) — harness crate
  that consumes these stages.
- [`eurosys-2026-contributions.md`](eurosys-2026-contributions.md) —
  paper contribution list.
- [`eurosys-2026-workload.md`](eurosys-2026-workload.md) — locked
  workload.
- [`eurosys-2026-section-outline.md`](eurosys-2026-section-outline.md)
  — paper section outline.
