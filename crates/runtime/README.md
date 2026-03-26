# tabula-runtime

`tabula-runtime` is the low-level native orchestration surface for Tabula. It
sits between sealed program semantics and the lower execution/proof backends,
and it owns the policy for turning a registered program into concrete runtime
resources.

For normal application embedding, the default product-facing surface is now
`tabula-sdk`. `tabula-runtime` remains the expert-oriented runtime layer below
that SDK boundary.

The rewritten path is now explicitly split into:

- `tabula_runtime::semantics` for semantic execution-journal reduction and
  semantic public-statement construction
- crate-root `tabula_runtime::{RuntimeBuilder, TabulaRuntime, Verifier,
  StateSnapshot, ProofStatement, ...}` for native runtime setup,
  execution, proving, and verification orchestration

## Role

This crate exists to answer two questions:

- "How should a sealed program be executed?"
- "How should its proof-related requirements be materialized and checked?"

The exact API split may evolve. The lasting boundary is that this crate is the
policy-and-orchestration layer above execution and backend proving.

## Owns

- the default caller-facing execution and proof orchestration surface
- policy for turning a sealed program into runtime resources
- runtime registries and scheme backends at the runtime boundary, while
  capability execution/proof contracts are consumed from sealed compiler data
- binding between sealed program expectations and backend verification inputs
- preparation of backend-ready inputs from already registered semantics
- native runtime/proving setup from
  `tabula_compiler::RegisteredProgram`
- native proof statements whose digest is bound into the machine
  transcript

## Does Not Own

- source parsing or semantic registration
- low-level execution semantics implemented by the executor
- native commitment semantics
- backend proof implementation details once inputs are prepared
- authoring-language concerns

## Design Intent

- Keep one clear integration boundary for applications even if the internal
  execution and proving flows evolve.
- Keep policy and resource wiring here so lower backend layers can stay focused
  on prepared inputs and proof mechanics.
- Preserve a clean separation between semantic authority above and backend
  implementation below.

## Core Contract

- Runtime is where sealed semantics become concrete execution and proof policy.
- Lower backend crates should receive prepared inputs, not ownership of runtime
  registry policy.
- Statement or binding checks that connect sealed program expectations to proof
  verification belong here, not in the machine layer.
- Convenience surfaces may evolve, but this crate should remain the default
  integration boundary for applications.
- AIR public values stay minimal: only old/new state roots belong in the AIR
  statement. Richer native semantic proof meaning belongs in the runtime
  `ProofStatement` digest bound through the transcript.
- Query execution is supported on the rewritten path, but query proving remains
  intentionally absent.

## Dependency Rules

- This crate may depend on compiler outputs and the executor.
- It may assemble lower proof-backend crates, but it should remain a consumer
  of semantic facts rather than a second semantic authority.
- If a change is primarily about caller policy, resource wiring, statement
  binding, or extension registration, it likely belongs here.

## How To Change This Crate Safely

- Keep policy here and backend mechanics below. Avoid letting the machine layer
  absorb runtime registry logic.
- Keep semantic authority above. Avoid letting runtime rediscover or repair
  semantic facts that should already be sealed by the compiler.
- If APIs change, preserve the conceptual split between execution-only use,
  verification against a sealed binding, and long-lived runtime setup.
- When in doubt, optimize for one clear integration boundary rather than many
  partially overlapping entry points.

## Tests

Start with:

- `cargo test -p tabula-runtime --all-features`

Preserve the behaviors that prove this crate still owns the runtime boundary:

- sealed-program requirements are checked before backend proving
- binding mismatches are rejected at the runtime/verifier boundary
- default integration paths continue to cover execution and proof workflows
- native proving stays legacy-bridge free
- semantic public statements remain public-context plus event-digest only

## Related Crates

- `tabula-compiler` produces the sealed inputs consumed here
- `tabula-executor` performs deterministic execution
- `tabula-witness` prepares proof-oriented logical inputs
- `tabula-machine` performs backend proving and verification
