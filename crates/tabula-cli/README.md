# tabula-cli

Command-line interface for the Tabula kernel.

Executes batches against JSON-encoded programs and state files.
Uses mock crypto implementations (Phase 1).

## Usage

```sh
# Execute a batch
tabula-cli execute \
  --program program.json \
  --state state.json \
  --batch batch.json \
  [--output-state new_state.json] \
  [--trace] [--json]

# Inspect a program
tabula-cli inspect --program program.json

# Generate example files
tabula-cli example
```

## Commands

| Command | Description |
|---------|-------------|
| `execute` | Run a batch of transactions against a state snapshot |
| `inspect` | Display program structure (tx types, schemas) |
| `example` | Generate example program/state/batch JSON files |

## JSON File Formats

**Program** (`ProgramFile`): tx type definitions + table schemas.

**State** (`StateFile`): list of `{ table, col, row, value }` cells.

**Batch** (`BatchFile`): list of transactions with params, sender, nonce, signature.

## Dependencies

`tabula-core`, `tabula-executor`, `tabula-commitment` (mock feature),
`tabula-proof`, `tabula-lang`, `clap`, `serde_json`.
