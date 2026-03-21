# tabula-chips

`tabula-chips` contains the concrete AIR chip implementations used by the
Tabula proof backend. It is where abstract proving infrastructure becomes
specific constraint systems and chip behavior.

## Role

This crate exists to answer one question:

"What concrete chips implement the proof logic needed by the backend?"

The exact chip set may evolve. The lasting boundary is that concrete chip
implementations live here, while chip-independent infrastructure lives below
and proving orchestration lives above.

## Owns

- concrete AIR chip implementations
- built-in chip modules and reusable chip families
- concrete `DynChip`-level behavior used by backend setup
- default chip groupings used by higher proving layers
- chip-side test utilities that help validate concrete constraints

## Does Not Own

- chip-independent proving abstractions
- semantic registration or compatibility policy
- runtime extension wiring
- backend proof orchestration
- native commitment policy outside what concrete chips must mirror

## Design Intent

- Keep concrete chip logic out of the chip-independent foundation.
- Prefer explicit chip implementations and composition over implicit backend magic.
- Let higher layers decide how chips are assembled without moving chip logic upward.

## Core Contract

- This crate is where concrete chip behavior lives.
- Chip implementations here should satisfy the contracts defined by the lower
  proving foundation rather than redefining those contracts.
- Built-in chip bundles may evolve, but concrete constraints should remain
  localized here rather than spreading into `tabula-stark` or `tabula-machine`.
- When chips mirror native semantics from other layers, semantic drift must be
  treated as a cross-layer bug.

## Dependency Rules

- This crate may depend on `tabula-stark`, `tabula-core`, gadgets, and
  proof-adjacent semantic layers such as commitments.
- It should not depend on compiler or runtime policy crates.
- If a change is about concrete constraint implementation rather than shared
  proving infrastructure or caller policy, it likely belongs here.

## How To Change This Crate Safely

- Keep chip identities and composition explicit when adding or refactoring chips.
- Coordinate semantic changes with the native layers and backend consumers that
  rely on these constraints.
- Avoid moving orchestration policy into chip code just because a particular
  backend path currently consumes it.
- Prefer reusable chip modules over hidden special cases in machine setup.

## Tests

Start with:

- `cargo test -p tabula-chips --all-features`

Preserve the behaviors that prove this crate still owns the concrete chip layer:

- chip constraints remain internally consistent
- chip bundles remain usable by backend setup
- test utilities continue to support concrete chip validation without changing
  the production boundary

## Related Crates

- `tabula-stark` defines the chip-independent proving contracts used here
- `tabula-machine` assembles and proves with chips defined here
- `tabula-commitment` and other semantic layers provide meanings that some chips mirror
