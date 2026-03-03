# tabula-core

Core types and trait definitions for the Tabula kernel.

## Role

Defines the shared vocabulary that every other crate depends on:
value types (`Value`, `CellKey`, `TableId`), execution events, errors,
and pluggable trait abstractions (`Hasher`, `StateSnapshot`, `SigVerifier`, etc.).

## Key Design

**Zero crypto dependencies.** All cryptographic operations are behind traits.
The crate depends only on `borsh`, `serde`, and `thiserror`. Different
deployments inject different trait implementations (Blake3 for testing,
Poseidon for proving).

**Null is not a value.** Cell absence is `Option<Value>`, not a variant of
`Value`. The two-slot IR pattern (`Read` produces `dst_val` + `dst_is_null`;
`Write` takes `src_val` + `src_is_null`) flows from this decision.

**`Value` is application-level only.** The four variants (U64, I64, Bool,
Bytes32) represent what developers work with. Field-element encoding for
the proof system is handled separately by the `ValueCodec` trait.

## Feature Flags

| Feature | Effect |
|---------|--------|
| `mock`  | Blake3-based test implementations of all traits |
