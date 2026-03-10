# Codebase Architecture Review

> Assessment of the Tabula codebase's modularity, extensibility, and readiness for the sharded proof architecture.
> Related: [master-roadmap.md](master-roadmap.md), [proving-layer-architecture.md](proving-layer-architecture.md), [architecture.md](architecture.md)

---

## System Overview

16 crates, ~58K lines, 979 tests. Organized into 7 layers with strict acyclic dependencies.

```
UI Layer          cli, daemon, web
Orchestration     driver, artifact
Governance        contract
Machine           machine
Witness           witness
STARK             stark, gadgets, chips
Foundation        core, ir, commitment
Execution         executor, lang
```

---

## Layer Assessment

### Foundation (core, ir, commitment) — Excellent

`tabula-core` defines 8 extension traits (`Hasher`, `SigVerifier`, `MembershipScheme`, `NoncePolicy`, `StateSnapshot`, `StaticTableProvider`, `ValueCodec`, `FieldHasher`) covering all pluggable surfaces. Adding a new cryptographic backend is a trait impl — no source modification.

`Value` (4 variants) and `Instruction` (13 variants) are **intentionally closed**. Exhaustive pattern matching forces compile-time updates across all handling sites when a variant is added. This is a security property, not a limitation.

`tabula-commitment` is generic over `H: FieldHasher`. `HybridVC<H>` auto-selects SSMC or SMT per column. New hasher = new impl, zero code changes in HybridVC.

### Execution (executor, lang) — Excellent

`tabula-executor` has **zero cryptographic dependencies** — enforced at compile time. All crypto flows through trait objects in `BatchEnv<'a>`. Execution is deterministic and testable without any proof system.

`tabula-lang` is a self-contained DSL compiler (hand-rolled lexer + recursive descent + Pratt parser) with rustc-style error diagnostics. Depends only on `core` and `ir`.

### STARK Framework (stark, gadgets, chips) — Excellent

Open newtypes (`ChipId(u16)`, `BusId(u16)`) prevent closed-enum bottlenecks. Downstream crates define custom chips (id ≥ 100) and buses without modifying core.

Consistent 3-file chip pattern (`columns.rs` / `air.rs` / `trace.rs`) with `#[repr(C)]` zero-copy column structs. 9 global chips + 5 shard chips follow identical patterns.

`define_bus!` macro generates typed `send_*`/`receive_*` methods. Chips communicate only through bus protocol — no direct chip-to-chip dependencies.

`ColumnCommitment` trait and `ProofPlan` in `stark/src/trace/column_commitment.rs` provide per-column commitment abstraction.

### Witness Pipeline (witness) — Good, Partitioning Gap

`WitnessGenerator` produces per-column `ColumnWitness` — data model is 80% sharding-ready. `TraceBuilder` orchestrates phase-ordered dispatch via `DynChip` + `BusConsumer` traits.

**Gap**: `build_all_traces()` operates on the full chip set. No support for chip-subset or per-proof-instance partitioning. String-keyed `WitnessStore` has runtime type checking (acceptable, errors are rare).

### Machine (machine) — Good, Monolithic Prove Pipeline

`MachineBuilder` + `ChipRegistry` provide extensible registration. Composition traits (`MemoryModel`, `RootProof`, `CommitmentScheme`) enable Layer 0/1 customization.

**Gap**: `prove_with_key()` is an 11-phase monolithic function. No intermediate state, no phase-level pause/resume, no `ProofInstance` abstraction. RAP folders, permutation trace generation, and quotient computation are STARK protocol math but live in `machine` instead of `stark`.

### Orchestration & Governance (driver, artifact, contract) — Excellent

`tabula-driver` is the canonical semantic hub — compile, register, validate, execute. CLI and daemon delegate here. Zero logic duplication.

`tabula-contract` provides fail-closed proof compatibility: profile hash, schema version, binding version must all match or proof is rejected. Versioned with expiry dates on deferred bindings.

---

## Extension Patterns

All extension points follow the **implement-and-register** pattern:

| Extension | Implement | Register |
|-----------|-----------|----------|
| Hash function | `Hasher` trait | Pass to `BatchEnv` |
| Field hasher | `FieldHasher` trait | Pass to `HybridVC` |
| Signature scheme | `SigVerifier` trait | Pass to `BatchEnv` |
| State backend | `StateSnapshot` trait | Pass to `Overlay` |
| Commitment scheme | `ColumnCommitment` trait | `MachineBuilder::with_commitment()` |
| AIR chip | `ChipSpec` + 3 files | `MachineBuilder::with_chip()` |
| Bus | `BusId::app(N)` + `define_bus!` | Used in chip `eval()` |
| Gadget | Constraint functions | Called from chip `eval()` |

**None require modifying existing code.** The Zero-Modification Principle is consistently upheld.

---

## Dependency Discipline

```
core  ←── everything (8 traits, stable foundation)
ir    ←── executor, lang, witness, chips
executor → core + ir only (zero crypto!)
chips → stark + gadgets only (no machine, no witness)
witness → chips + stark (no machine)
machine → witness + stark (unidirectional)
```

No circular dependencies. Feature flag `stark` isolates Plonky3 dependencies. Executor compiles and runs without any proof system.

---

## Sharding Readiness

| Component | Ready | Gap |
|-----------|-------|-----|
| Shard chips (MemoryShard, StateShard, MetaShard) | 100% | — |
| ChipId allocator for shards | 100% | — |
| ColumnCommitment trait | 100% | — |
| Gadgets, buses, AIR framework | 100% | — |
| Witness data model (ColumnWitness) | 80% | Partition API |
| Trace orchestration | 50% | chip-subset `build_traces_for()` |
| Prove pipeline | 30% | ProofInstance, phase decomposition |
| Protocol math location | 0% | RAP/perm/quotient in wrong crate |

The foundation and chip layers are sharding-ready. The proving pipeline is the bottleneck — addressed by [proving-layer-architecture.md](proving-layer-architecture.md) (Goal 2).

---

## Architectural Strengths to Preserve

1. **Compiler-enforced safety**: Closed enums with exhaustive matching. New variants = compile errors at all sites.
2. **Trait-based extension**: Eight traits cover all pluggable surfaces. No source modification for new backends.
3. **Strict dependency direction**: Lower layers never import upper layers. Feature flags for optional deps.
4. **Consistent patterns**: 3-file chips, `#[repr(C)]` columns, `define_bus!` macros. Every chip looks the same.
5. **Bus as universal interface**: LogUp is the sole cross-chip mechanism. No direct chip-to-chip coupling.
6. **Separation of execution and proving**: Executor has zero crypto deps. Testable independently.
