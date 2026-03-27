# tabula-cli

Command-line interface for product-facing and developer-facing Tabula workflows.

## Role

- compile `.tab` source to sealed artifact JSON
- validate programs (`check`) and inspect full schema (`schema`)
- execute read-only queries (`query`) and stateful batches (`execute`)
- turn `receipt.bin` bridges into canonical `proof.bin` artifacts (`prove`)
- verify canonical `proof.bin` artifacts against sealed programs (`verify`)
- author state, context, and batch files with symbol-based commands
- inspect state files with symbolic rendering
- inspect resolved config and extension bundles (`env doctor`)
- generate standard and extension-backed example directories

`tabula-cli` stays above `tabula-sdk` and uses explicit files plus project-local
`tabula.toml` configuration. It does not keep hidden mutable state across
invocations.

## Usage

```sh
cargo run -p tabula-cli -- check path/to/program.tab
cargo run -p tabula-cli -- compile path/to/program.tab
cargo run -p tabula-cli -- schema path/to/program.tab
cargo run -p tabula-cli -- query current_tier --program program.tab --state state.json --args "[1]"
cargo run -p tabula-cli -- execute --program program.tab --state state.json --batch batch.json --context context.json
cargo run -p tabula-cli -- prove --program program.tab --receipt receipt.bin --proof-out proof.bin --statement-out statement.json --summary-out proof_summary.json
cargo run -p tabula-cli -- verify --program program.tab --proof proof.bin
cargo run -p tabula-cli -- state inspect --state state.json --program program.tab
cargo run -p tabula-cli -- env doctor
cargo run -p tabula-cli -- example membership --dir /tmp/tabula-example
```

`receipt.bin` is a CLI/runtime handoff file, not a cross-layer contract
artifact. `proof.bin` and `statement.json` are contract-owned proof artifacts.

If installed as a binary, the command name is `tabula`.
