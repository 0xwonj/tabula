# tabula-machine

`tabula-machine` is the advanced backend API for Tabula proving. It owns the
backend setup, proof generation, and proof verification once higher layers have
already decided what should be proved and prepared the necessary inputs.

## Role

This crate exists to answer one question:

"Given prepared backend inputs, how do we generate and verify Tabula proofs?"

The concrete proof decomposition may evolve. The lasting boundary is that this
crate owns backend proof assembly and verification, not semantic interpretation
or runtime policy.

## Owns

- immutable backend setup and configuration
- backend trace construction from typed prepared inputs
- backend proof generation and verification
- the proof object and related backend proof types
- explicit backend extension seams
- validation that backend inputs are structurally acceptable for proving

## Does Not Own

- source parsing or semantic registration
- runtime registry policy or caller-facing integration policy
- deterministic execution
- discovery of what a program semantically requires
- native commitment semantics

## Design Intent

- Keep the backend usable as a proof engine over prepared inputs rather than as
  a second policy or semantic layer.
- Prefer explicit extension seams and validated composition over hidden
  special-case wiring.
- Preserve the separation between deciding what should be proved and deciding
  how prepared inputs are proved.

## Core Contract

- This crate consumes prepared backend inputs; it is not a semantic authority.
- Higher-layer policy should arrive here as explicit prepared data, not as
  runtime registries or semantic catalogs.
- The stable handoff is typed prepared input (execution tier, ordered
  per-column stores, and root tier), not raw setup or trace internals.
- Proof shape may evolve, but the ownership boundary should stay: backend proof
  assembly lives here, while semantic and runtime policy lives above.
- Extension points should remain explicit and mechanically validated.

## Dependency Rules

- This crate may depend on lower proving infrastructure and proof-related crates.
- It should stay ignorant of authoring-language details and compiler policy.
- If a change is really about what should be proved, rather than how prepared
  inputs are proved, it likely belongs in a higher layer.

## How To Change This Crate Safely

- Keep semantic and runtime policy out of the backend builder.
- Prefer explicit extension seams over special cases wired into the machine.
- Treat changes to proof structure or transcript binding as cross-layer changes
  that must be coordinated with upper callers.
- Keep the machine layer usable as a backend API even if the default runtime
  integration path changes.

## Tests

Start with:

- `cargo test -p tabula-machine`

Preserve the behaviors that prove this crate still owns the backend boundary:

- prepared inputs can be turned into traces, proofs, and verification checks
- structurally invalid backend inputs are rejected clearly
- extension registration remains explicit and validated

## Related Crates

- `tabula-runtime` is the default policy layer above this crate
- `tabula-witness` and `tabula-chips` help produce the inputs consumed here
- `tabula-stark` provides lower proving infrastructure
