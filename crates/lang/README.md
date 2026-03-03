# tabula-lang

DSL compiler for the Tabula kernel.

## Role

Compiles `.tab` source files into Tabula IR (`Vec<TableSchema>` +
`Vec<TxTypeDef>`) via a hand-rolled lexer, recursive-descent parser,
and single-pass lowerer with type checking and slot allocation.

Zero parser-generator dependencies. Depends on `tabula-core` and `tabula-ir`.

## Language Philosophy

**The language IS the constraint.** No loops, no recursion, no branches.
These are not missing features — they are design properties that enable
ZK proving. If a construct cannot be proven, it is not expressible.

**One binding, one slot.** Every `let` creates an immutable variable that
maps 1:1 to an IR SSA slot. No reassignment, no shadowing. The
developer's mental model of variable liveness matches the machine's.

**Cell addressing is first-class.** `table[row].col` — the developer
always knows which table, which row, which column. No query planner,
no implicit scan. One expression = one cell.

**Explicit state mutation.** Reads are `let` bindings; writes are
assignments (`table[row].col = expr`). You can always tell by looking
at a line whether it reads or writes state.

See [`docs/research/dsl-philosophy.md`](../../docs/research/dsl-philosophy.md) for full design rationale.
