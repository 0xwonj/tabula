# tabula-machine

`tabula-machine` is the pure backend primitive for Tabula proving. It owns the
backend setup, proof generation, and proof verification once higher layers have
already decided what should be proved and prepared the necessary inputs.

## Role

This crate exists to answer one question:

"Given prepared backend inputs, how do we generate and verify Tabula proofs?"

The concrete proof decomposition may evolve. The lasting boundary is that this
crate owns backend proof assembly and verification, not semantic interpretation
or runtime policy.

## Canonical Surface

External callers reach the backend through two borrowed facades around a
configured [`TabulaMachine`]:

- `BackendProver::new(&machine).prove_envelope(input)` returns the decoded
  `TabulaProof` **together with** the contract-owned `ProofEnvelope` that wraps
  the canonical encoded proof bytes (proof system `TABULA_STARK`, proof
  encoding `TABULA_MACHINE_BINARY_V1`). The tuple shape is deliberate: the
  prover already holds the decoded proof after proving, and the runtime needs
  both the wire-format envelope (for persistence and transport) and the
  decoded form (for statement-level chip-opening introspection during
  verification). Returning the tuple avoids a decode round-trip on the hot
  path without widening the verifier's API surface.
- `BackendVerifier::new(&machine).verify_envelope(envelope, binding_digest)`
  decodes the envelope bytes, re-checks that the caller's expected
  `binding_digest` matches the digest encoded in the decoded proof
  (defense-in-depth against byte-level tampering that would otherwise only
  surface as a transcript failure), verifies the machine proof, and returns
  the decoded `TabulaProof` on success.
- `BackendVerifier::new(&machine).verify_proof(&proof, binding_digest)` is the
  short path for callers that already hold a decoded `TabulaProof` (for example,
  the runtime after statement-level chip-opening checks). It applies the same
  binding-digest check `verify_envelope` does — a single discipline, no
  caller-managed footgun.

The machine proof binds only a 32-byte `binding_digest` into its Fiat-Shamir
transcript — it does **not** carry the artifact-bound `PublicStatement` on the
wire. Callers thread the public statement beside the proof and let the runtime
or SDK layer own statement-first verification.

## Owns

- immutable backend setup and configuration
- backend trace construction from typed prepared inputs
- backend proof generation and verification
- the envelope-level prover/verifier facades (`BackendProver`,
  `BackendVerifier`) and the decoded proof object
- canonical encoding/decoding of the concrete machine proof bytes embedded in
  contract-owned `proof.bin`
- explicit backend extension seams
- validation that backend inputs are structurally acceptable for proving

## Does Not Own

- source parsing or semantic registration
- runtime registry policy or caller-facing integration policy
- deterministic execution
- discovery of what a program semantically requires
- native commitment semantics
- the public statement or the statement-first verification policy
  (those live in `tabula-contract` and `tabula-runtime`)

## Design Intent

- Keep the backend usable as a proof engine over prepared inputs rather than as
  a second policy or semantic layer.
- Prefer explicit extension seams and validated composition over hidden
  special-case wiring.
- Preserve the separation between deciding what should be proved and deciding
  how prepared inputs are proved.
- Force external callers through the envelope facade so the
  "one canonical backend boundary" invariant is enforced by the type system,
  not by convention: `TabulaMachine::prove`/`verify` are crate-internal.

## Core Contract

- This crate consumes prepared backend inputs; it is not a semantic authority.
- Higher-layer policy should arrive here as explicit prepared data, not as
  runtime registries or semantic catalogs.
- The stable handoff is typed prepared input (execution tier, ordered
  per-column stores, and root tier), not raw setup or trace internals.
- The public surface is envelope-level: proofs leave and re-enter the machine
  wrapped in a `ProofEnvelope`, never as naked bytes produced outside the
  crate.
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
  through the envelope facade
- structurally invalid backend inputs are rejected clearly
- extension registration remains explicit and validated
- the machine does not leak the public statement onto the proof wire or into
  the Fiat-Shamir transcript

## Related Crates

- `tabula-runtime` is the default policy layer above this crate
- `tabula-contract` owns the outer proof envelope and statement contract
- `tabula-witness` and `tabula-chips` help produce the inputs consumed here
- `tabula-stark` provides lower proving infrastructure
