# Full Sharding Architecture — Research & Ideal Protocol

> **Primary motivation**: Prover time reduction (column width + parallelism)
> **Proof size**: Solved separately via recursive aggregation (D4)
> **Related**: [sharded-protocol-design.md](sharded-protocol-design.md), [commitment-architecture-research.md](commitment-architecture-research.md), [proof-optimization-architecture.md](proof-optimization-architecture.md)

---

## 1. Why Full Sharding

### 1.1 The Core Problem: Prover Time

The dominant prover costs in a STARK pipeline:

| Step | Cost | Scales with |
|------|------|-------------|
| **NTT** (trace → LDE) | O(W × H × log H) | Width × Height |
| **Constraint evaluation** | O(W × H × D) | Width × Height × Degree |
| **FRI commitment** | O(W × H × log H) | Width × Height |
| **FRI queries** | O(W × Q) | Width × Queries |

All costs scale **linearly with total trace width W**. Reducing width is the single most impactful optimization. Sharding attacks width from two angles:

1. **Per-chip width reduction**: Shard chips eliminate segment detection and lex ordering gadgets (−8 cols each), saving ~24 cols across memory+state+meta.

2. **Padding elimination**: Global chips share one trace height H = next_power_of_two(max_rows). A column with 100 rows gets padded to H=8192 (the height of the largest chip). Sharding lets each column use its own height: 100→128, saving 98.4% of padding.

3. **Parallelism**: C independent column proofs execute on C cores simultaneously. Wall-clock prover time becomes max(column_proof_time) instead of sum.

### 1.2 Quantitative Analysis: Beyond NTT

For C=50 columns, 100 rows/column, 1000 transactions:

**Global (current)**:
```
InterTxOrder:  56 cols × 5000 rows → pad to 8192
StateColumn:  101 cols × 5000 rows → pad to 8192
ColumnMeta:   104 cols ×   50 rows → pad to 8192 (99.4% waste!)
Execution:    278 cols × 1000 rows → pad to 8192 (87.8% waste)
Poseidon:      93 cols × ~4000 rows → pad to 8192
RangeCheck:     2 cols × 65536 rows → stays 65536

Total NTT:  (56+101+104+278+93) × 8192 × log₂(8192)
          = 632 × 8192 × 13
          ≈ 67.3M field multiplications

Total constraint eval: 632 × 8192 × D (degree ~3 average)
                     ≈ 15.5M multiplications
```

**Sharded (per-column)**:
```
Per column: MemoryShard(48) + StateShard(93) + PoseidonLocal(93) + RCLocal(2)
          = 236 cols × 100 rows → pad to 128

50 columns: 50 × 236 × 128 × log₂(128) = 50 × 236 × 128 × 7 ≈ 10.6M ops

Execution proof: 278 cols × 1000 rows → pad to 1024 (not 8192!)
               = 278 × 1024 × 10 ≈ 2.8M ops

Total sequential NTT: 10.6M + 2.8M = 13.4M  (vs 67.3M → 5.0x reduction)
Total parallel NTT (50 cores): max(212K, 2.8M) = 2.8M  (vs 67.3M → 24x reduction)
```

The key insight: **padding waste dominates**. ColumnMeta's 50 rows padded to 8192 is 99.4% wasted compute. Sharding each column to its natural height eliminates this entirely.

### 1.3 Prover Time Breakdown (Realistic)

A more realistic breakdown including all prover phases:

| Phase | Global (ms) | Sharded-seq (ms) | Sharded-50core (ms) |
|-------|------------|-------------------|---------------------|
| Witness generation | 50 | 50 (same) | 20 (parallel) |
| Trace building | 30 | 30 (same) | 5 (parallel) |
| NTT (LDE) | 200 | 40 | 3 |
| Constraint evaluation | 100 | 20 | 2 |
| FRI commitment (Merkle) | 150 | 100 | 5 |
| FRI queries + opening | 50 | 80 (more proofs) | 5 |
| Perm trace (LogUp) | 80 | 40 | 3 |
| **Total** | **660** | **360** | **43** |
| **Speedup** | 1x | 1.8x | **15x** |

Note: These are order-of-magnitude estimates. Actual times depend on hardware, FRI parameters, and implementation. The point is that parallelism gives an order-of-magnitude improvement.

### 1.4 Proof Size is Not a Concern

Sharded proofs are ~18-20x larger than global proofs (C=50). This is acceptable because:

1. **Recursive aggregation** (D4) reduces C+2 proofs to O(1). This is a well-proven technique (SP1, RISC Zero, Polygon).
2. **Off-chain proving**: Proofs are generated off-chain. Only the final recursive proof needs to be small for on-chain verification.
3. **The architecture must support recursion anyway** — designing for small monolithic proofs is a dead end at scale.

---

## 2. The Ideal Protocol

### 2.1 Design Principles

1. **Prover time minimization**: Every architectural choice optimizes for prover speed.
2. **Per-column independence**: Each column is an independent proving unit. No cross-column data dependencies except through bus balance.
3. **Natural width polymorphism**: Columns with different encoding widths (W=1, W=3, W=5, W=8) use their natural width without padding.
4. **Composable optimizations**: D1 (Poseidon delegation), D2+D3 (algebraic accumulator), KeyRoute, template chips all compose without interference.
5. **Recursive-ready**: The proof structure is designed for tree-reduction aggregation from day one.

### 2.2 Proof Architecture

Three independent proof tiers:

```
┌─────────────────────────────────────────────────────────┐
│ Tier 1: Execution Proof (1, global)                     │
│                                                         │
│   ExecutionChip     — instruction semantics, SSA carry  │
│   StaticTableChip   — lookup table membership           │
│   PoseidonLocal     — Hash opcode permutations          │
│   RangeCheckLocal   — execution range checks            │
│                                                         │
│   Public outputs: cumsum_exec (EF4)                     │
│   Buses: ReadAccess, WriteAccess (sends only)           │
├─────────────────────────────────────────────────────────┤
│ Tier 2: Column Proofs (C, embarrassingly parallel)      │
│                                                         │
│   For each (t, c):                                      │
│     MemoryShard<W>    — sorted memory for this column   │
│     StateShard<W>     — SSMC commitment transition      │
│     PoseidonLocal     — column's hash chain perms       │
│     RangeCheckLocal   — column's range checks           │
│                                                         │
│   Public outputs: cumsum_col (EF4), Com_old, Com_new    │
│   Buses: ReadAccess, WriteAccess (receives only)        │
│   Width: entirely determined by column's W              │
├─────────────────────────────────────────────────────────┤
│ Tier 3: Root Proof (1, global, lightweight)             │
│                                                         │
│   SmtColPathChip     — column-level SMT paths           │
│   SmtTablePathChip   — table-level SMT paths            │
│                                                         │
│   Public inputs: all Com_old[t,c], Com_new[t,c],        │
│                  all cumsums                             │
│   Verifies: Σ cumsums = 0, old_root → new_root          │
└─────────────────────────────────────────────────────────┘
```

### 2.3 Per-Column Self-Containment

Each column proof is a complete STARK proof. It contains all chips needed to verify:

1. **Memory consistency**: Every read returns the value from the most recent write (or base state).
2. **State transition**: Old SSMC commitment → apply writes → new SSMC commitment.
3. **Hash integrity**: All Poseidon permutations for this column's hash chains are correct.
4. **Range validity**: All key limbs, timestamp differences, and gap witnesses are in valid ranges.

No data crosses column proof boundaries except the Memory bus fingerprint, exported as a public value.

### 2.4 Width Polymorphism

Each column proof uses its column's encoding width W:

```
Bool column  (W=1):  MemoryShard<1>(44) + StateShard<1>(89)  + Poseidon(93) + RC(2) = 228 cols
U64 column   (W=3):  MemoryShard<3>(48) + StateShard<3>(93)  + Poseidon(93) + RC(2) = 236 cols
Bytes32 col  (W=8):  MemoryShard<8>(58) + StateShard<8>(103) + Poseidon(93) + RC(2) = 256 cols
Custom U128  (W=5):  MemoryShard<5>(52) + StateShard<5>(97)  + Poseidon(93) + RC(2) = 244 cols
```

No column knows about any other column's width. The Memory bus fingerprint adapts to W because each column proof computes its own fingerprint with its own tuple width. The shared LogUp challenges (α, β) are width-agnostic — they are random field elements used in the linear combination.

**Key insight**: Width polymorphism is **free** in a sharded architecture. Each column proof is an independent STARK — it can have any trace width. The only shared element (LogUp challenges) is scalar and width-independent.

### 2.5 Memory Bus in Sharded Context

The Memory bus (ReadAccess + WriteAccess) crosses proof boundaries:

```
Execution sends: fingerprint(t, c, r[3], tx_index, val[W], is_null)
Column receives: fingerprint(t, c, r[3], tx_index, val[W], is_null)
```

**Width adaptation**: The fingerprint polynomial `Σ βⁱ·vᵢ` naturally handles different-length tuples. A W=3 column computes `β⁰·t + β¹·c + ... + β⁷·val₀ + β⁸·val₁ + β⁹·val₂ + β¹⁰·is_null`. A W=5 column uses `...β⁷·val₀ + ... + β¹¹·val₄ + β¹²·is_null`.

**Soundness**: Different-width columns produce different fingerprints for the same (t,c,r). This is fine — the execution chip knows which column it's accessing (from the instruction) and computes the correct-width fingerprint. A W=3 column will never try to balance against a W=5 column because they have different (t,c) identifiers.

**The execution chip problem**: How does ExecutionChip compute fingerprints for columns of different widths?

Three approaches:

**Approach A: ExecutionChip uses MAX_W, zero-padded.**
The execution trace always carries val[MAX_W]. For a W=3 column, val[3..MAX_W] = 0. The fingerprint includes zero-padded terms. The column proof's MemoryShard also zero-pads its fingerprint to match.
- Pro: Simple, one execution chip instance.
- Con: ExecutionChip width grows with MAX_W. SSA carry columns also grow (16 slots × MAX_W).

**Approach B: Width-specific execution shards.**
ExecutionChip<3> handles instructions that access W=3 columns. ExecutionChip<5> handles W=5 columns. Instruction routing based on column schema.
- Pro: Minimal per-chip width. No padding.
- Con: Multiple execution chip instances. SSA slot carry across width boundaries requires cross-chip linking.

**Approach C: Precompile dispatch for non-standard widths.**
ExecutionChip<3> handles all core-type (W≤3) operations directly. Custom-width operations (W>3) dispatch through precompile chips. The precompile chip handles the read/write and produces the correct-width fingerprint.
- Pro: Core path unchanged. Custom types pay their own width cost.
- Con: Custom type operations have one extra indirection.

**Recommendation: Approach A for now, evolve to C.**

Approach A is simplest and handles the common case (most columns are W=3). When custom types are introduced, approach C naturally extends — the precompile framework already provides the dispatch mechanism. Approach B is over-engineered for current needs.

With approach A, the ExecutionChip width impact:
- MAX_W = max(1, 3, 8) = 8 for core types.
- SSA carry: 16 slots × 8 = 128 cols (vs current 48). This is +80 cols.
- Total ExecutionChip: 278 + 80 = ~358 cols.
- This increase is in a single chip that processes sequentially — the per-column savings dwarf it.

### 2.6 Challenge Derivation

LogUp soundness requires that (α, β) are unpredictable to the prover at trace construction time. In a multi-proof system:

```
Step 1 (parallel): Build all main traces, commit independently
           ↓
Step 2 (sync):     Collect all commitments → derive shared (α, β)
           ↓
Step 3 (parallel): Build all perm traces using shared (α, β)
           ↓
Step 4 (parallel): Quotient + FRI independently per proof
```

One synchronization point between Step 1 and Step 3. This is unavoidable — if challenges were derived per-proof, a malicious prover could craft one column's trace to cancel another's imbalance.

**Per-proof alpha and zeta** (constraint folding and OOD point): These can be derived independently per proof after Step 2. They only affect constraint evaluation within a single proof, not cross-proof bus balance. Each proof forks the transcript after receiving shared (α, β).

---

## 3. Composition with Other Optimizations

### 3.1 D1: Poseidon Chain Delegation + Sharding

D1 moves hash chain computation from StateColumn/StateShard into PoseidonChip.

**In the global model**: StateColumn (101 cols) is eliminated. PoseidonChip gains +3 cols (chain tracking). Net saving: 98 cols. Global width: 261→163.

**In the sharded model**: StateShard (93 cols) is reduced by ~48 cols (hash chain columns). PoseidonLocal gains +3 cols. Per-column saving: ~45 cols.

```
Pre-D1 per-column:  MemoryShard(48) + StateShard(93) + Poseidon(93)  + RC(2) = 236
Post-D1 per-column: MemoryShard(48) + StateShard(45) + Poseidon(96)  + RC(2) = 191
Saving: 19% per column
```

**D1 and sharding are orthogonal**: D1 can be applied to either global or sharded architecture. In sharding, it reduces per-column proof width.

### 3.2 D2+D3: Algebraic Accumulator + Sharding

D2+D3 replaces the Poseidon hash chain with an order-independent algebraic accumulator embedded in the memory chip.

**In the global model**: UnifiedMemoryChip = 56 + 17 = 73 cols. StateColumn eliminated entirely. Global width: 73 (memory+state combined, C-independent).

**In the sharded model**: UnifiedMemoryShard = 48 + 17 = 65 cols per column. StateShard and PoseidonLocal (for hash chains) both eliminated.

```
Pre-D2D3 per-column:  MemoryShard(48) + StateShard(93) + Poseidon(93)  + RC(2) = 236
Post-D2D3 per-column: UnifiedMemShard(65) + RC(2) = 67
Saving: 72% per column
```

**Impact on prover time**:
```
Global pre-D2D3:   632 cols × 8192 = 67.3M NTT ops
Sharded post-D2D3: 50 × 67 × 128 × 7 = 3.0M NTT ops (sequential)
                   On 50 cores: 60K ops per core

Combined speedup: 67.3M / 60K ≈ 1100x wall-clock improvement
```

D2+D3 is the most impactful optimization for sharding because it eliminates the two widest per-column chips (StateShard + PoseidonLocal).

**Caveat**: D2+D3 requires a security proof for the algebraic accumulator. Birthday bound in EF4 (~2^62) may be insufficient for 128-bit security. Mitigations (double accumulator, power-sum) need formal analysis.

### 3.3 KeyRoute + Sharding

KeyRoute (ReadOnlyOpening, ShortRun, SortedMemory) classifies memory accesses by complexity.

**In the global model**: ReadOnlyOpeningChip and ShortRunChip replace some rows of InterTxOrderChip.

**In the sharded model**: KeyRoute applies per-column. A column where all accesses are read-only needs no MemoryShard at all — just a ReadOnlyOpeningChip row in the execution proof.

```
Read-only column:  0 cols (no column proof needed!)
ShortRun column:   ShortRunChip (~22 cols per key) instead of MemoryShard(48) + StateShard(93)
Full column:       MemoryShard(48) + StateShard(93) + Poseidon(93) + RC(2) = 236
```

**Untouched column optimization**: A column not accessed by any transaction in the batch requires no column proof. Com_old = Com_new, trivially verified in the root proof. This is impossible in the global model (global chips process all columns in one trace).

### 3.4 Template Chips + Sharding

Template chips specialize the ExecutionChip for known tx patterns. In the global model, a template replaces 278 cols with ~60 cols for matched tx types.

**In the sharded model**: Template chips are purely execution-tier. They produce the same Memory bus fingerprints as the generic ExecutionChip. Column proofs are unaffected.

```
Execution with templates: 278 → ~60 cols for hot-path txs
Column proofs: unchanged (same bus interface)
```

Template chips and sharding compose trivially — they operate on different tiers.

### 3.5 Optimization Composition Summary

| Optimization | Global effect | Sharded per-col effect | Interaction |
|---|---|---|---|
| **Sharding** | N/A | 261→236 (-10%) | Baseline |
| **D1 Poseidon** | 261→163 (-37%) | 236→191 (-19%) | Orthogonal |
| **D2+D3 Accumulator** | 261→73 (-72%) | 236→67 (-72%) | Replaces D1 |
| **KeyRoute** | Partial row skip | Full column skip | Sharding amplifies |
| **Templates** | 278→~60 execution | Same (Tier 1 only) | Orthogonal |
| **All combined** | 73 + ~60 = 133 | 67 per-col + ~60 exec | Maximum reduction |

**With all optimizations on 50 cores:**
```
Execution: ~60 cols × 1024 × 10 ≈ 614K ops
50 columns: 50 × 67 × 128 × 7 = 3.0M → per core = 60K ops
Wall-clock bottleneck: max(614K, 60K) = 614K ops
vs baseline 67.3M → ~110x improvement
```

---

## 4. Custom Type Support

Custom types with arbitrary encoding width W are a natural consequence of sharding, not an additional feature.

### 4.1 Why Sharding Solves Custom Types

In the global model, all chips sharing the Memory bus must agree on val[W]. A W=5 custom type can't coexist with W=3 core types on the same bus without padding to MAX_W.

In the sharded model, each column proof is independent. A W=5 column uses MemoryShard<5> with its own fingerprint width. A W=3 column uses MemoryShard<3>. They never interact.

The only remaining question is how the execution chip handles multi-width values (§2.5). Approach A (MAX_W padding in execution only) confines the padding cost to a single chip. Approach C (precompile dispatch) eliminates it entirely for custom types.

### 4.2 What Custom Types Need from the Framework

With full sharding, custom type support requires:

1. **TypeTag or equivalent**: Open type identifier for schema declarations. (EncodingWidth already open)
2. **TypeEncoding trait (optional)**: Standard encode/decode interface. Useful but not required — DynChip can handle encoding internally.
3. **ColumnDef.encoding_width**: Already open (EncodingWidth(pub usize)).
4. **Shard chip instantiation**: `MemoryShard<W>`, `StateShard<W>` are already generic over W. MachineBuilder registers them.
5. **Bus**: No special handling. Each column proof computes its own fingerprint with its own W.

**What is NOT needed**: TypeEncoding registry, KoalaBearCodec changes, Value enum extension. These are only needed if the global pipeline must dispatch on arbitrary types. With sharding, the column proof handles everything internally.

---

## 5. Detailed Protocol Flow

### 5.1 Prover

```
Input: Program, Batch, ExecutionResult, Schemas, BaseState

Step 0: Witness Partitioning
  ├── Global: InstructionRecords, StaticTableRows
  └── Per-column: ColumnAccesses[t,c], ColumnBaseState[t,c], SmtPaths[t,c]

Step 1: Main Trace Construction (parallel, C+1 way)
  ├── Execution proof: build ExecutionChip + StaticTableChip + PoseidonLocal + RCLocal traces
  └── Column proof [i]: build MemoryShard<Wᵢ> + StateShard<Wᵢ> + PoseidonLocal + RCLocal traces
  → Each proof commits its traces independently → {C_exec, C_col[0..C]}

Step 2: Challenge Derivation (sync point)
  transcript.observe(statement, C_exec, C_col[0], ..., C_col[C-1])
  (α, β) ← transcript.sample_pair()

Step 3: Permutation Traces (parallel, C+1 way)
  ├── Execution: compute perm trace with (α, β), export cumsum_exec
  └── Column [i]: compute perm trace with (α, β), export cumsum_col[i]
  → Each proof commits perm traces → {C_exec_perm, C_col_perm[0..C]}

Step 4: Quotient + FRI (parallel, C+1 way)
  ├── Each proof independently: sample alpha, compute quotient, commit, sample zeta, FRI
  └── Output: {exec_proof, col_proof[0..C]}

Step 5: Root Proof (sequential)
  ├── Inputs: all Com_old[t,c], Com_new[t,c], all cumsums, old_root, new_root
  ├── SmtColPathChip: verify column inclusion proofs
  ├── SmtTablePathChip: verify table inclusion proofs
  ├── Arithmetic check: cumsum_exec + Σᵢ cumsum_col[i] = 0
  └── Output: root_proof
```

### 5.2 Verifier

```
Input: {exec_proof, col_proof[0..C], root_proof, statement}

Step 1: Reconstruct (α, β) from commitments in all proofs
Step 2: Verify exec_proof (parallel with Step 3)
Step 3: Verify each col_proof[i] (parallel, C-way)
Step 4: Verify root_proof:
  ├── Check all cumsums sum to zero
  ├── Check SMT paths: old_root → new_root via Com_old/Com_new
  └── Check public value consistency
```

### 5.3 Recursive Aggregation (Future)

```
Layer 0: C column proofs + 1 execution proof (leaf proofs)
Layer 1: ⌈C/2⌉ recursive proofs (each verifies 2 column proofs)
Layer 2: ⌈C/4⌉ recursive proofs
  ...
Layer k: 1 recursive proof (verifies last 2 subtrees)
Final:   1 proof verifying Layer k + execution proof + root proof

Proof size: O(1), independent of C
Verifier time: O(1), independent of C
Prover time: O(C × T_verify) for tree reduction (dominated by verifier circuit)
```

---

## 6. Impact on ExecutionChip

The ExecutionChip is the only chip that touches all columns. Its interaction with sharding deserves deep analysis.

### 6.1 Current Structure

ExecutionChip (278 cols at W=3) contains:
- Control + opcodes: 17 cols
- Memory access: 68 cols (including val[3])
- Operand values: 24 cols (src1_val[3], src2_val[3], cond_val[3] + null flags)
- SSA carry: 48 cols (16 slots × 3)
- Operation witnesses: 39 cols (Cmp, Mul, DivMod)
- Hash I/O: 24 cols
- Other: 58 cols (selectors, ordering, etc.)

**Value-dependent columns**: access_val[3] + src1_val[3] + src2_val[3] + cond_val[3] + SSA carry (48) = 60 cols scale with W. At W=8: 60→160, total ~378 cols. At W=3: 60 cols, total 278.

### 6.2 Options for Multi-Width Support

**Option A: MAX_W ExecutionChip**

ExecutionChip<MAX_W> carries all values at the widest type's width.
```
MAX_W = 8 (Bytes32):
  Value-dependent: 160 cols
  Fixed: 218 cols
  Total: 378 cols (+100 vs current)
```
Cost: +100 cols in a single trace. Acceptable for the execution proof (one instance, not per-column).

**Option B: Precompile Dispatch**

ExecutionChip<3> handles core types. Custom types (W>3) access memory through precompile chips.
```
ExecutionChip<3>: 278 cols (unchanged)
U128PrecompileChip<5>: ~30 cols (handles read/write for W=5 columns)
```
Cost: precompile overhead per custom-type operation. No change to core chip width.

**Option C: Value-Free Execution (radical)**

ExecutionChip doesn't carry values. Memory bus carries only references (t, c, r, tau, is_write). Values live entirely in MemoryShards.
```
ExecutionChip (no values): 278 - 60 = ~218 cols
```
Problem: SSA carry and arithmetic constraints need values. `dst = src1 + src2` can't be constrained without the values.

Solutions:
- **ArithmeticShard**: A separate chip that receives (src1, src2, op) and produces dst. But SSA carry (slot forwarding across instructions) requires sequential instruction-order access — not naturally shard-friendly.
- **Value commitments**: ExecutionChip carries hash(val) instead of val. Binding proven via Poseidon. But this adds hash operations per instruction.

**Verdict**: Option C is architecturally elegant but impractical due to SSA carry. The instruction pipeline is inherently sequential (each instruction may depend on the previous one's output), and slot values must be carried forward in the same trace.

**Recommended path**: Start with **Option A** (simple, handles all core types). Evolve to **Option B** when custom types are introduced (the precompile framework provides the dispatch mechanism). Option C remains a research direction for future exploration.

### 6.3 ExecutionChip is a Sequential Bottleneck

Regardless of sharding, the execution trace processes all instructions sequentially. With C=50 columns and 1000 txs × 20 instructions = 20,000 rows:

```
ExecutionChip: 278 cols × 20000 rows → pad to 32768
NTT: 278 × 32768 × 15 ≈ 136M ops

vs column proofs (50-way parallel, post-D2D3):
  50 × 67 × 128 × 7 = 3.0M total → 60K per core
```

At scale, the execution proof becomes the bottleneck, not column proofs. Mitigations:
1. **Template chips**: Reduce execution width 278→~60 for matched patterns.
2. **Segment sharding** (future): Split execution trace into segments (like SP1). Each segment is an independent execution proof. Segments link via intermediate state commitment.

---

## 7. Soundness Analysis

### 7.1 Cross-Proof Bus Balance

The fundamental soundness property: the multiset of memory accesses in the execution proof must exactly match the multiset of accesses across all column proofs.

**Mechanism**: Each proof computes a LogUp cumulative sum over its Memory bus interactions.

```
Execution:  cumsum_exec = Σ_{access i} mult_i / fingerprint_i       (sends)
Column [j]: cumsum_col_j = Σ_{access i ∈ column j} -mult_i / fingerprint_i  (receives)

Soundness: cumsum_exec + Σ_j cumsum_col_j = 0
```

This is verified arithmetically in the root proof. No STARK constraint needed — it's a direct EF4 equality check on public values.

**Security level**: EF4 = KoalaBear⁴ ≈ 2^124 bits. The probability of a false positive (accidental balance with incorrect accesses) is ~2^{-124}.

### 7.2 Per-Proof Internal Soundness

Each column proof is a standard STARK. Internal bus balance (BaseStateEntry, CoalescedWrite, PoseidonPerm, RangeCheck) is enforced by the STARK verifier's constraint check, exactly as in the global model.

### 7.3 Challenge Binding

LogUp challenges (α, β) are derived from all main commitments (Step 2). A malicious prover would need to:
1. Commit all traces (Step 1)
2. Only THEN learn (α, β)
3. Cannot modify traces after commitment

The sync point in Step 2 is essential. Without it, a prover could choose one column's trace to cancel another's imbalance.

### 7.4 Width-Heterogeneous Fingerprints

Different columns produce fingerprints of different polynomial degree (more β terms for wider values). This does not break soundness — fingerprints for different (t,c) pairs are already distinct due to the (t,c) prefix in the tuple. A W=3 column's fingerprint can never collide with a W=5 column's fingerprint for the same key, because they have different column IDs.

---

## 8. Comparison with Existing Systems

| System | Chip grouping | Parallelism | Proof aggregation | Width handling |
|--------|--------------|-------------|-------------------|---------------|
| **SP1** | Per-chip MMCS, batched commitment | Segment sharding (execution segments) | Recursive tree → Groth16 | Fixed chip widths |
| **OpenVM** | Per-AIR MMCS | Per-AIR parallel | Recursive aggregation | Fixed per-chip |
| **RISC Zero** | Monolithic | Segment sharding | Recursive | N/A (single trace) |
| **Stwo (StarkWare)** | Mixed-degree Circle STARK | Per-component | Planned recursive | Degree-aware packing |
| **Tabula (proposed)** | Per-column MMCS | C-way column parallel | Recursive tree | **Per-column W** |

Tabula's unique advantage: **domain-driven sharding**. While other systems shard by execution segments or by chip type, Tabula shards by **state column** — a natural domain boundary derived from the application's data model. This enables:
- Per-column width specialization (no other system has this)
- Untouched column skipping (zero cost for inactive state)
- Domain-parallel proving (each column is a separate state machine)

---

## 9. Implementation Gap Analysis

### 9.1 What Exists

- [x] Shard chips: MemoryShard<W>, StateShard<W>, MetaShard — implemented, tested
- [x] ColumnCommitment trait — batch API, supports both global and shard patterns
- [x] BusConsumer trait — auto-collection for Poseidon, RangeCheck
- [x] EncodingWidth — open newtype, arbitrary W
- [x] MachineBuilder — extensible chip registration
- [x] LogUp — cross-chip cumulative sum, EF4

### 9.2 What's Needed

| ID | Gap | Effort | Critical path? |
|----|-----|--------|----------------|
| G1 | ProofInstance abstraction (subset of chips with independent PCS) | Large | Yes |
| G2 | ShardedProver (C+2 parallel ProofInstances + sync point) | Large | Yes |
| G3 | Public value cumsum export (split internal vs cross-proof) | Medium | Yes |
| G4 | Cross-proof Fiat-Shamir (global transcript + per-proof fork) | Medium | Yes |
| G5 | PoseidonLocal / RangeCheckLocal per column proof | Small | Yes |
| G6 | ColumnMeta decomposition (Com as public values, SMT in root proof) | Small | Yes |
| G7 | ShardedVerifier | Medium | Yes |
| G8 | Witness partitioning (per-column witness store) | Small | No (optimization) |
| G9 | ExecutionChip MAX_W adaptation | Medium | No (only for W>3 types) |
| G10 | Recursive aggregation (STARK verifier circuit) | Very Large | No (future) |

### 9.3 Implementation Order

```
G1 → G2 → G4 → G3 → G5 → G6 → G7 → E2E test
                                        ↓
                              G8 (parallel optimization)
                              G9 (custom type support)
                              G10 (recursive aggregation, future)
```

---

## 10. Open Research Questions

| # | Question | Impact | Status |
|---|----------|--------|--------|
| Q1 | Can per-proof alpha/zeta be derived independently without soundness loss? | Reduces sync points | Believed yes (constraint folding is proof-local) |
| Q2 | What is the optimal column grouping? One column per proof, or group small columns? | Padding efficiency vs parallelism | Needs benchmarking |
| Q3 | Can the execution proof be segmented for parallelism (like SP1)? | Removes sequential bottleneck | Feasible but needs intermediate state commitment |
| Q4 | Is double-accumulator (D2+D3) sufficient for 128-bit security in EF4? | Enables 72% width reduction | Needs formal proof |
| Q5 | What is the optimal FRI parameter set for small per-column proofs? | Proof size vs verifier cost tradeoff | Different from global parameters |
| Q6 | How should untouched columns be handled — skip entirely or empty proof? | Prover optimization | Skip entirely (root proof handles trivially) |
| Q7 | Can column proofs from different batches be cached/reused? | Cross-batch optimization | Yes for untouched columns (same Com = same proof) |
