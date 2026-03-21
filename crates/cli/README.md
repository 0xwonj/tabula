# tabula-cli

Command-line interface for local Tabula compiler and runtime workflows.

## Role

- compile `.tab` source to registered JSON artifacts
- validate programs (`check`)
- execute batches against states
- inspect state files
- generate example inputs for local testing

`tabula-cli` calls `tabula-compiler` and `tabula-runtime` directly.

## Usage

```sh
cargo run -p tabula-cli -- check path/to/program.tab
cargo run -p tabula-cli -- compile path/to/program.tab
cargo run -p tabula-cli -- execute --program program.json --state state.json --batch batch.json
cargo run -p tabula-cli -- inspect --state state.json
cargo run -p tabula-cli -- example --dir /tmp/tabula-example
```

If installed as a binary, the command name is `tabula`.
