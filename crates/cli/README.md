# tabula-cli

`tabula-cli` is the file-based command surface for the current Tabula artifact.
It compiles sealed programs, executes read-only queries and stateful batches,
turns `receipt.bin` handoff files into `proof.bin`, and verifies proofs in a
statement-first flow against an explicit expected `public_statement.json`.

## Current Artifact Scope

- stateful transaction-batch proving
- query execution only, not query proving
- unary native user-state keys only
- public examples: `basic` and `membership`

`prove`, `verify`, and `inspect-proof` are only available when the CLI is built
with proof support.

## Primary Commands

- `check`, `compile`, `schema`
- `query`, `execute`
- `prove`, `verify`, `inspect-proof`

This repository builds the CLI as `target/debug/tabula-cli`.

## Common Flow

Build the proof-capable CLI:

```sh
cargo build -p tabula-cli --features prove
```

Generate one of the public example directories:

```sh
target/debug/tabula-cli example basic --dir /tmp/tabula-basic
target/debug/tabula-cli example membership --dir /tmp/tabula-membership
```

Each generated directory has the same file layout, so the core execution and
proof flow is:

```sh
target/debug/tabula-cli execute \
  --program DIR/program.tab \
  --state DIR/state.json \
  --batch DIR/batch.json \
  --context DIR/context.json \
  --receipt-out DIR/receipt.bin

target/debug/tabula-cli prove \
  --program DIR/program.tab \
  --receipt DIR/receipt.bin \
  --proof-out DIR/proof.bin \
  --public-statement-out DIR/public_statement.json \
  --summary-out DIR/proof_summary.json

target/debug/tabula-cli verify \
  --program DIR/program.tab \
  --proof DIR/proof.bin \
  --statement DIR/public_statement.json

target/debug/tabula-cli inspect-proof --proof DIR/proof.bin
```

There is no query-proof flow in the current artifact.

## Artifact Files

- `receipt.bin` is a CLI/runtime handoff file used to reconstruct proving
  inputs. It is not the stable external verification object.
- `proof.bin` is the proof envelope.
- `public_statement.json` is the caller-supplied stable verification object
  used by the statement-first verification path.

## Secondary Commands

- `state set` and `state inspect` help author and inspect local state files.
- `context init` / `context set` and `batch init` / `batch call` help build the
  typed JSON inputs consumed by `execute`.
- When using `state set`, logical keys are authored as JSON arrays through
  `--key`; the current proof-capable subset supports unary keys only.
