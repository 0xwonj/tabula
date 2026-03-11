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
| Sharding infrastructure | Phases A–E: ProofInstance, PartitionedStores, per-tier setup, root proof, E2E validation |
| Sharding migration | Monolithic → sharded: TabulaMachine wraps C+2 sub-proofs, all global code removed |
| Pipeline acceleration (Tier 1a) | BLAKE3 Merkle hash (~30% proving), trace ownership transfer (~50% memory) |
| Parallelization + batch inversion (Tier 1b) | rayon P-1..P-5 (quotient, perm trace, sub-proof, trace build, verify) + Montgomery batch inversion |
| Type foundation | Closed ValueType enum, soundness fixes (boolean constraints, W≥3 const assert, null encoding), dead code removed |

857 tests passing across workspace. Zero failures.

---

## Architecture Direction

**Full sharding is the base architecture.** Per-column independent proofs are the target, not an optimization.

- **Proof size**: solved by recursive aggregation (future Phase 5+)
- **Custom types**: not needed — closed ValueType (U64/I64/Bool/Bytes32) + bytes32 escape hatch
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
| 1a | Prover pipeline (BLAKE3 + trace ownership) | ✅ | [optimization.md](optimization.md) §Tier 1a | — |
| 1b | Parallelization + batch inversion | ✅ | [optimization.md](optimization.md) §Tier 1b | — |
| 2 | Proving layer refactoring | ✅ | [proving-layer.md](proving-layer.md) | — |
| 3 | Sharding infrastructure (25 gaps: G1–G13, W1–W11) | ✅ | [sharding.md](sharding.md) | 2 |
| 4 | Sharding migration | ✅ | [sharding.md](sharding.md) §Migration | 3 |
| 5 | Type foundation (closed ValueType) | ✅ | [custom-types.md](custom-types.md) | — |
| 6 | Extensibility API | 🔵 | [commitment-traits.md](commitment-traits.md), [composition.md](composition.md), [state-traits.md](state-traits.md) | — |
| 7 | Precompile framework | ⬜ | [precompile.md](precompile.md) | 6 |
| 8 | Execution templates | ⬜ | [execution-templates.md](execution-templates.md) | 6 |
| 9 | Optimization (sharded) | 🔵 | [optimization.md](optimization.md) §Sharded | — |
| 10 | Advanced research (incl. GKR-LogUp) | 🔬 | [research.md](research.md) | Various |

---

## Dependency Graph

```
✅ Complete
├── 1a. Pipeline (BLAKE3 + trace ownership) ── ~30% proving, ~50% memory
├── 1b. Parallelization + batch inversion ── rayon P-1..P-5 + Montgomery EF4 batch inverse
├── 2. Proving Layer ──────────── protocol math → stark, ProofInstance, witness partition
│   └──→ 3. Sharding Infra ───── G1-G13, W1-W11
│         └──→ 4. Migration ──── monolithic removed, sharded = base
└── 5. Type Foundation ──────── closed ValueType, soundness fixes, no custom types

Ready (can start now, in parallel)
├── 6. Extensibility API ────────── on sharded architecture
│     ├──→ 7. Precompile
│     └──→ 8. Execution Templates
└── 9. Optimization (sharded) ──── D1 per-column, constraint CSE, coprocessors

Future:
└──→ 10. Research (GKR-LogUp, D2+D3 accumulator, recursion, GPU, compiler redesign)
```

## Code Quality (Cross-Cutting)

Patterns adopted from analysis of OpenVM, SP1, RISC0, Triton VM, Jolt, Valida.
See `docs/research/zk-codebase-patterns.md` for the full comparison.

### Completed

- [x] Workspace-level lint configuration (`[workspace.lints]` in Cargo.toml)
- [x] Optimized test profile (`[profile.test] opt-level = 2`)
- [x] Auto-trait verification tests (Send + Sync for public types)
- [x] `VerificationError` / `ProveError`: `tier: String` → `tier: ProofTier` (typed context)
- [x] Gadget convenience functions: `eval_key_range_checked`, `eval_ordering_range_checked`
- [x] Witness-only types moved: `InterTxOrderRow`, `StateColumnRow` → witness crate
- [x] Dead code removed: `chips/inter_tx_order/`, `chips/state_column/`, `chips/public_input.rs`

### Deferred

- [ ] **Operation trait for ExecutionChip** (align with Goal 6)
  - Define `Operation<AB, W>` trait with `eval()` + `populate()` methods
  - Migrate 14 operations from function-based to trait-based dispatch
  - Extract common patterns first: `slot_gate_builder()`, `carry_chain()` helpers
  - Column layout stays monolithic until extensibility API (Goal 6)
  - Currently 9 files must change to add a new opcode — trait reduces to ~3
  - Ref: SP1's `SP1Operation` pattern
- [ ] **Feature-gate prover code** (align with Goal 9)
  - Separate `prove` feature from `verify` — keep verifier lightweight
  - Gate heavy crypto deps behind `prove` feature flag
  - Ref: RISC0/Jolt feature-gating pattern
- [ ] **IndexMap evaluation** — NOT needed currently
  - All maps use BTreeMap with natural key ordering (CellKey, BusId, ChipId)
  - BTreeMap provides deterministic sorted iteration, which is the correct choice
  - Revisit only if insertion-order iteration becomes needed

## Recommended Order

1. **6** — extensibility API on stable sharded architecture
2. **7 + 8** — precompile + templates (after 6)
3. **9** — D1 per-column, constraint CSE, coprocessor factoring (parallel with 6)
4. **10** — GKR-LogUp (after benchmarking perm cost), long-term research
