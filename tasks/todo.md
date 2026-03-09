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

979 tests passing across workspace. Zero failures.

---

## Goals

| # | Goal | Status | Detail | Depends On |
|---|------|--------|--------|------------|
| 1 | Machine code quality | 🔧 | [code-quality.md](code-quality.md) | — |
| 2 | Commitment traits | 🔵 | [commitment-traits.md](commitment-traits.md) | — |
| 3 | Composition framework | 🔵 | [composition.md](composition.md) | — |
| 4 | Precompile framework | ⬜ | [precompile.md](precompile.md) | 3 |
| 5 | State traits | 🔵 | [state-traits.md](state-traits.md) | — |
| 6 | Custom type extensibility | 🔬 | [custom-types.md](custom-types.md) | 3, 7 or 8b |
| 7 | Full sharding | 🔬 | [sharding.md](sharding.md) | Design decision |
| 8 | Optimization | ⬜ | [optimization.md](optimization.md) | 2, 3, 5 |
| 9 | Advanced research | 🔬 | [research.md](research.md) | Various |

---

## Dependency Graph

```
Independent (start now)
├── 1. Code Quality ──────────────── nearly done (uncommitted)
├── 2. Commitment Traits ─────────── key infrastructure
├── 3. Composition ───────────────── enables 4, 6
│   └──→ 4. Precompile
│   └──→ 6. Custom Types (+ design decision on bus width)
├── 5. State Traits ──────────────── independent
└── 7. Full Sharding ─────────────── research, design decision needed

After 2 + 3 + 5 complete:
└──→ 8. Optimization

Future:
└──→ 9. Research (depends on 8 for D1, independent for D4)
```

## Recommended Order

1. **1** Code quality — finish and commit (nearly done)
2. **2 + 3 + 5** — independent, can run in parallel
3. **4** Precompile — after 3 (needs BusId)
4. **6** Custom types — after design decision (depends on 7 vs 8b)
5. **7** Sharding — design research, then implementation
6. **8** Optimization — after extensibility framework (2+3+5)
7. **9** Research — long-term
