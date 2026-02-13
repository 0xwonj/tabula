# Conditional Branching in Tabula — Research & Design

> **Status**: Research Complete, Design Proposal v0.1
> **Scope**: How to add conditional branching to a ZK-provable, cell-addressed state machine
> **Prerequisites**: [dsl-philosophy.md](./dsl-philosophy.md), [architecture.md](./architecture.md)

---

## 1. Problem Statement

Tabula's IR is currently a flat `Vec<Instruction>` with no branching. The interpreter linearly walks instructions. This was a deliberate design choice (philosophy L1, L5): linear execution simplifies ZK proving because the circuit structure is fixed — one instruction = one constraint group, no variable-length paths.

However, real-world programs need conditional logic. A token transfer that charges a fee only if the sender is not whitelisted, a game action that has different effects based on unit type, an escrow that releases funds only after a deadline — all require branching.

**Question**: How should conditional branching be added to Tabula's DSL and IR while preserving ZK-provability, deterministic execution, and the "honest abstraction" principle?

---

## 2. How Existing ZK Systems Handle Branching

### 2.1 The Fundamental Constraint

In all arithmetic constraint systems (R1CS, PLONKish/AIR), the circuit is a **fixed structure** determined at compile time. Unlike a CPU that can jump to different addresses at runtime, a circuit evaluates **all gates** every time. There is no "skipping" a constraint.

This means: **both branches of an `if/else` always execute in the circuit**. The system then uses a multiplexer (conditional select) to pick the correct result.

This is the universal rule across R1CS, PLONK, Halo2, and AIR-based systems — with one notable exception (Cairo/STARK, discussed below).

### 2.2 Approach A: Conditional Select (Multiplexer)

**Used by**: Noir, Circom, Leo, o1js

The core primitive is:

```
out = condition * value_if_true + (1 - condition) * value_if_false
```

This is 1 multiplication + 1 addition = **1 R1CS constraint** per field element.

#### Noir (Aztec)

Noir supports native `if/else` syntax. The compiler "flattens" branches into ACIR constraints:

```rust
// Noir source
fn main(x: Field, y: Field) -> pub Field {
    if x > 0 { y + 1 } else { y - 1 }
}
```

Both `y + 1` and `y - 1` are computed. A branch selector witness is generated (via Brillig, Noir's unconstrained runtime), then an ACIR constraint enforces the correct output selection. Key quote from Noir docs:

> "An `if` statement is 'flattened' and gates created for each path even if execution uses only one path."

Noir recommends minimizing computation inside conditional blocks: compute what you can outside, branch only over essential logic.

**Sources**: [Noir — Thinking in Circuits](https://noir-lang.org/docs/explainers/explainer-writing-noir), [Noir Under the Hood](https://medium.com/distributed-lab/noir-under-the-hood-from-code-to-constraints-b3af7a54f00c)

#### Circom

Circom is more explicit about the limitation. Signals (circuit wires) **cannot be used in `if` conditions** at all — the language forbids it syntactically. Instead, developers must manually implement multiplexer patterns:

```circom
// Circom — multiplexer pattern
signal x_eq_5 <== IsEqual()([x, 5]);
signal x_eq_9 <== IsEqual()([x, 9]);
signal otherwise <== IsZero()(x_eq_5 + x_eq_9);

out <== (x_eq_5 * 14) + (x_eq_9 * 22) + (otherwise * 45);
```

Critical insight: **each condition doubles circuit size**, since all branches compute fully.

**Sources**: [Circom Conditional Statements](https://rareskills.io/post/circom-if-statement)

#### Leo (Aleo)

Leo has `if/else` but explicitly warns against it:

> "Conditional if-else statements in Leo are expensive, and it is preferred to use ternary `? :` expressions."

The compiler rewrites `if/else` into sequences of ternary expressions because "the underlying circuit construction does not support branching." With `if/else`, Leo "creates branches in the circuit so both paths can be evaluated, and the cost of every computation within the conditional will be doubled."

Ternary expressions are cheaper because both arms can be computed before the condition check — no dependency chain.

**Sources**: [Leo Paper (ePrint)](https://eprint.iacr.org/2021/651.pdf)

#### o1js (Mina)

o1js makes the multiplexer explicit as an API call:

```typescript
// o1js — explicit conditional select
let result = Provable.if(condition, valueIfTrue, valueIfFalse);
```

`Provable.if(bool, x, y)` costs **O(n) constraints** where n is the number of field elements in x and y. With `Hashed<T>`, this is reduced to **O(1)** by comparing hashes instead of full values.

Regular JavaScript `if/else` cannot be used for circuit logic — the circuit must have the same constraint structure regardless of input values.

**Sources**: [Provable API Reference](https://docs.minaprotocol.com/zkapps/o1js-reference/type-aliases/Provable), [o1js Security Strategies](https://veridise.com/blog/learn-blockchain/mastering-o1js-on-mina-four-key-strategies-for-secure-development/)

### 2.3 Approach B: True Branching (Cairo/STARK)

**Used by**: Cairo (StarkWare)

Cairo is a notable **exception** to the "both branches execute" rule. Because Cairo targets **STARK (AIR-based)** proofs rather than R1CS/PLONK, it can support **actual branching** with jump instructions.

How it works:

1. Cairo source compiles to **Sierra** (Safe Intermediate Representation)
2. Sierra compiles to **CASM** (Cairo Assembly), which has actual `jmp` instructions
3. CASM executes on the Cairo VM, generating an **execution trace**
4. The trace is proven via AIR (Algebraic Intermediate Representation) polynomials

Key mechanism: Sierra uses `felt252_is_zero` libfunc to test conditions, `branch_align` to equalize gas/ap across merge points, and `enum_match` for pattern matching. The `branch_align` libfunc ensures that different code paths have compatible execution costs for the prover.

**Why this works**: In AIR/STARK, constraints are defined over the **execution trace** (a matrix of field values over time). The trace records **only the taken path**. The polynomial constraints verify that each state transition was valid — they don't need to encode both branches. Instead, the constraint polynomial at each row checks: "given the current instruction and inputs, is the next row valid?"

**Trade-off**: Untaken branches add **zero** cost to the trace (they're never executed). But the CASM program itself must encode both paths, and the AIR constraint system must support the instruction set (including jumps). The overall circuit complexity comes from the **instruction set**, not the program size.

**Sources**: [Sierra IR Documentation](https://docs.starknet.io/build/starknet-by-example/advanced/sierra-ir), [Exploring Sierra (Nethermind)](https://medium.com/nethermind-eth/under-the-hood-of-cairo-1-0-exploring-sierra-7f32808421f5), [Cairo Book — Sierra Appendix](https://www.starknet.io/cairo-book/appendix-09-sierra.html)

### 2.4 Approach C: zkVM Branching

**Used by**: SP1, RISC Zero, Valida

zkVMs (zero-knowledge virtual machines) execute standard CPU instructions (typically RISC-V) and generate STARK proofs of correct execution. They support **full branching** because they prove the execution trace, not the program structure.

Key finding from [2025 research on compiler optimizations for zkVMs](https://arxiv.org/html/2508.17518):

> "Branches are relatively cheap and incur no misprediction penalty."
> "Predicated execution increases proving cost, since both paths must be proven and executed."

Specifically, replacing a branch with branchless arithmetic (predicated execution) on the `polybench-nussinov` benchmark:
- **x86 CPU**: 23.6% improvement (branch prediction helps)
- **SP1 zkVM**: Only 2.6% gain
- **RISC Zero zkVM**: **18.1% degradation** (more instructions to prove)

**Lesson**: In STARK-based systems, a taken branch that skips N instructions is **cheaper** than predicated execution that evaluates all N instructions. Branch elimination — a classic CPU optimization — can actively **hurt** zkVM performance.

**Sources**: [Evaluating Compiler Optimization Impacts on zkVM Performance](https://arxiv.org/html/2508.17518)

---

## 3. Cost Analysis

### 3.1 Conditional Select (R1CS / PLONKish)

For a single scalar value:
```
select(condition, a, b) = condition * a + (1 - condition) * b
```
- **R1CS**: 1 constraint (1 multiplication gate)
- **PLONKish**: 1 custom gate (with selector)
- **Constraint count scales linearly** with the number of field elements being selected

For Tabula's `Value` types:
| Type | Field Elements | Select Cost |
|------|---------------|-------------|
| U64 | 1 | 1 constraint |
| I64 | 1 | 1 constraint |
| Bool | 1 | 1 constraint |
| Bytes32 | 8 (in 32-bit field) or 1 (in 256-bit field) | 1-8 constraints |

### 3.2 Branch Execution Overhead

With conditional select, **both branches always execute**:

```
if condition {
    // Branch A: 10 instructions
} else {
    // Branch B: 5 instructions
}
// Total cost: 10 + 5 + 1 (select) = 16 instruction equivalents
```

With N levels of nesting:
```
if c1 {          // depth 1
    if c2 {      // depth 2
        if c3 {  // depth 3
            ...
```
- **Cost**: Sum of all branches at all levels
- **Not exponential** (common misconception) — it's the sum, not the product
- But deeply nested `if/else` chains can still bloat the circuit significantly

### 3.3 STARK/AIR Approach

With true branching (Cairo-style):
```
if condition {
    // Branch A: 10 instructions
} else {
    // Branch B: 5 instructions
}
// Total cost: max(10, 5) = 10 (only taken path executes)
// But: instruction set must support jumps (base cost in AIR)
```

Trade-off: Lower per-branch cost, but higher base cost for the instruction set.

### 3.4 Conditional Side Effects: The Hard Problem

Conditional **value selection** is cheap (1 constraint per field element). The expensive case is conditional **side effects**:

| Side Effect | Challenge | Cost |
|-------------|-----------|------|
| Conditional Read | Both reads execute, extra opening proofs | 2x Read cost |
| Conditional Write | Must write to same cell, select value | 1 Read + 1 Select + 1 Write |
| Conditional Assert | Must become `condition → predicate` | 1 extra constraint |
| Conditional Emit | Both emits execute, filter in post-processing | Doubled event log |

---

## 4. Design Space for Tabula

### 4.1 Tabula's Position in the Landscape

Tabula targets **STARK/FRI** proofs (architecture doc D9). This places it closer to Cairo's approach than to R1CS/PLONK systems. However, Tabula's IR is currently a flat slot-based system, not a register machine with a program counter.

Two viable paths exist:

| Path | Model | IR Change | Proof System Impact |
|------|-------|-----------|-------------------|
| **Path A**: Conditional Select | Noir/Circom/Leo | Add `Select` instruction | Minimal — flat IR preserved |
| **Path B**: True Branching | Cairo/zkVM | Add jump/branch instructions, PC | Major — fundamentally different IR model |

### 4.2 Path A: Conditional Select (Recommended for v2)

Add a `Select` instruction to the IR that evaluates **both branches** and picks the result:

```rust
/// Conditional value selection.
/// dst = if predicate { if_true } else { if_false }
/// Both if_true and if_false are always evaluated.
Select {
    dst: Slot,
    condition: Predicate,
    if_true: ValueExpr,
    if_false: ValueExpr,
}
```

**Derived instructions for side effects:**

```rust
/// Conditional write: writes if_true value when predicate holds, if_false otherwise.
/// Equivalent to: Select + Write (the select is implicit).
ConditionalWrite {
    table: TableId,
    row: RowExpr,
    col: ColId,
    condition: Predicate,
    src_true: ValueExpr,
    src_false: ValueExpr,
}
```

**DSL syntax:**

```
// Value selection (compiles to Select)
let result = if condition { expr_a } else { expr_b }

// Conditional write (compiles to ConditionalWrite or Read+Select+Write)
if condition {
    table[row].col = value_a
} else {
    table[row].col = value_b
}

// Conditional assert (compiles to Assert with condition → predicate)
if condition {
    assert balance >= amount
}
```

**Advantages**:
- Minimal IR change — one new instruction, flat `Vec<Instruction>` preserved
- Deterministic compilation — no variable-length code paths
- Preserves philosophy G2 (predictable compilation): developer can see both arms will execute
- Single-pass lowering in the compiler still works
- No changes needed in executor architecture (still linear walk)

**Disadvantages**:
- Both branches always execute — higher cost than necessary for STARK targets
- Conditional reads double the opening proof cost
- Deep nesting creates large instruction sequences
- Does not take advantage of STARK's ability to handle true branching

### 4.3 Path B: True Branching (Future v3+)

Restructure the IR around a program counter with conditional jumps:

```rust
/// Jump if predicate is true
JumpIf {
    condition: Predicate,
    target: InstructionIndex,
}

/// Unconditional jump
Jump {
    target: InstructionIndex,
}
```

**Advantages**:
- Untaken branches have **zero** execution cost
- Natural for STARK/AIR (only the taken path appears in the execution trace)
- Aligns with Cairo's proven approach
- More efficient for deeply nested or multi-branch logic

**Disadvantages**:
- **Major** IR redesign — slot allocation becomes non-trivial (slots may or may not be initialized depending on path)
- Requires **SSA form or phi nodes** to merge variable state at join points
- The interpreter needs a program counter instead of linear iteration
- Proof system must handle variable-length traces (padding to max-length or AIR-based approach)
- Slot liveness analysis becomes necessary
- Significantly more complex compiler (multi-pass)
- Single-pass lowering (philosophy C2) no longer feasible

### 4.4 Hybrid: Conditional Select Now, True Branching Later

The recommended path is **incremental**:

**Phase 1 (v2)**: Add `Select` + `ConditionalWrite` to the IR. Support `if/else` in the DSL with multiplexer-based lowering. This is correct, simple, and sufficient for most use cases.

**Phase 2 (v3+)**: If profiling shows the both-branches-execute cost is a bottleneck, consider restructuring the IR for true branching. This is a much larger project that should be driven by concrete performance data, not speculation.

**Rationale**: Path A is a **backward-compatible, additive** change. Path B is a **breaking** redesign. Starting with A allows:
1. Immediate DX improvement (developers get `if/else`)
2. Real-world programs to be written, revealing actual branching patterns
3. Performance data to justify (or not) the Path B investment
4. The DSL syntax remains the same — only the lowering changes

---

## 5. Detailed Design: Path A (Conditional Select)

### 5.1 IR Changes

Two new instructions:

```rust
pub enum Instruction {
    // ... existing instructions ...

    /// dst = if predicate { if_true } else { if_false }
    /// Both arms are always evaluated.
    Select {
        dst: Slot,
        condition: Predicate,
        if_true: ValueExpr,
        if_false: ValueExpr,
    },

    /// Writes src_true if condition holds, src_false otherwise.
    /// Semantically equivalent to Select + Write, but explicit
    /// about the side effect for the proof system.
    ConditionalWrite {
        table: TableId,
        row: RowExpr,
        col: ColId,
        condition: Predicate,
        src_true: ValueExpr,
        src_false: ValueExpr,
    },
}
```

**Why `ConditionalWrite` as a separate instruction?**

If we only had `Select`, a conditional write would require:
1. `Select { dst, condition, src_true, src_false }` — pick the value
2. `Write { table, row, col, src: Slot(dst) }` — write it

This is 2 instructions and works. But making `ConditionalWrite` explicit has benefits:
- The proof system knows **why** the write happened (conditional vs unconditional)
- The execution trace can distinguish conditional writes for more efficient proving
- When Path B is implemented, `ConditionalWrite` can be optimized to skip the write entirely when the condition is false

For v2, `ConditionalWrite` in the interpreter simply does `Select + Write` internally. The separation is a future-proofing measure.

### 5.2 DSL Syntax

#### Expression-level `if` (compiles to `Select`)

```
let fee = if is_whitelisted { 0 } else { base_fee }
```

Lowering:
```
Select { dst: slot_fee, condition: is_whitelisted, if_true: Literal(0), if_false: slot_base_fee }
```

Both `0` and `base_fee` are trivially available — no extra computation.

#### Block-level `if` with statements (compiles to per-statement lowering)

```
if has_enough_balance {
    accounts[sender].balance = sender_bal - amount
    accounts[receiver].balance = receiver_bal + amount
} else {
    accounts[sender].balance = sender_bal
    accounts[receiver].balance = receiver_bal
}
```

Each assignment in both branches targets the **same cell**. The compiler pairs them and emits:

```
// For sender balance:
Sub { dst: s_new_sender, lhs: sender_bal, rhs: amount }
Select { dst: s_final_sender, condition: has_enough_balance,
         if_true: Slot(s_new_sender), if_false: sender_bal }
Write { table: accounts, row: sender, col: balance, src: Slot(s_final_sender) }

// For receiver balance:
Add { dst: s_new_receiver, lhs: receiver_bal, rhs: amount }
Select { dst: s_final_receiver, condition: has_enough_balance,
         if_true: Slot(s_new_receiver), if_false: receiver_bal }
Write { table: accounts, row: receiver, col: balance, src: Slot(s_final_receiver) }
```

#### Conditional assert (compiles to guarded predicate)

```
if is_premium_user {
    assert balance >= premium_threshold
}
```

Lowering: `Assert { predicate: Or(Not(is_premium_user), Gte(balance, premium_threshold)) }`

This is `¬condition ∨ predicate`, equivalent to `condition → predicate` (material implication).

#### `if` without `else` (for writes)

```
if should_update {
    accounts[user].last_active = current_time
}
```

When there's no `else`, the "false" branch preserves the current state. The compiler emits:

```
// Read current value
Read { dst: s_current, table: accounts, row: user, col: last_active }
// Conditional write: new value if true, old value if false
ConditionalWrite { table: accounts, row: user, col: last_active,
                   condition: should_update,
                   src_true: current_time, src_false: Slot(s_current) }
```

### 5.3 Compiler Lowering Rules

The lowering phase handles `if/else` in the AST:

1. **Expression-level if**: `let x = if c { a } else { b }` → evaluate `a`, evaluate `b`, emit `Select`

2. **Block-level if/else with writes**:
   - Match each write in the `if` block to a corresponding write in the `else` block (same cell)
   - If a write appears only in one branch, auto-generate an identity write (read-back) for the other
   - Emit computation for both branches, then `Select` + `Write` for each paired write

3. **If-without-else**:
   - For writes: auto-generate `Read` of current value, use as `if_false` arm
   - For asserts: convert to material implication

4. **Nested if**: Flatten by chaining selects. `if a { if b { x } else { y } } else { z }` becomes two `Select` instructions.

### 5.4 Restrictions

To keep the system simple and honest:

1. **No `if` in expression position with side effects**: `if c { table[r].x = 1 } else { 2 }` is a compile error. Side effects are only allowed in block-level `if`.

2. **Nesting depth limit**: Max 4 levels (configurable). Beyond this, the circuit cost multiplier makes the program impractical.

3. **Both branches must write to the same cells**: In block-level `if/else`, the write targets must match. Writing to cell A in one branch and cell B in another is a compile error — it would require conditional addressing, which is much more expensive.

4. **No reads inside `if` blocks (v2)**: To keep the read set deterministic, all reads happen unconditionally before the `if`. The `if` block can only write, assert, or compute with already-read values. This can be relaxed in a future version.

5. **`if` is an expression, not a statement**: Like Rust, `if/else` always produces a value (or `()` for block-level). This keeps the semantics clean.

### 5.5 Cost Transparency

Per philosophy G2 (Predictable Compilation), the compiler should warn about cost:

```
warning[W001]: conditional block has high circuit cost
  --> transfer.tab:15:5
   |
15 |     if should_charge_fee {
   |     ^^ both branches will be fully evaluated
   |
   = note: if-branch: 4 instructions, else-branch: 2 instructions
   = note: total cost: 6 instructions + 2 select operations
   = help: move computations outside the if block where possible
```

---

## 6. Interpreter Changes

The interpreter update is minimal:

```rust
// In execute():
Instruction::Select { dst, condition, if_true, if_false } => {
    let cond = evaluate_predicate(condition, &slots, params)?;
    let t = resolve_value_expr(if_true, &slots, params)?;
    let f = resolve_value_expr(if_false, &slots, params)?;
    let result = if cond { t } else { f };
    set_slot(&mut slots, *dst, result)?;
}

Instruction::ConditionalWrite { table, row, col, condition, src_true, src_false } => {
    let cond = evaluate_predicate(condition, &slots, params)?;
    let t = resolve_value_expr(src_true, &slots, params)?;
    let f = resolve_value_expr(src_false, &slots, params)?;
    let value = if cond { t } else { f };
    let row_key = resolve_row_expr(row, &slots, params)?;
    let key = CellKey { table: *table, row: row_key, col: *col };
    overlay.write(&key, value);
}
```

Note: In the interpreter, `Select` short-circuits — it evaluates both arms but only stores one. This is fine for execution. For the **proof system**, both arms must be constrained.

---

## 7. Proof System Implications

### 7.1 Select Constraint

In the constraint system, `Select` becomes:

```
dst = condition * if_true + (1 - condition) * if_false
```

This is a single degree-2 constraint (one multiplication). For STARK/AIR:
- One trace row with columns: `[condition, if_true, if_false, dst]`
- Constraint: `dst - condition * if_true - (1 - condition) * if_false = 0`

### 7.2 ConditionalWrite in the Trace

A `ConditionalWrite` produces the same execution event as a regular `Write` — the overlay records the final value regardless of which branch was taken. The execution trace sees a single write with the selected value.

For the consistency checker: no change needed. The write is just a write.

### 7.3 Future: Leveraging STARK for True Branching

If Tabula moves to Path B in the future, the STARK-native approach would:
1. Execute only the taken branch (shorter trace)
2. Use AIR constraints to verify valid state transitions at each step
3. Use `branch_align`-like padding to equalize trace width across paths
4. Potentially save 40-50% on deeply branched programs vs the multiplexer approach

But this requires rethinking the entire IR, interpreter, and constraint system — a v3+ effort.

---

## 8. Comparison Summary

| Aspect | Path A (Select) | Path B (True Branch) | Cairo | Noir |
|--------|-----------------|---------------------|-------|------|
| IR model | Flat Vec + Select | PC + Jump | Sierra→CASM | ACIR + Brillig |
| Branch cost | Sum of all branches | Only taken branch | Only taken branch | Sum of all branches |
| Compiler complexity | Low (single-pass) | High (multi-pass, SSA) | High (Sierra→CASM) | Medium (ACIR gen) |
| Proof system impact | Minimal (1 new constraint type) | Major (new IR model) | Built-in (AIR) | Built-in (PLONK) |
| Nesting cost | Linear sum | None (trace-based) | None | Linear sum |
| Implementation effort | ~1 week | ~2-3 months | N/A | N/A |
| Breaking change | No | Yes | N/A | N/A |

---

## 9. Recommendation

**Implement Path A (Conditional Select) for Tabula v2.**

Justification:

1. **Correct**: Multiplexer-based conditional execution is proven and universal across ZK systems
2. **Honest**: The DSL syntax (`if/else`) maps transparently to `Select` instructions — the developer knows both branches execute
3. **Incremental**: No breaking changes to IR, executor, or proof system
4. **Sufficient**: Most real-world Tabula programs need 1-2 levels of branching, where the overhead is negligible
5. **Future-compatible**: The DSL syntax remains identical if/when Path B is implemented; only the lowering changes
6. **Philosophy-aligned**: Designed "from the proof system up" (T2), not from the language down

**Defer Path B** until:
- Multiple real-world programs demonstrate >3 levels of nesting
- Profiling shows conditional select overhead exceeds 20% of total proof cost
- The STARK proof system (Phase 3) is implemented and stable

---

## 10. References

### ZK Language Documentation
- [Noir — Thinking in Circuits](https://noir-lang.org/docs/explainers/explainer-writing-noir)
- [Noir Under the Hood: From Code to Constraints](https://medium.com/distributed-lab/noir-under-the-hood-from-code-to-constraints-b3af7a54f00c)
- [Circom — Conditional Statements](https://rareskills.io/post/circom-if-statement)
- [Cairo Book — Sierra Appendix](https://www.starknet.io/cairo-book/appendix-09-sierra.html)
- [Understanding Sierra: From High-Level Cairo to Safe CASM](https://docs.starknet.io/build/starknet-by-example/advanced/sierra-ir)
- [Exploring Sierra (Nethermind)](https://medium.com/nethermind-eth/under-the-hood-of-cairo-1-0-exploring-sierra-7f32808421f5)
- [Leo Paper — Formally Verified ZK Applications](https://eprint.iacr.org/2021/651.pdf)
- [o1js Provable API Reference](https://docs.minaprotocol.com/zkapps/o1js-reference/type-aliases/Provable)

### Architecture & Cost Analysis
- [zkEVM Architecture: Constraint-Level Design](https://arxiv.org/html/2510.05376v1)
- [Evaluating Compiler Optimization Impacts on zkVM Performance](https://arxiv.org/html/2508.17518)
- [Noir's Circuit Backend (jtriley)](https://jtriley.substack.com/p/noirs-circuit-backend)
- [R1CS Explainer (0xPARC)](https://learn.0xparc.org/materials/circom/additional-learning-resources/r1cs%20explainer/)

### General ZK Circuit Design
- [Unconstrained Functions in Noir (Aztec)](https://aztec.network/blog/unconstrained-functions-in-noir)
- [o1js Security Strategies (Veridise)](https://veridise.com/blog/learn-blockchain/mastering-o1js-on-mina-four-key-strategies-for-secure-development/)
- [Designing High-Performance zkVMs (RISC Zero)](https://risczero.com/blog/designing-high-performance-zkVMs)
