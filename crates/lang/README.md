# tabula-lang

`tabula-lang` is the authoring front-end for Tabula programs. It turns source
text into a typed, lowered program form that can be handed to the IR and
compiler layers without carrying parser concerns further down the stack.

## Role

This crate exists to answer one question:

"How should human-authored Tabula programs be parsed and lowered into the
structured program model?"

Concrete syntax and lowering details may evolve. The lasting boundary is that
authoring-language concerns live here, not in IR, runtime, or proof code.

## Owns

- source-language lexing and parsing
- authoring ASTs and source-span-aware diagnostics
- lowering from surface syntax into the structured program model
- source-level name resolution and early type-directed feedback
- authoring-oriented compile errors

## Does Not Own

- IR semantics after lowering
- sealed artifact policy
- runtime execution behavior
- compatibility or binding policy
- proof construction

## Design Intent

- Keep human-facing syntax and diagnostics isolated from lower operational layers.
- Reject ambiguity and authoring mistakes as early as possible.
- Let the language evolve without forcing IR or runtime layers to absorb
  surface-language complexity.

## Core Contract

- This crate is the front-end for authoring, not the semantic source of truth
  after lowering.
- Source-facing diagnostics should be produced here rather than reconstructed
  from lower layers.
- Lowered output should be structured enough that later layers can stop caring
  about parser details.
- Language evolution may change syntax, but the boundary between authoring and
  operational layers should remain.

## Dependency Rules

- This crate may depend on `tabula-core` and `tabula-ir`.
- It should not depend on compiler policy, runtime, or proof-backend crates.
- If a change is about source syntax, diagnostics, or lowering behavior rather
  than IR meaning, it likely belongs here.

## How To Change This Crate Safely

- Preserve clear authoring diagnostics when changing syntax or lowering.
- Keep source-level conveniences from leaking directly into runtime-facing semantics.
- Coordinate lowering changes with IR consumers, but avoid moving front-end
  logic into lower layers.
- Treat parser refactors as successful only if the authoring boundary stays
  easy to reason about.

## Tests

Start with:

- `cargo test -p tabula-lang`

Preserve the behaviors that prove this crate still owns the authoring boundary:

- valid source lowers deterministically
- invalid source is rejected with source-oriented diagnostics
- lower layers do not need to recover lost authoring information to proceed

## Related Crates

- `tabula-ir` defines the structured program model lowered into by this crate
- `tabula-compiler` consumes lowered programs as part of semantic registration
- `tabula-testing` provides shared source-level fixtures and helpers
