# tabula-ir

`tabula-ir` defines the structured intermediate representation consumed by the
execution and proving stack. It is the layer where authoring input stops being
surface syntax and becomes a validated operational form.

## Role

This crate exists to answer one question:

"What normalized program representation should execution-oriented layers consume?"

Exact instruction sets and validation passes may evolve. The lasting boundary
is that this crate owns the IR shape and the structural validation required for
other layers to rely on it.

## Owns

- instruction, expression, and transaction-definition IR types
- the registered `Program` representation
- structural typing and validation passes over IR bodies
- IR-level capability references that downstream layers inspect
- normalized program form suitable for deterministic execution

## Does Not Own

- source syntax or parsing
- sealed artifact policy
- runtime orchestration
- execution mechanics
- backend proof construction

## Design Intent

- Keep the IR explicit and validation-first so execution layers do not need to
  interpret ambiguous authoring constructs.
- Keep operational structure separate from surface-language concerns.
- Make proof-relevant program structure visible in the IR rather than hidden in
  parser or runtime behavior.

## Core Contract

- Programs consumed downstream should already be structurally validated here.
- The IR is an operational boundary, not a convenience mirror of source syntax.
- If execution or proving depends on a program property that can be known at
  IR registration time, that property should be made explicit here.
- Downstream layers may consume IR capability references, but they should not
  redefine what the IR means.

## Dependency Rules

- This crate may depend on `tabula-core`.
- It should not depend on language, compiler policy, runtime, or backend proof crates.
- If a change is about normalized program structure rather than source syntax
  or runtime policy, it likely belongs here.

## How To Change This Crate Safely

- Treat IR shape changes as cross-layer changes affecting compiler, executor,
  and proof-side consumers.
- Keep validation close to registration rather than pushing structural checks
  into execution or runtime layers.
- Avoid letting source-language conveniences leak directly into the IR unless
  they belong in the operational model.
- Preserve the separation between "authoring a program" and "executing a
  validated program."

## Tests

Start with:

- `cargo test -p tabula-ir`

Preserve the behaviors that prove this crate still owns the IR boundary:

- structurally invalid programs are rejected during IR validation
- registered programs expose deterministic operational structure
- downstream-relevant capability references remain explicit

## Related Crates

- `tabula-lang` produces inputs that lower into this IR
- `tabula-compiler` consumes IR to produce sealed semantic artifacts
- `tabula-executor` consumes registered IR for deterministic execution
