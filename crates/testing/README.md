# tabula-testing

`tabula-testing` is the shared black-box testing support crate for the
workspace. It centralizes fixtures, assertions, and harnesses that are meant to
be reused across crates without becoming part of the production architecture.

## Role

This crate exists to answer one question:

"What shared testing support should multiple workspace crates rely on?"

The exact fixtures and helpers may evolve. The lasting boundary is that this
crate owns shared test support, not production behavior.

## Owns

- shared black-box fixtures for programs, artifacts, state, batches, and cases
- reusable assertions over semantic behavior and public results
- test harnesses built on public seams of compiler, executor, runtime, and proof crates
- common testing support that reduces duplication across workspace crates

## Does Not Own

- production execution or proving behavior
- crate-local white-box test seams
- runtime or backend policy
- semantic authority for any production layer
- adapter-specific testing code that only one crate needs

## Design Intent

- Keep shared tests scenario-oriented and black-box where possible.
- Reuse public seams instead of privileged internal hooks whenever practical.
- Centralize only the testing support that genuinely benefits multiple crates.

## Core Contract

- This crate is support infrastructure only; it must not become a hidden production dependency.
- Shared fixtures and assertions should remain reusable across multiple consumers.
- Crate-local white-box testing helpers should stay in the owning crate rather
  than being promoted here too early.
- Test harnesses here should validate public behavior, not encode internal
  implementation details as if they were contracts.

## Dependency Rules

- This crate may depend broadly on workspace crates because it is test-only support.
- It should remain `publish = false` and non-production in spirit as well as configuration.
- If a helper is specific to one crate's private internals, it likely does not belong here.

## How To Change This Crate Safely

- Add shared fixtures or assertions only when at least two consumers benefit or
  the helper clearly belongs to a cross-workspace testing story.
- Prefer building helpers on public seams so tests keep validating real contracts.
- Avoid turning convenience harnesses into shadow implementations of production logic.
- Remove stale helpers aggressively if they stop reflecting real cross-crate testing needs.

## Tests

Start with:

- `cargo test -p tabula-testing`

Preserve the behaviors that prove this crate still owns the shared testing boundary:

- fixtures and assertions remain reusable across crates
- harnesses continue to exercise public production seams
- test support does not become a source of hidden production coupling

## Related Crates

- `tabula-compiler`, `tabula-executor`, `tabula-runtime`, and proof crates all consume helpers from this crate in tests
- crate-local tests should still keep private white-box helpers in their owning crates
