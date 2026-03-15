# Symbolic AIR Compilation: From DSL to Per-Tx-Type STARK Circuits

> Status: Research
> Date: 2026-03-13
> Extends: [execution-chip-evolution.md](../design/execution-chip-evolution.md) Level 4
> Related: [proof-optimization-architecture.md](../design/proof-optimization-architecture.md) §3,
>          [constraint-compilation.md](../design/constraint-compilation.md),
>          [extensibility-architecture.md](../design/extensibility-architecture.md) §3.6

---

## 1. Motivation

The ExecutionChip is a 278-column universal AIR that processes one instruction per row. For a simple `increment` transaction (Read + Add + Write = 3 instructions), this means 3 rows × 278 columns = 834 field elements. Of these, 176 columns per row exist solely for SSA slot management — carrying all 16 slot values forward and selecting which slot each operand reads from.

The fundamental inefficiency: **the prover re-derives data flow at proof time**, even though the compiler already knows the complete data flow at registration time.

[execution-chip-evolution.md](../design/execution-chip-evolution.md) describes a Level 0–4 evolution path. Level 4 (Template AIR) generates per-program circuits where each instruction occupies a fixed row with known constraints. This eliminates opcode dispatch and reduces column width to ~60–80.

This document explores taking Level 4 to its logical conclusion: **symbolic execution collapses the entire transaction into direct algebraic relations between state reads and writes, enabling a one-row-per-transaction model**.

---

## 2. Core Idea

### 2.1 From Instructions to Relations

A Tabula transaction is a pure function: it reads values from state, computes, and writes values to state. The intermediate steps (slot assignments, instruction sequencing) are implementation details that vanish when expressed mathematically.

Consider `increment`:

```
tx increment(id: u64) {
    let v = t[id].val
    t[id].val = v + 1
}
```

Compiled IR (3 instructions):
```
Read { dst_val: 0, dst_is_null: 1, table: 0, col: 0, row: Param(0) }
Arith { dst: 2, op: Add, lhs: Slot(0), rhs: Literal(U64(1)) }
Write { table: 0, col: 0, row: Param(0), src_val: Slot(2) }
```

Symbolic execution traces the data flow through slots:
```
slot[0] = R₀           (opaque: value from state)
slot[1] = is_null₀     (opaque: null flag from state)
slot[2] = R₀ + 1       (derived: algebraic expression over R₀)
write_value = slot[2] = R₀ + 1
```

The final relation: **write_value = read_value + 1**. This is a single degree-1 constraint. No slots, no selectors, no carry.

### 2.2 One Row Per Transaction

Since the entire tx reduces to a set of algebraic relations, all relations can be verified in a single trace row:

```
IncrementChip row: [is_real, tx_index, base_clk, param_id, read_val, read_null, carry]
                    1        +  1      + 1       + 3       + 3       + 1        + 1    = 11 columns
```

- `write_val` is not materialized as a column — it appears as the inline expression `read_val + 1` in the bus interaction
- The AIR constraint: `carry-chain arithmetic for (read_val + 1)`
- Bus interactions: `SEND READ_ACCESS(t=0, c=0, row=param_id, val=read_val, clk=base_clk)` and `SEND WRITE_ACCESS(t=0, c=0, row=param_id, val=read_val+1, clk=base_clk+1)`

**Current cost**: 3 rows × 278 cols = 834 field elements.
**Compiled cost**: 1 row × 11 cols = 11 field elements. **76x reduction.**

### 2.3 Comparison with Level 4

Level 4 (Template AIR from execution-chip-evolution.md) assigns one row per instruction, with per-row constraint specialization:

```
Level 4 transfer:
  Row 0: Read(s0, ...)    → key, old_val, old_null columns
  Row 1: Read(s2, ...)    → key, old_val, old_null columns
  Row 2: Sub(s4, s0, p2)  → constraint on val
  Row 3: Add(s5, s2, p2)  → constraint on val
  Row 4: Assert(Gte ...)  → ineq witness columns
  Row 5: Write(...)       → key, new_val, new_null
  Row 6: Write(...)       → key, new_val, new_null
  → 7 rows × ~70 cols = 490 field elements
```

Symbolic compilation collapses this:

```
Symbolic transfer:
  Row 0: [params, read_vals, cmp_witness, carry, access_meta]
  → 1 row × ~62 cols = 62 field elements
```

The difference: Level 4 eliminates opcode dispatch but retains the instruction-per-row model with transition constraints between rows. Symbolic compilation eliminates the instruction abstraction entirely, expressing the tx as a flat set of relations.

---

## 3. Symbolic Execution Engine

### 3.1 Symbolic Values

The engine tracks each SSA slot as a symbolic expression rather than a concrete value:

```
SymExpr:
  | Column(idx)          — trace column reference (opaque runtime value)
  | Const(field_element)  — compile-time constant
  | Add(SymExpr, SymExpr)
  | Sub(SymExpr, SymExpr)
  | Mul(SymExpr, SymExpr)

SymSlot:
  | Opaque(col_idx, value_type)   — value determined at runtime (Read result, Cmp result, etc.)
  | Derived(SymExpr)              — expressible as function of other columns
```

### 3.2 Execution Rules

For each IR instruction, the engine updates the symbolic slot map:

| Instruction | Symbolic Effect |
|------------|----------------|
| `Read { dst_val, dst_is_null, ... }` | `slot[dst_val] = Opaque(alloc_col(), ty)`, `slot[dst_is_null] = Opaque(alloc_col(), Bool)` |
| `Arith { dst, op: Add, lhs, rhs }` | `slot[dst] = Derived(resolve(lhs) + resolve(rhs))` |
| `Arith { dst, op: Sub, lhs, rhs }` | `slot[dst] = Derived(resolve(lhs) - resolve(rhs))` |
| `Arith { dst, op: Mul, lhs, rhs }` | `slot[dst] = Derived(resolve(lhs) * resolve(rhs))` |
| `Cmp { dst, ... }` | `slot[dst] = Opaque(alloc_col(), Bool)` (requires CmpWitness gadget) |
| `DivMod { dst_q, dst_r, ... }` | Both `Opaque` (requires DivModWitness gadget) |
| `Hash { dst, ... }` | `Opaque` (requires Poseidon permutation columns) |
| `Not { dst, src }` | `slot[dst] = Derived(Const(1) - resolve(src))` |
| `And { dst, lhs, rhs }` | `slot[dst] = Derived(resolve(lhs) * resolve(rhs))` |
| `Or { dst, lhs, rhs }` | `slot[dst] = Derived(resolve(lhs) + resolve(rhs) - resolve(lhs) * resolve(rhs))` |
| `Select { dst, cond, t, f }` | `slot[dst] = Derived(resolve(cond) * resolve(t) + (1 - resolve(cond)) * resolve(f))` |
| `Assert { cond }` | Emit constraint: `resolve(cond) = 1` |
| `Write { src_val, ... }` | Emit bus interaction with `resolve(src_val)` as value |

`resolve(expr)` maps `ValueExpr::Slot(s)` to `sym_slots[s]`, `ValueExpr::Param(i)` to `Column(param_col_i)`, and `ValueExpr::Literal(v)` to `Const(encode(v))`.

### 3.3 Output

The symbolic execution produces a `SymbolicResult`:

```
SymbolicResult {
    reads: Vec<AccessSpec>,          // (table, col, row_expr, value_columns, null_column)
    writes: Vec<WriteSpec>,          // (table, col, row_expr, value_sym_expr, null_sym_expr)
    asserts: Vec<SymExpr>,           // boolean expressions that must equal 1
    gadgets: Vec<GadgetRequirement>, // CmpWitness, DivModWitness, HashPerm, etc.
    opaque_columns: Vec<ColumnSpec>, // columns that need trace population
    derived_exprs: Vec<SymExpr>,     // expressions used in writes/asserts (for degree analysis)
}
```

---

## 4. Materialization and Degree Management

### 4.1 The Materialization Decision

Not every intermediate value needs a trace column. Values that are `Derived` can either:

- **Inline**: appear as an expression in constraints/bus interactions (no column allocated)
- **Materialize**: get a dedicated column with a constraint proving it equals the expression

The decision depends on two factors:

1. **Constraint degree**: inlining increases the degree of constraints that reference the expression
2. **Use count**: values referenced multiple times benefit from materialization (compute once, reference the column)

Rules:

| Value | Materialize? | Reason |
|-------|-------------|--------|
| Read result | Always | Opaque witness from prover |
| Param | Always | Transaction input |
| Hash/Precompile output | Always | Opaque (non-algebraic) |
| Cmp result | Always | Requires CmpWitness gadget |
| DivMod quotient/remainder | Always | Requires DivModWitness gadget |
| `read + literal` | Never | Degree 1, inline is free |
| `read - param` | Never | Degree 1, inline is free |
| `a * b` (both columns) | If degree budget allows | Degree 2 inline |
| `select(c, a, b)` | If used >1 time or in chain | Degree 2, chains escalate |

### 4.2 Degree Budget

The constraint degree determines the number of quotient chunks in FRI:

| Max degree | Quotient chunks | log_blowup needed |
|-----------|----------------|-------------------|
| 2 | 1 | 1 |
| 4 | 3 | 2 |
| 8 | 7 | 3 (current) |

The current universal ExecutionChip has max degree 6–9, requiring `log_blowup = 3`.

For compiled chips, the degree depends on the program:

- **Arithmetic-only tx** (Read, Add/Sub, Write): degree 1–2. Could use `log_blowup = 1`.
- **Comparison tx** (with CmpWitness): degree 3–4. Could use `log_blowup = 2`.
- **Complex tx** (nested Select chains): degree 4–6. Needs `log_blowup = 3`.

This is a per-chip optimization: simple tx types get faster proofs with smaller blowup, regardless of what other tx types exist in the system.

### 4.3 Automatic Degree Splitting

When a `Derived` expression exceeds the degree budget, the compiler inserts a materialization point:

```
Before splitting (degree 4):
  result = select(c1, select(c2, a, b), d)
         = c1 * (c2 * a + (1-c2) * b) + (1-c1) * d

After splitting (max degree 2):
  intermediate = c2 * a + (1-c2) * b     ← new column + degree-2 constraint
  result = c1 * intermediate + (1-c1) * d  ← degree-2 constraint
```

Algorithm: bottom-up traversal of the expression tree. At each node, if `node.degree > budget`, mark the highest-degree child for materialization. Repeat until all nodes are within budget.

---

## 5. Compiled Chip Structure

### 5.1 Data-Driven AIR (Interpreted)

Rather than generating Rust source code per tx type, the compiler produces a data structure that the prover interprets:

```rust
struct CompiledAir {
    chip_id: ChipId,
    name: String,
    width: usize,
    constraints: Vec<CNode>,              // algebraic constraints (assert_zero)
    interactions: Vec<CompiledInteraction>, // bus send/receive
    transition_constraints: Vec<CNode>,    // row-to-row (e.g., base_clk progression)
    max_constraint_degree: usize,
}

enum CNode {
    Col(usize),
    Const(KoalaBear),
    Add(Box<CNode>, Box<CNode>),
    Sub(Box<CNode>, Box<CNode>),
    Mul(Box<CNode>, Box<CNode>),
}
```

The `Air` trait implementation evaluates `CNode` trees over `AB::Expr`, building the same symbolic expressions that hand-written code would produce. Since p3's prover operates on symbolic types during quotient computation, there is no performance difference between interpreted and compiled constraint evaluation — the bottleneck is NTT and Merkle commitment, not constraint evaluation.

### 5.2 Clock Management

Each compiled chip row represents one transaction. A transaction may perform multiple state accesses, each needing a unique clock value for the memory consistency argument.

The clock offsets within a tx are compile-time constants (determined by the instruction order). Only the base clock varies at runtime:

```
TransferChip (4 accesses per tx):
  row.base_clk = C

  SEND READ_ACCESS(..., clk = C + 0)   // offset 0: first read
  SEND READ_ACCESS(..., clk = C + 1)   // offset 1: second read
  SEND WRITE_ACCESS(..., clk = C + 2)  // offset 2: first write
  SEND WRITE_ACCESS(..., clk = C + 3)  // offset 3: second write
```

Transition constraint: `next.base_clk = local.base_clk + NUM_ACCESSES` (a constant per chip).

Clock coordination across multiple compiled chips: the machine layer assigns non-overlapping clock ranges to each chip's trace.

### 5.3 Trace Population

A `CompiledPopulator` fills the trace from execution results:

```rust
struct CompiledPopulator {
    column_sources: Vec<ColumnSource>,
}

enum ColumnSource {
    Constant(KoalaBear),
    ReadValue { access_idx: usize, limb: usize },
    ReadNull { access_idx: usize },
    Param { param_idx: usize, limb: usize },
    GadgetWitness(GadgetField),
    BaseClock,
    TxIndex,
}
```

Given a `TxExecutionOutput` (the existing executor result), the populator extracts each column value by index. No `InstructionRecord`, no slot arrays, no one-hot selectors.

---

## 6. Concrete Examples

### 6.1 Increment: 1 Read, 1 Add, 1 Write

```
tx increment(id: u64) {
    let v = t[id].val
    t[id].val = v + 1
}
```

Symbolic result:
- `write_val = read_val + 1`

Compiled columns:

| Column | Width | Purpose |
|--------|-------|---------|
| is_real | 1 | Padding flag |
| tx_index | 1 | Transaction index in batch |
| base_clk | 1 | Starting clock |
| param_id | 3 | Transaction parameter (U64) |
| read_val | 3 | Value from state |
| read_null | 1 | Null flag |
| access_r | 6 | Row key (KeyRangeChecked) |
| carry | 1 | Addition carry |
| **Total** | **17** | |

Constraints:
1. Carry-chain addition: `read_val + 1 = write_val` (expressed inline in bus send)

Bus interactions:
1. `SEND READ_ACCESS(t=0, c=0, row=param_id, val=read_val, clk=base_clk)`
2. `SEND WRITE_ACCESS(t=0, c=0, row=param_id, val=read_val+1, clk=base_clk+1)`

**Current**: 3 rows × 278 = 834 FE. **Compiled**: 1 row × 17 = 17 FE. **49x reduction.**

### 6.2 Transfer: 2 Read, 1 Cmp, 1 Assert, 2 Write

```
tx transfer(from: u64, to: u64, amount: u64) {
    let bf = accounts[from].balance
    let bt = accounts[to].balance
    assert bf >= amount
    accounts[from].balance = bf - amount
    accounts[to].balance = bt + amount
}
```

Symbolic result:
- `write₀ = read₀ - param₂`
- `write₁ = read₁ + param₂`
- `assert: read₀ >= param₂`

Compiled columns:

| Column | Width | Purpose |
|--------|-------|---------|
| is_real | 1 | |
| tx_index | 1 | |
| base_clk | 1 | |
| param_from, param_to, param_amount | 9 | 3 params × 3 FE |
| read_bal_from | 3 | |
| read_null_from | 1 | |
| read_bal_to | 3 | |
| read_null_to | 1 | |
| cmp_witness | 27 | CmpWitness (read₀ >= param₂) |
| carry_sub | 1 | Subtraction borrow |
| carry_add | 1 | Addition carry |
| access_r × 2 | 12 | Two row keys |
| **Total** | **62** | |

Write values are inline expressions:
- `write₀ = read_bal_from - param_amount` (degree 1)
- `write₁ = read_bal_to + param_amount` (degree 1)

**Current**: 7 rows × 278 = 1946 FE. **Compiled**: 1 row × 62 = 62 FE. **31x reduction.**

### 6.3 Complex: 4 Read, 2 Cmp, 2 DivMod, 1 Hash, 4 Write

Compiled columns:

| Group | Columns |
|-------|---------|
| Control (is_real, tx_index, base_clk) | 3 |
| Params (5 params × 3 FE) | 15 |
| Reads (4 × (3 val + 1 null)) | 16 |
| CmpWitness × 2 | 54 |
| DivModWitness × 2 (no q_sel — direct columns) | 40 |
| Hash perm (16 input + 8 output) | 24 |
| Write values (4 × 3, those not expressible inline) | 12 |
| Carry columns (4 arithmetic ops) | 4 |
| Access row keys (4 × 6) | 24 |
| **Total** | **~192** |

**Current**: ~18 rows × 278 = 5004 FE. **Compiled**: 1 row × 192 = 192 FE. **26x reduction.**

Note: DivModWitness drops from 36 to ~20 columns because the `q_sel[MAX_SLOTS]` array (16 columns selecting which slot holds the quotient) is unnecessary — the quotient and remainder have dedicated columns.

---

## 7. Batching Strategy

### 7.1 Architecture

Each compiled chip handles all transactions of one type within a batch:

```
Batch: 500 transfers + 300 swaps + 94 misc (47 types × 2 each)

TransferChip:    500 rows × 62 cols   → 1 proof
SwapChip:        300 rows × 30 cols   → 1 proof
ExecutionChip:   94 txs × ~7 rows = 658 rows × 278 cols → 1 proof (generic fallback)
                                      ─────────
                                      3 execution proofs total
```

Column and root tiers receive bus messages from all execution chips identically. No changes needed downstream.

### 7.2 Threshold Rule

Each FRI proof has a fixed overhead independent of trace size (folding rounds, challenge sampling, proof-of-work). For very small traces (1–3 rows), this fixed overhead exceeds the savings from narrower columns.

Decision rule:

```
if count(tx_type in batch) >= THRESHOLD → use compiled chip
if count(tx_type in batch) < THRESHOLD  → use generic ExecutionChip
```

Estimated THRESHOLD: **4–8 transactions**. Below this, the FRI fixed cost of a separate proof exceeds the trace area savings. The exact value should be determined empirically.

### 7.3 Proof Count Impact

Current architecture: 1 execution + C column + 1 root = C + 2 proofs.

With compiled chips: T execution + C column + 1 root = T + C + 1 proofs, where T = number of compiled chip types used in the batch (typically 2–5).

For a typical application (3 dominant tx types + generic fallback): T = 4, adding 3 proofs. This is a modest increase for a 30x trace reduction.

---

## 8. Soundness

### 8.1 Threat Model

A malicious prover attempts to create a valid proof for an incorrect state transition. The compiled AIR must constrain every write value as a function of read values and parameters, leaving no free witnesses that could be exploited.

### 8.2 Verification Layers

**Layer 1 — Structural coverage (compile-time):**

The compiler verifies that the `CompiledAir` constrains all data paths:
- Every `Write` instruction's value is either a constrained column or an inline expression referencing only constrained columns
- Every `Assert` instruction's condition appears as a constraint (`expr = 1`)
- Every `Read` instruction produces a bus interaction (READ_ACCESS)
- Every witness column (opaque) participates in at least one constraint

**Layer 2 — Equivalence testing (test-time):**

For each tx type, verify that the compiled chip and the generic ExecutionChip produce identical bus messages for random inputs:

```
for random (params, state):
    compiled_bus_messages = run_compiled_chip(params, state)
    generic_bus_messages = run_generic_chip(params, state)
    assert_eq(compiled_bus_messages, generic_bus_messages)
```

This is the TemplateChip equivalence harness described in [extensibility-architecture.md](../design/extensibility-architecture.md) §3.6.

**Layer 3 — Formal argument:**

Each step of symbolic execution preserves the semantic equivalence between the source program and the generated constraints. The proof structure:

1. **Base case**: Read/Param produce `Opaque` columns whose values are constrained by bus interactions
2. **Inductive step**: each Derived expression correctly represents the IR instruction's semantics (Add, Sub, Mul, Not, And, Or, Select are algebraically equivalent by construction)
3. **Completeness**: every Write's value expression and every Assert's condition expression is included in the constraint set

---

## 9. Gadget Specialization

The generic ExecutionChip allocates all gadgets unconditionally (CmpWitness: 27 cols, DivModWitness: 36 cols, MulCarry: 5 cols, HashPerm: 24 cols = 92 cols total). A compiled chip allocates only the gadgets used by its tx type.

Further optimization: gadgets can be specialized to the specific operation used.

| Generic gadget | Columns | Specialized | Columns |
|---------------|---------|-------------|---------|
| CmpWitness (6 comparison variants) | 27 | GteWitness (1 variant, no sub-selectors) | ~15 |
| DivModWitness (with q_sel[MAX_SLOTS]) | 36 | DivModDirect (no slot selection) | ~20 |
| MulCarry (shared arith sub-selectors) | 5 | MulDirect (no sub-selector gating) | 5 |

The compiler knows which comparison operator (Eq/Ne/Lt/Lte/Gt/Gte) is used and generates the minimal witness structure.

---

## 10. Limitations and Boundaries

### 10.1 When Symbolic Compilation Is Not Beneficial

| Condition | Reason | Mitigation |
|-----------|--------|------------|
| Very complex tx (30+ instructions, 10+ accesses) | One-row becomes very wide (200+ cols); diminishing returns | Multi-row compiled chip (one row per access) |
| Hash-heavy tx (5+ hash operations) | 24 cols × N hashes dominates the width | Hash delegation to PoseidonChip via bus |
| Many distinct tx types (50+) in one batch | Proof count explosion | Generic fallback for rare types |
| Dynamic row key chains (slot→read→slot) | Symbolic expressions become deep | Materialization at intermediate points |

### 10.2 Break-Even Analysis

For a compiled chip to be beneficial, the trace area savings must exceed the FRI proof fixed overhead:

```
Savings = (generic_rows × generic_width) - (compiled_rows × compiled_width)
Cost = FRI_fixed_overhead

Beneficial when: Savings > Cost
```

With `generic_width = 278`, `compiled_width ≈ 60`, `generic_rows_per_tx ≈ 7`:

```
Per tx savings ≈ 7 × 278 - 1 × 60 = 1886 field elements
FRI_fixed_overhead ≈ 4000-8000 field elements (empirical estimate)
Break-even: ~3-5 transactions of the same type
```

---

## 11. Relationship to Existing Work

### 11.1 Within Tabula

- **Level 4 Template AIR** (execution-chip-evolution.md): Symbolic compilation extends Level 4 from one-row-per-instruction to one-row-per-tx via symbolic slot elimination. The key conceptual step: replacing "fixed rows with known constraints" with "algebraic relations derived from symbolic execution."

- **Constraint CSE** (constraint-compilation.md): CSE optimizes constraint evaluation within a given AIR. Symbolic compilation changes which AIR is being evaluated. The two compose: CSE can further optimize the compiled chip's constraint DAG.

- **ChipExtension** (extensibility-architecture.md): Compiled chips register via the existing `ChipExtension` trait. The `CompiledAir` implements `AnyRap`, and the `CompiledPopulator` implements `DynChip`/`TraceContributor`.

### 11.2 External

| System | Approach | Per-program? | Intermediate slots? |
|--------|----------|-------------|-------------------|
| Circom → R1CS | Arithmetic circuit compilation | Yes | No (wire-based) |
| Noir → Plonk | High-level → gate constraints | Yes | No (copy constraints) |
| Cairo → STARK | Fixed CPU AIR | No | Yes (registers) |
| RISC Zero → STARK | Fixed RISC-V AIR | No | Yes (registers) |
| SP1 → STARK | Fixed RISC-V AIR | No | Yes (registers) |
| **Tabula → STARK** | **DSL → per-tx-type AIR** | **Yes** | **No (symbolic elimination)** |

The combination of per-program STARK AIR compilation with LogUp bus integration is novel. Existing per-program systems (Circom, Noir) target Plonk/R1CS with copy constraints; existing STARK systems (Cairo, SP1) use fixed universal AIRs. Tabula's position — a domain-specific DSL with linear execution and known-at-registration programs — uniquely enables STARK AIR compilation without the machinery of either approach.

### 11.3 Why General-Purpose VMs Cannot Do This

General-purpose VMs execute arbitrary programs with loops, branches, recursion, and dynamic dispatch. The instruction sequence is unknown until runtime, so:

1. Symbolic execution may not terminate (loops)
2. Data flow depends on runtime values (branches)
3. The circuit must handle any instruction at any position (universality)

Tabula programs have none of these properties: no loops, no branches (Select is algebraic), no recursion, linear execution, and the program is fixed at registration. This is the structural invariant that makes symbolic compilation possible.

---

## 12. Open Research Questions

**RQ1: Optimal materialization strategy.** Given a symbolic DAG with degree constraints and a column budget, finding the minimum-width materialization is a combinatorial optimization problem. For small DAGs (< 50 nodes, typical for Tabula programs), exact solvers are feasible. For larger programs, what heuristics perform well?

**RQ2: Multi-row compiled chips.** For complex transactions, a single wide row may not be optimal. An access-based layout (one row per state access, with inter-row constraints for computation) could be narrower while maintaining the symbolic approach. What is the optimal row/column tradeoff for different program profiles?

**RQ3: Gadget template generation.** Can the compiler automatically derive specialized gadget variants (e.g., GteWitness from CmpWitness) by pruning unused cases? This is constraint dead-code elimination at the gadget level.

**RQ4: Per-chip FRI parameters.** Since each compiled chip has a different max constraint degree, each could use a different `log_blowup`. Simple tx types (degree 2) could use `log_blowup = 1` while complex types use `log_blowup = 3`. How much does per-chip parameter selection improve total proving time?

**RQ5: Incremental compilation.** When a new tx type is registered at runtime, the compiler generates a `CompiledAir` and the machine generates a proving key. What is the latency of this process, and can proving keys be cached and reused across batches?

---

## 13. Feasibility Validation Path

### Phase 0: Manual Prototype (2 weeks)

Hand-write 2–3 compiled chips (IncrementChip, TransferChip) as regular Rust `Air` implementations. Benchmark prove/verify time against the generic ExecutionChip for the same transactions. Verify bus message equivalence.

This answers the critical question: **does the theoretical trace reduction translate to proportional proof time reduction?**

The answer is not guaranteed — FRI overhead, Merkle hashing, and challenge computation may dominate for small traces, reducing the effective speedup. Phase 0 provides the empirical basis for proceeding.

### Phase 1: Symbolic Execution Engine (3 weeks)

Extend the IR crate with symbolic evaluation. Input: `TxTypeDef`. Output: `SymbolicResult` (reads, writes, asserts, gadgets, derived expressions). Integrate with the existing `typecheck.rs` pass at `Program::register()`.

### Phase 2: Constraint Compiler (3 weeks)

Convert `SymbolicResult` to `CompiledAir`. Implement materialization decisions, degree analysis, column layout generation, and structural coverage checking.

### Phase 3: Machine Integration (3 weeks)

Implement `Air` trait for `CompiledAir`. Implement `TraceContributor` for `CompiledPopulator`. Add per-tx-type trace partitioning to the machine layer. Implement threshold-based fallback to generic ExecutionChip.

### Phase 4: Testing and Benchmarks (2 weeks)

Equivalence testing across all registered tx types. End-to-end benchmarks (prove time, verify time, proof size). Regression suite ensuring compiled and generic paths produce identical proofs.

---

## References

- [execution-chip-evolution.md](../design/execution-chip-evolution.md) — Level 0–4 evolution path
- [proof-optimization-architecture.md](../design/proof-optimization-architecture.md) — Two-axis optimization context
- [constraint-compilation.md](../design/constraint-compilation.md) — Constraint CSE via symbolic DAG
- [extensibility-architecture.md](../design/extensibility-architecture.md) — ChipExtension, TemplateChip trait
- [air-chip-architecture.md](../design/air-chip-architecture.md) — Chip patterns and bus architecture
- `crates/chips/src/execution/columns.rs` — Current ExecutionCols layout (278 columns)
- `crates/ir/src/pass/typecheck.rs` — Compile-time type inference and SSA validation
- `crates/lang/src/lower/stmt.rs` — DSL-to-IR lowering (deterministic slot allocation)
