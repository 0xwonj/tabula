# Conditional Branching in Tabula

> **Status**: Path A (Select) implemented in M1; Path B (CFG) is future design
> **Prerequisites**: [dsl-philosophy.md](./dsl-philosophy.md), [architecture.md](../design/architecture.md)

---

## 1. Background: Branching in ZK Systems

### 1.1 The Fundamental Constraint

In arithmetic constraint systems (R1CS, PLONKish/AIR), circuits are **fixed structures** determined at compile time. There is no "skipping" a constraint at runtime — all gates evaluate every time.

Two fundamental approaches exist:

| Approach | Used By | Mechanism | Branch Cost |
|----------|---------|-----------|-------------|
| **Conditional Select** | Noir, Circom, Leo, o1js | `out = cond * a + (1-cond) * b` — both branches always execute | Sum of all branches |
| **True Branching** | Cairo, SP1, RISC Zero | Trace records only taken path; untaken branches = zero rows | Only taken path |

### 1.2 Why STARK Enables True Branching

In STARK/AIR systems, constraints are defined over the **execution trace** (a matrix of field values). The trace records only the taken path. Polynomial constraints verify each state transition was valid — they don't need to encode both branches.

Key evidence from [2025 zkVM research](https://arxiv.org/html/2508.17518): replacing branches with branchless arithmetic (predicated execution) **degraded** RISC Zero performance by 18.1%. In STARK-based systems, skipping untaken branches is cheaper than evaluating everything.

### 1.3 Tabula's Position

Tabula targets STARK/FRI proofs, placing it closer to Cairo than to R1CS/PLONK systems. Two viable paths:

| Path | IR Change | Proof Impact | Status |
|------|-----------|-------------|--------|
| **Path A**: Conditional Select | Add `Select` instruction, flat IR preserved | Minimal — 1 new constraint type | **Implemented (M1 S4)** |
| **Path B**: True Branching (CFG) | Basic block CFG with terminators | Major — variable-length traces | Future (post-STARK) |

**Incremental strategy**: Path A gives immediate `if/else` support. Path B defers the larger redesign until profiling shows the both-branches-execute cost is a bottleneck.

---

## 2. Path A: Conditional Select (Implemented)

Path A adds a `Select` instruction: `dst = cond * if_true + (1 - cond) * if_false`. Both arms always evaluate. This is implemented as:

- **IR**: `Select { dst, condition, if_true, if_false }` in `tabula-ir/src/instruction.rs`
- **Interpreter**: Evaluates both arms, stores selected result
- **AIR**: Single degree-2 constraint per select (ExecutionChip, opcode `op_select`)
- **DSL**: `let x = if cond { a } else { b }` lowers to Select

**Limitations** (motivating Path B):
- Both branches always execute — higher cost than necessary for STARK
- Conditional reads double opening proof cost
- Deep nesting creates large instruction sequences
- No conditional side effects (reads/writes/emits inside branches)

---

## 3. Path B: True Branching via CFG (Future Design)

The remainder of this document specifies the ideal branching design for Tabula — a CFG-based IR with true branching, where only the taken path appears in the execution trace.

### 3.1 IR: Basic Block Control Flow Graph

The flat `Vec<Instruction>` becomes a DAG of basic blocks:

```rust
/// Block identifier, indexing into TxBody.blocks.
/// Blocks are topologically ordered: jumps target strictly higher IDs.
/// This guarantees the CFG is a DAG (no cycles, guaranteed termination).
pub struct BlockId(pub u16);

/// A transaction body as a control flow graph.
pub struct TxBody {
    /// Basic blocks in topological order. Entry is always blocks[0].
    pub blocks: Vec<BasicBlock>,
}

/// A basic block: straight-line instructions + one terminator.
pub struct BasicBlock {
    /// Block parameters: slots 0..param_count pre-filled by predecessor.
    pub param_count: u16,
    pub param_types: Vec<ValueType>,
    /// Straight-line body (no control flow).
    pub body: Vec<Instruction>,
    /// How this block ends.
    pub terminator: Terminator,
}

/// Block terminator — the sole control flow mechanism.
pub enum Terminator {
    Jump { target: BlockId, args: Vec<ValueExpr> },
    Branch {
        condition: Predicate,
        if_true: BlockId,  true_args: Vec<ValueExpr>,
        if_false: BlockId, false_args: Vec<ValueExpr>,
    },
    Return,
    Abort { message: String },
}
```

### 3.2 Design Rationale

**Block parameters** (Sierra/MLIR style, not SSA phi nodes):
- Predecessor decides what to pass — no ambiguity at join points
- Block signature declares inputs — verifier checks types at each edge
- Natural mapping to slots: block params = pre-filled slots `0..param_count`

**Topological ordering invariant**: `Jump`/`Branch` targets must have strictly higher `BlockId`. This guarantees:
1. No cycles → no infinite loops → guaranteed termination without gas metering
2. Maximum trace length computable at compile time
3. Single forward pass for verification and interpretation

**Instruction enum is unchanged** — Read, Write, Add, Sub, etc. work identically within basic blocks. Only `Terminator` is new.

### 3.3 Slot Semantics

Each block has a local slot space. Block params fill `0..param_count`; instructions allocate from `param_count` onward. Block boundaries reset the slot space; data flows between blocks via terminator arguments.

```
Block 0 (entry):
  s0 = Read(accounts, Param(0), balance)
  s1 = Read(accounts, Param(1), balance)
  Assert(Gte(Slot(0), Param(2)))
  Branch(is_vip, Block(1) [Slot(0), Slot(1)],
                 Block(2) [Slot(0), Slot(1)])

Block 1 (params: [u64, u64]):      // VIP path: no fee
  s2 = Sub(Slot(0), Param(2))
  s3 = Add(Slot(1), Param(2))
  Write(accounts, Param(0), balance, Slot(2))
  Write(accounts, Param(1), balance, Slot(3))
  Jump(Block(3) [])

Block 2 (params: [u64, u64]):      // Normal path: with fee
  s2 = DivMod(Param(2), Literal(100))
  s4 = Sub(Param(2), Slot(2))
  s5 = Sub(Slot(0), Param(2))
  s6 = Add(Slot(1), Slot(4))
  Write(accounts, Param(0), balance, Slot(5))
  Write(accounts, Param(1), balance, Slot(6))
  Jump(Block(3) [])

Block 3 (params: []):               // Join
  Emit("transfer", [Param(0), Param(1), Param(2)])
  Return
```

### 3.4 Backward Compatibility

Programs with no `if/else` compile to a single-block CFG — semantically identical to today's `Vec<Instruction>`:

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

---

## 4. Execution Model

### 4.1 Block-Walking Interpreter

```rust
pub fn execute(body: &TxBody, params: &[Value], overlay: &mut Overlay, ...) -> Result<...> {
    let mut current_block = BlockId(0);
    let mut block_args: Vec<Value> = Vec::new();

    loop {
        let block = &body.blocks[current_block.0 as usize];
        let mut slots: Vec<Value> = block_args.clone();

        // Execute straight-line body
        for instr in &block.body {
            execute_instruction(instr, &mut slots, params, overlay, ...)?;
        }

        // Process terminator
        match &block.terminator {
            Terminator::Return => return Ok(output),
            Terminator::Abort { message } => return Err(abort_error),
            Terminator::Jump { target, args } => {
                block_args = resolve_args(args, &slots, params)?;
                current_block = *target;
            }
            Terminator::Branch { condition, if_true, true_args, if_false, false_args } => {
                if evaluate_predicate(condition, &slots, params)? {
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

### 4.2 Overlay Interaction

Overlay semantics are **unchanged**. Read/write sets become path-dependent (only actually-executed operations appear), which is correct — the proof verifies the execution trace including exactly the reads/writes that happened.

### 4.3 Trace Structure

Each instruction = one trace row. Block transitions produce a special `BlockTransition` row recording the condition, target, and passed arguments. The trace is variable-length, padded to power-of-2 for FFT.

---

## 5. DSL Syntax

### 5.1 `if` / `else`

```
tx transfer(sender: u64, receiver: u64, amount: u64, is_vip: bool) {
    let sender_bal = accounts[sender].balance
    assert sender_bal >= amount

    if is_vip {
        accounts[sender].balance = sender_bal - amount
        accounts[receiver].balance = accounts[receiver].balance + amount
    } else {
        let fee = amount / 100
        accounts[sender].balance = sender_bal - amount
        accounts[receiver].balance = accounts[receiver].balance + (amount - fee)
    }
}
```

### 5.2 `if` / `else if` / `else` Chain

```
let price = if tier == 1 {
    base_price * 90 / 100
} else if tier == 2 {
    base_price * 80 / 100
} else {
    base_price
}
```

Desugars to chained `Branch` terminators. Only the matching tier's block executes.

### 5.3 `if` Without `else`

```
if should_log {
    emit "updated" (user, new_balance)
}
```

False path jumps directly to the join block — zero trace rows if `should_log` is false.

### 5.4 Conditional Reads

With true branching, reads inside `if` blocks are safe and efficient:

```
if use_premium {
    let premium_rate = @premium_rates[user].value   // only read when needed
    accounts[user].rate = premium_rate
} else {
    accounts[user].rate = base_rate
}
```

In the multiplexer approach, `premium_rates` would always be read (wasting an opening proof).

---

## 6. Compiler Pipeline

```
Source (.tab) → Lex → Parse (AST with if/else nodes)
    → Lower: AST → CFG (basic blocks)
    → Live Value Analysis (backward pass — one pass since DAG)
    → Type Check (block param types match at every edge)
    → Verify (topological order, reachability, all paths terminate)
    → TxBody (CFG IR)
```

The lowering converts AST `if/else` nodes into basic blocks by:
1. Creating a join block for code after the `if`
2. Creating if-true and if-false blocks for each arm
3. Setting the current block's terminator to `Branch`
4. Live value analysis determines which values must be passed as block arguments

This requires 2-3 passes (vs current single-pass), but the DAG structure makes each pass simple.

---

## 7. AIR Constraints

### 7.1 Constraint Groups

**Instruction constraints**: Same as current (Read, Write, Add, Sub, etc.) — applied per-row based on opcode selector.

**Intra-block transitions** (same block, consecutive rows):
- `block_id[i+1] == block_id[i]`
- `instr_idx[i+1] == instr_idx[i] + 1`
- Slot values carry forward unless modified

**Block transitions** (terminator → next block's first row):
- Jump: `block_id[i+1] == target`
- Branch: `block_id[i+1] == (condition ? if_true : if_false)`
- Block params in next row match arguments from terminator

**Structural**: `block_id` non-decreasing; last non-pad row is `Return`.

### 7.2 STARK Efficiency Comparison

For a transfer with conditional fee (2 paths of ~8-10 instructions each):

| Metric | Select (Path A) | CFG (Path B) |
|--------|----------------|--------------|
| Trace rows (VIP) | 18 (both branches) | 8 (entry + VIP only) |
| Trace rows (normal) | 18 (both branches) | 10 (entry + normal only) |
| Opening proofs | All reads from both paths | Only path-relevant reads |

For programs with N branches of average size M:
- Select: O(N x M) constraints always
- CFG: O(M) constraints (only taken path)

---

## 8. Well-Formedness Rules

A `TxBody` is well-formed if:

1. `blocks.len() >= 1`
2. Block 0 has `param_count == 0`
3. All jump/branch targets have strictly higher `BlockId` (DAG)
4. Every block is reachable from Block 0
5. Every path reaches `Return` or `Abort`
6. At every edge, argument types match target's `param_types`
7. Within each block, slots accessed only after definition

All rules are statically checkable. Rule 3 is trivially enforced by topological ordering.

---

## 9. When to Implement Path B

**Defer Path B until**:
- Multiple real-world programs demonstrate >3 levels of nesting
- Profiling shows Select overhead exceeds 20% of total proof cost
- The STARK proof system (M9+) is implemented and stable

**Path B is backward-compatible**: straight-line programs become single-block CFGs. The DSL syntax (`if/else`) remains identical — only the lowering changes.

---

## 10. References

### ZK Language Documentation
- [Noir — Thinking in Circuits](https://noir-lang.org/docs/explainers/explainer-writing-noir)
- [Circom — Conditional Statements](https://rareskills.io/post/circom-if-statement)
- [Cairo Book — Sierra Appendix](https://www.starknet.io/cairo-book/appendix-09-sierra.html)
- [Understanding Sierra (StarkNet)](https://docs.starknet.io/build/starknet-by-example/advanced/sierra-ir)
- [Exploring Sierra (Nethermind)](https://medium.com/nethermind-eth/under-the-hood-of-cairo-1-0-exploring-sierra-7f32808421f5)
- [Leo Paper (ePrint)](https://eprint.iacr.org/2021/651.pdf)
- [o1js Provable API](https://docs.minaprotocol.com/zkapps/o1js-reference/type-aliases/Provable)

### Cost Analysis & Architecture
- [Evaluating Compiler Optimizations for zkVMs (2025)](https://arxiv.org/html/2508.17518)
- [zkEVM Architecture: Constraint-Level Design](https://arxiv.org/html/2510.05376v1)
- [R1CS Explainer (0xPARC)](https://learn.0xparc.org/materials/circom/additional-learning-resources/r1cs%20explainer/)
