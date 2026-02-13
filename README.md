# Tabula

A table-native zero-knowledge state-transition kernel.

Tabula proves semantic database operations (Read, Write, Hash, Assert, ...) directly,
avoiding the ISA overhead of general-purpose zkVMs like RISC-V.

## Architecture

```
tabula-core           Types, traits, IR, errors
    ↑
tabula-executor       Deterministic execution engine
    ↑
tabula-lang           DSL compiler (.tab → IR)

tabula-commitment     Protocol crypto (out-of-circuit): Poseidon, SMT, SSMC
    ↑
tabula-proof          STARK proof system (in-circuit): AIR, constraints, Plonky3

tabula-cli            Command-line interface
```

**Core principle**: The executor has zero crypto dependencies. All cryptographic
and policy decisions are abstracted behind traits defined in `tabula-core` and
injected at the call site.

## Crates

| Crate | Role |
|-------|------|
| [`tabula-core`](crates/tabula-core/) | Interfaces — types, traits, IR, errors |
| [`tabula-executor`](crates/tabula-executor/) | Execution — interpreter, overlay, batch, NF validation |
| [`tabula-lang`](crates/tabula-lang/) | Compiler — `.tab` DSL to IR |
| [`tabula-commitment`](crates/tabula-commitment/) | Crypto — protocol-level cryptographic primitives |
| [`tabula-proof`](crates/tabula-proof/) | Proving — STARK proof generation and verification |
| [`tabula-cli`](crates/tabula-cli/) | CLI — JSON-based batch execution and inspection |

## Key Specs

| Document | Version | Scope |
|----------|---------|-------|
| [`semantics-spec.md`](docs/semantics-spec.md) | v0.2.1 | Core IR contract, execution model, normal form |
| [`proof-spec.md`](docs/proof-spec.md) | v0.9 | AIR constraints, LogUp, trace layout, STARK |
| [`architecture.md`](docs/architecture.md) | v0.4.4 | Crate structure, data flow, phasing |

## Building

```sh
cargo build
cargo test
cargo clippy --all-targets
```

## License

MIT OR Apache-2.0
