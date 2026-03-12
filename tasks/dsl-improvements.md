# Goal 11: DSL Improvements

> Status: ⬜ Blocked on Goal 7 (Precompile adds IR instructions that DSL must support)
> Design: [docs/design/dsl-language-design.md](../docs/design/dsl-language-design.md)
> Depends: Goal 7 (Precompile framework)
> Crate: `lang` (~1,600 LOC), touches `ir` for 2 items only

## Goal

Bring the DSL from prototype to production quality. Fix soundness gaps, add essential sugar, improve diagnostics, and build the module system. All improvements except 2 are pure compiler changes — no IR or ExecutionChip modifications.

## Design Constraint

**IR changes are minimized.** Of 25+ improvements identified, only 2 touch the IR:
- `assert` with message (optional string field)
- Bitwise operators (new `Instruction::Bitwise`)

Everything else lowers to existing instructions (`Select`, `Read`, `Write`, `Arith`, etc.).

## Tier 1: Foundation (Must-Have)

> Soundness + basic usability. Estimated ~2 weeks.

### D1: Eliminate type inference fallback ✅→⬜

The lowering pass defaults to `U64` when type inference fails (`lower/expr.rs` line 23, 77). This is a **soundness bug** — unknown types should produce compile errors.

- [ ] Replace all `unwrap_or(ValueType::U64)` with error propagation
- [ ] Emit `ErrorKind::TypeInferenceFailed` when type cannot be determined
- [ ] Test: `let x = ambiguous_expr` → compile error (not silent U64)

### D2: Type annotations on `let` bindings ⬜

Users cannot disambiguate types. `let x = 5` is always U64 even when I64 is intended.

- [ ] Add optional `TypeName` field to `StmtKind::Let`
- [ ] Parser: `let name : type = expr`
- [ ] Lowering: use annotation to guide inference, validate RHS matches
- [ ] Test: `let x: i64 = 5` produces `Value::I64(5)`

### D3: `const` declarations ⬜

No way to name constants. Magic numbers scattered across tx bodies.

- [ ] Add `Token::Const` keyword
- [ ] New top-level declaration: `const NAME: type = literal`
- [ ] Constants inlined at lowering time (no IR change)
- [ ] Scoped to module — available across all tx types in the file
- [ ] Test: `const MAX: u64 = 1000; assert x <= MAX`

### D4: Assert with message ⬜

Assert failures produce opaque `"Slot(4)"` errors. No debugging context.

- [ ] Syntax: `assert condition, "message string"`
- [ ] Add optional `message: Option<String>` to `Instruction::Assert`
- [ ] Executor includes message in `TabulaError::AssertionFailed`
- [ ] IR change: backward-compatible (existing Assert has `message: None`)
- [ ] Test: assertion failure includes user message

### D5: Human-friendly error messages ⬜

Parser errors show `expected RBrace, found Ident("x")` instead of `expected '}', found identifier 'x'`.

- [ ] Implement `Display` for `Token` with human-readable formatting
- [ ] Update all `format!("expected {:?}")` to use `Display`
- [ ] Test: error message contains `'}'` not `RBrace`

## Tier 2: Productivity (Should-Have)

> Developer experience + essential sugar. Estimated ~8-10 weeks.

### Sugar — Zero IR Change

#### D6: `if/else` expression ⬜

`select(cond, a, b)` is verbose for nested conditions.

- [ ] New `ExprKind::IfElse { condition, then_expr, else_expr }`
- [ ] Reserve `if` and `else` as keywords
- [ ] Lowering: emit `Instruction::Select`
- [ ] Test: `let x = if a > b { a } else { b }`

#### D7: Compound assignment operators ⬜

`table[row].col = table[row].col + amount` is the most common pattern.

- [ ] Syntax: `+=`, `-=`, `*=`
- [ ] New `StmtKind::CompoundAssign { table, row, col, op, value }`
- [ ] Lowering: Read + Arith + Write (3 instructions)
- [ ] Handle is_null from intermediate Read
- [ ] Test: `accounts[id].balance -= amount`

#### D8: `??` null-coalescing operator ⬜

`let tmp = table[row].col; let val = select(tmp == null, default, tmp)` is 2 lines for a common pattern.

- [ ] Syntax: `expr ?? default`
- [ ] Lowering: Read + Select(is_null, default, value)
- [ ] Only valid on cell-read expressions
- [ ] Test: `let balance = accounts[id].balance ?? 0`

#### D9: `min` / `max` builtins ⬜

- [ ] `min(a, b)` → `select(a < b, a, b)`
- [ ] `max(a, b)` → `select(a > b, a, b)`
- [ ] Test: `let capped = min(amount, balance)`

#### D10: Multi-column write block ⬜

3 separate writes for 3 columns of the same row is verbose and error-prone.

- [ ] Syntax: `table[row] { col1 = expr1, col2 = expr2, ... }`
- [ ] New `StmtKind::MultiAssign`
- [ ] Lowering: N × `Write` instructions
- [ ] Test: `orders[key] { price = p, qty = q, status = 1 }`

#### D11: `repeat N as i { ... }` bounded iteration ⬜

Manual unrolling for Merkle proofs, multi-field operations.

- [ ] Syntax: `repeat <literal> as <ident> { body }`
- [ ] New `StmtKind::Repeat { count, var, body }`
- [ ] Lowering: copy body N times, replacing `var` with constant `i`
- [ ] Bound N to prevent abuse (e.g., N ≤ 256)
- [ ] Test: `repeat 4 as i { hash = hash(hash, data[i]) }`

### Compiler Improvements

#### D12: Levenshtein suggestions ⬜

Typos produce bare "undefined X" errors.

- [ ] On `ErrorKind::UndefinedColumn` / `UndefinedVariable`, compute edit distance to known names
- [ ] Suggest closest match if distance ≤ 2
- [ ] Test: `"undefined 'balace' — did you mean 'balance'?"`

#### D13: Unused binding warnings ⬜

No warning system for dead code.

- [ ] After lowering, scan for `let` bindings never referenced
- [ ] Emit warnings (non-fatal, separate from errors)
- [ ] Test: `let unused = 5` → warning

#### D14: Compile-time constant folding ⬜

Literal arithmetic is not optimized. `let x = 3 + 5` generates Add instruction.

- [ ] When both operands are literals, compute at compile time
- [ ] Report overflow on literal arithmetic as compile error
- [ ] Depends: D3 (`const` values are candidates for folding)
- [ ] Test: `let x = 3 + 5` → single literal 8, no Arith instruction

#### D15: Null dereference lint ⬜

No warning when cell-read values used without null check.

- [ ] Track which ReadSlot bindings have `is_null` checked by an assert
- [ ] Warn if value used without prior null guard
- [ ] Lint (non-fatal) — can be suppressed
- [ ] Test: `let x = table[row].col; let y = x + 1` → warning

### Module System

#### D16: Multi-file `import` ⬜

Single-file programs only. No sharing of table definitions or constants.

- [ ] Syntax: `import "path.tab"`
- [ ] Compiler resolves imports, merges `TableSchema` + `TxTypeDef` namespaces
- [ ] Duplicate table/tx name detection across files
- [ ] Circular import detection
- [ ] Test: shared table definition imported by two tx files

#### D17: Inline functions ⬜

No code reuse. Balance check logic duplicated across tx types.

- [ ] Syntax: `fn name(params) { body }`
- [ ] Functions are always inlined at call sites (no runtime call stack)
- [ ] Alpha-renaming of bindings to avoid slot collision
- [ ] Functions share the caller's table/column scope
- [ ] Test: `fn check_bal(key: u64, amt: u64) { ... }; check_bal(from, amount)`

### IR-Changing Feature

#### D18: Bitwise operators ⬜

No `&`, `|`, `^`, `<<`, `>>`. Needed for flag manipulation, nonce computation, fixed-point shift.

- [ ] New tokens: `Amp`, `Pipe`, `Caret`, `LtLt`, `GtGt`
- [ ] New `Instruction::Bitwise { op, dst, lhs, rhs }` (or extend `ArithOp`)
- [ ] Executor implementation (U64 only)
- [ ] ExecutionChip: new constraint rows for bitwise ops
- [ ] Witness trace generation for bitwise
- [ ] Test: `let flags = old_flags | (1 << position)`

## Tier 3: Nice-to-Have

> Polish + tooling. Estimated ~4-5 weeks.

### D19: `abs(x)` builtin ⬜
- `select(x >= 0, x, -x)` sugar for I64

### D20: Row variable binding ⬜
- `let row = table[key]` → `row.col1`, `row.col2`
- Defers column read to field-access sites

### D21: Named emit fields ⬜
- `emit "transfer" { from, to, amount }` with self-documenting field names

### D22: `debug_assert` ⬜
- Stripped before proving, kept for executor-level testing

### D23: I64 literal suffix ⬜
- `5i64` syntax for explicit signed literals

### D24: LSP server ⬜
- `tabula-lsp` crate for IDE support (autocomplete, hover, diagnostics)
- Reuses lexer/parser/lowering infrastructure

## Explicitly Not Planned

| Feature | Reason |
|---------|--------|
| Runtime loops | Straight-line execution is fundamental to AIR tractability |
| Closures / lambdas | No higher-order functions in ZK model |
| Generics | 4 types only — parametric polymorphism has no value |
| Traits / typeclasses | No user-defined types |
| Pattern matching | No sum types / enums |
| Refinement types | Complexity disproportionate to value |
| Proc-macro DSL rewrite | Custom syntax is cleaner; invest in LSP instead |

## Completion Criteria

- [ ] Tier 1 all items passing tests
- [ ] Tier 2 sugar items: each compiles to correct IR (verified via lowering tests)
- [ ] Tier 2 diagnostics: error/warning output tested
- [ ] Zero existing test regressions
- [ ] All `.tab` test files updated for new syntax where applicable

## Verification

```bash
cargo test -p tabula-lang
cargo test -p tabula-ir
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets
```
