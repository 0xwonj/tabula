# tabula-executor

`tabula-executor` is the deterministic execution engine for validated Tabula
programs. It interprets registered program bodies against state and batches
without taking ownership of higher-level runtime or proof policy.

## Role

This crate exists to answer one question:

"Given a validated program, state, and batch, what execution result should occur?"

Specific environment wiring may evolve. The lasting boundary is that this crate
owns deterministic execution mechanics and execution-side state transition
behavior.

## Owns

- transaction and batch execution over validated IR programs
- deterministic execution outcomes, read/write sets, and access events
- overlay-based execution state management
- capability and property-query execution hooks at the engine boundary
- consistency checks over execution results

## Does Not Own

- source parsing or semantic registration
- runtime integration policy
- compatibility metadata
- proof preparation and proving
- native commitment semantics

## Design Intent

- Keep execution deterministic and policy-light.
- Keep execution mechanics separate from runtime orchestration and proof concerns.
- Make state effects and failure outcomes explicit rather than implicit.

## Core Contract

- Given the same validated inputs and environment, execution should remain deterministic.
- This crate executes validated programs; it should not become a second semantic-registration layer.
- Execution behavior and state-effect recording belong here even when higher
  layers use those results for proving.
- Runtime may wire the environment, but the engine should remain reusable as a
  standalone deterministic execution layer.

## Dependency Rules

- This crate may depend on `tabula-core` and `tabula-ir`.
- It should not depend on compiler policy, runtime policy, or backend proof crates.
- If a change is about execution semantics or state-effect recording rather than
  orchestration or proving, it likely belongs here.

## How To Change This Crate Safely

- Preserve determinism whenever adding new environment hooks or instruction support.
- Keep execution-side rollback and failure handling explicit rather than
  scattering them across callers.
- Avoid moving runtime policy into the executor just because runtime is a
  prominent caller.
- Coordinate changes with proof consumers when execution events or result shapes
  change, but keep ownership of execution semantics here.

## Tests

Start with:

- `cargo test -p tabula-executor`

Preserve the behaviors that prove this crate still owns the execution boundary:

- execution remains deterministic for the same inputs
- state effects and access events stay coherent
- failure handling does not depend on caller-specific orchestration

## Related Crates

- `tabula-ir` provides the validated operational input consumed here
- `tabula-runtime` is the default policy and orchestration layer above this crate
- `tabula-testing` provides shared execution fixtures and harnesses
