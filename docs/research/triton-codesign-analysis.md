# Triton VM Co-Design Analysis and Tabula Implications

> Deep architectural comparison between Triton VM's ISA-AIR co-design philosophy
> and Tabula's language-first approach. Explores what Tabula can learn, what it
> should NOT adopt, and where it can go further than Triton.
>
> Date: 2026-03-09

---

## Core Thesis

Triton VM and Tabula occupy fundamentally different design spaces:

| | Triton VM | Tabula |
|---|---|---|
| **What it proves** | Arbitrary programs (general VM) | Fixed-schema state transitions |
| **Design driver** | Recursive STARK verification | Correct state commitment updates |
| **ISA philosophy** | AIR-first: "what constraints need" | Language-first: "what semantics mean" |
| **Instruction set** | 47 opcodes, 7-bit encoded | 13 operations, 12-way one-hot |
| **State model** | Stack machine, field-native | SSA registers, typed values |
| **Programs** | Arbitrary (unknown at setup) | Fixed at registration (known at setup) |

This last point is the key divergence. Triton must handle any program, so its
ISA and AIR must be universal. Tabula knows its programs at compile time,
which means it can be **more specialized than Triton, not less.**

---

## 1. Opcode Encoding: Bit Decomposition vs One-Hot

### Triton's Approach

7 bits, where the low 3 encode structural properties:

```
bit 0 (HasArg):      Gates instruction fetch width (1-word vs 2-word)
bit 1 (ShrinksStack): Gates OpStack permutation argument
bit 2 (IsU32):        Gates U32 coprocessor connection
bits 3-6:             Opcode index within category
```

Deselector polynomial for instruction I:

```
desel_I = ∏_{j=0}^{6} (if bit_j(I)=1 then ib_j else 1-ib_j)
```

Degree: 7 (product of 7 linear terms). Combined constraint:

```
C_k = Σ_I desel_I(ib0..ib6) · constraint_k(I)
```

**Columns used: 7** (ib0..ib6) + 7 boolean constraints.

### Tabula's Approach

12 one-hot selector flags:

```
op_read + op_write + op_arith + op_divmod + op_cmp + op_not + op_and +
op_or + op_assert + op_select + op_hash + op_lookup = 1
```

Plus 2 arith sub-selectors and 6 cmp sub-selectors = **20 selector columns.**

Constraint gating: `op_X * (constraint expression) = 0`

Degree: 1 (selector) + degree(constraint). No deselector product needed.

### Analysis

| Metric | Triton (7-bit) | Tabula (one-hot) |
|--------|---------------|------------------|
| Selector columns | 7 | 20 |
| Selector constraint degree | 7 (deselector product) | 1 (direct multiplication) |
| Combined constraint degree | 7 + max(instruction) = 19 | 1 + max(instruction) ≈ 4 |
| CSE potential | High (deselector sharing) | Low (each gate independent) |

**The tradeoff is clear**: Triton saves 13 columns but pays degree 19, requiring
degree lowering (adding ~230 auxiliary columns to bring to degree 4, or ~130
for degree 8). Tabula's one-hot approach avoids degree explosion but wastes
columns on the selector array.

### What Should Tabula Do?

**Neither approach is optimal for Tabula.** Here's why:

Triton needs 7 bits because it has 47 opcodes and must handle arbitrary programs
at runtime. The bit decomposition is a compression strategy for a large opcode
space.

Tabula has only 12 opcodes AND knows which ones appear at compile time. This
unlocks a third option: **compile-time opcode subsetting.**

```rust
// Program P uses only {Read, Write, Add, Assert}
// At registration time, generate a 4-opcode ExecutionChip:
//   op_read, op_write, op_add, op_assert (4 selectors)
//   No DivMod columns, no Cmp columns, no Hash columns
//   Total width: ~150 instead of 278
```

This is impossible for Triton (it doesn't know the program at setup), but
natural for Tabula. The compile-time knowledge makes both Triton's
bit-decomposition AND Tabula's current 12-way one-hot unnecessarily general.

---

## 2. Instruction Groups: Constraint Sharing

### Triton's 15 Groups

Triton defines reusable constraint bundles:

```
instruction_add = [
    st0' - (st0 + st1),           // add-specific
    ...step_1(...),                // ip' = ip + 1 (shared with ~25 opcodes)
    ...binop(...),                 // binary operation stack effect (shared with ~10)
    ...no_ram(...),                // RAM unchanged (shared with ~30)
    ...no_io(...),                 // I/O unchanged (shared with ~35)
    ...keep_jump_stack(...),       // JSP unchanged (shared with ~25)
]
```

Each instruction returns a flat vector of constraints, most of which come from
groups. The final combined constraint (via deselector) sums all instructions:

```
C_k = Σ_I desel_I * instruction_constraint_k(I)
```

Because the same group constraint appears in multiple instructions multiplied
by different deselectors, the multicircuit CSE can factor:

```
// Before CSE:
desel_add * (ip' - ip - 1) + desel_mul * (ip' - ip - 1) + ...

// After CSE:
(desel_add + desel_mul + ...) * (ip' - ip - 1)
= (1 - desel_hash - desel_call - ...) * (ip' - ip - 1)
```

This is why Triton's CSE achieves 1,790×: the deselector-weighted sums have
massive common factors.

### Tabula's Current Approach

No instruction grouping. Each opcode has independent constraint logic:

```rust
fn eval<AB: AirBuilder>(&self, builder: &mut AB) {
    self.constrain_common(builder);         // is_real, clock, tx_index
    self.constrain_access(builder);         // R/W access log
    self.constrain_operand_linkage(builder); // src1/src2/cond selectors
    self.constrain_slot_carry(builder);     // SSA state forward
    self.constrain_arith(builder);          // op_arith * (...)
    self.constrain_cmp(builder);            // op_cmp * (...)
    self.constrain_mul(builder);            // arith_is_mul * (...)
    self.constrain_divmod(builder);         // op_divmod * (...)
    self.constrain_hash(builder);           // op_hash * (...)
    self.constrain_select(builder);         // op_select * (...)
    self.constrain_control(builder);        // assert, not, and, or
}
```

Each `constrain_*` function is independently gated by its opcode selector.
There is some implicit sharing (e.g., `constrain_common` runs for all opcodes),
but no explicit instruction group abstraction.

### What Should Tabula Do?

Instruction groups are less impactful for Tabula because:

1. **Tabula has only 12 opcodes** (vs Triton's 47). The sharing surface is smaller.
2. **Tabula doesn't use deselector products**. One-hot selectors don't create
   the massive shared factors that make Triton's CSE so effective.
3. **Tabula's constraint structure is already flat**. `constrain_common` and
   `constrain_operand_linkage` already serve as implicit groups.

However, explicit grouping would still help for documentation and for the
**compile-time opcode subsetting** proposed above. Groups would formalize
which constraints are shared:

```
Group: "needs_src1"  →  {Arith, Cmp, DivMod, Not, And, Or, Assert, Select, Hash}
Group: "needs_src2"  →  {Arith, Cmp, DivMod, And, Or, Select}
Group: "writes_slot" →  {Read, Arith, Cmp, DivMod, Not, And, Or, Select, Hash, Lookup}
Group: "is_access"   →  {Read, Write}
```

When subsetting to `{Read, Write, Add, Assert}`, only groups that include at
least one active opcode need their constraints. The others are dead code.

---

## 3. Coprocessor Architecture: The Deeper Lesson

### Triton's Coprocessor Design

Triton delegates three categories of work to specialized tables:

| Coprocessor | Purpose | Connection | Why |
|-------------|---------|------------|-----|
| Hash Table | Tip5 permutation | Evaluation Argument | 6 rows per perm, 67 main cols, would triple ProcessorTable otherwise |
| U32 Table | Bitwise ops, div_mod | Lookup Argument | Decomposition over multiple rows, processor sees 1-cycle result |
| Cascade+Lookup | S-box decomposition | Lookup chain | 16-bit → 2×8-bit decomposition for Tip5's lookup-based S-box |

The key principle: **the processor sees expensive operations as single-cycle
instructions, but the proof cost is borne by the coprocessor table.** The
processor's AIR stays simple (low degree, few columns for the delegation), and
the coprocessor's AIR is specialized for its computation.

### Tabula's Coprocessor Architecture

Tabula already uses this pattern via LogUp buses:

| "Coprocessor" Chip | Purpose | Bus |
|---------------------|---------|-----|
| PoseidonChip | Poseidon2 permutation | PoseidonPerm (bus 5) |
| RangeCheckChip | u64 limb validation | RangeCheck (bus 8) |
| StaticTableChip | Static lookup | StaticTableLookup (bus 9) |
| StateColumnChip | SSMC commitment | CommitmentVerif (bus 6) |

### The Gap

Tabula's ExecutionChip still carries **inline witness columns** for
coprocessor-delegated operations:

```
hash_perm_input: [T; 16]   // 16 columns stored in ExecutionChip
hash_perm_output: [T; 8]    // 8 columns stored in ExecutionChip
                             // Then SENT to PoseidonChip via bus 5
```

These 24 columns exist in ExecutionChip purely to formulate the bus send.
In Triton's design, the Hash Table reads its input directly from the
processor state (stack values) via the cross-table evaluation argument —
the processor doesn't store the full permutation state.

**Tabula could eliminate these 24 columns** by having PoseidonChip directly
read from the ExecutionChip's slot values via a bus-level protocol that
carries slot indices rather than full values. The cost is a slightly more
complex bus fingerprint. Net savings: 24 columns (9% of ExecutionChip width).

Similarly, the **Cmp and DivMod witness columns** (27 + 36 = 63 columns)
could be factored into dedicated "ArithmeticWitnessChip" coprocessors:

```
// Current: ExecutionChip stores full CmpWitness inline (27 cols)
// Alternative: CmpChip with its own trace
//   ExecutionChip sends (src1_val, src2_val, cmp_op) via bus
//   CmpChip returns (result_bool) via bus
//   CmpChip's trace has 27 cols but only for rows that actually do Cmp
```

This trades inline columns for bus interactions. The net effect:
- ExecutionChip drops from 278 to ~191 columns (-31%)
- CmpChip: 27 cols × (num_cmp_instructions / next_power_of_two) rows
- DivModChip: 36 cols × (num_divmod_instructions / next_power_of_two) rows
- ArithChip: 5 cols × (num_mul_instructions / next_power_of_two) rows

When an opcode appears rarely, the dedicated chip's trace is tiny, and the
PCS cost is proportional to actual usage rather than padded into every
ExecutionChip row.

---

## 4. Recursion as Architectural Input

### Triton's Position

Triton treats recursive verification as the **top-level design constraint**.
Every architectural decision is filtered through "does this make the recursive
verifier cheaper?"

Evidence:
- ISA includes `merkle_step`, `xx_dot_step`, `xb_dot_step` — opcodes that
  exist solely to accelerate the STARK verifier
- `recurse_or_return` encodes a specific loop shape (same-frame re-entry)
  optimized for Merkle path traversal
- Program attestation (`st11..st15` = self-digest) solves the bootstrapping
  problem for recursive self-verification
- Tip5 chosen specifically for recursive STARK friendliness
- The spec section "Triton Assembly Constraint Evaluation" quantifies recursive
  verifier cost as a design metric

### Tabula's Position

Tabula's current architecture is **recursion-compatible but not recursion-
optimized**:

- Poseidon2/BabyBear is the right hash/field choice for future recursion
- LogUp bus composition reduces to scalar balance checks (minimal recursive
  interface)
- Static (t,c) addressing means per-column proofs have known structure
- Public input is minimal (old_root, new_root: 16 FEs)

### Should Tabula Optimize for Recursion?

**Not now. But the answer changes with scale.**

The crossover analysis from Tabula's existing research:

| Scale | Global Proof | Recursive | Winner |
|-------|-------------|-----------|--------|
| C ≤ 50 columns | 2–5s | ~60s | Global |
| C ≈ 200 columns | 20–50s | ~60s | Tie |
| C ≥ 1000 columns | minutes | ~60s | Recursive |
| R > 100K rows/col | OOM risk | Bounded | Recursive |

Tabula's current test cases have C < 50. Recursion is justified when:
1. Production deployments reach hundreds of columns per batch
2. Proof parallelism across machines/GPUs is needed
3. Proof size must be constant regardless of batch size

**The critical insight**: Tabula does NOT need Triton-style ISA co-design for
recursion. Triton needs `merkle_step` because it's a general VM that must
execute the verifier as a Triton program. Tabula doesn't execute its verifier
as a Tabula program — it would build a **separate recursive verifier AIR**, as
SP1 and RISC Zero do.

This means recursion is an **add-on layer**, not an architectural redesign:

```
Tabula's recursion path:

1. Shard batch into per-column proofs (already planned: Goal 7)
2. Each shard proof has public inputs: old_com, new_com, bus_cumsum
3. Build a dedicated RecursiveVerifierChip that:
   - Verifies N shard proofs
   - Checks Σ bus_cumsum = 0
   - Checks Merkle root consistency
4. Wrap in Groth16 for on-chain verification

None of this requires changing Tabula's IR or ExecutionChip.
```

---

## 5. What Tabula Can Do That Triton Cannot

The most important takeaway is not what Tabula should copy from Triton, but
what Tabula can do that Triton fundamentally cannot:

### 5.1 Program-Specific Proving

Triton must handle any program → universal constraint set → high overhead.
Tabula knows the program at registration → program-specific constraint set.

**Concrete example**: A transfer program with 8 instructions:
```
Read(balance_from)      // slot 0
Read(balance_to)        // slot 1
Arith(Sub, s0, amount)  // slot 2
Arith(Add, s1, amount)  // slot 3
Assert(Cmp(Gte, s2, 0)) // slot 4, 5
Write(balance_from, s2)
Write(balance_to, s3)
```

**Triton**: Must support all 47 opcodes × 150 transition constraints × degree
19 (→ degree-lowered to degree 4 with 643-width trace). The transfer program
uses only 4 of 47 opcodes but pays for all 47.

**Tabula (optimized)**: Generate a TransferChip with:
- 4 opcode selectors (Read, Write, Add, Assert)
- 4 slots (not 16)
- No Mul/DivMod/Cmp/Hash/Select/Lookup columns
- Width: ~80 columns (vs 278 universal, vs 643 Triton)

This is a **8× column reduction over Triton** for the same semantic operation.

### 5.2 Schema-Aware Type Specialization

Triton's stack is untyped (every element is a field element). Tabula knows the
schema type of every column at compile time.

**What this enables**: If a program only touches Bool and U64 columns:
- Skip Digest-width (W=8) encoding entirely
- Bool columns: 1 FE per value (not 3)
- Range checks: tailored to actual value ranges
- Estimated savings: 30–50% of trace width for Bool-heavy programs

### 5.3 Static Access Pattern Optimization

Tabula's IR has static `(table, col)` addresses (invariant I2). This means the
memory access pattern is known at compile time.

**What this enables**: The `classify_keys()` analysis in `witness/classify.rs`
already identifies:
- ReadOnlyOpening: keys that are only read → no merge proof needed
- ShortRun: keys accessed by a single transaction → lightweight chip
- SortedMemory: keys accessed across transactions → full GlobalSortedMem

A Triton-style VM cannot do this because memory access patterns are runtime-
determined.

### 5.4 Batch-Level Amortization

Tabula processes batches of transactions. The batch structure enables:
- Inter-transaction state coalescing (GlobalSortedMem)
- Shared PCS commitment across all transactions
- Bus balance amortized over the full batch

Triton processes one program execution at a time. Multi-execution aggregation
requires external sharding + recursion.

---

## 6. The Design Space Tabula Should Explore

Based on this analysis, Tabula's optimization path is NOT "become more like
Triton." It is "exploit program-specific knowledge more aggressively."

### Level 0: Current Architecture (278-col universal ExecutionChip)

No specialization. All programs pay for all opcodes.

### Level 1: Constraint Mask (skip unused opcode evaluation)

Cheapest optimization. At registration, compute which opcodes appear:
```rust
struct ConstraintMask { has_arith: bool, has_cmp: bool, ... }
```
In `eval()`, skip constraint blocks for absent opcodes. Columns still present
but zero; PCS still commits them. Saves: ~15–25% on constraint evaluation.

### Level 2: Column Subsetting (remove unused opcode columns)

At registration, generate a specialized column layout:
```rust
// Program uses {Read, Write, Add, Assert} with max_slot=4
struct ExecutionCols_Transfer<T> {
    // 4 selectors (not 12)
    // 4 slots × 4 (not 16 × 4)
    // No cmp/mul/divmod/hash witness columns
    // Width: ~100 (not 278)
}
```
Requires code generation (proc-macro or build script). Saves: ~40–60% on all
PCS operations.

### Level 3: Per-Opcode Coprocessor Chips

Factor witness-heavy opcodes into dedicated chips:
```
ExecutionChip (~100 cols, common + lightweight opcodes)
  ├── CmpChip (27 cols, only Cmp rows)
  ├── MulChip (5 cols, only Mul rows)
  ├── DivModChip (36 cols, only DivMod rows)
  └── HashDelegationChip (24 cols, only Hash rows)

Connected via LogUp buses.
```
Each dedicated chip's trace height is proportional to its instruction count,
not the total instruction count. Saves: ~60–70% for programs that use few
complex operations.

### Level 4: Template Chips (Program-Specific AIR)

Generate a complete program-specific AIR at registration time:
```
transfer_program.tab → compile → register → generate TransferAIR
                                              ├── 8 rows (one per instruction)
                                              ├── ~60 columns (exactly what's needed)
                                              ├── Degree ≤ 3 (no selector products)
                                              └── No unused witness columns
```
This is the endpoint: a program-specific constraint circuit with zero overhead.
It is what Triton's multicircuit + CSE approximates at runtime, but Tabula can
achieve exactly at compile time.

### Level 5: Recursive Composition (when scale demands)

Build a separate RecursiveVerifierChip (not expressed in Tabula IR). This layer
wraps per-column shard proofs and verifies bus balance. Implementation is
independent of Levels 0–4.

---

## 7. Comparison Summary

| Design Decision | Triton VM | Tabula (Current) | Tabula (Proposed) |
|---|---|---|---|
| Opcode encoding | 7-bit decomposition | 12-way one-hot | Per-program subset |
| Constraint sharing | 15 instruction groups + CSE | Implicit (`constrain_common`) | Compile-time opcode subsetting |
| Coprocessor delegation | Hash Table, U32 Table | PoseidonChip, RangeCheckChip | + CmpChip, DivModChip, MulChip |
| Witness columns | Per-row (universal) | Per-row (universal) | Per-opcode chip (proportional) |
| Recursion | Top-level design input | Compatible, not optimized | Separate layer when needed |
| Column count (transfer) | ~643 (degree 4) | ~278 | ~60–100 (Level 2–4) |
| Program knowledge | None (arbitrary) | Full (at registration) | Aggressively exploited |

---

## 8. Key Takeaways

### What Tabula Should Learn From Triton

1. **Coprocessor delegation is the right pattern.** Move witness-heavy operations
   (Cmp, DivMod, Mul) into dedicated chips. This is Triton's Hash Table /
   U32 Table pattern applied to Tabula's operations.

2. **CSE matters enormously.** Triton's 1,790× speedup from multicircuit CSE
   motivates extracting a constraint DAG from Tabula's `eval()` and applying
   subexpression elimination. Even without Triton's deselector-level sharing,
   Tabula's `is_real` gating + selector flags offer 5–15× CSE potential.

3. **Constraint degree drives architecture.** Triton's degree-19 penalty from
   7-bit deselectors required 130–230 auxiliary columns for degree lowering.
   Tabula's degree-4 constraints avoid this entirely. Preserve this advantage.

### What Tabula Should NOT Copy From Triton

1. **Bit-decomposed opcode encoding.** Triton needs it for 47 opcodes; Tabula's
   12 opcodes don't justify the degree explosion. Program-specific subsetting
   is strictly better.

2. **Recursion-driven ISA design.** Triton needs `merkle_step` because it runs
   the verifier as a Triton program. Tabula would use a separate verifier
   circuit. ISA co-design for recursion is unnecessary.

3. **Universal constraint set.** Triton must support all programs with one AIR.
   Tabula can generate program-specific AIRs, which is fundamentally more
   efficient.

### Where Tabula Can Go Further Than Triton

1. **Program-specific chip generation** (Level 2–4) achieves what Triton's
   multicircuit approximates — but exactly, at compile time, with zero runtime
   overhead.

2. **Type-aware constraint elision** (abstract interpretation on typed IR)
   is impossible for Triton's untyped stack machine.

3. **Static access pattern optimization** (KeyRoute classification) exploits
   Tabula's invariant I2 in ways unavailable to general VMs.

4. **Batch-level amortization** (shared PCS, inter-tx coalescing) is a
   structural advantage of Tabula's state machine model.

---

## References

- Triton VM Specification: https://triton-vm.org/spec/
- Triton VM GitHub: https://github.com/TritonVM/triton-vm
- Tip5 Hash: https://eprint.iacr.org/2023/107
- Neptune Cash: Speed Up STARK Provers with Multicircuits
- SP1 Architecture: https://blog.succinct.xyz/introducing-sp1/
- OpenVM No-CPU Architecture: https://openvm.dev/whitepaper.pdf
- Tabula proof-optimization-architecture.md (internal)
- Tabula full-sharding-research.md (internal)
