# Tabula

A zero-knowledge kernel for typed, tabular state transitions.

State is represented as schema-typed tables (rows and columns). Transactions are small, deterministic state-transition programs that explicitly read, write, and check this state, producing a new committed state from an old one.

Tabula is designed end-to-end around this tabular model so that proofs match the structure of state directly—verification focuses on **what changed** in persistent state, not low-level execution details. The goal is to make stateful ZK systems feel *native* to how applications actually store and update data.

## Thesis

Tabula moves the abstraction boundary from **ISA-level execution** to
**schema-typed state transitions**. In Tabula, **memory consistency
scales with persistent state accesses — not total computation**.

### Structural overhead in general-purpose zkVMs

General-purpose zkVMs (SP1, RISC Zero, ...) prove *machine execution*.
For stateful workloads this is structurally expensive:

- **ISA overhead on state access:** a "DB read/write" becomes many ISA
  instructions, each constrained in the trace.
- **Memory consistency over everything:** sorted-memory covers
  stack/heap/temporaries as well as application state.
- **Untyped flat memory:** the prover sees bytes, losing schema/type
  structure.
- **State commitment in application code:** state root updates are paid
  through the same execution machinery.

### The core idea

ZK work splits into two cost regimes:

- **Stateful:** persistent reads/writes (infrastructure-heavy).
- **Stateless:** pure ops (localized, predictable cost per op).

zkVMs run both through the same ISA trace. Tabula **separates them**:

- **State lives in typed tables** (column-partitioned, key-addressed)
  with proof-aware commitment and constraints.
- **Computation lives in chips:** operation-specific AIR chips connected
  via a LogUp-style bus, without an ISA intermediary.

### Tables and SSA as proof primitives

- **Per-type specialization:** schema types drive narrow, optimized chips
  (Bool vs U64 limbs vs digests).
- **Per-column commitment:** each column chooses the right commitment
  strategy; untouched columns incur no proof work.
- **Normal Form (NF):** structural rules remove *intra-transaction*
  memory consistency; only **inter-transaction** consistency remains,
  scoped per (table, column).
- **SSA locals are trace wires:** locals are trace columns, so
  sorted-memory complexity is driven by **state touches**, not
  stack/heap traffic.

See [`docs/thesis.md`](docs/thesis.md) for the full argument.

## How It Works

```
  .tab source          IR            ExecutionResult         STARK proof
 ┌───────────┐    ┌──────────┐    ┌──────────────────┐    ┌──────────────┐
 │  table .. │───→│ Read     │───→│ read set         │───→│ AIR chips    │
 │  tx ..    │    │ Assert   │    │ write set        │    │ witness      │
 │           │    │ Write    │    │ events           │    │ constraints  │
 └───────────┘    └──────────┘    └──────────────────┘    └──────────────┘
   compile         execute          commitment + prove
```

**Three stages, strictly separated:**

1. **Execution** — Interpret IR against a state snapshot. Deterministic,
   crypto-free. Produces read/write sets and execution events.
2. **Commitment** — Compute cryptographic state roots (Poseidon, SMT, SSMC).
   Native computation, no constraints.
3. **Proving** — Encode everything as AIR constraints over Plonky3/BabyBear.
   Verify the execution was correct and the state transition is valid.

The executor has zero crypto dependencies. All cryptographic operations
are behind traits defined in `tabula-core` and injected at the call site.
This means the same execution engine works with Blake3 (testing),
Poseidon (proving), or any future hash function.

## Tabula IR

Transaction bodies are sequences of these instructions:

| Instruction | Effect |
|-------------|--------|
| `Read`      | Load a cell from state → `(value, is_null)` |
| `Write`     | Store a value to a cell (or delete if null) |
| `Arith`     | Add, Sub, Mul on typed values |
| `DivMod`    | Division with quotient and remainder |
| `Cmp`       | Compare two values → Bool |
| `Assert`    | Fail the tx if condition is false |
| `Hash`      | Cryptographic hash of values → Bytes32 |
| `Select`    | Conditional value: `if cond then a else b` |
| `Lookup`    | Read from a static (fixed) table |
| `Emit`      | Application-level event (out-of-protocol) |

Every instruction writes to SSA slots (assigned exactly once).
Four **normal-form rules** are enforced at compile time:

- **NF-1:** One Read per cell per tx
- **NF-2:** One Write per cell per tx
- **NF-3:** No Read after Write to the same cell
- **NF-4:** Row keys must be provably equal or provably distinct

These guarantee that every valid program has a unique, deterministic
execution trace — which is exactly what the proof system needs.

## Example

```
table balances { balance: u64 }

tx transfer(from: u64, to: u64, amount: u64) {
    let sender_bal = balances[from].balance
    let recv_bal = balances[to].balance
    assert sender_bal >= amount
    let new_sender = sender_bal - amount
    let new_recv = recv_bal + amount
    balances[from].balance = new_sender
    balances[to].balance = new_recv
}
```

This compiles to 8 IR instructions: 2 Reads, 1 Cmp, 1 Assert,
2 Ariths, 2 Writes. Each maps directly to proof constraints.

## Architecture

```
tabula-core           Types, traits, errors
    ↑
tabula-ir             IR definitions, SSA validation, normal form
    ↑
tabula-executor       Deterministic execution engine
    ↑
tabula-lang           DSL compiler (.tab → IR)

tabula-commitment     Protocol crypto (out-of-circuit): Poseidon, SMT, SSMC
    ↑
tabula-proof          STARK proof system (in-circuit): AIR, constraints, Plonky3

tabula-cli            Command-line interface
tabula-daemon         Local HTTP control plane
tabula-web            Leptos browser IDE
```

| Crate | Role |
|-------|------|
| [`tabula-core`](crates/core/) | Interfaces — types, traits, errors |
| [`tabula-ir`](crates/ir/) | IR — instructions, tx type defs, SSA/NF validation |
| [`tabula-executor`](crates/executor/) | Execution — interpreter, overlay, batch |
| [`tabula-lang`](crates/lang/) | Compiler — `.tab` DSL to IR |
| [`tabula-artifact`](crates/artifact/) | Artifacts — canonical program/state/batch/receipt models and helpers |
| [`tabula-compiler`](crates/compiler/) | Static pipeline — load, compile, register, compatibility validation |
| [`tabula-commitment`](crates/commitment/) | Crypto — Poseidon, SMT, SSMC (out-of-circuit) |
| [`tabula-proof`](crates/proof/) | Proving — STARK proof generation via Plonky3 |
| [`tabula-cli`](crates/cli/) | CLI — JSON-based batch execution and inspection |
| [`tabula-daemon`](crates/daemon/) | Local API — check/compile/execute/prove/verify endpoints |
| [`tabula-web`](crates/web/) | Web IDE — browser UI for program/state/tx/proof workflows |

## Specs

| Document | Scope |
|----------|-------|
| [`semantics-spec.md`](docs/spec/semantics-spec.md) | Core IR contract, execution model, normal form |
| [`proof-spec.md`](docs/spec/proof-spec.md) | AIR constraints, LogUp, trace layout, STARK |
| [`architecture.md`](docs/design/architecture.md) | Crate structure, data flow, phasing |
| [`final-target-architecture.md`](docs/design/final-target-architecture.md) | Final target architecture, crate boundaries, big-bang migration playbook |

## Getting Started

```sh
cargo build
cargo test
cargo clippy --all-targets

# Generate and run an example batch
cargo run -p tabula-cli -- example
cargo run -p tabula-cli -- execute \
    --program example_program.json \
    --state example_state.json \
    --batch example_batch.json

# Run daemon + web IDE
TABULA_DAEMON_TOKEN=secret cargo run -p tabula-daemon -- \
    --host 127.0.0.1 --port 4317 \
    --allow-path /tmp \
    --allow-origin http://127.0.0.1:8080
trunk serve --manifest-path crates/web/Cargo.toml
```

## License

MIT OR Apache-2.0
