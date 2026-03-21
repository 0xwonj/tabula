# tabula-stark

`tabula-stark` is the chip-independent proving foundation for the Tabula proof
backend. It defines the abstractions, trace machinery, and proving interfaces
that concrete chips and backend assembly build on top of.

## Role

This crate exists to answer one question:

"What common proving foundation should every concrete chip and backend setup use?"

Concrete chips, proof layouts, and higher-level policies may evolve. The
lasting boundary is that shared proving infrastructure lives here and remains
independent of any one chip set.

## Owns

- AIR and RAP foundation abstractions
- chip identification and chip capability contracts
- trace, permutation, and proving support infrastructure
- debug and validation helpers for lower proving mechanics
- shared proof-side types used across concrete chip implementations

## Does Not Own

- concrete chip implementations
- semantic registration
- runtime integration policy
- backend orchestration policy specific to one machine
- transport or artifact canonicalization

## Design Intent

- Keep the proving foundation reusable across many concrete chip sets.
- Use explicit traits and identifiers so downstream crates can extend the proof
  system without modifying the foundation every time.
- Prevent application-specific semantics from leaking into the shared proving substrate.

## Core Contract

- This crate should remain chip-independent.
- Concrete chip crates should implement the interfaces defined here rather than
  rebuilding local proving frameworks.
- Extension seams such as chip identifiers and trait contracts are part of the
  foundation boundary and should stay explicit.
- Algorithmic improvements are welcome as long as they preserve the
  chip-independent role of the crate.

## Dependency Rules

- This crate may depend on core types, commitment-adjacent primitives, and low-level proving libraries.
- It should not depend on concrete chip crates, compiler code, or runtime policy.
- If a change is about generic proving mechanics rather than one specific chip
  family or backend integration path, it likely belongs here.

## How To Change This Crate Safely

- Keep concrete chip logic and application semantics out of the foundation.
- Preserve extension-oriented interfaces when refactoring lower proving machinery.
- Avoid coupling the crate to one machine assembly strategy or one caller path.
- Treat changes to shared chip contracts as downstream coordination points, not
  isolated refactors.

## Tests

Start with:

- `cargo test -p tabula-stark`

Preserve the behaviors that prove this crate still owns the proving foundation:

- generic proving utilities remain usable by downstream chip crates
- chip-identification and trace contracts remain coherent
- debug and validation helpers continue to support backend consumers

## Related Crates

- `tabula-chips` implements concrete chips on top of this foundation
- `tabula-machine` assembles backend proofs using the machinery defined here
- `tabula-core` provides shared domain types consumed by proof infrastructure
