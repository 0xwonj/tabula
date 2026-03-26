# tabula-contract

`tabula-contract` is the fail-closed compatibility, binding, and proof-visible
format layer for Tabula. It defines the versioned metadata, shared artifact
schemas, and canonical encoding rules that let separated compiler, runtime,
witness, chip, and verification paths agree on what they are allowed to trust.

## Role

This crate exists to answer one question:

"When should two sides of the system consider a sealed program and proof setup
compatible?"

Specific versions and fields will evolve. The lasting boundary is that
compatibility policy, binding metadata, sealed artifact schemas, proof-visible
canonical encodings, and fail-closed validation live here.

## Owns

- versioned contract metadata carried with sealed artifacts
- compatibility policy applied at proof and verification entry points
- binding registry and public-input field policy
- proof-visible artifact schemas shared across compiler/runtime/verifier paths
- canonical digest and encoding rules that multiple layers must match bit-for-bit
- fail-closed validation for known and unknown compatibility versions
- contract-level rules that multiple layers must interpret the same way

## Does Not Own

- program semantics
- artifact storage or canonical serialization
- execution behavior
- runtime orchestration
- semantic registration or IR traversal
- witness aggregation policy
- backend proof implementation

## Design Intent

- Prefer explicit versioned contracts over implicit compatibility.
- Fail closed when the system encounters unknown or mismatched compatibility data.
- Keep binding and compatibility rules reusable across compiler, runtime, and verifier paths.

## Core Contract

- Unknown compatibility versions are incompatible by default.
- Compatibility checks should reject mismatches rather than auto-repairing or
  silently downgrading them.
- Binding metadata defined here is shared policy, not local implementation detail.
- Changes here are cross-layer contract changes and should be treated as such.

## Dependency Rules

- This crate may build shared contract rules on top of `tabula-core`,
  `tabula-types`, `tabula-ir`, and native commitment primitives when those rules
  define canonical proof-visible encodings.
- It should not depend on compiler, runtime, or backend proof crates.
- If a rule is about whether two sides should trust the same sealed contract,
  it belongs here before it belongs in consumers.

## How To Change This Crate Safely

- Introduce compatibility changes deliberately, with coordinated updates across
  compiler, runtime, and verifier consumers.
- Resist adding permissive fallback behavior for version mismatches.
- Keep the policy layer small and explicit instead of spreading contract rules
  across multiple consuming crates.
- Treat binding-field changes as contract changes, not ordinary refactors.

## Tests

Start with:

- `cargo test -p tabula-contract`

Preserve the behaviors that prove this crate still owns the contract boundary:

- unknown or mismatched versions fail closed
- compatibility policies reject invalid envelopes consistently
- binding expectations remain explicit and complete

## Related Crates

- `tabula-core` provides the underlying shared vocabulary
- `tabula-compiler` seals contract metadata into `RegisteredProgram`
- `tabula-runtime` and verifiers enforce that sealed contract at execution and proof boundaries
