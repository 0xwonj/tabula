# Artifact Guide

This document is the reproduction guide for the Tabula repository.

The root [README.md](README.md) explains the project. This file explains how to run the current proof-capable path and what to expect from it.

## Scope

The current artifact covers the active proof-capable implementation:

- stateful transaction-batch execution
- proof generation from `receipt.bin`
- verification against a sealed program and explicit `public_statement.json`

Out of scope for the current artifact:

- query proving
- non-unary native user-state proving
- broader future architectures described in exploratory notes

## Requirements

- macOS or Linux
- Rust stable toolchain
- `wasm32-unknown-unknown` Rust target

The repository already pins the toolchain in
[rust-toolchain.toml](rust-toolchain.toml).

## Build

Build the proof-capable CLI:

```sh
cargo build -p tabula-cli --features prove
```

## Quick Check

Generate a small example project:

```sh
cargo run -p tabula-cli -- example basic --dir /tmp/tabula-example
```

Execute the example batch:

```sh
target/debug/tabula-cli execute \
  --program /tmp/tabula-example/program.tab \
  --state /tmp/tabula-example/state.json \
  --batch /tmp/tabula-example/batch.json \
  --context /tmp/tabula-example/context.json \
  --receipt-out /tmp/tabula-example/receipt.bin
```

Produce a proof and public statement:

```sh
target/debug/tabula-cli prove \
  --program /tmp/tabula-example/program.tab \
  --receipt /tmp/tabula-example/receipt.bin \
  --proof-out /tmp/tabula-example/proof.bin \
  --public-statement-out /tmp/tabula-example/public_statement.json \
  --summary-out /tmp/tabula-example/proof_summary.json
```

Verify the proof:

```sh
target/debug/tabula-cli verify \
  --program /tmp/tabula-example/program.tab \
  --proof /tmp/tabula-example/proof.bin \
  --statement /tmp/tabula-example/public_statement.json
```

Inspect the proof payload:

```sh
target/debug/tabula-cli inspect-proof --proof /tmp/tabula-example/proof.bin
```

## Expected Outputs

After `execute`, the example directory should contain `receipt.bin`.

After `prove`, the example directory should contain:

- `proof.bin`
- `public_statement.json`
- `proof_summary.json`

`verify` should succeed when run against the matching sealed program and
`public_statement.json`.

`inspect-proof` should print proof-envelope metadata and the carried public
statement.

## Notes

- `receipt.bin` is a CLI/runtime handoff file used to reconstruct the proving
  input. It is not the stable external verification object.
- `proof.bin` is the proof envelope.
- `public_statement.json` is the caller-supplied stable verification file.

For the proof vocabulary and architecture behind this flow, see
[docs/design/architecture.md](docs/design/architecture.md).

## More Reading

- [README.md](README.md)
- [crates/cli/README.md](crates/cli/README.md)
- [docs/README.md](docs/README.md)
