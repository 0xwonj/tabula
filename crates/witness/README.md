# tabula-witness

`tabula-witness` is the logical proof-input preparation seam for Tabula. It
turns execution results and declared proof-relevant inputs into deterministic,
proof-oriented data structures without taking ownership of runtime policy or
backend proof assembly.

## Role

This crate exists to answer one question:

"What logical proof inputs should lower proof layers receive?"

The exact witness format and backend lowering may evolve. The lasting boundary
is that this crate prepares proof-oriented logical inputs and keeps that seam
separate from both runtime policy and backend proving.

## Owns

- logical preparation of proof inputs from execution-derived data
- typed per-column or per-unit prepared input structures
- completeness and consistency checks for prepared proof inputs
- a stable root-level preparation seam even if backend-specific lowerings move

## Does Not Own

- semantic registration or compatibility policy
- runtime capability wiring
- deterministic execution
- backend proof orchestration or final proof assembly
- authoring-language concerns

## Design Intent

- Keep the root seam small, logical, and stable even if backend-specific
  lowering changes frequently.
- Prefer deterministic, validated prepared inputs over convenience shortcuts.
- Prevent runtime policy and backend proof assembly concerns from collapsing
  into the witness layer.

## Core Contract

- The root API should stay about logical preparation, not backend policy.
- Prepared inputs should be deterministic, typed, and validated before lower
  proof layers consume them.
- This crate should make lower proof layers simpler, not become a second
  runtime or semantic-registration layer.
- Backend-specific lowerings may change without invalidating the crate's
  high-level role.

## Dependency Rules

- This crate sits after execution and before backend proof assembly.
- It may depend on lower proof crates for specialized lowerings, but the crate
  boundary should remain preparation-focused.
- If a change is really about runtime policy, semantic meaning, or backend
  proving, it likely belongs in another layer.

## How To Change This Crate Safely

- Preserve the root preparation seam even if backend internals churn.
- Keep deterministic ordering and completeness checks strong; those are the
  easiest guarantees to weaken accidentally.
- Prefer pushing backend-specific complexity downward instead of widening the
  root API every time a backend detail changes.
- Do not let this crate become the place where runtime policy leaks in.

## Tests

Start with:

- `cargo test -p tabula-witness`

Preserve the behaviors that prove this crate still owns the preparation seam:

- invalid or incomplete prepared inputs are rejected early
- prepared inputs remain deterministic
- runtime-facing preparation flows still round-trip into lower proof consumers

## Related Crates

- `tabula-runtime` is the main caller above this layer
- `tabula-commitment` defines native commitment semantics used below
- `tabula-chips` and `tabula-machine` consume the proof-oriented outputs
