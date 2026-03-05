# STARK Backend Landscape Analysis

> Comparative analysis of multi-chip STARK backends for Tabula's proving layer.

## Overview

Tabula's `tabula-proof` crate is ~15,300 LOC, of which ~60% (11,500 LOC) is custom
implementation built on top of Plonky3 (p3). This document surveys existing multi-chip
STARK backends and evaluates their suitability as dependencies or reference implementations.

## Current Tabula Architecture

### What Uses Plonky3 (External)

| Crate | Purpose |
|-------|---------|
| p3-field, p3-baby-bear | BabyBear field arithmetic (p = 2^31 - 2^27 + 1) |
| p3-poseidon2 | Poseidon2 permutation (width=16, S-box=x^7) |
| p3-air | `AirBuilder` trait + constraint evaluation |
| p3-matrix | Trace matrix types (`RowMajorMatrix<F>`) |
| p3-uni-stark | Per-chip STARK proving/verification |
| p3-fri, p3-commit | FRI-based polynomial commitment |
| p3-challenger | Fiat-Shamir transcript (DuplexChallenger) |
| p3-merkle-tree | Merkle tree + MMCS |

### What Is Custom-Built

| Component | LOC | Purpose |
|-----------|-----|---------|
| `air/builder.rs` | 60 | `InteractionAirBuilder` trait (LogUp send/receive) |
| `air/bus_macro.rs` | 294 | Typed bus generation (`define_bus!` macro) |
| `air/chip_set.rs` | 273 | Chip composition (`define_chip_set!` macro) |
| `air/chip_instance.rs` | 112 | Unified chip wrapper for prover/verifier |
| `air/interaction.rs` | 234 | LogUp interaction types + fingerprint formula |
| `air/extractor.rs` | 214 | Column-scanning interaction extraction |
| `stark/prover.rs` | 181 | Per-chip STARK proofs + LogUp balance |
| `stark/verifier.rs` | 120 | Per-chip verification + LogUp check |
| `stark/permutation.rs` | 319 | Fiat-Shamir challenges + EF4 cumsums |
| `stark/config.rs` | 91 | STARK configuration wiring |
| `debug/` | 935 | Offline constraint + LogUp verification |
| **Total infrastructure** | **~2,600** | |

### Known Limitations

- **C1 (open)**: LogUp cumulative sums are NOT PCS-committed (soundness gap)
- **Per-chip PCS**: 9 chips × individual STARK proofs = 9 PCS opening proofs
- `EmptyMessageBuilder` bridge pattern: functional but inelegant

---

## openvm-stark-backend

**Repository**: [openvm-org/stark-backend](https://github.com/openvm-org/stark-backend)
**Version**: v1.3.0 (stable), v2.0.0-alpha (SWIRL multilinear)
**License**: MIT/Apache-2.0

### What It Provides

A modular proof system backend for multi-chip STARK circuits with LogUp-based
inter-chip communication. Self-described as "Multi-matrix STARK backend with
logup built on top of Plonky3."

### Key Properties

| Property | Detail |
|----------|--------|
| **p3 version** | Vanilla p3 `=0.4.1` (NOT a fork) |
| **Audit status** | Cantina external audit (Jan-Mar 2025) + Axiom internal |
| **Security advisory** | GHSA-f69f-5fx9-w9r9 fixed in v1.1.0 |
| **Production use** | Ethereum block proving (OpenVM) |
| **GPU support** | HAL abstraction (CPU + CUDA backends) |

### Core Traits

```
Rap<AB>                    — Randomized AIR with Preprocessing (replaces Air<AB>)
InteractionBuilder         — push_interaction() for LogUp messages
PartitionedBaseAir<F>      — cached/common trace partitioning
Chip<SC>                   — records → AirProvingContext
StarkEngine                — top-level keygen/prove/verify
StarkGenericConfig         — PCS + challenge config
```

### LogUp Implementation

- **Phase 1**: Main trace commitment
- **Phase 2**: Fiat-Shamir challenge → permutation trace generation → PCS commit
- **Phase 3**: Quotient + opening proof
- Cumsums ARE PCS-committed (solves Tabula's C1 issue)
- EF4 fingerprints (~124-bit security)
- Interaction chunking for degree-aware LogUp partitioning

### Bus Types

- `PermutationCheckBus`: Multiset equality (send/receive, sum cancels to zero)
- `LookupBus`: Subset argument (table entries + lookups)

### Shared PCS

All chips' traces batched into a single PCS commitment → single opening proof.
This is fundamentally more efficient than Tabula's current per-chip approach.

### Mapping to Tabula's Custom Code

| Tabula Custom | LOC | openvm-stark-backend Equivalent |
|---------------|-----|---------------------------------|
| `InteractionAirBuilder` + `EmptyMessageBuilder` | 60 | `InteractionBuilder` + `Rap` blanket impl |
| `define_bus!` macro | 294 | `PermutationCheckBus` / `LookupBus` |
| `define_chip_set!` macro | 273 | Builder pattern (`add_air()`) |
| `permutation.rs` (LogUp) | 319 | `FriLogUpPhase` (automatic) |
| `extractor.rs` (interaction extraction) | 214 | Symbolic DAG-based automatic extraction |
| `chip_instance.rs` | 112 | `AnyRap<SC>` dynamic dispatch |
| `debug/` (constraint checker) | 935 | `DebugConstraintBuilder` built-in |
| `stark/prover.rs` | 181 | `Coordinator` (shared PCS) |
| `stark/verifier.rs` | 120 | `MultiTraceStarkVerifier` |

### Risks

1. **v2.0.0-alpha (SWIRL)**: Multilinear proof system replacing FRI-based STARK.
   v1.x API will likely be deprecated long-term.
2. **zkVM-centric design**: `Chip` trait expects `records`-based trace generation;
   Tabula uses `BatchWitness`-based pipeline.
3. **Typed bus loss**: `define_bus!` provides per-bus typed methods; openvm uses
   raw `bus_index` → less type safety.
4. **HAL complexity**: GPU abstraction layer adds conceptual overhead Tabula
   doesn't need yet.

---

## SP1 (Succinct)

### Key Properties

| Property | Detail |
|----------|--------|
| **p3 version** | Fork (`sp1-plonky3`) — NOT vanilla p3 |
| **Focus** | zkVM (RISC-V execution proving) |
| **LogUp** | Custom, similar to openvm but tightly coupled to VM |

### Patterns Adopted by Tabula

- SP1-style LogUp: shared challenge pair (α,β)
- `AlignedBorrow` concept → manual `borrow_cols()` in Tabula
- `EmptyMessageBuilder` bridge pattern (originated in SP1)
- Operations pattern → planned in `proof-refactoring.md`

### Why Not a Direct Dependency

- p3 fork (`sp1-plonky3`) is incompatible with vanilla p3
- Designed exclusively for RISC-V zkVM
- Release cycle coupling with Succinct's roadmap

---

## Comparison Matrix

| Feature | Tabula (current) | openvm-stark-backend | SP1 |
|---------|------------------|---------------------|-----|
| p3 version | vanilla 0.4 | vanilla =0.4.1 | fork |
| LogUp PCS-committed | No (C1 open) | Yes | Yes |
| Shared PCS | No (per-chip) | Yes | Yes |
| Audited | No | Yes (Cantina) | Yes |
| Typed buses | Yes (`define_bus!`) | No (raw index) | No |
| Chip composition | Static enum macro | Dynamic builder | Static |
| GPU support | No | Yes (HAL) | Yes |
| Designed for | State-machine proving | General STARK | zkVM |
| Dependency risk | None | v2.0 SWIRL transition | p3 fork |

---

## Recommendation

**Do not adopt openvm-stark-backend as a direct Cargo dependency.**

Instead, adopt **Option C: Pattern Borrowing + Tabula-Owned Machine Layer**.

Rationale:
1. Same p3 version means patterns translate cleanly without dependency
2. v2.0 SWIRL transition creates long-term API stability risk
3. zkVM-centric API (`Chip<SC>`, `HAL`, `PartitionedBaseAir`) adds unnecessary
   abstraction for Tabula's static chip structure
4. Tabula's unique properties (NF rules, static coordinates, schema typing)
   enable optimizations that no generic backend can provide

Key patterns to borrow:
- **RAP blanket impl**: Replaces `EmptyMessageBuilder` bridge
- **Shared PCS batching**: Single opening proof across all chips
- **PCS-committed LogUp** (FriLogUp phase): Solves C1 soundness gap
- **Interaction chunking**: Degree-aware LogUp partitioning

See: [Machine Layer Architecture](machine-layer-architecture.md) for detailed design.
See: [Tabula-Native Optimizations](tabula-native-optimizations.md) for domain-specific patterns.
