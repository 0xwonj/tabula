# tabula-compiler

`tabula-compiler` is the semantic authority for Tabula programs. It is the
layer that turns authoring input into a sealed program description that the
rest of the system can consume without redoing semantic interpretation.

## Current Surfaces

- crate-root `tabula_compiler::compile_program_source` and
  `tabula_compiler::compile_program_source_with_catalogs` are the canonical
  compiler entry points for the rewritten V3 structured-control language.
- crate-root `CompiledProgram` is the pure compile result.
- crate-root `RegisteredProgram` is the native sealed result
  for execution and proving setup. It carries validated `tabula_ir`,
  ordered state-field scheme bindings, sealed table/profile materialization
  metadata, a native metadata envelope, and a native binding.
- crate-root `compile_and_register_program_source` and
  `register_compiled_program` expose the native sealing step.
- the rewritten native path is consumed from the crate root rather than
  through a public `next` namespace.

The compiler artifact also carries compiler-owned state-field scheme
bindings as sidecar metadata. Those bindings stay out of canonical IR on
purpose.

The native sealing path does not back-convert rewritten programs into a
legacy artifact or legacy IR just to reach runtime proving. Binding and
registration now come directly from sealed native bytes.

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
- the sealed native in-memory program representation used for later setup
- deterministic program binding inputs derived from sealed native payloads

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
- Rewritten-path proving inputs must be sealed here as native metadata.
  Downstream crates must not recreate legacy artifacts as a compatibility shim.

## Dependency Rules

- This crate may depend on authoring, IR, and contract crates.
- It should not depend on runtime, witness, or backend proof crates.
- If a new cross-cutting requirement is semantic in nature, prefer moving it
  into this layer instead of letting multiple downstream crates infer it.

## How To Change This Crate Safely

- When adding a new capability, requirement, or compatibility rule, make the
  compiler the source of truth first.
- When changing registered-program shape or metadata policy, update the
  contract layer together with downstream consumers.
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

- `tabula-lang` and `tabula-ir` provide authoring and canonical IR inputs
- `tabula-contract` defines compatibility policy
- `tabula-runtime` consumes the sealed native program as a setup input
