# Compiler Optimization Research for Tabula

> Exploratory research analyzing compiler optimization techniques applicable
> across the full Tabula proving pipeline. Considers aggressive architectural
> changes where justified by performance gains.
>
> Date: 2026-03-09

---

## Thesis

Tabula's proving pipeline is a **compilation pipeline in disguise**:

```
IR Program  →  Execution  →  Witness  →  Trace Matrices  →  Polynomials  →  Proof
    ↑              ↑            ↑              ↑                 ↑             ↑
 source code   interpret    lower/encode    layout data     evaluate/NTT    commit
```

Each arrow is a transformation where compiler techniques apply. The key insight:
**Tabula programs are small (10–100 instructions), fixed at registration time,
and executed repeatedly across transactions.** This profile is ideal for
aggressive ahead-of-time specialization — the compilation cost is amortized
over many proving invocations.

---

## 1. Partial Evaluation: The First Futamura Projection

### Concept

The first Futamura projection states: **specializing an interpreter with
respect to a known program produces a compiled program.** Given interpreter
`I(program, input) → output` and a fixed program `P`, the specializer produces
`I_P(input) → output` where all program-dependent dispatch is eliminated.

### Application to Tabula

At `Program::register()` time, the full instruction sequence, opcode selectors,
slot indices, table/column IDs, and SSA def-use chain are known. A partially
evaluated executor would:

**Eliminate dispatch overhead:**
```rust
// Before (interpreter): 13-way match per instruction
for instr in instructions {
    match instr {
        Read { .. } => { ... }
        Arith { .. } => { ... }
        // 11 more arms
    }
}

// After (specialized): direct function call sequence
fn execute_tx_transfer(params: &[Value], overlay: &mut Overlay) {
    let v0 = overlay.read(&CELL_BALANCE_FROM)?;    // inlined Read
    let v1 = v0.checked_sub(&params[0])?;           // inlined Arith(Sub)
    overlay.write(&CELL_BALANCE_FROM, v1)?;          // inlined Write
    // ... no dispatch, no enum matching
}
```

**Eliminate resolution overhead:**
```rust
// Before: resolve_value_expr dispatches on Literal/Slot/Param per operand
let lhs = resolve_value_expr(&expr.lhs, slots, params)?;

// After: operand source is known at specialization time
let lhs = slots[3];     // Slot(3) → direct index, no match
let rhs = Value::U64(1); // Literal → inlined constant
```

**Eliminate bounds checking:**
SSA validation at registration proves all slot accesses are in-bounds. The
specialized executor can use unchecked indexing.

### Impact Assessment

The interpreter itself is not a bottleneck (5,000 dispatches × 10ns = 50μs vs
proving in seconds). **However**, partial evaluation's real value is downstream:

**Witness lowering specialization** is where this matters most. The
`lower_program_batch` function in `witness/src/trace/lowering/` re-walks the IR
to produce `InstructionRecord` values. A specialized lowerer pre-computes:

- Which `InstructionRecord` fields are non-zero for each instruction
- The exact `src1_slot_idx`, `src2_slot_idx` values (static)
- Which instructions produce access events
- The carry/witness column pattern for each arithmetic instruction

This eliminates per-instruction branching during the hot path of trace emission.

### Implementation Path

Rather than a full Futamura-style specializer (complex infrastructure), use
**staged compilation** via Rust's type system:

```rust
// At registration time, produce a SpecializedTxType
struct SpecializedTxType {
    // Pre-computed lowering plan: Vec<LoweringStep>
    // Each step knows exactly which trace columns to fill
    steps: Vec<LoweringStep>,
    slot_count: usize,
    access_pattern: Vec<(TableId, ColId)>,  // static
}

enum LoweringStep {
    ReadAndEncode { dst_slot: usize, cell: CellKey, ... },
    ArithAdd { dst_slot: usize, src1_slot: usize, src2_slot: usize, ... },
    // ... one variant per concrete instruction, no dispatch needed
}
```

---

## 2. Constraint Subexpression Elimination (CSE)

### The Triton VM Result

Triton VM achieved **1,790× speedup** on constraint evaluation by converting
from HashMap-based polynomial representation to a shared DAG with common
subexpression elimination. Even though Tabula doesn't use HashMaps (Plonky3's
trait-based `eval()` is monomorphized), CSE still offers significant gains.

### Why rustc/LLVM Cannot Do This

LLVM's CSE pass operates on LLVM IR, where expressions are low-level operations
(loads, stores, arithmetic). It cannot see through:

- **Trait method boundaries**: `AirBuilder::send()` and `AirBuilder::receive()`
  are opaque to LLVM — it cannot identify that two `send()` calls share a
  common fingerprint prefix.
- **Generic instantiation boundaries**: Each chip's `eval()` is independently
  monomorphized. LLVM does not share subexpressions across chip evaluations.
- **Runtime values masquerading as compile-time structure**: Selector flags
  (`op_arith`, `op_cmp`, etc.) are algebraically known to be one-hot, but
  LLVM sees them as runtime `BabyBear` values with no algebraic properties.

### What Domain-Specific CSE Can Exploit

Tabula's constraint structure has three categories of sharable subexpressions:

**Category A: Selector products (high sharing)**

```
// ExecutionChip has ~30 boolean columns, each appearing in multiple constraints.
// is_real alone gates ~50% of all constraints.
// Selector combinations like (op_arith * arith_is_mul) appear in 5+ constraints.

// Pre-compute:
let s_add = op_arith * (one - arith_is_sub) * (one - arith_is_mul);
let s_mul = op_arith * arith_is_mul;
// Reuse across all Add/Mul constraints
```

**Category B: Limb encodings (medium sharing)**

```
// src1_val[0..3] encoding is read by Arith, Cmp, Mul, DivMod, Select constraints
// Instead of each constraint independently reading src1_val_limbs:
let src1 = (src1_val[0], src1_val[1], src1_val[2]);  // read once
// Pass to all constraint groups
```

**Category C: Bus fingerprints (high sharing)**

```
// Multiple bus sends share the same Horner evaluation structure:
//   f = alpha + tag + beta * v0 + beta^2 * v1 + ...
// If two sends have overlapping prefix fields (t, c, r), the prefix
// computation can be shared.
```

### Implementation Approach: SymbolicAirBuilder Extension

Plonky3 already has `SymbolicAirBuilder` that collects constraint expressions
as `SymbolicExpression` DAG nodes during keygen. Extend this to:

1. Collect all constraint expressions from `eval()` into a shared DAG
2. Run topological sort + reference counting
3. Identify nodes with refcount > 1 (shared subexpressions)
4. Generate an optimized `eval_cse()` function via `proc-macro2` + `quote`
5. Compile the generated code at build time (like Triton VM's approach)

### Estimated Impact

| Chip | Columns | Est. Shareable Nodes | Est. Eval Speedup |
|------|---------|---------------------|-------------------|
| ExecutionChip | 278 | ~40% of expression nodes | 5–20× |
| MergeChip | 74 | ~25% | 3–10× |
| StateColumnChip | 67 | ~30% | 3–10× |
| InterTxOrderChip | 67 | ~20% | 2–5× |
| PoseidonChip | 93 | ~15% (mostly unique S-box) | 1.5–3× |

Overall constraint evaluation phase: **5–15× speedup** (vs Triton's 1,790× —
smaller because Tabula already avoids HashMap overhead).

---

## 3. Program-Specific Chip Specialization

### The Architectural Opportunity

This is the most aggressive optimization: **generate a program-specific chip
layout at compile time**, eliminating the universal ExecutionChip entirely.

Currently, ExecutionChip has 278 columns to handle all 13 instruction types.
A program using only `{Read, Write, Arith(Add), Assert}` wastes columns for
Hash (24), Mul (5), DivMod (36), Cmp (27), Select (4), Lookup (3) = **99
columns of waste** (36% of width).

### Three Levels of Specialization

**Level 1: Constraint Mask (low effort, 15–25% speedup)**

Keep the 278-column layout but skip evaluating constraints for unused opcodes.
A bitmask computed at registration time gates constraint evaluation:

```rust
struct ConstraintMask {
    has_arith: bool,
    has_mul: bool,
    has_divmod: bool,
    has_cmp: bool,
    has_hash: bool,
    // ...
}

fn eval_masked<AB>(&self, builder: &mut AB, mask: &ConstraintMask) {
    self.constrain_common(builder);        // always
    if mask.has_arith { self.constrain_arith(builder); }
    if mask.has_cmp   { self.constrain_cmp(builder); }
    // ...
}
```

Benefit: Zero architectural change. Constraint evaluation skips dead blocks.
Cost: Trace width unchanged (columns still committed via PCS).

**Level 2: Width Reduction (medium effort, 30–50% speedup)**

Generate a program-specific column layout where unused opcode groups are
removed. The column struct becomes compile-time specialized:

```rust
// Universal: 278 columns
struct ExecutionCols<T> {
    common: CommonCols<T>,        // ~100 cols (always present)
    arith: ArithCols<T>,          // ~30 cols (if has_arith)
    mul: MulCols<T>,              // ~5 cols  (if has_mul)
    divmod: DivModCols<T>,        // ~36 cols (if has_divmod)
    cmp: CmpCols<T>,              // ~27 cols (if has_cmp)
    hash: HashCols<T>,            // ~24 cols (if has_hash)
    slots: [SlotCols<T>; S],      // S * 4 cols (S = actual max_slot)
    selectors: [T; S],            // S cols per selector array
}

// Specialized for {Read, Write, Add, Assert} with max_slot=4:
// common(100) + arith(30) + slots(4*4=16) + selectors(4*3=12) = 158 cols
// Savings: 278 - 158 = 120 columns (43% reduction)
```

This requires generating the column struct, `eval()`, and `generate_trace()`
at build time, parameterized by the program's instruction profile.

**Level 3: Template Chips (high effort, 60–80% speedup)**

The most aggressive: decompose the universal ExecutionChip into
**per-instruction-type micro-chips** connected via LogUp buses:

```
ExecutionChip(278 cols)  →  ReadChip(20 cols) + WriteChip(20 cols)
                            + ArithAddChip(15 cols) + AssertChip(5 cols)
                            + InstructionOrderingBus (ensures correct execution order)
```

This is OpenVM's "no-CPU" architecture. Each micro-chip has minimal columns for
its instruction type. The instruction ordering bus ensures correct sequencing.

Benefits:
- Each micro-chip's trace height matches its instruction count (no padding for
  other instruction types)
- Column width is minimal per chip
- Constraint degree is lower (no selector multiplication needed)

Costs:
- Additional LogUp bus overhead for instruction ordering
- More chips means more FRI overhead (one commitment per chip)
- Complex code generation infrastructure

### Per-Program Slot Count Optimization

`MAX_SLOTS = 16` is hardcoded but programs typically use 4–8 slots. The slot
count determines:

- `slot_val[S][W]` = S × W columns (16 × 3 = 48 currently)
- `slot_is_null[S]` = S columns (16 currently)
- `src1_sel[S]`, `src2_sel[S]`, `cond_sel[S]` = 3 × S columns (48 currently)
- `slot_written[S]` = S columns (16 currently)

Total slot-related: **5S + SW = 5(16) + 16(3) = 128 columns**.
With S=4: **5(4) + 4(3) = 32 columns**. Savings: **96 columns**.

This alone (without any opcode specialization) reduces ExecutionChip from
278 to 182 columns (34% reduction).

---

## 4. Abstract Interpretation for Constraint Elision

### Type-Driven Elision

`BodyTypeInfo` already computes `slot_types: Vec<Option<ValueType>>`. This
information, currently used only for validation, can drive constraint
simplification:

**Boolean slots (W=1 instead of W=3):**
```
If slot_types[s] = Some(Bool):
  - slot_val[s] needs only 1 column (not 3)
  - No range-check bus sends needed (boolean constraint suffices)
  - Carry columns for this slot are always zero
  Savings: 2 value columns + 4 RC sends per boolean slot
```

**Never-null slots:**
```
If a slot is the output of Arith/Cmp/Not/And/Or/Select/Hash:
  - These instructions never produce null
  - slot_is_null[s] is provably 0
  - Can be constrained as constant rather than checked per-row
  Savings: 1 constraint per such slot (minor)
```

### Range Analysis

A range abstract domain tracks `[lo, hi]` intervals through the instruction
sequence:

```
slot[0] = Param(0)           // range: [0, 2^64 - 1] (unknown U64)
slot[1] = Literal(U64(100))  // range: [100, 100]
slot[2] = Cmp(Lt, 0, 1)      // range: [0, 1] (boolean result)

// If slot[2] is asserted true:
// slot[0] range narrows to [0, 99] — fits in 7 bits
// No u64 limb decomposition needed for slot[0] after this point
```

**Constraint elision opportunities:**

| Condition | Savings |
|-----------|---------|
| Value fits in single BabyBear limb (< 2^30) | Skip 2 upper limb columns + 4 RC sends |
| Value is known constant | Replace column with preprocessed constant |
| Comparison result is statically known | Skip CmpWitness (27 cols for that instruction) |
| Mul operands < 2^15 | Carry chain is provably zero, skip MulCarry (5 cols) |
| DivMod divisor is known constant | Simplify constraint to modular reduction |

### Algebraic Constraint Distillation

The "Distilling Constraints" technique (CAV 2022) applies Gaussian elimination
over the constraint system to identify and remove redundant constraints:

1. Collect all constraints as polynomials over BabyBear
2. The one-hot constraint `Σ op_i = 1` implies `op_i * op_j = 0` for i ≠ j
3. Any constraint containing `op_i * op_j` as a factor is trivially zero
4. Boolean constraints `x * (1-x) = 0` imply `x^2 = x`, allowing degree
   reduction in constraints containing `x^2`

For ExecutionChip with ~30 boolean columns and 13 one-hot opcode selectors,
this can eliminate **15–30% of constraint terms** through algebraic simplification.

---

## 5. Data Layout and Memory Optimizations

### Trace Matrix Layout

Plonky3 uses row-major `RowMajorMatrix<BabyBear>` — optimal for trace
generation (sequential row writes) and constraint evaluation (row-parallel
SIMD). However, **NTT operates per-column**, making row-major suboptimal for
the NTT phase.

**Recommendation: Explicit transpose before NTT**

```
Trace Generation:  Row-major (natural write order)
                        ↓ transpose
NTT / LDE:         Column-major (natural NTT order)
                        ↓ transpose
Constraint Eval:    Row-major (natural eval order)
```

The transpose cost is O(n) (one pass over the data). The NTT speedup from
column-major access is estimated at **1.5–2× for large traces** (> L2 cache)
due to elimination of stride-`width` memory access patterns.

### Huge Pages

For a 278-column trace with 2^20 rows:
- Size: ~1.1 GB
- Standard 4KB pages: 275,000 TLB entries (severe TLB pressure)
- 2MB huge pages: ~550 TLB entries

Research shows **2–3× speedup** from huge pages for sequential access patterns
of this scale. On Linux, this is achieved via `madvise(MADV_HUGEPAGE)` on
mmap'd allocations. On macOS (current dev), transparent huge pages are limited.

### Eliminating Trace Clones

The current prover clones trace matrices when building `ChipProveInfo`. For a
multi-chip system with total trace size of 2–3 GB, this doubles peak memory.
Solution: transfer ownership or use `Arc<RowMajorMatrix>`.

### SIMD Vectorization Gaps

Plonky3 provides packed BabyBear types (AVX2: 8 elements, AVX-512: 16 elements,
NEON: 4 elements). Constraint evaluation uses these via `PackedVal`. However:

- **Trace generation is fully scalar** — each chip's `generate_*_trace()` writes
  one row at a time. Independent chips could generate traces in parallel
  (rayon), but within a chip, the SSA slot carry creates sequential dependency.
- **Permutation trace generation is scalar** — `generate_permutation_trace_from_interactions()`
  computes EF4 fingerprints row-by-row. Could vectorize by computing fingerprints
  for 4/8 rows simultaneously using packed EF4.

---

## 6. NTT and FRI Optimizations

### NTT Algorithm Selection

| Algorithm | Cache Behavior | Arithmetic Ops | Memory Traffic |
|-----------|---------------|----------------|----------------|
| Radix-2 DIT (current) | Poor for large N | 5N log N | N log N |
| Radix-4 / Radix-2² | Good (half passes) | 4.25N log N | 0.5N log N |
| 6-step FFT | Excellent (cache-blocked) | 5N log N + 2N | N + sub-FFT |
| Bowers gFFT (in Plonky3) | Good (fewer twiddle loads) | 5N log N | N |

**Recommendation**: For traces exceeding L2 cache (> 256KB, i.e., > 64K rows
at width 1), the 6-step variant provides the best cache behavior. Plonky3's
`Radix2DitParallel` already uses a split approach but could benefit from
explicit radix-4 butterflies within each block.

### FRI Configuration Tuning

Current: `log_blowup = 3, num_queries = 2, pow_bits = 1` (test-friendly).

Production targets:

| Security Target | log_blowup | num_queries | Proof Size | Prover Cost |
|----------------|------------|-------------|------------|-------------|
| 100-bit | 3 | 34 | ~200 KB | baseline |
| 100-bit | 4 | 25 | ~170 KB | 1.33× NTT |
| 128-bit | 3 | 43 | ~250 KB | baseline |
| 128-bit | 4 | 32 | ~210 KB | 1.33× NTT |

Higher blowup reduces query count (smaller proof) at the cost of larger LDE
(more NTT work). **Fold-by-4** (instead of fold-by-2) halves the number of FRI
layers, reducing Merkle tree construction cost by ~50%.

### Non-Algebraic Merkle Hash

Poseidon2 is used for both in-circuit hashing (PoseidonChip constraints) and
Merkle tree commitment. These serve different purposes:

- **In-circuit**: Must be algebraically expressible (AIR constraints). Poseidon2
  is optimal.
- **Commitment Merkle tree**: No algebraic constraint needed. The verifier
  re-computes hashes natively.

Switching the Merkle tree hash from Poseidon2 to **BLAKE3** would provide:
- ~10× faster hashing on CPU (BLAKE3 uses SIMD natively)
- Merkle tree construction is ~60% of commitment time
- Net commitment speedup: **~5×**

Plonky3 supports configurable hash backends via the `Compress` trait. This is
a configuration change, not an architectural change.

### GKR for LogUp (Medium-Term)

The GKR sum-check protocol replaces polynomial commitments with interactive
rounds for LogUp accumulation:

- **Standard LogUp**: Commit permutation trace columns (4 × interactions_per_row
  per chip) via PCS. Cost: O(N log N) for NTT + Merkle.
- **GKR LogUp**: Run sum-check protocol. Prover cost: O(N). No additional PCS
  commitment needed.

For Tabula's 11 buses, the permutation trace can be significant. GKR could
reduce the PCS commitment phases by **20–30%** by eliminating permutation trace
commitments.

---

## 7. Cross-Chip and Whole-Program Optimizations

### Compile-Time Proof Planning

A `ProofPlan` computed from the `Program` IR before execution:

```rust
struct ProofPlan {
    // Which chips are needed (skip unused ones entirely)
    active_chips: Vec<ChipId>,

    // Per-chip width specialization
    width_profile: BTreeMap<ChipId, usize>,  // W=1, W=3, or W=8

    // Execution chip specialization
    max_slots: usize,           // actual max, not hardcoded 16
    used_opcodes: OpcodeSet,    // which instruction types appear
    constraint_mask: u32,       // bitmask of active constraint groups

    // Height predictions (from ProgramBudgets)
    height_estimates: BTreeMap<ChipId, usize>,

    // Active buses (skip permutation trace for inactive buses)
    active_buses: BTreeSet<BusId>,

    // Shard plan (for large batches)
    shard_boundaries: Option<Vec<usize>>,
}
```

This enables:
- Skip PCS commitment for zero-traffic buses
- Skip keygen for unused chips
- Pre-allocate trace buffers at predicted sizes
- Select optimal FRI parameters based on predicted trace heights

### Chip Fusion Candidates

| Candidate | Shared Structure | Height Match? | Net Benefit |
|-----------|-----------------|---------------|-------------|
| SmtColPath + SmtTablePath | Column layout identical | Similar | Save 1 chip overhead |
| StateColumn + ColumnMeta | Share (t,c) identifiers | Different | Net loss (padding) |
| RangeCheck integration | Move RC into sender chips | N/A | Eliminate 1 global chip |

**RangeCheck integration** is the most impactful: instead of a separate
RangeCheck chip with preprocessed table, embed range-check constraints directly
into the sender chips using half-decomposition columns. This eliminates:
- The RangeCheck chip (2 main columns + 65,536-row preprocessed trace)
- All RangeCheck bus sends/receives
- The associated permutation trace columns

Cost: Each sender chip adds half-decomposition columns locally (already present
in some chips). This trades LogUp overhead for local constraint overhead.

### Batch Sharding for Parallelism

For large batches, split the ExecutionChip trace into shards of ~2^16 rows:

```
Batch with 100,000 instructions
  → Shard 0: instructions 0–65535     (proven on GPU 0)
  → Shard 1: instructions 65536–99999 (proven on GPU 1)

Each shard:
  - Independent ExecutionChip trace
  - Contributes to shared LogUp bus sums
  - Recursive proof combines shard proofs
```

This enables **linear horizontal scaling** of proving throughput with hardware.
The LogUp bus balance check becomes: `Σ_shards cumsum_final = 0`, verified in
the recursive proof.

---

## 8. GPU Offloading Strategy

### Phase Breakdown by GPU Suitability

| Phase | % of Runtime | GPU Suitability | Reason |
|-------|-------------|-----------------|--------|
| NTT (in PCS commit) | 35–91% | Excellent | Embarrassingly parallel |
| Merkle hashing | 30–60% | Excellent | Tree-parallel |
| Constraint eval | 5–15% | Good | Per-row independent |
| Trace generation | 2–10% | Poor | Sequential dependencies |
| Fiat-Shamir | < 1% | N/A | Sequential |

### Recommended Path

1. **Phase 1**: Use ICICLE library for NTT and Merkle tree construction.
   Highest ROI, least invasive. Keep trace generation and constraint evaluation
   on CPU.
2. **Phase 2**: Port constraint evaluation to GPU. The CSE-optimized constraint
   evaluator (Section 2) produces straight-line field arithmetic code that maps
   directly to GPU compute shaders.
3. **Phase 3**: On-GPU trace generation for simple chips (RangeCheck,
   StaticTable). Complex chips (ExecutionChip) stay on CPU.

### CPU-GPU Transfer Minimization

For 278 columns × 2^20 rows = ~1.1 GB:
- PCIe 4.0: ~44ms transfer time
- GPU NTT: ~5–20ms compute time

Transfer dominates! Mitigation:
- **Stream trace rows to GPU** as they are generated (overlapping transfer and
  computation)
- **Keep all trace data on GPU** across phases (generate → commit → evaluate →
  open)
- **Persistent GPU allocation**: Pre-allocate GPU memory at machine setup time,
  reuse across batches

---

## 9. Synthesis: The Compilation Stack

Combining all techniques into a unified optimization pipeline:

```
┌─────────────────────────────────────────────────────────────┐
│  Registration Time (once per Program)                       │
│                                                             │
│  1. Analyze IR → ProofPlan (Section 7)                     │
│     - active_chips, used_opcodes, max_slots                │
│     - height_estimates, active_buses                        │
│                                                             │
│  2. Abstract Interpretation → TypeProfile (Section 4)      │
│     - slot_types, value_ranges, never_null_slots           │
│                                                             │
│  3. Generate Specialized Executor (Section 1)              │
│     - Partially evaluated interpreter                      │
│     - Specialized lowering plan                            │
│                                                             │
│  4. Generate Specialized Chip Layout (Section 3)           │
│     - Column struct with only used opcode groups           │
│     - Slot count = actual max_slot                         │
│                                                             │
│  5. Generate CSE-Optimized Constraint Evaluator (Section 2)│
│     - SymbolicAirBuilder → DAG → CSE → codegen            │
│     - Constraint mask applied                              │
│                                                             │
│  6. Configure Machine (Section 7)                          │
│     - Active chips, buses, FRI parameters                  │
│     - Shard plan (if large batch)                          │
│                                                             │
│  Output: SpecializedMachine<P>                              │
│     - Statically typed for program P                       │
│     - Optimal column widths                                │
│     - Minimal constraint evaluator                         │
│     - Pre-allocated trace buffers                          │
└─────────────────────────────────┬───────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────┐
│  Prove Time (once per batch)                                │
│                                                             │
│  1. Execute with specialized executor (fast)               │
│  2. Lower with specialized plan (no per-instr branching)   │
│  3. Generate traces into pre-allocated buffers             │
│  4. Transpose for NTT → NTT on GPU → transpose back       │
│  5. Merkle commit with BLAKE3 (non-algebraic)              │
│  6. Evaluate CSE-optimized constraints                     │
│  7. Batch-inverted permutation fingerprints                │
│  8. FRI with fold-by-4                                     │
│                                                             │
│  All steps use SpecializedMachine<P>'s pre-computed plans   │
└─────────────────────────────────────────────────────────────┘
```

### Cumulative Speedup Estimate

| Optimization | Phase Affected | Phase Speedup | Overall Impact |
|---|---|---|---|
| CSE constraint eval | Quotient (35%) | 5–15× | **15–30%** |
| Width reduction (S=4, skip opcodes) | All PCS (60%) | 1.4–2× | **25–40%** |
| BLAKE3 Merkle | Commitment (40%) | 5× | **30%** |
| Batch inversion | Permutation (10%) | 6× | **5%** |
| Fold-by-4 FRI | FRI (15%) | 1.5× | **7%** |
| GPU NTT+Merkle | PCS+FRI (60%) | 5–20× | **50–80%** |
| Specialized lowering | Trace gen (5%) | 3× | **3%** |

**Conservative overall (CPU only, no GPU)**: 3–5× faster proving.
**Aggressive (GPU + all optimizations)**: 10–30× faster proving.

---

## 10. Research Priority Ranking

### Tier 1: High Impact, Feasible Now

| # | Optimization | Effort | Impact |
|---|---|---|---|
| R1 | BLAKE3 Merkle hash (Plonky3 config change) | 1 day | 30% proving time |
| R2 | Batch inversion in permutation trace | 1 day | 5% proving time |
| R3 | Eliminate trace matrix clones in prover | 1 day | 50% memory |
| R4 | ProofPlan: chip/bus elision for unused features | 1 week | 10–20% proving |
| R5 | Parallelize quotient computation across chips | 1 day | 2–3× on Phase 8 |

### Tier 2: High Impact, Medium Effort

| # | Optimization | Effort | Impact |
|---|---|---|---|
| R6 | Constraint CSE via SymbolicAirBuilder | 2–4 weeks | 5–15× on eval |
| R7 | Dynamic max_slot (S=actual, not 16) | 1 week | 96 fewer columns |
| R8 | Constraint mask (skip unused opcodes) | 3 days | 15–25% on eval |
| R9 | FRI fold-by-4 | 3 days | 7% proving time |
| R10 | Specialized lowering plan | 2 weeks | 3× on trace gen |

### Tier 3: Transformative, High Effort

| # | Optimization | Effort | Impact |
|---|---|---|---|
| R11 | Width-reduced specialized ExecutionChip | 1 month | 30–50% all PCS |
| R12 | GPU offloading (ICICLE NTT+Merkle) | 1 month | 50–80% all PCS |
| R13 | GKR for LogUp | 2 months | 20–30% PCS |
| R14 | Template micro-chips (no-CPU style) | 2 months | 60–80% ExecutionChip |
| R15 | Batch sharding + recursive composition | 3 months | Linear scaling |

---

## References

### Partial Evaluation & Specialization
- Futamura, Y. (1971). Partial evaluation of computation process
- GraalVM Truffle Language Implementation Framework
- eprint 2025/1110: Compiling Custom Languages as Verifiable VMs

### Constraint Optimization
- Neptune Cash: Speed Up STARK Provers with Multicircuits (1,790× result)
- Albert et al. (CAV 2022): Distilling Constraints in ZK Protocols
- RNA: R1CS Normalization Algorithm (FAC 2024)

### Data Layout & SIMD
- Plonky3 delayed reduction (Issue #252): 2.6–3.7× on dot products
- Intel Memory Layout Transformations guide
- Plonky3 PackedBabyBearAVX2 / AVX512 / Neon implementations

### NTT & FRI
- van der Hoeven: Truncated Fourier Transform (2004)
- STIR: RS Proximity Testing with Fewer Queries (2024)
- Bowers et al.: gFFT twiddle optimization

### Cross-Chip & System-Level
- OpenVM Whitepaper: No-CPU architecture
- SP1 sharding + recursion architecture
- STARKPack: STARK proof aggregation
- GKR-based LogUp (eprint 2023/1284)
- LogUp* (eprint 2025/946)

### GPU
- ICICLE + Plonky3 (AIR-ICICLE, Ingonyama)
- ZKPoG (eprint 2025/765): 22.8× end-to-end GPU speedup
- OpenVM GPU proving (v1.4.0+)

### PCS Alternatives
- Circle STARKs (Stwo, Mersenne-31)
- Binius (binary tower fields)
- DeepFold (2024), BaseFold (2023)
