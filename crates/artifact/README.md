# tabula-artifact

`tabula-artifact` defines the sealed portable models that move across compiler,
runtime, storage, and adapter boundaries. It also owns the canonicalization
rules that make those models hashable and reproducible.

## Role

This crate exists to answer one question:

"What is the sealed, portable form of the data that moves between major layers?"

Concrete fields may evolve. The lasting boundary is that sealed models,
canonical serialization, and deterministic state/statement helpers live here.

## Owns

- sealed portable models for programs, state, batches, and execution statements
- canonical byte and digest helpers for artifact-bound data
- deterministic normalization and merge helpers for sealed state models
- JSON load/write helpers for non-wasm environments
- portable descriptors and plans that cross process or storage boundaries

## Does Not Own

- semantic interpretation of program inputs
- compatibility policy
- execution behavior
- runtime resource wiring
- backend proof construction

## Design Intent

- Keep cross-boundary data explicit, sealed, and reproducible.
- Keep canonicalization deterministic so hashes and statements mean the same
  thing across environments.
- Keep transport and storage models separate from the layers that interpret or
  execute them.

## Core Contract

- Canonical bytes and digests produced here are binding material, not advisory helpers.
- Normalization rules for sealed state and statement data must remain deterministic.
- Portable artifacts should be self-contained enough to cross process and
  storage boundaries without hidden runtime context.
- Consumer crates may validate or use sealed models, but they should not invent
  alternate canonical forms of the same data.

## Dependency Rules

- This crate may depend on `tabula-core`, `tabula-contract`, and IR-facing data models.
- It should not depend on compiler, runtime, or backend proof crates.
- If a model exists primarily to cross a boundary or be hashed canonically, it
  likely belongs here.

## How To Change This Crate Safely

- Treat shape changes to sealed models as compatibility changes, not local cleanup.
- Preserve deterministic canonicalization whenever adding fields or helpers.
- Avoid embedding runtime-only or adapter-only convenience semantics into the
  portable models themselves.
- Coordinate changes with compiler and runtime consumers before modifying
  statement or artifact-bound digests.

## Tests

Start with:

- `cargo test -p tabula-artifact`

Preserve the behaviors that prove this crate still owns the sealed boundary:

- canonical digests remain deterministic
- normalization and merge behavior stay predictable
- sealed models continue to round-trip cleanly across serialization boundaries

## Related Crates

- `tabula-core` provides the underlying shared types
- `tabula-contract` defines policy carried by artifact metadata
- `tabula-compiler` produces sealed artifacts that `tabula-runtime` and adapters consume
