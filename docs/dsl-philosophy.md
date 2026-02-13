# Tabula DSL — Design Philosophy

> **Status**: Draft v0.1
> **Scope**: Goals, principles, and design philosophy for the Tabula DSL. This document governs *why* and *how* the language is shaped — not the grammar spec itself.
> **Prerequisites**: [summary.md](./summary.md), [architecture.md](./architecture.md)

---

## 1. Why a DSL

Tabula programs are currently defined as JSON arrays of IR instructions:

```json
{"Read": {"dst": 0, "table": 1, "row": {"Param": 0}, "col": 0}},
{"Assert": {"predicate": {"Gte": [{"Slot": 0}, {"Param": 2}]}}},
{"Sub": {"dst": 2, "lhs": {"Slot": 0}, "rhs": {"Param": 2}}}
```

This is the machine's language, not the developer's. Slot indices, numeric table/column IDs, and explicit instruction sequencing are necessary for the executor and prover, but they are a liability for anyone *writing* programs.

The DSL exists to close this gap: let developers express intent in a human-readable form, and let a compiler handle the mechanical translation to IR.

**The DSL is a skin over the IR, not a replacement for it.** The IR remains the source of truth for execution and proving. The DSL compiles to it.

---

## 2. Core Goals

### G1. Correctness Over Convenience

The primary purpose of Tabula is **provable state transitions**. The DSL must never allow the developer to express something that cannot be deterministically executed and proven. If a convenience feature would compromise this property, it is rejected.

### G2. Predictable Compilation

A developer reading Tabula source should be able to mentally trace what IR instructions will be emitted. No hidden reads, no implicit state mutations, no surprising instruction generation. The compilation model is *transparent*.

### G3. Minimal Viable Surface

The language should be as small as possible while covering the full IR instruction set. Every keyword, operator, and syntactic form must justify its existence by mapping to a concrete IR concept. Features not needed today are not added speculatively.

### G4. Honest Abstraction

The DSL must faithfully represent the underlying execution model. Tabula is not SQL (no set operations), not Solidity (no control flow), not a general-purpose language (no loops, no function calls). The syntax should make these constraints obvious rather than hiding them behind familiar-looking constructs that behave differently than expected.

---

## 3. Language Design Principles

### L1. The Language IS the Constraint

Tabula's execution model is intentionally restricted: linear instruction sequences, no branching, no loops, no recursion. These are not limitations to work around — they are **design properties** that enable ZK proving.

The DSL enforces these by omission: there is no `if`, no `while`, no `fn`. If a construct cannot be proven in the target constraint system, it is not expressible. The type system and grammar make *un-provable programs un-writeable*.

### L2. Cell Addressing is First-Class

Tabula state is `(Table, Row, Column) → Value`. This is the fundamental access pattern, and the syntax must make it explicit:

```
let bal = balances[row].balance;
```

This reads: "from table `balances`, at row `row`, read column `balance`." The developer always knows *which table*, *which row*, *which column*. There is no query planner, no implicit scan, no WHERE clause. One expression = one cell.

### L3. One Binding, One Slot

Every `let` binding creates an **immutable** local variable that maps 1:1 to an IR slot. No reassignment, no shadowing. The slot is assigned once and read zero or more times.

```
let x = a + b;   // slot N assigned here
let y = x * c;   // slot N read here, slot N+1 assigned
```

This mirrors the IR's register machine exactly. The developer's mental model of variable liveness matches the machine's.

**Why immutable?** Mutable variables would require the compiler to track which version of a variable is "current" and potentially introduce SSA-like renaming. Immutable bindings keep the slot model trivially transparent.

### L4. Explicit State Mutation

Reads and writes are syntactically distinct. A read is a `let` binding; a write is an assignment to a cell:

```
let bal = accounts[id].balance;       // READ: state → local
accounts[id].balance = bal + amount;  // WRITE: local → state
```

There is no combined "update" operation. The developer explicitly reads, computes, and writes back. This maps directly to the IR's separate `Read` and `Write` instructions, and makes the read-set and write-set visible at the source level.

### L5. Assert as the Only Control Mechanism

The only way to conditionally affect execution is `assert`. If the predicate is false, the entire transaction is rolled back. There is no "else" branch.

```
assert sender_bal >= amount;
```

This is deliberate: branching would require the prover to handle multiple execution paths. Linear execution with assert-or-abort is the simplest model that is both useful and provable.

### L6. Types Flow Forward

Types are declared at boundaries (parameters, table schemas) and inferred everywhere else:

- Transaction parameters have explicit type annotations
- Table columns have types from the schema definition
- Local bindings (`let`) infer their type from the right-hand side
- Arithmetic preserves the operand type (u64 + u64 = u64)
- Hash always produces bytes32

There are no type annotations on local bindings. If the compiler cannot infer the type, it is an error — not a prompt for the developer to add annotations.

### L7. No Implicit Conversions

`u64` and `i64` are different types. They cannot be mixed in arithmetic. There is no widening, no narrowing, no coercion. If the developer wants a conversion, it must be explicit (future: cast operators). This prevents a class of subtle bugs where signed/unsigned confusion leads to incorrect state transitions.

---

## 4. Execution Model Alignment

### E1. Linear Execution, Linear Source

The source reads top to bottom. Execution proceeds top to bottom. There is no jumping, no returning, no calling. The order of statements in the source is the order of IR instructions emitted.

```
// source order = execution order = IR instruction order
let a = table1[r].x;      // instruction 0: Read
let b = a + param;         // instruction 1: Add
assert b > 0;              // instruction 2: Assert
table1[r].x = b;           // instruction 3: Write
```

### E2. Transaction as the Unit of Composition

The `tx` declaration is the only top-level executable construct. There are no functions, no modules, no imports between tx types. Each tx type is self-contained: its parameters, its reads, its computations, its writes.

This matches the IR's `TxTypeDef`: one name, one parameter schema, one instruction body.

**Why no functions?** Function calls would require either inlining (duplicating IR) or a call/return mechanism in the IR (not present, not planned for v1). Inlining could be added as a compiler feature later, but the language should not promise something the IR cannot support.

### E3. Schema Declarations are Metadata, Not Executable

`table` declarations define schemas — they do not create state. State exists independently (loaded from snapshot). Schemas tell the compiler what tables and columns exist, their types, and their IDs. They are used for:

- Name resolution (`balances` → `TableId(1)`)
- Type checking (column `balance` is `u64`)
- Error messages ("column `foo` does not exist in table `bar`")

### E4. Static Tables are Visually Distinct

Static tables (used by `Lookup` instruction) are fixed, read-only datasets. They are visually distinguished from mutable state tables to prevent confusion:

```
let val = @ranges[key].value;    // static table lookup
let bal = accounts[id].balance;  // mutable state read
```

The `@` prefix (or similar marker) signals: "this does not go through the overlay, this is a fixed table proven via lookup arguments."

### E5. DivMod Produces Two Values

The IR's `DivMod` instruction produces both quotient and remainder. The DSL must reflect this:

```
let (q, r) = divmod(a, b);   // both slots assigned
```

For convenience, `/` and `%` are also supported and compile to `DivMod` with the unused output assigned to a dead slot:

```
let q = a / b;    // DivMod, remainder discarded
let r = a % b;    // DivMod, quotient discarded
```

The compiler may optimize adjacent `/` and `%` on the same operands into a single `DivMod` instruction.

---

## 5. Compiler Design Principles

### C1. Compilation is Deterministic

Same source → same IR. Always. No randomness, no environment-dependent behavior, no optimization passes that may or may not fire. The compiler is a pure function from source text to IR.

### C2. Single-Pass is the Goal

The DSL is simple enough that a single forward pass over the AST should suffice for lowering. Name resolution and type inference flow forward (schemas and parameters are declared before use). Slot allocation is a monotonically increasing counter.

If a feature would require multi-pass compilation (e.g., forward references, mutual recursion), that is a strong signal the feature does not belong in the language.

### C3. The Compiler is a Thin Layer

The compiler does four things:

1. **Parse**: source text → AST
2. **Resolve**: names → numeric IDs (TableId, ColId, Slot)
3. **Type-check**: verify type consistency using schemas and parameter declarations
4. **Lower**: AST → `Vec<Instruction>` (IR)

There is no optimization pass. The IR that comes out of lowering is final. Optimization belongs to future work (dead slot elimination, DivMod fusion), and even then it must be deterministic and optional.

### C4. Errors Reference Source Locations

When the compiler rejects a program, the error message points to the source file, line, and column — not to IR instruction indices. The developer should never need to understand the IR to fix a compilation error.

```
error[T003]: type mismatch in assignment
  --> transfer.tab:8:5
   |
8  |     accounts[id].balance = flag;
   |     ^^^^^^^^^^^^^^^^^^^^^^^^ expected u64, found bool
```

### C5. Output is Standard IR

The compiler output is exactly `Vec<TableSchema>` + `Vec<TxTypeDef>` — the same types the executor already consumes. There is no intermediate format, no special "compiled" representation. The DSL compiler is just another way to produce what JSON currently produces.

This means the compiler has **zero impact** on the executor, commitment, or proof layers. It is a pure front-end concern.

### C6. The Compiler Never Guesses

If the source is ambiguous, the compiler rejects it. It does not pick a "reasonable default." Examples:

- Mixed numeric types without explicit cast → error (not implicit widening)
- Reference to undefined table or column → error (not a runtime failure)
- Slot used after scope ends → error (not silent reuse)
- Duplicate binding name → error (not shadowing)

---

## 6. What the DSL is NOT

### Not SQL

SQL is **set-based**: `SELECT * FROM t WHERE condition` scans rows and returns a result set. Tabula is **cell-based**: one read = one cell, addressed by `(table, row, column)`. The DSL does not use `SELECT`, `FROM`, `WHERE`, `JOIN`, `GROUP BY`, or any SQL keyword. Using SQL syntax would set false expectations about query capabilities.

### Not a Smart Contract Language

Solidity and Move have control flow (if/else, loops), function calls, inheritance/modules, error handling (try/catch, abort codes), gas metering, and reentrancy concerns. Tabula has none of these. The DSL does not borrow their syntax to avoid implying capabilities that don't exist.

### Not a General-Purpose Language

There are no functions, no closures, no generics, no trait impls, no modules, no standard library. The DSL is a **domain-specific notation** for expressing read-compute-assert-write sequences over typed table cells. It is closer to a configuration language with arithmetic than to a programming language.

### Not an Abstraction Layer

The DSL does not hide the execution model — it *exposes* it with better names. A developer who understands the DSL understands the IR. There is no conceptual gap to bridge, no "compilation magic" to reason about.

---

## 7. Design Tensions and Resolutions

### T1. Familiarity vs. Honesty

**Tension**: Developers know Rust/TypeScript/SQL. Familiar syntax lowers the learning curve. But familiar syntax carries semantic expectations that may not hold.

**Resolution**: Borrow *lexical* familiarity (curly braces, `let`, `:` for type annotations) from Rust, but *invent* syntax where Tabula's semantics diverge (`table[row].col` for cell access, `assert` without `else`, `@table` for static lookups). The language should feel "Rust-like" at a glance but clearly be its own thing within five minutes of reading.

### T2. Expressiveness vs. Provability

**Tension**: Developers want `if/else`, loops, and functions. The proof system requires linear, deterministic execution.

**Resolution**: The v1 DSL has *no* control flow. This is not an omission — it is a feature. The language's job is to make straight-line read-compute-assert-write *ergonomic*, not to approximate a general-purpose language. If conditional logic is needed in the future, it will be designed from the proof system up (e.g., conditional writes that execute both branches and select), not from the language down.

### T3. Conciseness vs. Transparency

**Tension**: Complex expressions (`a + b * c - d`) are concise but hide intermediate slots. Explicit slot-by-slot statements are transparent but verbose.

**Resolution**: Allow compound expressions. The compiler emits temporaries, but the developer doesn't name them. This is acceptable because (a) the IR instruction count is predictable from the expression structure, and (b) no hidden *state access* occurs — only arithmetic. Hidden arithmetic temporaries are fine; hidden READs are not.

### T4. Single File vs. Multi-File

**Tension**: Small programs fit one file. Large programs with many tables and tx types may benefit from splitting.

**Resolution**: v1 is single-file. One `.tab` file = one program. Multi-file support (imports, modules) is deferred. When/if added, it must respect the "no cross-tx-type references" constraint — imports would only share schema declarations, not logic.

---

## 8. Relationship to the IR

The DSL is a **lossless projection** of the IR's capabilities. Every IR instruction is expressible in the DSL, and every DSL construct compiles to known IR instructions.

| DSL Construct | IR Instruction(s) | Notes |
|---|---|---|
| `let x = table[row].col` | `Read` | One read, one slot |
| `table[row].col = expr` | `Write` | One write |
| `let x = @static[key].col` | `Lookup` | Static table access |
| `let x = a + b` | `Add` | Also `-` → `Sub`, `*` → `Mul` |
| `let (q, r) = divmod(a, b)` | `DivMod` | Two slots assigned |
| `let x = a / b` | `DivMod` | Remainder slot unused |
| `let x = a % b` | `DivMod` | Quotient slot unused |
| `assert <predicate>` | `Assert` | Comparison operators → Predicate variants |
| `let h = hash(a, b, ...)` | `Hash` | Built-in, always bytes32 |
| `emit "topic" (a, b, ...)` | `Emit` | Event emission |
| `a + b * c` | `Mul` then `Add` | Compiler emits temporaries |

There is no IR instruction without a DSL form, and no DSL form without a corresponding IR instruction sequence.

---

## 9. Future Considerations (Explicitly Deferred)

These features are *not* in scope for v1 but are acknowledged as potential future work. The v1 design should not *preclude* them.

| Feature | Why Deferred | Constraint on v1 Design |
|---|---|---|
| Conditional writes (`if/else`) | Requires proof system support for branching | Don't use `if` as a keyword for anything else |
| Bounded loops (`for i in 0..N`) | Must be compile-time unrollable for ZK | Don't use `for`/`while` as keywords |
| Inline functions / macros | Requires inlining pass; unclear if needed | Don't create syntax that looks like function definition |
| Multi-file imports | Needs module resolution rules | Keep schema declarations self-contained |
| Custom types / structs | IR operates on primitive values only | Don't add struct syntax |
| String values | Not in `ValueType` | Don't add string literals |
| Explicit casts (`x as u64`) | Needs IR support for cast instructions | Reserve `as` keyword |

---

## 10. Success Criteria

The DSL design is successful if:

1. **A developer can write a token transfer program in under 5 minutes** without reading the IR spec.
2. **The compiled IR is identical** to what an expert would write by hand.
3. **Compilation errors are actionable** — they tell the developer what to fix, not what went wrong inside the compiler.
4. **The language can be fully explained in a single page** of reference documentation.
5. **No developer is surprised** by what their code does at runtime, because the source makes the execution model visible.
