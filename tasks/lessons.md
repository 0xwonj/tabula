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

## Architecture

- **No separate core/custom pipelines**: Core types are "pre-registered instances", not special-cased. One unified pipeline for everything.
- **Full sharding motivation is prover time, not custom types**: Custom type support is a side benefit. The primary driver is column width reduction + parallelism.
