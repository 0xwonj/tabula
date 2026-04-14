# tabula-cli

Command-line interface for product-facing and developer-facing Tabula
workflows.

## Role

- compile `.tab` source to sealed artifact JSON
- validate programs (`check`) and inspect full schema (`schema`)
- execute read-only queries (`query`) and stateful batches (`execute`)
- turn `receipt.bin` handoff files into canonical `proof.bin` artifacts
  (`prove`)
- verify canonical `proof.bin` artifacts against sealed programs and explicit
  expected public statements (`verify`)
- inspect proved public statements and proof-envelope metadata (`inspect-proof`)
- author state, context, and batch files with symbol-based commands
- inspect state files with symbolic rendering
- inspect resolved config and extension bundles (`env doctor`)
- generate standard and extension-backed example directories

`tabula-cli` stays above `tabula-sdk` and uses explicit files plus
project-local `tabula.toml` configuration. It does not keep hidden mutable
state across invocations.

## Usage

```sh
cargo run -p tabula-cli -- check path/to/program.tab
cargo run -p tabula-cli -- compile path/to/program.tab
cargo run -p tabula-cli -- schema path/to/program.tab
cargo run -p tabula-cli -- query current_tier --program program.tab --state state.json --args "[1]"
cargo run -p tabula-cli -- state set --program program.tab --state state.json accounts --key '[1]' balance 100
cargo run -p tabula-cli -- execute --program program.tab --state state.json --batch batch.json --context context.json
cargo run -p tabula-cli -- state inspect --state state.json --program program.tab
cargo run -p tabula-cli -- env doctor
cargo run -p tabula-cli -- example bank --dir /tmp/tabula-example
```

`prove` and `verify` are only available when the CLI is built with proof
support:

```sh
cargo build -p tabula-cli --features prove
target/debug/tabula-cli prove --program program.tab --receipt receipt.bin --proof-out proof.bin --public-statement-out public_statement.json --summary-out proof_summary.json
target/debug/tabula-cli verify --program program.tab --proof proof.bin --statement public_statement.json
target/debug/tabula-cli inspect-proof --proof proof.bin
```

`receipt.bin` is a CLI/runtime handoff file, not a cross-layer contract
artifact. `proof.bin` is the contract-owned proof envelope, while
`public_statement.json` is the caller-supplied stable verification object used
by the secure verification path.

Logical user-state keys are always authored as JSON arrays through `--key`,
even for unary keys.

Current proof-capable CLI scope:

- stateful transaction batches only
- query execution only, not query proving
- unary native user-state keys only (`1 component / 3 FE`)

If installed as a binary, the command name is `tabula`.
