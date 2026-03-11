# Lessons Learned

> Patterns and corrections captured during development.
> Updated after every user correction to prevent recurrence.

## Documentation

- **Never duplicate between docs/ and tasks/**: docs = timeless design, tasks = time-bound work
- **Single source of truth**: If information exists in docs, tasks should reference it, not copy it
- **No phase numbers in docs/**: Use descriptive names. Phases are a planning concept, not architecture.

## Code Navigation

- **Use LSP over Grep** for symbol-based queries (definitions, references, implementations)
- **Use Grep for text-based queries** (string literals, comments, config values, TODOs)

## Technical Judgment

- **SubAir trait (OpenVM pattern)**: The column struct vs Air struct split (using GATs) solves the `AB::Var` type parameter issue. Previous incorrect conclusion: "Rust's type system prevents it." Actual answer: use a separate zero-sized Air type. However, the trait is only worthwhile at scale (dozens of composable sub-circuits). For <10 gadgets, free functions are cleaner.

## Architecture

- **No separate core/custom pipelines**: Core types are "pre-registered instances", not special-cased. One unified pipeline for everything.
- **Full sharding IS the base architecture**: Not an optimization or future goal — it's the fundamental proof structure. Three tiers: Execution (1, global), Column proofs (C, parallel), Root proof (1, lightweight). All API design, extensibility traits, and optimization work must be built on the sharded model, not retrofitted onto global.
- **Architectural decisions before API design**: Design extensibility APIs (commitment traits, composition framework, state traits) on the target architecture (sharded), not the interim one (global). Otherwise APIs need redesign after migration.
- **Full sharding motivation is prover time, not custom types**: Custom type support is a side benefit. The primary driver is column width reduction + parallelism.
