# tabula-contract

`tabula-contract` is the fail-closed trust-contract layer for Tabula. It
defines the versioned metadata, proof-visible schemas, and canonical encoding
rules that compiler, runtime, machine, SDK, and CLI code must interpret
identically.

## Role

This crate exists to answer two questions:

- "When should two sides of the system consider a sealed artifact compatible?"
- "What exactly is the public object that a proof certifies?"

The lasting boundary is that compatibility policy, artifact-visible metadata,
public-statement schemas, proof-envelope contracts, canonical encodings, and
fail-closed validation live here.

## Owns

- versioned contract metadata carried with sealed artifacts
- compatibility policy applied at proof and verification entry points
- artifact-derived verifier context shared across runtime and verifier paths
- the stable proved public object: `PublicStatement`
- the verifier-side outer binding object: `BoundStatement`
- the canonical `proof.bin` outer envelope (`ProofEnvelope`)
- proof-visible schemas shared across compiler/runtime/verifier paths
- canonical digest and encoding rules that multiple layers must match bit-for-bit
- fail-closed validation for known and unknown compatibility versions

## Does Not Own

- program semantics
- compiler-native artifact storage or canonical serialization
- execution behavior
- runtime orchestration
- witness aggregation policy
- backend proof implementation or concrete proof codecs

## Design Intent

- Prefer explicit versioned contracts over implicit compatibility.
- Fail closed when the system encounters unknown or mismatched compatibility
  data.
- Keep compatibility rules reusable across compiler, runtime, and verifier
  entry points.
- Make the stable public theorem object fixed-size and explicit.

## Core Contract

- Unknown compatibility or proof-envelope versions are incompatible by default.
- Compatibility checks reject mismatches rather than auto-repairing or silently
  downgrading them.
- `PublicStatement` is the proved public object:
  `(old_root, new_root, h_ctx, h_tx, h_evt)`.
- `ArtifactContext` is recomputed from the sealed artifact by the
  verifier.
- `BoundStatement` combines artifact-derived invariants and the proved
  `PublicStatement` into the outer transcript binding.
- `ProofEnvelope` is transport metadata plus opaque proof bytes. It is not
  the theorem root.
- Secure verification is statement-first:
  `VerifyPublicStatement(artifact, public_statement, proof)`.
- A canonical derivation API, `public_statement_from_record(artifact, record)`,
  is planned so `PublicStatement` has one authoritative constructor in this
  crate. It is deferred until `ExecutionRecord` lands (likely the SP that
  consolidates runtime materialization), because lifting the current
  materializer would pull runtime and `tabula-types` registries into contract
  and violate the dep-set above.

## Dependency Rules

- This crate depends on `tabula-core` and `tabula-commitment` (shared-foundation
  primitives: `PoseidonHasher`, `NativeDigest`, `FieldHasher`). It does not
  depend on `tabula-ir`, `tabula-stark`, `tabula-types`, compiler, runtime, or
  backend proof crates.
- As the wire-type authority, every other workspace crate is free to depend on
  `tabula-contract`; the reverse is forbidden.
- If a rule is about whether two sides should trust the same sealed contract,
  it belongs here before it belongs in consumers.

## How To Change This Crate Safely

- Introduce compatibility changes deliberately, with coordinated updates across
  compiler, runtime, and verifier consumers.
- Resist adding permissive fallback behavior for version mismatches.
- Treat public-statement field changes and outer-binding changes as contract
  changes, not ordinary refactors.
- Keep verifier-visible concepts explicit; do not reintroduce documentary
  surfaces that are not consumed by the live proof path.

## Tests

Start with:

- `cargo test -p tabula-contract`

Preserve the behaviors that prove this crate still owns the contract boundary:

- unknown or mismatched versions fail closed
- `BoundStatement` canonical bytes and binding digests remain stable
  for a given schema
- `ProofEnvelope` encode/decode stays stable and fail-closed
- compatibility policies reject invalid envelopes consistently
- statement and artifact-binding expectations remain explicit and complete

## Related Crates

- `tabula-core` provides the underlying shared vocabulary
- `tabula-compiler` seals contract metadata into `RegisteredProgram`
- `tabula-runtime` materializes and verifies `PublicStatement`
- `tabula-machine` owns the concrete proof bytes embedded inside `proof.bin`
