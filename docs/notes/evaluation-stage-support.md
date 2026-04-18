# Evaluation Stage Support Types

Working note. **Not authoritative** — canonical architecture lives in
[`docs/design/architecture.md`](../design/architecture.md); canonical
stage types live in
[`evaluation-stage-interfaces.md`](evaluation-stage-interfaces.md).

This note collects the top-level data definitions referenced by the
stage-interfaces and harness notes but not fully laid out there,
such as `Digest`, `ArtifactId`, `PublicContext`, `WitnessEnvelope`,
and the harness-side identity and event types. It is deliberately
**balanced**: enough to pin roles, ownership, and the shape that
appears in cross-crate signatures, but not an exhaustive field list.
Fields whose exact layout is best decided during implementation are
noted with `/* ... */`.

## 1. Purpose

Centralise the small support types so they can be referenced by
signature without each caller having to re-derive them. The
stage-interfaces note is the authority for *which* types appear
where; this note is the authority for *what each of those types is,
at the top level*.

Anything more detailed (field-by-field codec rules, canonical-bytes
layout, error hierarchies, performance-sensitive internal
representations) belongs in the owning crate's README or source
comments, not here.

## 2. Core Digest Types (`tabula-contract`)

```rust
/// A single content-addressed hash output. The pipeline-level
/// digest (used inside `PublicStatement`, `ArtifactContext`, and
/// the transcript binding) is Poseidon2 over canonical bytes with
/// a domain tag. Engineering-only digests (cache keys, file
/// content checks) are Blake3.
pub struct Digest(pub [u8; 32]);

/// Namespaced artifact identity. Not a single digest — the triple
/// expresses which axes of an artifact are independently bound.
pub struct ArtifactId {
    pub program_hash:      Digest,
    pub metadata_hash:     Digest,
    pub static_table_root: Digest,
}

/// Cryptographic domain separation tag; a constant byte string
/// mixed into every pipeline-level digest that could otherwise
/// collide across semantic roles.
pub struct DomainTag(pub &'static [u8]);
```

Rules:
- `ArtifactId` never degrades to a bare `Digest` or `String`; every
  accessor on `SealedArtifact` returns components of this triple.
- `Digest` is a newtype, not a `[u8; 32]` alias, so the type system
  keeps Poseidon2 and Blake3 outputs distinguishable at the
  boundary even though the byte layout is identical.

## 3. Public and Artifact Context (`tabula-contract`)

```rust
/// The caller-facing part of a sealed artifact's public context:
/// everything that must be observable to honest provers and
/// verifiers but is *not* a secret witness. Exact fields are
/// defined by the contract crate.
pub struct PublicContext { /* ... */ }

/// Verifier-rebuilt binding context. Constructed from a
/// `SealedArtifact` and the verifier's `VerifierVersion`; mixed
/// into the transcript so that a proof cannot be cross-claimed
/// against a different artifact.
pub struct ArtifactContext { /* ... */ }

/// Canonicalized schema of the typed state + batch surface the
/// artifact commits to. Used by the executor and by
/// `BehaviorOracle` comparisons in the harness.
pub struct Schema { /* ... */ }

/// Static lookup tables sealed into the artifact. The digest over
/// these feeds `ArtifactId::static_table_root`.
pub struct StaticTables { /* ... */ }

/// Verifier-observable event log produced by execution. Exact
/// event shape is the executor's business; the contract types
/// expose it to downstream code only through a canonical digest.
pub struct EventLog { /* ... */ }
```

## 4. Proof Objects (`tabula-contract` + `tabula-machine`)

```rust
/// Top-level proof envelope. Carries the binding-digest commitment
/// and enough metadata for the verifier to recompute which
/// artifact it belongs to without reading the full artifact first.
pub struct Proof {
    pub artifact_id:             ArtifactId,
    pub public_statement_digest: Digest,
    pub envelope:                ProofEnvelope,
}

/// Verifier-issued attested fact — the return of
/// `PreparedVerifier::verify`. Unlike `PublicStatement` (a
/// caller-supplied claim), a `BoundStatement` exists only after a
/// proof has successfully verified against a specific
/// `ArtifactContext`. See stage-interfaces §7 for the binding rule.
pub struct BoundStatement {
    pub artifact_id:     ArtifactId,
    pub claim_digest:    Digest,   // Digest(PublicStatement)
    pub binding_digest:  Digest,   // Poseidon2 over ArtifactContext + claim + version
    pub verifier_version: VerifierVersion,
    /* ... */
}

/// Opaque proof-system payload (STARK proof, FRI commitments,
/// grand-product quotients, ...). The machine crate owns its
/// internal structure; downstream code treats it as canonical
/// bytes plus `ProofLayout`.
pub struct ProofEnvelope { /* ... */ }

/// Shape of a concrete proof: shard count, per-shard row count,
/// column counts, commitment sizes. Observable to the harness for
/// `ProofLayoutSummary`; immaterial to verification.
pub struct ProofLayout { /* ... */ }

/// Prepared verifier's bound reference program: the machine-level
/// verifying key plus any cached precomputation.
pub struct VerifyingKey { /* ... */ }

/// Human-readable version of the verification pipeline. Bumped on
/// any change to the binding-digest derivation or the proof-system
/// verifier's semantics.
pub struct VerifierVersion(pub u32);
```

## 5. Execution Support Types (`tabula-contract`, `tabula-executor`, `tabula-witness`)

```rust
/// Input to `execute`: the typed-tabular state before the batch.
/// Canonical bytes feed cache keys and the `PublicStatement`.
pub struct StateSnapshot { /* ... */ }

/// Input to `execute`: the ordered transaction batch. Canonical
/// bytes feed cache keys and the `PublicStatement`.
pub struct TxBatch { /* ... */ }

/// Output of `execute`, verifier-observable. Exact fields are
/// defined by the contract crate; stage-interfaces §5.2 lists
/// the ownership rule.
pub struct ExecutionRecord { /* ... */ }

/// Output of `execute`, prover-only. Opaque to contract and
/// machine-verifier code. The witness crate owns the byte format;
/// no other crate peers inside this type.
pub struct WitnessEnvelope { /* opaque */ }
```

## 6. Error Types

One top-level error type per crate; cross-crate flows carry the
source via `#[from]`. The fields below are indicative — actual
codec and I/O errors live inside each.

```rust
pub enum CodecError   { /* canonical-bytes / borsh decode errors */ }
pub enum PrepareError { /* artifact-to-prepared transitions */ }
pub enum ExecuteError { /* runtime errors inside the executor */ }
pub enum ProveError   { /* prover setup + proof construction */ }
pub enum VerifyError  { /* binding mismatches + proof-system nak */ }
pub enum SdkError     { /* wraps the above via `#[from]` */ }
```

Rules:
- Every variant carries structured context (artifact id, stage,
  input digest) rather than a bare `&'static str`.
- `VerifyError` distinguishes *binding* failures (wrong
  `ArtifactId`, wrong `public_statement_digest`) from *proof-system*
  failures (transcript mismatch, FRI failure). The harness uses
  this split to classify cross-system rejection reasons.

## 7. Harness Identity Types (`tabula-eval`)

```rust
/// Stable human-chosen identifier for a `Workload`. Matched
/// against the registry; also appears on every `BenchmarkRecord`.
pub struct WorkloadId(pub String);

/// Fingerprint of a `WorkloadVariant` — canonical-bytes digest
/// over the variant struct. Used for filtering in the output
/// schema without reconstructing the variant.
pub struct WorkloadVariantFingerprint(pub Digest);

/// Cross-system, content-addressed semantic identity of a variant.
/// Two adapters on the same variant share this digest; it is the
/// join key the harness uses for cross-system summaries.
pub type SemanticWorkloadDigest = Digest;

/// Enumeration of the registered external systems. Kept as an enum
/// rather than a string so missing adapters fail at compile time.
pub enum SystemId {
    Tabula,
    Sp1,
    Risc0,
}

/// Toolchain / build identity of a system. Field layout is
/// adapter-specific; the harness treats it as a canonical-bytes
/// struct for fingerprinting.
pub struct SystemVersion { /* ... */ }

/// Byte form of a per-system `SystemArtifactId`. Stored as opaque
/// bytes in the output schema so different systems' identities
/// coexist on the same row shape.
pub struct SystemArtifactIdBytes(pub Vec<u8>);
```

## 8. Fixture and Sweep Types (`tabula-eval`)

```rust
/// 256-bit fixture seed. Derivable from a name via Blake3 so the
/// CLI accepts either raw bytes or `"paper_headline_run_1"`.
pub struct FixtureSeed(pub [u8; 32]);

/// Value for a single workload parameter. Implementations choose
/// the arm; the harness does not flatten into strings.
pub enum ParameterValue {
    U64(u64),
    I64(i64),
    Str(String),
    Bool(bool),
    List(Vec<ParameterValue>),
    /* intentionally no `Float` — fixtures forbid floating-point;
       sweeps express ratios as two integers. */
}

/// Declaration of what parameters a workload accepts. Consumed by
/// the CLI for validation and by `generate_fixture` for typing.
pub struct ParameterSchema { /* ... */ }

/// Per-adapter summary of proof shape, populated after `prove` for
/// the row's system. Shape is adapter-specific; the struct only
/// pins the top-level name.
pub struct ProofLayoutSummary { /* ... */ }
```

## 9. Harness Runtime Types (`tabula-eval`)

```rust
/// One cache lookup outcome, emitted per stage per repetition.
pub struct CacheEvent {
    pub stage:   StageKind,
    pub outcome: CacheOutcome,
    pub key:     Digest,
    /* ... */
}

pub enum CacheOutcome {
    Hit,
    Miss,
    Rehydrated,
    Mismatch,   // hard error; populated on the row that failed it
}

/// Content-addressed key for the harness's on-disk cache. The
/// concrete construction for each stage is specified in
/// evaluation-harness.md §13.1.
pub struct CacheKey(pub Digest);

/// Process-level environment captured per row: CPU pinning, env
/// sanitisation, subprocess transport, etc.
pub struct RunEnv { /* ... */ }

/// Indicator for `BenchmarkRecord::phase`. `Warmup` and `Replay`
/// rows are excluded from headline statistics; `ThermalOutlier`
/// is excluded from summary rows unless explicitly overridden.
pub enum RecordPhase {
    Warmup,
    Measurement,
    Replay,
    ThermalOutlier,
    Debug,
}
```

## 10. Out Of Scope

- Byte-level canonical encoding for any type above. Those rules
  belong in the owning crate; this note deliberately avoids
  committing the codebase to a specific layout before
  implementation.
- Internal field lists for types marked `/* ... */`. The
  stage-interfaces note and the harness note reference these types
  *by role only*; pinning fields here would force two updates per
  change.
- Error variant lists. Each error type's variants are a thing the
  owning crate iterates on during implementation; listing them
  here just creates drift.

## 11. References

- [`docs/design/architecture.md`](../design/architecture.md) —
  canonical dependency direction, layer boundaries, and
  verification vocabulary.
- [`evaluation-stage-interfaces.md`](evaluation-stage-interfaces.md)
  — cross-role stage types that reference these support types.
- [`evaluation-harness.md`](evaluation-harness.md) — harness crate
  that references the harness-side identity types.
