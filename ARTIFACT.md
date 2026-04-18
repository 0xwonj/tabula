# Artifact Guide

This document is the self-contained reproduction guide for the current
proof-capable Tabula artifact.

The artifact demonstrates semantic-first proving of typed tabular transaction
batches with statement-first verification: the verifier checks a sealed program,
an explicit expected `public_statement.json`, and the proof together.

## Supported Scope

The supported subset is:

- stateful transaction-batch execution and proving
- verification against a sealed program and explicit `public_statement.json`
- unary native user-state keys only
- public examples: `basic` and `membership`

The following are intentionally out of scope:

- query proving
- non-unary native user-state proving
- broader architecture or future-system material elsewhere in the repository

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

## Fastest End-To-End Run: `basic`

Generate a small example project:

```sh
target/debug/tabula-cli example basic --dir /tmp/tabula-basic
```

Execute the example batch:

```sh
target/debug/tabula-cli execute \
  --program /tmp/tabula-basic/program.tab \
  --state /tmp/tabula-basic/state.json \
  --batch /tmp/tabula-basic/batch.json \
  --context /tmp/tabula-basic/context.json \
  --receipt-out /tmp/tabula-basic/receipt.bin
```

Produce a proof and public statement:

```sh
target/debug/tabula-cli prove \
  --program /tmp/tabula-basic/program.tab \
  --receipt /tmp/tabula-basic/receipt.bin \
  --proof-out /tmp/tabula-basic/proof.bin \
  --public-statement-out /tmp/tabula-basic/public_statement.json \
  --summary-out /tmp/tabula-basic/proof_summary.json
```

Verify the proof:

```sh
target/debug/tabula-cli verify \
  --program /tmp/tabula-basic/program.tab \
  --proof /tmp/tabula-basic/proof.bin \
  --statement /tmp/tabula-basic/public_statement.json
```

Inspect the proof payload:

```sh
target/debug/tabula-cli inspect-proof --proof /tmp/tabula-basic/proof.bin
```

## Other Public Example

You can run the same execute/prove/verify flow on the other public example:

```sh
target/debug/tabula-cli example membership --dir /tmp/tabula-membership
```

Each generated directory contains the same file layout used above:
`program.tab`, `state.json`, `batch.json`, and `context.json`.

## Expected Outputs

After `execute`, the example directory should contain `receipt.bin`.

After `prove`, the example directory should contain:

- `proof.bin`
- `public_statement.json`
- `proof_summary.json`

`verify` should succeed when run against the matching sealed program and
`public_statement.json`.

`inspect-proof` should print proof-envelope metadata (proof system, proof
encoding, binding digest). The envelope wire format does not carry the public
statement; callers wanting statement-level fields must read
`public_statement.json` directly.

## Output Meaning

- `receipt.bin` is a CLI/runtime handoff file used to reconstruct the proving
  input. It is not the stable external verification object.
- `proof.bin` is the proof envelope wrapping the machine proof bytes. It
  does **not** carry the public statement; verification requires a paired
  `public_statement.json`.
- `public_statement.json` is the caller-supplied stable verification file and
  the sole carrier of the artifact-bound public statement across processes.
