# Tabula — Task Index

> Single entry point for all planned work.
> Each goal links to a detail file. Detail files reference `docs/` for design context.

## Status Legend

- ✅ Complete
- 🔧 In Progress
- 🔵 Ready (dependencies met, can start)
- ⬜ Blocked (dependencies not met)
- 🔬 Research (design decision needed)

---

## Completed

| Work | Notes |
|------|-------|
| M1–M13 Foundation | All milestones complete. 9 core chips, 11 LogUp buses, 6 E2E STARK tests |
| Machine Layer | TabulaMachine, ChipRegistry, shared PCS, two-round protocol, C1 soundness fix |
| Foundation Refactoring | TracePhase open newtype, EncodingWidth, DEFAULT_VALUE_WIDTH, Composition buses, boundary audit |
| Machine code quality | RAP folder encapsulation, error types, function extraction, directory structure |
| Proving layer refactoring | STARK protocol math → stark, ProofInstance abstraction, witness partitioning |

981 tests passing across workspace. Zero failures.

---

## Architecture Direction

**Full sharding is the base architecture.** Per-column independent proofs are the target, not an optimization.

- **Proof size**: solved by recursive aggregation (future Phase 5+)
- **Custom types**: solved by per-column width polymorphism (natural in sharded)
- **Bus width**: solved by per-column fingerprints (no MAX_W padding)
- **SortedMem**: no global sorted memory; MemoryShard handles per-column

Three-tier proof structure:
- **Tier 1**: Execution proof (1, global) — ExecutionChip, StaticTableChip, PoseidonLocal, RCLocal
- **Tier 2**: Column proofs (C, parallel) — MemoryShard\<W\>, StateShard\<W\>, PoseidonLocal, RCLocal
- **Tier 3**: Root proof (1, lightweight) — SMT paths, cumsum balance

See [docs/design/full-sharding-research.md](../docs/design/full-sharding-research.md) for the ideal protocol.

---

## Goals

| # | Goal | Status | Detail | Depends On |
|---|------|--------|--------|------------|
| 1 | Prover pipeline acceleration | 🔵 | [optimization.md](optimization.md) §Tier 1 | — |
| 2 | Proving layer refactoring | ✅ | [proving-layer.md](proving-layer.md) | — |
| 3 | Sharding infrastructure (25 gaps: G1–G13, W1–W11) | 🔵 | [sharding.md](sharding.md) | 2 |
| 4 | Sharding migration | ⬜ | [sharding.md](sharding.md) §Migration | 3 |
| 5 | Type foundation (TypeTag) | 🔵 | [custom-types.md](custom-types.md) | — |
| 6 | Extensibility API | ⬜ | [commitment-traits.md](commitment-traits.md), [composition.md](composition.md), [state-traits.md](state-traits.md) | 4 |
| 7 | Precompile framework | ⬜ | [precompile.md](precompile.md) | 6 |
| 8 | Execution templates | ⬜ | [execution-templates.md](execution-templates.md) | 6 |
| 9 | Optimization (sharded) | ⬜ | [optimization.md](optimization.md) §Sharded | 4 |
| 10 | Advanced research | 🔬 | [research.md](research.md) | Various |

---

## Dependency Graph

```
Independent (start now, in parallel)
├── 1. Prover Pipeline ─────────── ~40% proving, ~50% memory, ~4 days
├── 2. Proving Layer ──────────── protocol math → stark, ProofInstance, witness partition
│   └──→ 3. Sharding Infra ───── G1-G7: THE critical path
│         └──→ 4. Migration (sharded E2E → deprecate global)
│               ├──→ 6. Extensibility API (on sharded architecture)
│               │     ├──→ 7. Precompile
│               │     └──→ 8. Execution Templates
│               └──→ 9. Optimization on sharded (D1, CSE, coprocessors)
└── 5. Type Foundation ─────────── TypeTag + TypeEncoding (small, independent)

Future:
└──→ 10. Research (D2+D3 accumulator, recursion, GPU, compiler redesign)
```

## Recommended Order

1. **1 + 2** — in parallel: pipeline is independent, proving layer is sharding prerequisite
2. **5** — TypeTag/TypeEncoding (small, independent, useful everywhere)
3. **3** — sharding infrastructure on clean layer boundaries
4. **4** — sharded E2E working → deprecate global chips
5. **6** — extensibility on stable sharded architecture
6. **7 + 8** — precompile + templates on sharded
7. **9** — D1 per-column, constraint CSE, coprocessor factoring
8. **10** — long-term research
