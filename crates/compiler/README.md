# tabula-compiler

`tabula-compiler` is the semantic authority for Tabula programs. It is the
layer that turns authoring input into a sealed program description that the
rest of the system can consume without redoing semantic interpretation.

## Role

This crate exists to answer one question:

"What program semantics should the rest of the stack trust?"

Exact input formats and helper names may evolve. The lasting boundary is that
semantic registration, compatibility metadata, and proof-relevant semantic
requirements are derived here rather than rediscovered later.

## Owns

- semantic registration of program inputs
- fail-closed compatibility and binding metadata attached to registered programs
- derivation of semantic requirements that downstream layers must honor
- the sealed in-memory program representation used for later setup
- conversion between authoring inputs and portable sealed artifacts

## Does Not Own

- batch execution
- runtime policy or resource assembly
- witness preparation or proving
- backend proof construction
- native commitment semantics

## Design Intent

- Derive semantic meaning once, then carry it downstream explicitly instead of
  letting later layers rediscover it.
- Favor fail-closed behavior over permissive compatibility. Ambiguous semantics
  should be rejected here, not tolerated elsewhere.
- Keep authoring and semantic evolution isolated from runtime and backend
  changes as much as possible.

## Core Contract

- Semantic validation must happen here or below this layer, not later in
  runtime or backend code.
- Compatibility mismatches must fail closed.
- If execution, verification, or proving depends on a semantic fact, that fact
  should be derived here and carried downstream explicitly.
- Downstream crates may consume compiler outputs, but they should not repair or
  reinterpret their semantics.

## Dependency Rules

- This crate may depend on authoring, IR, contract, and artifact crates.
- It should not depend on runtime, witness, or backend proof crates.
- If a new cross-cutting requirement is semantic in nature, prefer moving it
  into this layer instead of letting multiple downstream crates infer it.

## How To Change This Crate Safely

- When adding a new capability, requirement, or compatibility rule, make the
  compiler the source of truth first.
- When changing sealed artifact shape or metadata policy, update the contract
  and artifact layers together with downstream consumers.
- Keep the boundary fail-closed even during refactors. Temporary leniency here
  tends to become permanent semantic ambiguity downstream.
- Resist pushing semantic checks into runtime just because runtime happens to
  be the next consumer.

## Tests

Start with:

- `cargo test -p tabula-compiler`

Preserve the behaviors that prove this crate remains the semantic authority:

- invalid programs are rejected during registration
- compatibility mismatches are rejected rather than patched up downstream
- derived semantic requirements remain deterministic

## Related Crates

- `tabula-lang` and `tabula-ir` provide authoring and IR inputs
- `tabula-contract` defines compatibility policy
- `tabula-artifact` defines sealed portable outputs
- `tabula-runtime` consumes the sealed program as a setup input
