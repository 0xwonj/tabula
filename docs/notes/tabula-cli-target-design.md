# Tabula CLI Target Design

> Status: proposed target
> Audience: maintainers and AI agents
> Scope: ideal product-facing and developer-facing structure for `tabula-cli`
> Related: [../design/architecture.md](../design/architecture.md),
> [sdk-ext-ideal-architecture.md](sdk-ext-ideal-architecture.md),
> [../../crates/cli/README.md](../../crates/cli/README.md)

This note defines the target design for `tabula-cli`.

It is not a description of the current implementation. It is the intended
end-state CLI shape that should guide the next rounds of implementation work.

The goal is to make `tabula-cli` good enough for two audiences at once:

- external users who want one obvious, documented command-line interface for
  Tabula workflows
- maintainers who still need a direct, scriptable, low-friction tool for local
  development and debugging

## 1. Design Thesis

The ideal CLI is:

- a thin product-facing shell on top of `tabula-sdk`
- file-oriented and stateless
- environment-aware through explicit project-local configuration
- symbolic and human-readable by default
- machine-readable through stable JSON and binary contracts
- complete enough that a user can compile, inspect, query, execute, prove, and
  verify a program without dropping down into the SDK

The CLI must not become:

- a second semantic authority
- a second runtime policy layer
- a daemon with hidden mutable state
- a pile of one-off debug commands with no stable contracts

The core architectural rule remains:

```text
tabula-cli
  -> tabula-sdk
    -> tabula-runtime / tabula-compiler
```

`tabula-cli` must stay an adapter above the canonical SDK surface. It should
compose the SDK, not bypass it.

## 2. Hard Requirements

The target CLI should satisfy all of these at once:

1. A user should be able to complete the normal Tabula lifecycle entirely from
   the CLI.
2. The CLI should support both standard environments and extension-backed
   environments.
3. The default output should use source-level names whenever a program schema is
   available.
4. The CLI should remain stateless across invocations except for explicit files
   and optional discardable caches.
5. The CLI should expose stable machine-readable contracts for automation.
6. The CLI should never require the user to understand raw ids for standard
   flows.
7. Proof-related commands should use explicit versioned file contracts rather
   than ad hoc in-memory-only behavior.
8. Every command should carry actionable diagnostics, including which file and
   which phase failed.

## 3. Non-Goals

The target CLI is not trying to do these things:

- replace the SDK for embedding use cases
- expose every internal compiler or runtime type directly
- hide all file boundaries from users
- make proof generation always-on by default
- invent a hidden user-home configuration model
- create a background service or long-lived session model

## 4. User Model

The target user model is:

- Tabula programs are authored as `.tab` files.
- Sealed registered programs are stored as artifact files.
- State, context, and batch data are explicit files.
- Execution produces both a human report and a reusable execution receipt.
- Proving consumes a receipt and emits a proof envelope plus human-readable
  metadata.
- Verification consumes an artifact plus a proof envelope.

The CLI must support both interactive human use and shell scripting.

That means:

- human-readable output goes to stdout in a stable, ergonomic format
- diagnostics go to stderr
- `--json` emits typed structured output suitable for automation
- commands should compose through files rather than hidden in-memory state

## 5. State Model

The CLI itself should be stateless.

More precisely:

- no hidden mutable process state across invocations
- no required user-home global config
- no opaque daemon-owned session state

Allowed state:

- explicit user-managed files
- project-local `tabula.toml`
- optional caches under `target/` or another clearly disposable location

The distinction matters:

- configuration is allowed
- cache is allowed
- hidden workflow state is not allowed

## 6. Configuration Model

The CLI should support one explicit project-local configuration file:

- `tabula.toml`

Config resolution order:

1. `--config <PATH>` if provided
2. nearest `tabula.toml` found by walking upward from the current working
   directory
3. empty config

There should be no implicit user-home config in the default design.

The config is where extension-backed environments become usable from the CLI.

Example:

```toml
[environment]
extensions = ["./extensions/poseidon.bundle"]

[output]
format = "human"
color = "auto"
```

The config should be able to express:

- extension bundle paths
- default output format
- color policy
- optional debug defaults
- future proof backend selection if needed

## 7. Canonical File Contracts

The ideal CLI should standardize the following file-level resources.

### 7.1 Source Program

- extension: `.tab`
- meaning: source authoring input

### 7.2 Artifact

- extension: `.json`
- meaning: sealed registered program artifact
- source type: `tabula_sdk::Artifact`

This is the output of `compile`.

The CLI should stop describing this as "IR JSON". It is an artifact, not raw
IR.

### 7.3 State

- extension: `.json`
- meaning: committed state snapshot
- source type: `tabula_sdk::State`

### 7.4 Context

- extension: `.json`
- meaning: public context input
- source type: `tabula_sdk::Context`

### 7.5 Batch

- extension: `.json`
- meaning: portable transaction batch
- source type: `tabula_sdk::TransactionBatch`

### 7.6 Schema Report

- extension: `.json` when persisted
- meaning: stable CLI-facing schema discovery contract

This should be a CLI-defined typed JSON model, not just a debug dump of SDK
internals.

### 7.7 Execution Receipt

- extension: `.bin`
- meaning: exact reusable execute-to-prove bridge object
- encoding: versioned binary envelope

This is a new contract the CLI needs.

It should carry the exact information needed for later proving, not just a
human-readable summary. In practice that means one versioned envelope around:

- program digest
- pre-state snapshot
- batch
- context
- post-state snapshot
- exact execution journal or equivalent prove input

This is intentionally binary, not JSON:

- it is workflow-critical rather than human-authored
- some internal proof-adjacent structures are not currently JSON contracts
- binary leaves room for exact round-tripping without over-designing a large
  JSON schema

### 7.8 Execution Report

- extension: `.json` when persisted
- meaning: human/machine summary of one execute command

This is separate from the binary execution receipt. It is the reporting surface
for users and automation, not the canonical prove input.

### 7.9 Proof Envelope

- extension: `.bin`
- meaning: exact reusable proof object for later verification
- encoding: versioned binary envelope

This is another new contract the CLI needs.

The ideal proof envelope should contain:

- program digest
- statement
- proof payload

The exact envelope type can evolve, but the CLI contract must be versioned and
explicit from the start.

### 7.10 Statement

- extension: `.json`
- meaning: transcript-bound public claim
- source type: `tabula_runtime::ProofStatement`

### 7.11 Proof Summary

- extension: `.json`
- meaning: human-readable proof diagnostics
- source type: `tabula_runtime::ProofSummary`

## 8. Target Command Surface

The target command surface should be complete but still legible.

### 8.1 Program Commands

- `tabula check <PROGRAM>`
- `tabula compile <PROGRAM> --out <ARTIFACT>`
- `tabula schema <PROGRAM> [--json]`

Responsibilities:

- validate source or artifact
- compile source to artifact
- expose tables, fields, txs, queries, context fields, and types

### 8.2 Authoring Commands

- `tabula state init --program <PROGRAM> --out <STATE>`
- `tabula state set --program <PROGRAM> --state <STATE> <TABLE> <ROW> <FIELD> <VALUE>`
- `tabula state inspect --state <STATE> [--program <PROGRAM>] [--json]`
- `tabula context init --program <PROGRAM> --out <CONTEXT>`
- `tabula context set --program <PROGRAM> --context <CONTEXT> <FIELD> <VALUE>`
- `tabula batch init --out <BATCH>`
- `tabula batch call --program <PROGRAM> --batch <BATCH> <TX> --args <JSON>`

These commands exist so users can author the required data files for arbitrary
programs without dropping into Rust.

### 8.3 Query And Execution Commands

- `tabula query <QUERY> --program <PROGRAM> --state <STATE> --args <JSON> [--context <CONTEXT>] [--json]`
- `tabula execute --program <PROGRAM> --state <STATE> --batch <BATCH> [--context <CONTEXT>] [--state-out <STATE>] [--receipt-out <RECEIPT>] [--report-out <REPORT>] [--json]`

`query` is mandatory in the final product surface because query execution is
part of the supported runtime model.

`execute` should be able to emit:

- human-readable stdout summary
- JSON report
- optional state-out file
- optional binary receipt for later proving

### 8.4 Proof Commands

- `tabula prove --program <PROGRAM> --receipt <RECEIPT> --proof-out <PROOF> [--statement-out <STATEMENT>] [--summary-out <SUMMARY>] [--json]`
- `tabula verify --program <PROGRAM> --proof <PROOF> [--json]`

These commands should be available only when the build enables proof features.

They should not appear as partially implemented placeholders before the binary
receipt and proof-envelope contracts exist.

### 8.5 Utility Commands

- `tabula example [NAME] --dir <DIR>`
- `tabula env doctor [--json]`

`example` should remain a workflow bootstrap command.

`env doctor` should report:

- whether config was found
- which extensions were loaded
- whether proof commands are available in this build
- major environment/setup mismatches

## 9. Output Rules

The CLI should support three output modes conceptually:

- human
- json
- raw/debug

Human mode rules:

- default mode
- symbolic names whenever the program schema is available
- concise summaries first, supporting detail second

JSON mode rules:

- stable CLI-defined JSON contracts
- no `serde_json::Value` blobs for primary output models
- intended for scripts and automation

Raw/debug mode rules:

- explicit opt-in
- may include raw ids, bytes, and low-level payloads
- never the default external-user experience

## 10. Internal Architecture

The target internal structure should look roughly like this:

```text
crates/cli/src/
  lib.rs
  main.rs
  app.rs
  cli/
    mod.rs
    workflow.rs
    authoring.rs
  config/
    mod.rs
    file.rs
    resolve.rs
  environment/
    mod.rs
    bundle.rs
    install.rs
    status.rs
  io/
    mod.rs
    fs.rs
    load.rs
    values.rs
    hex.rs
  output/
    mod.rs
    models.rs
    project.rs
    human.rs
    values.rs
  contracts/
    mod.rs
    receipt.rs
  commands/
    mod.rs
    check.rs
    compile.rs
    schema.rs
    query.rs
    execute.rs
    state.rs
    context.rs
    batch.rs
    example.rs
    env.rs
```

Responsibilities:

- `cli/`
  - clap type definitions only
- `app.rs`
  - process-wide command context
- `config/`
  - project-local config discovery and parsing
- `environment/`
  - build `Sdk` from resolved CLI config
  - bundle parsing and internal environment status
- `io/`
  - load source/artifact/state/context/batch
  - write text/JSON/binary files
  - parse CLI JSON literals and arrays
- `output/`
  - stable CLI-facing JSON models
  - domain-to-output projection
  - human rendering only
- `contracts/`
  - stable binary workflow contracts such as `receipt.bin`
- `commands/`
  - workflow orchestration

The important design rule is that command modules should not each construct
their own `Sdk::standard()` or repeat program-loading logic.

## 11. Environment Resolution

The target CLI should never hardcode the standard environment at the command
site.

Instead:

- all commands should ask one central `AppContext` for an `Sdk`
- `AppContext` should resolve config, build the environment, and cache that
  result for the process lifetime
- commands should only describe what they need, not how the environment is
  constructed

This is the change that makes extension-backed programs usable from the CLI.

## 12. Schema-First UX

The CLI should be schema-aware whenever it has a program or artifact.

That means:

- `schema` should be a first-class command
- `execute` should print tx names, not only entry ids
- `state inspect` should print table and field symbols when `--program` is
  provided
- query and batch authoring should validate names against schema before doing
  work

The CLI already has access to this information through `tabula_sdk::Schema`.
The target design requires using it systematically.

## 13. Recommended Workflows

### 13.1 Compile And Inspect

```sh
tabula compile program.tab --out program.artifact.json
tabula schema program.artifact.json
```

### 13.2 Build Input Files And Execute

```sh
tabula state init --program program.artifact.json --out state.json
tabula state set --program program.artifact.json --state state.json balances 1 amount 100
tabula context init --program program.artifact.json --out context.json
tabula batch init --out batch.json
tabula batch call --program program.artifact.json --batch batch.json transfer --args '[1,2,50]'
tabula execute \
  --program program.artifact.json \
  --state state.json \
  --batch batch.json \
  --context context.json \
  --state-out state.after.json \
  --receipt-out receipt.bin
```

### 13.3 Query

```sh
tabula query balance_of \
  --program program.artifact.json \
  --state state.after.json \
  --args '[2]' \
  --context context.json
```

### 13.4 Prove And Verify

```sh
tabula prove \
  --program program.artifact.json \
  --receipt receipt.bin \
  --proof-out proof.bin \
  --statement-out statement.json \
  --summary-out proof_summary.json

tabula verify \
  --program program.artifact.json \
  --proof proof.bin
```

## 14. Sequencing

The implementation order should be:

### Phase 1: Lock The CLI Design

- finalize command names
- finalize config shape
- finalize output expectations
- finalize which resources are JSON vs binary

This note is the Phase 1 artifact.

### Phase 2: Internal CLI Refactor

- add shared `app.rs` context layer
- add `config/` discovery and resolution
- add centralized `environment/` construction
- add shared `io/`, `output/`, and `contracts/` boundaries

This phase should not try to land all new commands at once.

### Phase 3: Non-Proof User Surface

- add `schema`
- add `query`
- add state/context/batch authoring commands
- improve `check`
- improve `execute` symbolic output
- add `env doctor`

This phase is intentionally independent of proof file contracts.

### Phase 4: Execution Receipt Contract

- define a versioned binary CLI receipt envelope
- implement read/write support
- add round-trip tests
- make `execute --receipt-out` emit it

This is the execute-to-prove bridge.

### Phase 5: Proof Contract

- define a versioned binary proof envelope
- implement read/write support
- add round-trip tests
- document its compatibility/versioning rules

### Phase 6: Proof Commands

- add `prove`
- add `verify`
- add statement and summary outputs
- add end-to-end tests

The important sequencing rule is:

- do not expose final `prove` and `verify` command UX until the receipt and
  proof contracts are real

### Phase 7: Polish

- `--help` snapshots
- README and example updates
- shell-completion support if desired
- CLI integration tests covering full workflows

## 15. Acceptance Criteria

The target CLI is done when all of these are true:

1. A user can run compile, schema, query, execute, prove, and verify from the
   CLI without touching Rust.
2. Extension-backed programs can be compiled and executed through config.
3. Default output is symbolic and readable.
4. `--json` surfaces are typed and stable.
5. Proof workflows are file-based and versioned.
6. There is no required hidden global state.
7. The CLI remains architecturally a consumer of `tabula-sdk`, not a competing
   execution or proving boundary.

## 16. Current Gaps Against This Design

Today the largest known gaps are:

- no `schema` command
- no `query` command
- no proof commands
- no receipt contract
- no proof-envelope contract
- no extension/config-driven environment loading
- no state/context/batch authoring commands for arbitrary programs
- output and docs still reflect an internal-tool shape in several places

That is acceptable as long as the implementation work now converges on this
target shape rather than adding more one-off command behavior.
