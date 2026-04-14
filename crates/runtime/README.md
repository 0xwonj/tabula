# tabula-runtime

`tabula-runtime` is the low-level native orchestration surface for Tabula. It
sits between sealed program semantics and the lower execution/proof backends,
and it owns the policy for turning a registered program into concrete runtime
resources.

For normal application embedding, the default product-facing surface is
`tabula-sdk`. `tabula-runtime` remains the expert-oriented runtime layer below
that SDK boundary.

The current path is explicitly split into:

- `tabula_runtime::semantics` for semantic execution-journal reduction and
  public-statement materialization
- crate-root `tabula_runtime::{RuntimeBuilder, TabulaRuntime, Verifier,
  CommittedStateSnapshot, PublicStatement, BoundStatement, ...}` for
  native runtime setup, execution, proving, and verification orchestration

## Role

This crate exists to answer two questions:

- "How should a sealed program be executed?"
- "How should its proved public statement be materialized and checked?"

The exact API split may evolve. The lasting boundary is that this crate is the
policy-and-orchestration layer above execution and backend proving.

## Owns

- the default caller-facing execution and proof orchestration surface
- policy for turning a sealed program into runtime resources
- runtime registries and scheme backends at the runtime boundary
- materialization of `PublicStatement` from execution truth
- verification that a proof certifies an expected `PublicStatement`
- preparation of backend-ready inputs from already registered semantics
- native runtime/proving setup from `tabula_compiler::RegisteredProgram`
- recomputation of artifact-derived verifier invariants from the sealed program

## Does Not Own

- source parsing or semantic registration
- low-level execution semantics implemented by the executor
- native commitment semantics
- backend proof implementation details once inputs are prepared
- the canonical `proof.bin` outer schema
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
- Statement checks and artifact-derived binding checks that connect sealed
  program expectations to proof verification belong here, not in the machine
  layer.
- The secure verification surface is statement-first:
  `verify_public_statement(proof, expected_public_statement)`.
- `verify_proof(proof)` is only a convenience wrapper around the proof's own
  carried `PublicStatement` and the configured sealed artifact.
- `PublicStatement` is the proved object.
- `BoundStatement` is the verifier-side outer binding over artifact
  invariants plus the proved public statement.
- AIR public values remain fixed-size:
  `old_root`, `new_root`, `public_context_digest`, `applied_tx_digest`,
  `event_digest`.
- Query execution is supported on the rewritten path, but query proving remains
  intentionally absent.

## Dependency Rules

- This crate may depend on compiler outputs and the executor.
- It may assemble lower proof-backend crates, but it should remain a consumer
  of semantic facts rather than a second semantic authority.
- If a change is primarily about caller policy, resource wiring,
  public-statement materialization, or verifier-side artifact binding, it
  likely belongs here.

## How To Change This Crate Safely

- Keep policy here and backend mechanics below. Avoid letting the machine layer
  absorb runtime registry logic.
- Keep semantic authority above. Avoid letting runtime rediscover or repair
  semantic facts that should already be sealed by the compiler.
- If APIs change, preserve the conceptual split between execution-only use,
  statement-first verification, and long-lived runtime setup.
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
- public-statement materialization remains execution-derived, while
  artifact-derived verifier context is recomputed from the sealed program

## Related Crates

- `tabula-compiler` produces the sealed inputs consumed here
- `tabula-contract` owns the public-statement and outer-binding contracts
- `tabula-executor` performs deterministic execution
- `tabula-witness` prepares proof-oriented logical inputs
- `tabula-machine` performs backend proving and verification
