# tabula-lang

DSL compiler for the Tabula kernel.

Compiles `.tab` source files into Tabula IR (`Vec<TableSchema>` + `Vec<TxTypeDef>`).

## Pipeline

```
Source (.tab)
    ↓
  Lexer     → Vec<Token>
    ↓
  Parser    → AST (Vec<Item>)
    ↓
  Lowerer   → CompiledProgram { schemas, tx_types }
```

**Entry point**: `tabula_lang::compile(source) -> Result<CompiledProgram, Vec<CompileError>>`

## Modules

| Module | Role |
|--------|------|
| `lexer` | Hand-rolled tokenizer, zero external dependencies |
| `parser` | Recursive descent + Pratt expression parsing |
| `lower` | AST → IR lowering with type checking and slot allocation |
| `ast` | AST node types |
| `token` | Token types and keywords |
| `span` | Source location tracking |
| `error` | `CompileError` with span information |

## Language Design

- No loops, no recursion, no branches (flat instruction sequence)
- Immutable bindings (`let x = ...`) map 1:1 to SSA slots
- Cell addressing: `table[row].col`
- Explicit Read/Write separation
- `assert` as the only control mechanism
- Types inferred from schema boundaries
- `select(cond, a, b)` for conditional values

See [`docs/dsl-philosophy.md`](../../docs/dsl-philosophy.md) for design rationale.

## Dependencies

Only `tabula-core` + `thiserror`. Zero parser generator dependencies.
