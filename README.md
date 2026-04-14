# Tabula

Tabula is a zero-knowledge system for typed, tabular state transitions.

Most proving systems start from a low-level execution trace and then try to recover the meaning of a program from machine steps. Tabula starts from the other direction. It treats structured state, explicit reads and writes, and registered program semantics as first-class inputs to execution and proof.

The project is aimed at applications that already think in terms of tables and state updates: balances, ledgers, permissions, orders, and similar structured data models.

## Why Tabular State

Many applications do not naturally think in terms of flat VM memory. They
think in terms of:

- typed tables
- explicit keys and columns
- well-defined state transitions
- schema-level meaning that should stay visible across the stack

Tabula tries to preserve that structure through compilation, execution, and proof preparation instead of flattening it away too early.

That makes the system easier to reason about at the semantic level, and it lets more validation and proof planning happen before proving starts.

## Quick Start

Build the CLI with proving support and generate an example project:

```sh
cargo build -p tabula-cli --features prove
cargo run -p tabula-cli -- example basic --dir /tmp/tabula-example
```

Run the example batch:

```sh
target/debug/tabula-cli execute \
  --program /tmp/tabula-example/program.tab \
  --state /tmp/tabula-example/state.json \
  --batch /tmp/tabula-example/batch.json \
  --context /tmp/tabula-example/context.json \
  --receipt-out /tmp/tabula-example/receipt.bin
```

Produce and verify a proof:

```sh
target/debug/tabula-cli prove \
  --program /tmp/tabula-example/program.tab \
  --receipt /tmp/tabula-example/receipt.bin \
  --proof-out /tmp/tabula-example/proof.bin \
  --public-statement-out /tmp/tabula-example/public_statement.json \
  --summary-out /tmp/tabula-example/proof_summary.json

target/debug/tabula-cli verify \
  --program /tmp/tabula-example/program.tab \
  --proof /tmp/tabula-example/proof.bin \
  --statement /tmp/tabula-example/public_statement.json
```

For command-line details, see [crates/cli/README.md](crates/cli/README.md).

## Proof Surface

The current verification path is statement-first.

The verifier checks a sealed program artifact, an expected public statement, and a proof. The public statement commits to the state transition and the public outputs of the executed batch.

The canonical current verifier vocabulary and cross-crate structure live in [docs/design/architecture.md](docs/design/architecture.md).

## Repository Guide

The main entry points are:

- `tabula-sdk` for product-facing embedding
- `tabula-runtime` for native execution, proving, and verification orchestration
- `tabula-contract` for proof-visible and verifier-facing contract types
- `tabula-cli` for repository-owned workflows and examples

For the current architecture and documentation map, start with:

- [docs/design/architecture.md](docs/design/architecture.md)
- [docs/README.md](docs/README.md)
- crate `README.md` files under [crates/](crates)

## Artifact Evaluation

If you are reviewing this repository as a research artifact, use [ARTIFACT.md](ARTIFACT.md).

## Status

Tabula is research code. The implementation is still evolving, and the repository is not suitable to be used in production.

## License

MIT OR Apache-2.0
