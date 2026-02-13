# Tabula Conditional Branching — Ideal Design

> **Status**: Design Specification v0.1
> **Scope**: The theoretically optimal conditional branching design for Tabula, unconstrained by implementation effort
> **Prerequisites**: [conditional-branching-research.md](./conditional-branching-research.md), [architecture.md](./architecture.md), [dsl-philosophy.md](./dsl-philosophy.md)

---

## 0. Design Thesis

Tabula targets STARK/FRI proofs. In STARK systems, the prover generates an **execution trace** and proves its correctness via AIR polynomial constraints. The trace records **only executed instructions** — untaken branches contribute zero rows.

Therefore, the theoretically optimal design uses **true branching** (like Cairo), not multiplexer-based conditional select (like Noir/Circom). The multiplexer approach — evaluating both branches and selecting a result — is a workaround for R1CS/PLONK systems where the circuit structure must be fixed. It is suboptimal for STARK.

However, Tabula is not Cairo. Tabula is a cell-addressed state machine with table schemas, not a general-purpose language. The ideal design adapts Cairo's core insight (trace-based branching) to Tabula's unique execution model (read-compute-assert-write over typed cells).

Additionally, Tabula has a property Cairo lacks: **guaranteed termination without gas metering**. By restricting the CFG to a directed acyclic graph (no back-edges), all programs terminate and the maximum trace length is computable at compile time.

---

## 1. IR: Basic Block Control Flow Graph

### 1.1 From Flat Vec to CFG

The current IR is a flat `Vec<Instruction>`. The ideal IR is a **control flow graph** of basic blocks:

```rust
/// A basic block identifier, indexing into TxBody.blocks.
/// Blocks are topologically ordered: a block can only jump
/// to blocks with strictly higher IDs. This guarantees
/// the CFG is a DAG (no cycles, no loops).
pub struct BlockId(pub u16);

/// A transaction body as a control flow graph.
pub struct TxBody {
    /// Basic blocks in topological order. Entry is always blocks[0].
    pub blocks: Vec<BasicBlock>,
}

/// A basic block: a straight-line sequence of instructions
/// ending with exactly one terminator.
pub struct BasicBlock {
    /// Number of block parameters. Slots 0..param_count are
    /// pre-filled by the predecessor's jump/branch arguments.
    pub param_count: u16,
    /// Type of each block parameter (for verification).
    pub param_types: Vec<ValueType>,
    /// Straight-line instruction body. No control flow here.
    /// Instructions allocate slots starting at param_count.
    pub body: Vec<Instruction>,
    /// How this block ends.
    pub terminator: Terminator,
}

/// Block terminator — the sole control flow mechanism.
pub enum Terminator {
    /// Unconditional jump to a successor block.
    Jump {
        target: BlockId,
        args: Vec<ValueExpr>,
    },
    /// Conditional branch: jump to if_true or if_false.
    Branch {
        condition: Predicate,
        if_true: BlockId,
        true_args: Vec<ValueExpr>,
        if_false: BlockId,
        false_args: Vec<ValueExpr>,
    },
    /// Transaction completes successfully.
    Return,
    /// Transaction aborts (assertion failure).
    Abort { message: String },
}
```

### 1.2 Why Basic Blocks with Block Parameters

This design borrows from Sierra (Cairo), MLIR, and Cranelift. The key ideas:

**Basic blocks** are maximal straight-line sequences — no control flow within a block, control flow only at terminators. This means:
- Within a block, execution is identical to today's flat interpreter
- The only new complexity is at block boundaries

**Block parameters** (instead of SSA phi nodes) make data flow explicit:

```
// SSA phi nodes (LLVM style) — implicit:
block3:
  %x = phi [block1: %a, block2: %b]

// Block parameters (Sierra/MLIR style) — explicit:
block3(%x):
  ...
block1: jump block3(%a)
block2: jump block3(%b)
```

Block parameters are:
- **More explicit**: The predecessor decides what to pass — no ambiguity at the join point
- **Easier to verify**: The block signature declares its inputs — the verifier checks types at each edge
- **Single-pass verifiable**: No need to look forward to find phi nodes
- **Natural for slots**: Block params are just pre-filled slots (0..param_count)

### 1.3 Topological Ordering Invariant

**Critical property**: `BlockId` values are topologically ordered. A `Jump` or `Branch` can only target blocks with **strictly higher** IDs. This guarantees:

1. **No cycles** → no infinite loops → guaranteed termination
2. **No gas metering needed** — termination is a static, compile-time property
3. **Maximum trace length computable at compile time**: `max(sum(block.body.len()) over all paths)`
4. **Single forward pass** for verification and interpretation

This is a fundamental advantage over Cairo, which allows back-edges (loops) and therefore requires gas metering for termination.

### 1.4 Slot Semantics Within Blocks

Each block has a local slot space:

```
Block params:  slot 0, slot 1, ..., slot (param_count - 1)
Local slots:   slot param_count, slot param_count + 1, ...
```

Instructions within a block allocate slots monotonically, exactly as today. The only difference: block boundaries reset the slot space, and data flows between blocks via arguments.

```
Block 0 (entry, params: []):
  s0 = Read(accounts, Param(0), balance)     // slot 0
  s1 = Read(accounts, Param(1), balance)     // slot 1
  Assert(Gte(Slot(0), Param(2)))
  Branch(is_vip, Block(1) [Slot(0), Slot(1)],
                 Block(2) [Slot(0), Slot(1)])

Block 1 (params: [u64, u64]):           // s0=sender_bal, s1=receiver_bal
  s2 = Sub(Slot(0), Param(2))           // new_sender
  s3 = Add(Slot(1), Param(2))           // new_receiver
  Write(accounts, Param(0), balance, Slot(2))
  Write(accounts, Param(1), balance, Slot(3))
  Jump(Block(3) [])

Block 2 (params: [u64, u64]):           // s0=sender_bal, s1=receiver_bal
  s2 = DivMod(Param(2), Literal(100))   // fee = amount / 100
  s4 = Sub(Param(2), Slot(2))           // net = amount - fee
  s5 = Sub(Slot(0), Param(2))           // new_sender
  s6 = Add(Slot(1), Slot(4))            // new_receiver
  Write(accounts, Param(0), balance, Slot(5))
  Write(accounts, Param(1), balance, Slot(6))
  Jump(Block(3) [])

Block 3 (params: []):                    // join point
  Emit("transfer", [Param(0), Param(1), Param(2)])
  Return
```

### 1.5 ValueExpr in the Block Context

`ValueExpr` remains unchanged — `Slot(n)` now refers to the current block's slot `n` (which may be a block parameter or a locally computed value). `Param(n)` still refers to the transaction's parameter `n`, which is globally accessible in all blocks.

```rust
pub enum ValueExpr {
    Literal(Value),
    Slot(Slot),     // current block's slot
    Param(u16),     // tx parameter (global)
}
```

---

## 2. Execution Model

### 2.1 PC-Based Interpreter

The interpreter becomes a block-walking state machine:

```rust
pub fn execute(
    body: &TxBody,
    params: &[Value],
    overlay: &mut Overlay,
    hasher: &dyn Hasher,
    static_tables: &dyn StaticTableProvider,
) -> Result<TxExecutionOutput, InterpreterError> {
    let mut current_block = BlockId(0);
    let mut block_args: Vec<Value> = Vec::new();
    let mut emitted: Vec<EmittedEvent> = Vec::new();

    loop {
        let block = &body.blocks[current_block.0 as usize];

        // Initialize slots: block params first, then empty
        let mut slots: Vec<Value> = block_args.clone();

        // Execute straight-line body
        for (idx, instr) in block.body.iter().enumerate() {
            execute_instruction(instr, &mut slots, params,
                                overlay, hasher, static_tables,
                                &mut emitted)?;
        }

        // Process terminator
        match &block.terminator {
            Terminator::Return => {
                return Ok(TxExecutionOutput { emitted });
            }
            Terminator::Abort { message } => {
                return Err(InterpreterError {
                    error: TabulaError::AssertionFailed(message.clone()),
                    instruction_index: block.body.len(),
                });
            }
            Terminator::Jump { target, args } => {
                block_args = resolve_args(args, &slots, params)?;
                current_block = *target;
            }
            Terminator::Branch {
                condition, if_true, true_args,
                if_false, false_args,
            } => {
                let cond = evaluate_predicate(condition, &slots, params)?;
                if cond {
                    block_args = resolve_args(true_args, &slots, params)?;
                    current_block = *if_true;
                } else {
                    block_args = resolve_args(false_args, &slots, params)?;
                    current_block = *if_false;
                }
            }
        }
    }
}
```

**Key property**: Only the taken path executes. Untaken blocks produce zero instructions, zero reads, zero writes, zero events.

### 2.2 Execution Trace Structure

Each executed instruction produces one trace row. The trace records:

| Column | Description |
|--------|-------------|
| `block_id` | Which block is executing |
| `instr_idx` | Instruction index within the block |
| `opcode` | Instruction type (Read, Write, Add, ...) |
| `slots[0..N]` | All slot values after this instruction |
| `key` | CellKey (for Read/Write) |
| `value` | Value read or written |

Block transitions produce a special trace row:

| Column | Description |
|--------|-------------|
| `block_id` | Source block |
| `opcode` | `BlockTransition` |
| `condition` | Branch condition value (for Branch terminators) |
| `target_block` | Destination block ID |
| `args[0..M]` | Values passed to the target block |

The trace is **variable-length** — different inputs produce different-length traces. This is natural for STARK: the trace is padded to a power-of-2 length for the FFT.

### 2.3 Overlay Interaction

The overlay semantics are **unchanged**. Read, Write, checkpoint, and rollback work identically. The only difference is that conditional branches may cause different state cells to be read/written depending on the path taken.

This means:
- **Read set is path-dependent**: If branch A reads cell X and branch B reads cell Y, only the taken branch's read appears in `read_set_old`
- **Write set is path-dependent**: Same logic — only actually-executed writes appear
- **This is correct**: The proof verifies the execution trace, which includes exactly the reads/writes that happened

The consistency checker also works unchanged — it validates that the sequence of events satisfies last-write semantics, regardless of which blocks produced them.

---

## 3. DSL Syntax

### 3.1 `if` / `else` Expression

```
tx transfer(sender: u64, receiver: u64, amount: u64, is_vip: bool) {
    let sender_bal = accounts[sender].balance
    let receiver_bal = accounts[receiver].balance
    assert sender_bal >= amount

    if is_vip {
        // VIP path: no fee
        accounts[sender].balance = sender_bal - amount
        accounts[receiver].balance = receiver_bal + amount
        emit "transfer" (sender, receiver, amount, 0)
    } else {
        // Normal path: 1% fee
        let fee = amount / 100
        let net = amount - fee
        accounts[sender].balance = sender_bal - amount
        accounts[receiver].balance = receiver_bal + net
        emit "transfer" (sender, receiver, net, fee)
    }
}
```

The `if/else` block compiles to a `Branch` terminator. Each arm becomes its own basic block. Code after the `if/else` becomes a join block.

### 3.2 `if` / `else if` / `else` Chain

```
tx apply_discount(user: u64, tier: u64, base_price: u64) {
    let price = if tier == 1 {
        base_price * 90 / 100        // 10% off
    } else if tier == 2 {
        base_price * 80 / 100        // 20% off
    } else if tier == 3 {
        base_price * 50 / 100        // 50% off
    } else {
        base_price                   // no discount
    }

    orders[user].total_price = price
}
```

This desugars to a chain of `Branch` terminators. Each `else if` is the false-branch's entry, which immediately branches again.

```
Block 0: Branch(tier==1, Block(1), Block(2))
Block 1: [compute 90%] → Jump(Block(5), [result])
Block 2: Branch(tier==2, Block(3), Block(4))
Block 3: [compute 80%] → Jump(Block(5), [result])
Block 4: Branch(tier==3, Block(6), Block(7))
Block 6: [compute 50%] → Jump(Block(5), [result])
Block 7: [base_price]   → Jump(Block(5), [result])
Block 5(price): Write + Return
```

Only the matching tier's block executes. All other blocks contribute zero to the trace.

### 3.3 `match` Expression

For multi-way branching on values (future extension):

```
tx process(action: u64, target: u64, amount: u64) {
    match action {
        0 => {
            // deposit
            let bal = accounts[target].balance
            accounts[target].balance = bal + amount
        }
        1 => {
            // withdraw
            let bal = accounts[target].balance
            assert bal >= amount
            accounts[target].balance = bal - amount
        }
        2 => {
            // query (no-op, just emit)
            let bal = accounts[target].balance
            emit "query" (target, bal)
        }
    }
}
```

`match` compiles to a chain of `Branch` terminators (equality checks), identical to `if/else if/else`. A future optimization could use a lookup table for dense integer ranges.

### 3.4 `if` Without `else`

```
tx maybe_update(user: u64, should_log: bool) {
    let bal = accounts[user].balance
    accounts[user].balance = bal + 1

    if should_log {
        emit "updated" (user, bal + 1)
    }
}
```

An `if` without `else` compiles to a `Branch` where the false-path jumps directly to the join block:

```
Block 0: ... Branch(should_log, Block(1), Block(2))
Block 1: Emit("updated", ...) → Jump(Block(2), [])
Block 2: Return
```

If `should_log` is false, Block 1 is never executed — zero trace rows, zero events.

### 3.5 Nested `if`

```
tx complex(a: bool, b: bool, x: u64) {
    if a {
        if b {
            accounts[0].value = x
        } else {
            accounts[1].value = x
        }
    } else {
        accounts[2].value = x
    }
}
```

Compiles to:

```
Block 0: Branch(a, Block(1), Block(4))
Block 1: Branch(b, Block(2), Block(3))
Block 2: Write(accounts, 0, value, x) → Jump(Block(5))
Block 3: Write(accounts, 1, value, x) → Jump(Block(5))
Block 4: Write(accounts, 2, value, x) → Jump(Block(5))
Block 5: Return
```

Only one of Block 2/3/4 executes. Cost = entry block + 1 branch block + exit block. No matter how deep the nesting, only the taken path contributes to the trace.

### 3.6 Conditional Reads

With true branching, reads inside `if` blocks are safe and efficient:

```
tx conditional_lookup(user: u64, use_premium: bool) {
    let base_rate = @rates[0].value

    if use_premium {
        // This read ONLY happens when use_premium is true
        let premium_rate = @premium_rates[user].value
        accounts[user].rate = premium_rate
    } else {
        accounts[user].rate = base_rate
    }
}
```

In the multiplexer approach, `@premium_rates[user].value` would always be read (wasting an opening proof). With true branching, it's only read when needed.

---

## 4. Compiler Pipeline

### 4.1 Overview

```
Source (.tab)
    │
    ▼
┌─────────┐
│   Lex   │  tokens
└────┬────┘
     ▼
┌─────────┐
│  Parse  │  AST (with if/else nodes)
└────┬────┘
     ▼
┌─────────┐
│  Lower  │  Phase 1: AST → CFG (basic blocks)
│  (CFG)  │  Phase 2: Name resolution + type checking
│         │  Phase 3: Slot allocation per block
└────┬────┘
     ▼
┌─────────┐
│ Verify  │  Structural checks:
│         │  - Topological order
│         │  - Block param types match at edges
│         │  - All paths reach Return or Abort
│         │  - No unreachable blocks
└────┬────┘
     ▼
TxBody (CFG IR)
```

### 4.2 AST → CFG Lowering

The lowering phase converts AST `if/else` nodes into basic blocks:

**Algorithm** (sketch):

```
fn lower_block(stmts: &[Stmt], current_block: &mut BasicBlock,
               blocks: &mut Vec<BasicBlock>) -> BlockId {
    for stmt in stmts {
        match stmt {
            Stmt::Let { .. } | Stmt::Assign { .. } | Stmt::Assert { .. }
            | Stmt::Emit { .. } => {
                // Lower to instruction, append to current_block.body
                lower_instruction(stmt, current_block);
            }
            Stmt::If { condition, if_body, else_body } => {
                // 1. Create join block (receives merged values)
                let join_block_id = blocks.reserve_block_id();

                // 2. Create if-true block
                let true_block_id = blocks.reserve_block_id();
                let mut true_block = BasicBlock::new();
                lower_block(if_body, &mut true_block, blocks);
                // true_block terminates with Jump to join_block

                // 3. Create if-false block
                let false_block_id = blocks.reserve_block_id();
                let mut false_block = BasicBlock::new();
                lower_block(else_body, &mut false_block, blocks);
                // false_block terminates with Jump to join_block

                // 4. Current block terminates with Branch
                current_block.terminator = Terminator::Branch {
                    condition: lower_predicate(condition),
                    if_true: true_block_id,
                    true_args: /* values live at branch point */,
                    if_false: false_block_id,
                    false_args: /* same */,
                };

                // 5. Continue lowering remaining stmts into join block
                current_block = &mut blocks[join_block_id];
            }
        }
    }
}
```

### 4.3 Live Value Analysis

At each `Branch` terminator, the compiler must determine which values need to be passed to successor blocks. This is **live variable analysis**:

1. For each block, compute which slots are **used** (read) before being **redefined** (written)
2. At branch points, pass all live values as block arguments
3. At join points, block parameters receive values from both predecessors

This is the only part that requires a backward pass (or iterative dataflow). Since the CFG is a DAG, one backward pass suffices.

### 4.4 Type Checking Across Blocks

The type checker verifies:
- At each `Branch` or `Jump`, the argument types match the target block's `param_types`
- If two predecessors jump to the same join block, they must provide values of the same types
- This ensures type safety across all execution paths

```
// Type error: paths produce different types
let x = if condition {
    42          // u64
} else {
    true        // bool  ← ERROR: type mismatch at join point
}
```

---

## 5. Proof System: AIR Constraints

### 5.1 Trace Layout

The execution trace is a matrix where each row represents one step:

```
| row | block_id | instr_idx | opcode | is_terminator | condition | target | s0 | s1 | ... |
|-----|----------|-----------|--------|---------------|-----------|--------|-----|-----|-----|
| 0   | 0        | 0         | Read   | 0             | -         | -      | 100 | -   | ... |
| 1   | 0        | 1         | Read   | 0             | -         | -      | 100 | 200 | ... |
| 2   | 0        | 2         | Assert | 0             | -         | -      | 100 | 200 | ... |
| 3   | 0        | -         | Branch | 1             | 1         | 1      | 100 | 200 | ... |
| 4   | 1        | 0         | Sub    | 0             | -         | -      | 100 | 200 | ... |
| 5   | 1        | 1         | Add    | 0             | -         | -      | 100 | 200 | ... |
| 6   | 1        | 2         | Write  | 0             | -         | -      | 100 | 200 | ... |
| 7   | 1        | 3         | Write  | 0             | -         | -      | 100 | 200 | ... |
| 8   | 1        | -         | Return | 1             | -         | -      | 100 | 200 | ... |
| 9   | -        | -         | Pad    | -             | -         | -      | -   | -   | ... |
```

### 5.2 AIR Constraint Groups

The constraints are organized by instruction type. At each row, a selector activates the relevant constraint:

**1. Instruction constraints** (same as current, applied per-row):
- `Read`: Verify the read value matches the state commitment
- `Write`: Verify the written value is recorded in the trace
- `Add/Sub/Mul/DivMod`: Verify arithmetic correctness
- `Assert`: Verify predicate evaluates to true
- `Hash`: Verify hash computation
- `Emit`: Verify event recording

**2. Intra-block transition constraints** (row i → row i+1, same block):
- `block_id[i+1] == block_id[i]` (within a block)
- `instr_idx[i+1] == instr_idx[i] + 1` (sequential execution)
- Slot values carry forward unless explicitly modified

**3. Block transition constraints** (terminator row → next row, different block):
- For `Jump`: `block_id[i+1] == target`
- For `Branch`: `if condition then block_id[i+1] == if_true else block_id[i+1] == if_false`
- Block parameter values in row `i+1` match the arguments from row `i`

**4. Structural constraints**:
- `block_id` is non-decreasing (topological order)
- The last non-pad row has opcode `Return`

### 5.3 Why This is Optimal for STARK

In STARK/AIR:
- Constraints are evaluated **over the trace**, not over the program
- The trace records **only executed instructions**
- Untaken branches produce **zero trace rows**
- The prover's work scales with **trace length**, not program size

With the multiplexer approach:
- Both branches always produce trace rows → longer trace
- The `Select` instruction adds extra constraints
- Conditional reads generate unnecessary opening proofs

With true branching:
- Only the taken path produces trace rows → shorter trace
- No extra `Select` constraints
- Only actually-needed reads generate opening proofs

**Quantitative comparison** (transfer with conditional fee example):

| Metric | Multiplexer (Select) | True Branching (CFG) |
|--------|---------------------|---------------------|
| Trace rows (VIP path) | 18 (both branches) | 8 (entry + VIP only) |
| Trace rows (normal path) | 18 (both branches) | 10 (entry + normal only) |
| Read count | All reads from both paths | Only path-relevant reads |
| Opening proofs | All reads from both paths | Only path-relevant reads |
| Constraint count | 18 + select overhead | 8 or 10 (path-dependent) |

For programs with N branches of average size M:
- Multiplexer: O(N × M) constraints always
- True branching: O(M) constraints (only taken path)

The savings grow linearly with the number of branches.

---

## 6. Comparison: Current IR vs Ideal IR

### 6.1 TxTypeDef Change

```rust
// Current:
pub struct TxTypeDef {
    pub id: TxTypeId,
    pub name: String,
    pub param_schema: Vec<ParamDef>,
    pub body: Vec<Instruction>,        // flat list
}

// Ideal:
pub struct TxTypeDef {
    pub id: TxTypeId,
    pub name: String,
    pub param_schema: Vec<ParamDef>,
    pub body: TxBody,                  // CFG
}
```

### 6.2 Backward Compatibility

Programs with no `if/else` compile to a **single-block CFG**:

```rust
TxBody {
    blocks: vec![BasicBlock {
        param_count: 0,
        param_types: vec![],
        body: vec![/* all instructions */],
        terminator: Terminator::Return,
    }],
}
```

This is semantically identical to today's `Vec<Instruction>`. The interpreter executes one block, hits `Return`, and finishes.

### 6.3 Instruction Enum

The `Instruction` enum is **unchanged**. All existing instructions (Read, Write, Lookup, Add, Sub, Mul, DivMod, Assert, Hash, Emit) work identically within basic blocks. The only new types are `BasicBlock`, `TxBody`, `Terminator`, and `BlockId`.

`Assert` within a block still aborts the entire transaction (the overlay rolls back as today). The `Abort` terminator is simply an explicit way to reach the same outcome.

---

## 7. Advanced Features Enabled by CFG

### 7.1 Early Return on Assert

With the CFG model, `assert` can optionally branch instead of abort:

```
tx safe_transfer(sender: u64, receiver: u64, amount: u64) {
    let bal = accounts[sender].balance

    if bal < amount {
        // Insufficient funds: emit event and return (no abort)
        emit "insufficient_funds" (sender, amount)
    } else {
        accounts[sender].balance = bal - amount
        accounts[receiver].balance = accounts[receiver].balance + amount
        emit "transfer" (sender, receiver, amount)
    }
}
```

In the flat IR, this is impossible — `assert` either passes or the tx fails. With CFG branching, the developer can choose between "abort on failure" and "handle failure gracefully."

### 7.2 Multi-Table Conditional Logic

```
tx complex_action(user: u64, action_type: u64) {
    if action_type == 0 {
        // Only reads/writes to inventory table
        let item = inventory[user].equipped
        inventory[user].equipped = item + 1
    } else if action_type == 1 {
        // Only reads/writes to stats table
        let hp = stats[user].health
        stats[user].health = hp + 10
    } else {
        // Reads from both tables
        let item = inventory[user].equipped
        let hp = stats[user].health
        emit "status" (user, item, hp)
    }
}
```

With true branching:
- `action_type == 0`: Only `inventory` table is touched — zero opening proofs for `stats`
- `action_type == 1`: Only `stats` table is touched — zero opening proofs for `inventory`
- The proof only covers actually-accessed cells

With multiplexer: All reads from all branches execute, generating opening proofs for both tables regardless of the action type.

### 7.3 Compile-Time Trace Bound

Because the CFG is a DAG, the compiler can compute:

```
max_trace_length(block) =
    block.body.len() + 1 (terminator) +
    match block.terminator {
        Return | Abort => 0,
        Jump(target) => max_trace_length(target),
        Branch(_, if_true, if_false) =>
            max(max_trace_length(if_true), max_trace_length(if_false)),
    }
```

This bound is:
- **Static** — computed at compile time, deterministic
- **Tight** — the actual trace length equals this bound for the worst-case path
- **Useful** — the proof system can pre-allocate the trace matrix

### 7.4 Dead Block Elimination

An optimization pass can remove unreachable blocks:

```
tx example(x: u64) {
    if true {
        // always taken
        accounts[0].value = x
    } else {
        // dead code — compiler can remove this block entirely
        accounts[1].value = x
    }
}
```

When the condition is a compile-time constant, the compiler emits only the taken branch's block. This optimization is **semantics-preserving** — the program produces the same trace with or without the dead block, since the dead block was never executed anyway.

---

## 8. Verification Properties

### 8.1 Well-Formedness Rules

A `TxBody` is well-formed if:

1. **Non-empty**: `blocks.len() >= 1`
2. **Entry**: Block 0 has `param_count == 0`
3. **DAG**: For all `Jump(target)` and `Branch(_, if_true, if_false)`, `target.0 > current_block.0`
4. **Reachability**: Every block is reachable from Block 0
5. **Termination**: Every path from Block 0 reaches `Return` or `Abort`
6. **Type safety**: For every edge `(src, dst)`, the argument types match `dst.param_types`
7. **Slot safety**: Within each block, slots are accessed only after being defined (param or instruction dst)

Rules 3-7 are statically checkable by the compiler. Rule 3 is trivially enforced by the topological ordering.

### 8.2 Determinism

Given the same `TxBody`, `params`, and `StateSnapshot`, the execution produces the same:
- Taken path (sequence of block IDs)
- Trace (sequence of instruction executions)
- Overlay mutations (read set, write set)
- Emitted events

This follows from: deterministic condition evaluation → deterministic branch choices → deterministic block sequence.

---

## 9. DSL Examples: Full Programs

### 9.1 Token Transfer with Tiered Fees

```
table accounts {
    balance: u64
}

table fee_config {
    rate_bps: u64       // basis points (100 = 1%)
}

tx transfer(sender: u64, receiver: u64, amount: u64, tier: u64) {
    let sender_bal = accounts[sender].balance
    let receiver_bal = accounts[receiver].balance
    assert sender_bal >= amount

    let fee_bps = if tier == 0 {
        0                               // free tier
    } else if tier == 1 {
        50                              // 0.5%
    } else {
        let config_rate = @fee_config[tier].rate_bps
        config_rate                     // dynamic rate from static table
    }

    let fee = amount * fee_bps / 10000
    let net = amount - fee

    assert sender_bal >= amount + fee

    accounts[sender].balance = sender_bal - amount - fee
    accounts[receiver].balance = receiver_bal + net
    emit "transfer" (sender, receiver, net, fee)
}
```

Note: `@fee_config[tier].rate_bps` is a static table lookup that **only executes** when tier >= 2. In the multiplexer approach, this lookup would always execute.

### 9.2 Game Action with Branching

```
table units {
    hp: u64
    atk: u64
    unit_type: u64
}

tx attack(attacker: u64, defender: u64) {
    let atk_power = units[attacker].atk
    let def_hp = units[defender].hp
    let def_type = units[defender].unit_type

    // Type advantage: 2x damage against type 1
    let damage = if def_type == 1 {
        atk_power * 2
    } else {
        atk_power
    }

    if damage >= def_hp {
        // Defeated: set HP to 0
        units[defender].hp = 0
        emit "defeated" (attacker, defender)
    } else {
        // Survived: reduce HP
        units[defender].hp = def_hp - damage
        emit "damaged" (attacker, defender, damage)
    }
}
```

This produces 4 possible execution paths (2 conditions × 2 conditions), but only 1 path executes per transaction. With multiplexer, all 4 paths always execute.

---

## 10. Summary

| Aspect | Current (Flat IR) | Ideal (CFG IR) |
|--------|------------------|----------------|
| IR structure | `Vec<Instruction>` | `Vec<BasicBlock>` with terminators |
| Control flow | None | if/else, if/else-if/else, match |
| Branch cost | N/A | Only taken path executes |
| Trace length | Fixed per program | Path-dependent (bounded by DAG max) |
| Loops | None | None (DAG only — guaranteed termination) |
| Slot scope | Global (one Vec per tx) | Per-block (with block params for cross-block flow) |
| Instruction set | 9 instructions | Same 9 + Terminator variants |
| Compiler passes | 1 (single-pass lower) | 2-3 (lower → liveness → verify) |
| Proof model | Flat trace, fixed length | Variable trace, padded to power-of-2 |
| STARK efficiency | Optimal (no branches) | Optimal (only taken path in trace) |
| Opening proofs | All reads unconditional | Only actually-executed reads |

The ideal design maximizes STARK efficiency by ensuring the proving cost is proportional to **what actually executes**, not what could possibly execute. It achieves this while maintaining:
- Guaranteed termination (DAG, no loops)
- Deterministic execution (same inputs → same trace)
- Type safety (block param types verified at every edge)
- Backward compatibility (straight-line programs become single-block CFGs)
- Compile-time trace bounds (max path length computable statically)
