# tabula-cli

Command-line interface for the Tabula kernel.

## Role

Executes batches against JSON-encoded programs and state files.
Supports `.tab` source compilation, program inspection, and
example file generation.
Runtime command paths are routed through `tabula-orchestrator`.

## Usage

```sh
tabula-cli execute --program program.json --state state.json --batch batch.json
tabula-cli inspect --program program.json
tabula-cli example
```
