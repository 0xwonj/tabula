# tabula-gadgets

`tabula-gadgets` contains reusable constraint-building components for the proof
stack. It packages lower-level constraint logic so concrete chips can compose
shared behavior without duplicating it.

## Role

This crate exists to answer one question:

"What reusable constraint fragments should concrete proof chips share?"

Specific gadgets may evolve. The lasting boundary is that reusable constraint
logic lives here, while full chip protocols and backend orchestration live elsewhere.

## Owns

- reusable primitive constraint gadgets
- composite constraint helpers built from those primitives
- gadget-level witness-population and evaluation helpers where appropriate
- shared proof-side building blocks used by multiple chips

## Does Not Own

- full chip implementations
- chip-independent proving foundation
- semantic registration
- runtime orchestration
- backend proof assembly

## Design Intent

- Keep common constraint logic reusable instead of duplicating it across chips.
- Keep gadgets smaller in scope than full chip protocols.
- Let concrete chips compose gadgets without pushing gadget policy upward into runtime or machine code.

## Core Contract

- Gadgets should remain reusable building blocks rather than grow into hidden chips.
- Shared constraint logic should be centralized here when multiple chip
  implementations depend on the same behavior.
- Gadget interfaces may evolve, but the boundary between "reusable fragment"
  and "full chip protocol" should remain clear.
- This crate should reduce duplication in proof code, not become a second chip layer with implicit policy.

## Dependency Rules

- This crate may depend on proof-foundation crates and low-level field/AIR libraries.
- It should not depend on compiler, runtime, or backend orchestration policy crates.
- If a change is about a reusable constraint fragment rather than a full chip or
  generic proving framework, it likely belongs here.

## How To Change This Crate Safely

- Prefer extracting logic here only when it is genuinely shared across chips.
- Keep gadget APIs explicit enough that chip users can see what is being constrained.
- Avoid encoding chip-specific orchestration assumptions into shared gadgets.
- Coordinate changes with chip consumers when altering gadget semantics or witness expectations.

## Tests

Start with:

- `cargo test -p tabula-gadgets`

Preserve the behaviors that prove this crate still owns the gadget layer:

- reusable constraints stay correct and composable
- gadget changes do not force unrelated chip-specific policy into shared code
- downstream chip crates can continue to embed gadgets without duplicating logic

## Related Crates

- `tabula-stark` provides the proving foundation gadgets target
- `tabula-chips` composes these gadgets into concrete chips
- `tabula-testing` supplies shared proof-side helpers used by gadget consumers
