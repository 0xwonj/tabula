# Tabula

Tabula is a zero-knowledge kernel for typed, tabular state transitions.

Instead of treating application logic as a general-purpose machine trace and
then proving that trace, Tabula treats typed state transitions themselves as
the thing to execute, validate, and prove. The project is built around the idea
that many applications do not naturally think in terms of flat VM memory. They
think in terms of structured state, explicit reads and writes, and schema-level
meaning. Accounts, balances, orders, permissions, and ledgers are usually
closer to tables than to machine memory.

## Why This Approach

General-purpose proving systems are powerful, but they flatten structured
application logic into machine steps. Once that happens, the system sees less
of what actually matters: state shape, read/write structure, and program meaning.

Tabula keeps that structure visible. By working with typed state transitions,
it can push more validation, analysis, and proof planning to compile time
instead of rediscovering the same facts inside every proof. That is one of the
central ways it aims to reduce proving cost.

## Core Idea

Tabula is organized around a few durable ideas:

- state lives in typed tables addressed by `(table, column, row)`
- programs are registered as explicit semantics, not just raw source text
- compile-time analysis is part of the optimization story: work resolved
  statically is work the prover does not need to pay for repeatedly
- execution, commitment semantics, and proving are separate layers
- the runtime is the default integration boundary
- the proof backend should be replaceable without redefining program meaning

## Architecture At A Glance

```text
authoring input
  -> language front-end
  -> IR
  -> semantic registration
  -> runtime execution and policy
  -> proof preparation
  -> proof backend
```

At the workspace level, the architecture is split into a few clear layers:

- shared meaning: `tabula-core`, `tabula-contract`, `tabula-artifact`
- authoring and registration: `tabula-lang`, `tabula-ir`, `tabula-compiler`
- execution and runtime policy: `tabula-executor`, `tabula-runtime`
- proof backend: `tabula-commitment`, `tabula-witness`, `tabula-gadgets`, `tabula-chips`, `tabula-stark`, `tabula-machine`
- package surfaces: `tabula-ext`, `tabula-sdk`

For the canonical current architecture, read
[`docs/design/architecture.md`](docs/design/architecture.md).

## Where To Read Next

- [`docs/design/architecture.md`](docs/design/architecture.md)
  Cross-crate architecture and dependency direction.
- [`docs/README.md`](docs/README.md)
  How to interpret `design/`, `notes/`, `research/`, and `archive/`.
- crate `README.md` files under [`crates/`](crates/)
  Crate-local contracts, design intent, and ownership boundaries.

## Getting Started

Build and test the workspace:

```sh
cargo build
cargo test
```

Generate example inputs and run a local batch:

```sh
cargo run -p tabula-cli -- example --dir /tmp/tabula-example
cargo run -p tabula-cli -- execute \
  --program /tmp/tabula-example/example_program.json \
  --state /tmp/tabula-example/example_state.json \
  --batch /tmp/tabula-example/example_batch.json
```

Check or compile a `.tab` program:

```sh
cargo run -p tabula-cli -- check path/to/program.tab
cargo run -p tabula-cli -- compile path/to/program.tab
```

## Project Status

Tabula is still early-stage and the architecture is evolving quickly.

The canonical documentation set therefore tries to optimize for durable
boundaries rather than implementation detail:

- the root `README.md` explains the project and its thesis
- [`docs/design/architecture.md`](docs/design/architecture.md) explains the
  current cross-crate architecture
- crate `README.md` files explain local contracts and design intent

Exploratory material and historical documents still exist, but they should not
be treated as the primary source of truth for the current workspace.

## License

MIT OR Apache-2.0
