# tabula-commitment

`tabula-commitment` owns the native commitment semantics used by the proof
stack. It is the source of truth for how committed state and state-binding data
are computed outside the proving backend.

## Role

This crate exists to answer one question:

"What is the native commitment result that the proof system must attest to?"

Concrete schemes and cryptographic choices may evolve. The lasting boundary is
that this crate defines the native commitment semantics, while proof-side crates
mirror and verify those semantics.

## Owns

- native commitment primitives and digest types
- native state-binding and commitment-composition rules
- verifier-visible commitment metadata
- scheme-specific native commitment implementations
- root or summary computations that bind committed state for downstream checks

## Does Not Own

- execution semantics
- semantic registration or runtime policy
- witness orchestration
- backend proof assembly or verification
- authoring-language concerns

## Design Intent

- Keep the native commitment meaning in one place even if concrete algorithms
  and schemes evolve over time.
- Separate "what the committed result is" from "how that result is proved."
- Make downstream proof code mirror this crate's semantics rather than invent
  parallel definitions of them.

## Core Contract

- This crate is the native source of truth for commitment semantics.
- Proof-side crates should mirror these semantics, not redefine them.
- Commitment metadata exposed from this crate is binding material, not a loose
  cache that downstream code may reinterpret.
- Changes here are cross-stack design changes, not local implementation detail.

## Dependency Rules

- This crate may depend on core types and native cryptographic primitives.
- It should not depend on runtime, compiler, or execution-policy crates.
- If a rule answers "what is the committed native result?", it belongs here
  before it belongs in witness, chips, or machine code.

## How To Change This Crate Safely

- Treat changes to commitment semantics as full-stack changes. Update proof-side
  consumers and verifiers together.
- Prefer describing durable semantic intent over freezing one concrete scheme
  forever. The algorithms may change; the ownership boundary should not.
- Do not let downstream crates quietly compensate for semantic drift here.
- Keep this crate focused on native semantics rather than proof orchestration.

## Tests

Start with:

- `cargo test -p tabula-commitment --all-features`

Preserve the behaviors that prove this crate remains the native authority:

- native commitment computations are deterministic
- invalid commitment inputs are rejected clearly
- exported binding values remain consistent with proof-side expectations

## Related Crates

- `tabula-witness` prepares proof inputs around the native semantics defined here
- `tabula-chips` constrains those semantics in-circuit
- `tabula-machine` proves and verifies the resulting commitments
