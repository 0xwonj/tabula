# DSL Language Design

> Tabula's domain-specific language for ZK-provable state machine transactions.

## Overview

The Tabula DSL compiles `.tab` files into a flat IR (15 instruction types, straight-line execution). The compilation pipeline is: **lex → parse → lower → IR**. The language targets financial state machine transactions — short, bounded programs that read/write table-based state.

The DSL is intentionally minimal. Expressiveness comes from three orthogonal mechanisms, not from control flow:

| Mechanism | Covers | Example |
|-----------|--------|---------|
| **DSL sugar** (compile-time) | Bounded, static iteration | `repeat 32 { hash = @poseidon(hash, path[i]) }` |
| **Batch decomposition** (runtime) | Data-dependent iteration | N × `fill_order` txs in one batch |
| **PropertyRead** (declarative) | Aggregation, extrema, existence | `property_read(orders, Minimum, col: price)` |

## Design Principles

### Straight-line execution

Every transaction compiles to a fixed-length instruction sequence. No loops, branches, or dynamic dispatch at the IR level. This guarantees:

- Fixed-width execution trace (278 columns in ExecutionChip)
- One instruction = one trace row — no padding, no variable-length traces
- Simple AIR constraints — no program counter, no branch condition logic

### DSL sugar is zero-cost

All syntactic conveniences compile to existing IR instructions. `if/else` → `Select`. `min(a,b)` → `Select(a<b, a, b)`. `repeat N` → N copies of the body. `??` → `Select(is_null, default, value)`. No new constraint rows, no wider traces.

### State access is the hard part

Financial applications have simple computation but complex state patterns. The DSL optimizes for state access ergonomics: cell read/write syntax, null handling, multi-column operations, typed schemas.

## Type System

### Core types

Four value types, intentionally closed:

| Type | Width | Field elements | Range |
|------|-------|----------------|-------|
| `u64` | 64-bit | 3 (limb decomposition) | 0 to 2^64-1 |
| `i64` | 64-bit | 3 (limb decomposition) | -2^63 to 2^63-1 |
| `bool` | 1-bit | 1 | true, false |
| `bytes32` | 256-bit | 8 | arbitrary 32 bytes |

### Type inference

Forward-flowing from literals, parameters, and column schemas. Type annotations on `let` bindings provide disambiguation when inference is insufficient.

```
let x = 5                    // inferred U64 from literal
let y: i64 = 5               // explicit I64 annotation
let z = accounts[id].balance  // inferred from column schema
```

### Null handling

Cell reads produce `(value, is_null)` pairs. Null is not a standalone value — it only arises from reading uninitialized cells. The `??` operator provides null-coalescing.

```
let balance = accounts[id].balance ?? 0    // 0 if null
assert accounts[id].owner != null          // null check
```

## Expression Language

### Operators (by precedence, low to high)

| Precedence | Operators | Notes |
|------------|-----------|-------|
| 1 | `\|\|` | Logical OR (no short-circuit — both sides evaluate) |
| 2 | `&&` | Logical AND |
| 3 | `==`, `!=` | Equality (all types) |
| 4 | `<`, `<=`, `>`, `>=` | Ordering (numeric types only) |
| 5 | `\|` | Bitwise OR |
| 6 | `^` | Bitwise XOR |
| 7 | `&` | Bitwise AND |
| 8 | `<<`, `>>` | Bitwise shift (U64 only) |
| 9 | `+`, `-` | Addition, subtraction |
| 10 | `*`, `/`, `%` | Multiplication, division, modulo |
| 11 | `!`, `-` (unary) | Logical NOT, negation |

### Built-in functions

| Function | Lowers to | Description |
|----------|-----------|-------------|
| `hash(args...)` | `Hash` instruction | Poseidon hash |
| `divmod(a, b)` | `DivMod` instruction | Returns `(quotient, remainder)` |
| `select(cond, a, b)` | `Select` instruction | Conditional value |
| `min(a, b)` | `Select(a < b, a, b)` | Minimum |
| `max(a, b)` | `Select(a > b, a, b)` | Maximum |
| `abs(x)` | `Select(x >= 0, x, -x)` | Absolute value (I64) |
| `@name(args...)` | `Precompile` instruction | Custom precompile call |

### Conditional expression

```
let result = if balance >= amount { balance - amount } else { 0 }
// Lowers to: Select instruction
```

## Statement Language

### Declarations

```
const MAX_PRICE: u64 = 1000000000        // Module-level constant (inlined)
let x = expr                              // Immutable binding (type inferred)
let y: i64 = expr                         // With type annotation
let (q, r) = divmod(a, b)                 // Destructuring (divmod only)
```

### State access

```
let val = table[row_key].column           // Cell read
table[row_key].column = expr              // Cell write
let val = @static_table[key].column       // Static table read

// Multi-column write block
orders[key] {
    price = new_price,
    quantity = new_qty,
    status = ACTIVE,
}

// Compound assignment
accounts[id].balance -= amount            // Read + Arith + Write
```

### Assertions

```
assert condition                          // Runtime constraint
assert balance >= amount, "insufficient balance"   // With message
```

### Events

```
emit "transfer" (from, to, amount)        // Positional args
```

### Bounded iteration

```
repeat 32 as i {
    hash = @poseidon(hash, path[i])       // Unrolled at compile time
}
```

## Module System

### File structure

```
import "common.tab"                       // Import shared definitions

const TICK_SIZE: u64 = 100

table orders {
    price: u64,
    quantity: u64,
    owner: bytes32,
    side: u64,
}

tx place_order(price: u64, qty: u64, owner: bytes32) {
    assert price % TICK_SIZE == 0, "price must be tick-aligned"
    orders[next_key].price = price
    orders[next_key].quantity = qty
    orders[next_key].owner = owner
}
```

### Inline functions

```
fn check_balance(account_key: u64, required: u64) {
    let balance = accounts[account_key].balance ?? 0
    assert balance >= required, "insufficient balance"
}

tx transfer(from: u64, to: u64, amount: u64) {
    check_balance(from, amount)           // Inlined at compile time
    accounts[from].balance -= amount
    accounts[to].balance += amount
}
```

Functions are **always inlined** — no runtime call stack. The function body is alpha-renamed and spliced at each call site.

## Safety Properties

### Checked arithmetic

All arithmetic operations use checked semantics. Overflow, underflow, and division by zero produce runtime errors (and proof failure).

### Compile-time safety

- **No type inference fallback**: Unknown types are compile errors, not silent U64 assumptions
- **Constant folding**: Literal expressions evaluated at compile time with overflow detection
- **Null dereference lint**: Warning when cell-read values are used without null guard
- **Unused binding warnings**: `let x = ...` where `x` is never read

### Soundness invariants

- Bindings are immutable (SSA) — no double-spend of values
- Overlay semantics handle read-after-write correctly within a tx
- All constraints are checked at proving time — runtime errors cause proof failure

## Non-goals

The following are intentionally excluded:

| Feature | Reason |
|---------|--------|
| Runtime loops | Straight-line execution keeps AIR tractable |
| Closures / lambdas | No higher-order functions in ZK straight-line model |
| Generics | 4 types only; parametric polymorphism has no value |
| Traits / typeclasses | No user-defined types |
| Pattern matching | No sum types / enums to match against |
| Refinement types | Runtime asserts suffice; implementation complexity disproportionate |
| Dynamic dispatch | Incompatible with fixed-width trace |

For computation that requires these features, use precompiles (custom AIR chips that implement arbitrary logic).

## Comparison with Related Languages

| Feature | Cairo | Noir | Move | Tabula |
|---------|-------|------|------|--------|
| Execution model | Full VM | Straight-line | Full VM | Straight-line |
| Types | Rich (felt, struct, enum) | Rich (struct, array) | Rich (struct, resource) | Minimal (4 types) |
| Control flow | Loops, branches | Bounded loops | Full | Batch decomposition |
| State model | Contract storage | Witness inputs | Global objects | Tables + columns |
| Module system | Full | Full | Full | Import + inline fn |
| Target | General ZK computation | General ZK circuits | Blockchain state | Financial state machines |

Tabula's DSL is narrower by design. The table-based state model, commitment schemes, and structural queries provide the domain-specific power that general ZK languages lack.
