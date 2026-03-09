# Full Sharding

> Status: 🔬 Research (design decision needed before implementation)
> Depends: Design decision
> Research: [docs/design/full-sharding-research.md](../docs/design/full-sharding-research.md)
> Related: [docs/design/sharded-protocol-design.md](../docs/design/sharded-protocol-design.md), [docs/design/commitment-architecture-research.md](../docs/design/commitment-architecture-research.md)

## Goal

Replace the global proving architecture with per-column independent proofs. Each column is a self-contained STARK proof with its own encoding width W, eliminating padding waste and enabling C-way parallelism.

**Primary motivation**: Prover time reduction. See [full-sharding-research.md](../docs/design/full-sharding-research.md) for quantitative analysis.

## Architecture Summary

Three-tier proof structure:

| Tier | Scope | Parallelism |
|------|-------|-------------|
| 1. Execution | Instructions, SSA, static tables | 1 (sequential) |
| 2. Column | Memory, state, commitment per (t,c) | C-way parallel |
| 3. Root | SMT paths, cumsum balance | 1 (lightweight) |

Cross-proof soundness via LogUp cumsum exported as public values. Root proof verifies cumsum sum = 0.

## Implementation Gaps

From [full-sharding-research.md](../docs/design/full-sharding-research.md):

| ID | Gap | Effort | Critical? |
|----|-----|--------|-----------|
| G1 | ProofInstance abstraction (chip subset with independent PCS) | Large | Yes |
| G2 | ShardedProver (C+2 parallel ProofInstances + sync point) | Large | Yes |
| G3 | Public value cumsum export | Medium | Yes |
| G4 | Cross-proof Fiat-Shamir (global transcript + per-proof fork) | Medium | Yes |
| G5 | PoseidonLocal / RangeCheckLocal per column proof | Small | Yes |
| G6 | ColumnMeta decomposition (Com as public values) | Small | Yes |
| G7 | ShardedVerifier | Medium | Yes |
| G8 | Witness partitioning (per-column witness store) | Small | No |
| G9 | ExecutionChip MAX_W adaptation | Medium | No |
| G10 | Recursive aggregation (STARK verifier circuit) | Very Large | No |

## Implementation Order

```
G1 → G2 → G4 → G3 → G5 → G6 → G7 → E2E test
                                        ↓
                              G8 (optimization)
                              G9 (custom type support)
                              G10 (recursive, future)
```

## Open Research Questions

| # | Question | Status |
|---|----------|--------|
| Q1 | Independent per-proof alpha/zeta derivation? | Believed safe |
| Q2 | Optimal column grouping (one per proof vs grouped)? | Needs benchmarking |
| Q3 | Execution proof segmentation for parallelism? | Feasible, needs design |
| Q4 | Double-accumulator security for 128-bit in EF4? | Needs formal proof |
| Q5 | Optimal FRI parameters for small per-column proofs? | Needs benchmarking |

## Verification

```bash
cargo check --workspace
cargo test --workspace
cargo bench -p tabula-machine  # performance comparison
```
