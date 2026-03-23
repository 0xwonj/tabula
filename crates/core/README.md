# tabula-core

`tabula-core` is the shared kernel vocabulary for the workspace. It defines the
basic types, traits, and error model that other crates use to talk about the
same state, values, transactions, and execution outcomes.

## Role

This crate exists to answer one question:

"What are the shared concepts that the rest of the system should mean in the
same way?"

Concrete execution, proving, transport, and compatibility layers may evolve.
The lasting boundary is that shared domain vocabulary and pluggable low-level
traits live here.

## Owns

- shared identifiers, schemas, portable boundary values, and transaction model types
- execution result and event vocabulary
- core error types used across the workspace
- low-level traits for hashing, codecs, state access, signatures, and nonce policy
- small default implementations that satisfy those traits without defining higher-level policy

## Does Not Own

- transport or canonical artifact formats
- compatibility and contract versioning policy
- semantic registration
- runtime orchestration
- proof construction or commitment semantics

## Design Intent

- Keep the shared vocabulary small, stable, and dependency-light.
- Keep cryptographic and policy choices behind traits rather than hardcoding
  them into the kernel.
- Keep absence separate from value. Nullability should not silently collapse
  into the value domain.

## Core Contract

- Types defined here are shared meaning, not convenience wrappers local to one crate.
- If a concept must mean the same thing across compiler, runtime, and proof
  layers, it likely belongs here.
- Low-level traits should stay generic and reusable rather than encoding
  runtime or proving policy.
- Changes to core value or event semantics are cross-workspace design changes.

## Dependency Rules

- This crate should stay close to the bottom of the dependency graph.
- It may depend on lightweight serialization and error libraries, plus minimal
  optional test-only helpers.
- It should not depend on compiler, runtime, artifact, or proof-backend crates.

## How To Change This Crate Safely

- Add new shared concepts here only when multiple layers genuinely need the
  same meaning.
- Prefer traits over concrete policy when introducing new low-level capabilities.
- Treat changes to `PortableValue`, state keys, or execution events as
  cross-stack changes that require coordinated updates.
- Avoid turning this crate into a dumping ground for utilities that are not
  truly part of the kernel vocabulary.

## Tests

Start with:

- `cargo test -p tabula-core`

Preserve the behaviors that prove this crate still owns the kernel boundary:

- shared types remain deterministic and serializable
- trait-based abstractions remain usable without higher-level policy crates
- portable value and absence semantics remain unambiguous

## Related Crates

- `tabula-contract` builds policy on top of core concepts
- `tabula-artifact` serializes core-aligned data models across boundaries
- `tabula-compiler`, `tabula-runtime`, and the proof crates all consume this shared vocabulary
