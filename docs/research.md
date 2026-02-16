# Tabula Structural Optimization Research

> Status: Draft
> Date: 2025-02-13
> Context: Analysis of how well Tabula exploits its table-based model vs. traditional zkVM RAM, and concrete improvement directions.

---

## 1. Background: Tabula's Structural Advantage

### 1.1 The zkVM Memory Problem

Traditional zkVMs model computation over flat RAM. Every memory access (read or write) must be proven consistent — the prover cannot fabricate values or reorder operations. The standard approaches:

- **Sorted memory argument**: Sort all accesses by (address, timestamp), then check transition constraints. Cost: O(A log A) sorting witness + O(A) transition constraints, where A = total accesses.
- **Grand product / permutation argument**: Prove the unsorted access log is a permutation of the sorted one. Cost: O(A) auxiliary columns + O(A) constraints.

For a typical zkVM executing 10^6 instructions with 2-3 memory accesses each, this is 2-3 million rows of sorted memory trace — a dominant cost.

### 1.2 What Tabula Does Differently

Tabula replaces flat RAM with **schema-defined tables** and enforces structural invariants at the IR level:

| Invariant | Rule | Effect |
|-----------|------|--------|
| NF-1 (Unique-Read) | At most one `Read(…, t, c, r)` per key per tx | No intra-tx read-consistency needed |
| NF-2 (Unique-Write) | At most one `Write(t, c, r, …)` per key per tx | No intra-tx write-coalescing needed |
| NF-3 (No-Read-After-Write) | No `Read` after `Write` to same key in same tx | SSA wire reuse enforces read-your-writes |
| NF-4 (Key-Alias Resolvability) | Row expressions must be provably equal or distinct | Compile-time deterministic key identity |

Additionally:

- **True SSA**: Each slot defined exactly once → no register-file propagation argument.
- **Static (t,c)**: Table and column IDs are compile-time constants (MUST invariant) → deterministic per-(t,c) sharding without runtime dispatch.
- **Known schema types**: Each column has a declared type → width-class specialization (Narrow/Standard/Wide).

### 1.3 Current Exploitation Level

**Fully exploited:**

- **Intra-tx memory consistency**: Zero cost. NF rules + SSA eliminate the need entirely. A zkVM pays O(A_tx) per transaction; Tabula pays O(0). This is the single biggest structural win.
- **Static (t,c) sharding**: Inter-tx memory argument is partitioned into G independent per-(t,c) groups. Each group is sorted independently. A zkVM sorts globally; Tabula sorts G small groups.
- **Width-class AIR chips**: Known types → Narrow(1 FE), Standard(3 FE), Wide(8 FE) chips with type-specific constraints.
- **Init-row amortization**: One base-state opening per unique (t,c,r) per batch (not per tx). NF-1 guarantees 1:1 within a single tx.

**Partially exploited (room for improvement):**

- Inter-tx sorted memory still uses a generic sorted-memory argument per (t,c) group.
- No distinction between read-only and read-write keys in the proving path.
- Interpreter-style execution trace (generic instruction decoding) despite knowing the program at compile time.
- All keys go through dynamic sorted-memory discovery, even when key values are known statically.
- Full re-commitment (SSMC streaming hash) even for single-key updates.

---

## 2. Improvement Direction: Read-Only Key Fast Path

### 2.1 Problem Statement

A key that is **only read** across the entire batch (no tx writes to it) currently follows the full GlobalSortedMem path:

1. Init row created in GlobalSortedMem (τ=0, is_init=1, is_write=0)
2. One access row per tx that reads this key (τ=clk+1, is_write=0)
3. Transition constraints check `val_{i+1} = mem_i` (read sees prior memory)
4. LogUp fingerprint linking execution access to sorted memory
5. Auxiliary columns: `mem`, `mem_is_null`, `same_key` inverse, `is_last_for_key`, `has_written`
6. Write-set extraction: `is_last_for_key ∧ has_written` → false (correctly excluded)

**Total cost per read-only key**: (1 + N_readers) rows in GlobalSortedMem, each with ~11 auxiliary columns, plus transition constraints and LogUp overhead.

**But the only thing we need to prove is**: the read value matches the base state.

### 2.2 Proposed Optimization

Partition keys into two categories at witness generation time:

- **Read-only keys**: `(t, c, r)` where no tx in the batch writes to this key.
- **Read-write keys**: `(t, c, r)` where at least one tx writes.

For read-only keys, bypass GlobalSortedMem entirely:

```
Read-only path:
1. Prove VC opening: val = VC.Open(Com_old[t,c], r, π)
   - SSMC: LogUp membership in GlobalSSMC sorted list
   - SMT: Merkle path opening
2. Wire val directly to execution access slots via LogUp
3. No sorted memory, no transition constraints, no auxiliary columns
```

For read-write keys, continue using GlobalSortedMem as before.

### 2.3 Estimated Impact

Many real programs are read-heavy:
- Balance check before transfer: reads sender balance, receiver balance, config parameters — writes only sender/receiver balances.
- Governance vote: reads proposal state, voter eligibility, voting rules — writes only vote count.
- Typical ratio: 60-80% of unique keys are read-only in a given batch.

**Constraint savings**: For a batch touching U unique keys where R are read-only:
- Current: (U + A) rows × ~11 columns in GlobalSortedMem
- Optimized: ((U-R) + A_rw) rows × ~11 columns + R × O(VC_open)
- Net savings: ~R × (1 + avg_readers) × 11 columns worth of constraints

For 100 unique keys, 60 read-only, 2 readers each on average:
- Current: ~260 rows × 11 = ~2860 cells
- Optimized: ~140 rows × 11 + 60 × O(VC_open) = ~1540 + VC_open cost
- SSMC VC_open is essentially free (already in GlobalSSMC via LogUp)
- **Net savings: ~46% reduction in sorted-memory trace**

### 2.4 Implementation Sketch

```
WitnessGenerator changes:
1. After collecting all access events, partition by (t,c,r):
   - read_only_keys: Set<CellKey> where all accesses are is_write=false
   - read_write_keys: remaining

2. GlobalSortedMem: only include read_write_keys + their accesses

3. New ReadOnlyOpeningChip:
   - Columns: (t, c, r, val, val_is_null, opening_proof_aux)
   - Constraint: val matches VC opening
   - LogUp: link to execution access events for this key

4. Execution access LogUp modified:
   - Read-only accesses link to ReadOnlyOpeningChip
   - Read-write accesses link to GlobalSortedMem (as before)
   - Discriminated by a witness bit (is_read_only)
```

### 2.5 Considerations

- **Detection**: Requires scanning the entire batch's access events before partitioning. This is already done during witness generation (we build the sorted memory trace from collected events).
- **Correctness**: Read-only keys have no state transition to prove — only opening validity. The VC opening proof is the same one currently done via init rows. We're just skipping the unnecessary sorted-memory wrapping.
- **Interaction with SSMC**: Read-only keys still appear in GlobalSSMC (they're part of the committed state). The optimization only skips GlobalSortedMem.
- **Edge case**: A key read by multiple txs in the batch. Currently, init + N read rows, all seeing the same value. With the optimization: 1 opening proof + N LogUp links. Strictly cheaper.

### 2.6 Risk Assessment

- **Low risk**: This is a pure optimization — the read-only path proves strictly less (no transition) because strictly less needs to be proven. Soundness is preserved: if a read-only key's value is wrong, the VC opening proof fails.
- **Complexity**: Adds a new chip type (ReadOnlyOpeningChip) and a partitioning step in witness generation. Moderate implementation effort.

---

## 3. Improvement Direction: Short-Run Specialization

### 3.1 Problem Statement

NF rules guarantee at most 1 read + 1 write per (t,c,r) per tx. For a batch of B txs, a key touched by K of them has at most 2K+1 rows in GlobalSortedMem (1 init + up to 2K accesses).

In practice, most keys are touched by 1-2 txs, giving 3-5 rows per key. The sorted-memory machinery — lexicographic ordering gadget (integer comparison, borrow-chain), transition constraints, running-memory columns — is designed for **arbitrary-length runs**. For the common case of 2-3 rows per key, it's overengineered.

**Overhead breakdown** for a typical key with pattern (init → read → write):

| Column | Purpose | Needed for 3-row pattern? |
|--------|---------|---------------------------|
| `r` (3 limbs) | Row key | Yes |
| `τ` (3 limbs) | Timestamp | Partially (only need τ>0 for non-init) |
| `is_init` | Init flag | Can be positional (row 0 = init) |
| `is_write` | Write flag | Can be positional (row 1 = read, row 2 = write) |
| `val` (w(T) FEs) | Value | Yes |
| `val_is_null` | Null flag | Yes |
| `mem` (w(T) FEs) | Running memory | Redundant (= val for writes, = prev val for reads) |
| `mem_is_null` | Running null | Redundant (mirrors above) |
| `same_key` inv | Zero-test helper | Not needed (fixed 3-row group) |
| `is_last_for_key` | Extraction flag | Positional (always row 2) |
| `has_written` | Running write-OR | Positional (always true at row 2) |

The `mem`, `mem_is_null`, `same_key` inverse, and running flags are **pure overhead** for the common case.

### 3.2 Proposed Optimization

Introduce a **ShortRunChip** for keys with a fixed access pattern:

**Pattern A: Init-Read-Write (most common)**
```
ShortRunChip_IRW columns (per key):
  t, c, r[3],           // key identity (5 FEs)
  τ_read, τ_write,      // timestamps (2 FEs, not 6 — we know init τ=0)
  init_val[w], init_null, // base state value
  read_val[w], read_null, // read value (constrained = init_val)
  write_val[w], write_null, // written value

Constraints:
  read_val = init_val          // read sees base state
  read_null = init_null        // null flag matches
  τ_read < τ_write             // temporal ordering (simple comparison)
  τ_read > 0, τ_write > 0     // not init timestamps
  VC.Open(Com_old, r) = init_val  // base state opening
```

**Pattern B: Init-Read (read-only, if not using fast path from §2)**
```
ShortRunChip_IR columns (per key):
  t, c, r[3], τ_read,
  init_val[w], init_null,
  // read_val = init_val (implicit, not stored)
```

**Pattern C: Init-Write (blind write, no prior read)**
```
ShortRunChip_IW columns (per key):
  t, c, r[3], τ_write,
  init_val[w], init_null,
  write_val[w], write_null,
```

Keys with more complex access patterns (touched by 3+ txs, or multiple reads by different txs) fall back to the general GlobalSortedMem.

### 3.3 Estimated Impact

For a batch where 80% of keys have pattern A/B/C (common case):

- **Column savings**: ~40% fewer auxiliary columns per key (no mem, no same_key, no running flags)
- **Constraint savings**: ~30% fewer constraints per key (no general transition logic, no ordering gadget for fixed patterns)
- **Row savings**: 1 row per key instead of 3 rows (data packed into single wide row vs. 3 narrow rows in sorted memory)

The packed single-row representation also improves **trace density** — fewer padding rows needed to reach power-of-2 size.

### 3.4 Considerations

- **Dispatch complexity**: Witness generator must classify keys by access pattern and route to appropriate chip. This is a compile-time decision for literal keys, runtime for dynamic keys.
- **Multiple chips**: Each pattern is a separate AIR chip. More chips = more proving overhead (separate commitment per chip). Need to balance chip count vs. per-chip efficiency.
- **Interaction with read-only fast path**: If §2 is implemented, Pattern B keys already bypass sorted memory. ShortRunChip then handles only Pattern A and C, simplifying the design.
- **Fallback**: Keys not matching any short-run pattern go to GlobalSortedMem. This must always be available as a fallback.

### 3.5 Risk Assessment

- **Medium risk**: More AIR chips means more surface area for constraint bugs. Each pattern needs independent verification.
- **Complexity**: Significant implementation effort — new chips, new witness routing, new LogUp interactions.
- **Diminishing returns if combined with §2**: Read-only fast path handles the biggest category (read-only keys). Short-run specialization handles the next biggest (single-tx read-write). The remaining keys are rare enough that general GlobalSortedMem is acceptable.

---

## 4. Improvement Direction: Program-Specialized Circuits

### 4.1 Problem Statement

Tabula currently uses an **interpreter model** for execution proving:

```
[Generic instruction trace]
    ↓ opcode dispatch
[Per-instruction constraints]
    ↓ access events
[Generic GlobalSortedMem]
    ↓ sorted memory argument
[Generic merge + commitment]
```

Every instruction row carries:
- Opcode selector columns (which instruction type?)
- Operand slot references (which SSA slots?)
- SSA carry columns (forward unused slots to next row)
- is_access flag + clock counter

For a program with 20 instructions executing over 1000 txs, this is 20,000 instruction rows, each carrying the full generic column set.

**But the program is known at compile time.** The instruction sequence, operand wiring, and access pattern are all static. The only dynamic inputs are parameters and base-state values.

### 4.2 Proposed Optimization: Circuit Compilation

Instead of interpreting the program in a generic instruction-trace AIR, **compile the program into a custom AIR chip**.

**Example program:**
```
Read(s0, s1, t=0, c=0, r=p0)    // read balance
Add(s2, s0, Lit(100))            // add 100
Write(t=0, c=0, r=p0, s2, s1)   // write new balance
```

**Interpreter model (current):**
- 3 instruction rows per tx, each ~30+ columns wide
- SSA carry: s0, s1 forwarded from row 0 to row 1; s0, s1, s2 forwarded to row 2
- Generic opcode dispatch, slot routing

**Compiled model (proposed):**
```
BalanceAddChip columns (per tx):
  p0[3],                    // parameter: row key (3 limbs)
  old_val[3], old_null,     // Read result (from VC opening)
  new_val[3],               // = old_val + 100 (constrained)
  new_null,                 // = old_null (carried through)

Constraints:
  new_val = old_val + 100                  // fused Add
  new_null = old_null                      // null propagation
  VC.Open(Com_old, p0) = (old_val, old_null)  // base opening
  VC.Update(Com_old, p0, new_val, new_null) = Com_new  // commitment update
```

**Savings:**
- 1 row per tx instead of 3
- ~10 columns instead of ~30+
- No opcode dispatch, no SSA carry, no is_access flag
- Read and write fused with compute — no separate GlobalSortedMem entry
- VC opening/update inlined

### 4.3 Architecture

```
                    ┌─────────────────┐
  Program IR ──────►│ Circuit Compiler │
                    └────────┬────────┘
                             │
                    ┌────────▼────────┐
                    │ Custom AIR Chip  │  per program
                    │ (compiled)       │
                    └────────┬────────┘
                             │
              ┌──────────────┼──────────────┐
              │              │              │
     ┌────────▼──────┐ ┌────▼─────┐ ┌──────▼───────┐
     │ Fused R/W     │ │ Compute  │ │ VC Open/     │
     │ constraints   │ │ (Add,Mul)│ │ Update       │
     └───────────────┘ └──────────┘ └──────────────┘
```

The compiler would:
1. Analyze the IR instruction sequence
2. Determine which SSA slots are intermediate (never externally observed)
3. Fuse chains of compute instructions into compound constraints
4. Inline VC openings/updates for each accessed key
5. Generate a custom AIR chip with minimal columns

### 4.4 What This Eliminates

| Component | Interpreter | Compiled |
|-----------|-------------|----------|
| Instruction trace | O(I × W_instr) per tx | Eliminated |
| SSA carry | O(S × I) per tx | Eliminated (intermediate slots inlined) |
| Opcode dispatch | O(num_opcodes) selectors | Eliminated |
| GlobalSortedMem | O(A) rows per batch | Eliminated (fused with VC) |
| Execution-to-memory LogUp | O(A) fingerprints | Eliminated (direct wiring) |
| Clock/timestamp binding | O(I) per tx | Eliminated |

### 4.5 What Remains

Even with compilation, some components are still needed:

- **VC opening/update proofs**: Still need SSMC or SMT proofs. But these can be inlined into the custom chip rather than going through GlobalSortedMem.
- **Inter-tx state transition**: The compiled chip handles a single tx. Batch-level state transition (Com_old → Com_new across txs) still needs to be proven.
- **Dynamic key routing**: For `Param(p)` keys, the prover still needs to prove the opening at the runtime-determined key. This can't be fully compiled away.
- **ColumnMeta + state root**: Still needed for global state consistency.

### 4.6 Comparison: Interpreter vs. Compiled

| Aspect | Interpreter | Compiled |
|--------|-------------|----------|
| Setup cost | One-time (generic chips) | Per-program (custom chip generation) |
| Per-tx proving cost | O(I × W) + O(A × W_mem) | O(W_custom) where W_custom << I × W |
| Flexibility | Any program, same prover | New program = new chip |
| Verification | Generic verifier | Per-program verifier (or verifier parametrized by chip descriptor) |
| Implementation complexity | Moderate (current) | High (compiler + chip generator) |
| Auditability | Easier (generic constraints) | Harder (generated constraints per program) |

### 4.7 Hybrid Approach

A pragmatic middle ground:

1. **Phase 1 (current)**: Generic interpreter model. All programs use the same AIR chips. Maximum flexibility, easier to audit.
2. **Phase 2**: Identify hot-path patterns (e.g., simple read-compute-write) and create **template chips** for common patterns. Programs matching a template use the optimized chip; others fall back to interpreter.
3. **Phase 3**: Full circuit compiler. Any program compiles to a custom AIR chip. Generic interpreter kept as reference implementation for testing.

### 4.8 Estimated Impact

For a program with 20 instructions, 5 state accesses, running over 1000 txs:

- **Interpreter**: 20,000 instruction rows × ~30 cols + 6,000 sorted-memory rows × ~11 cols = ~666,000 cells
- **Compiled**: 1,000 tx rows × ~15 cols = ~15,000 cells
- **Reduction: ~97%**

Even accounting for VC proof overhead (which both models share), the compiled model is dramatically more efficient.

### 4.9 Risk Assessment

- **High risk**: Circuit compilation is a fundamentally different architecture. Bugs in the compiler could produce unsound circuits.
- **High effort**: Requires building a compiler from IR to AIR constraints, plus per-program verification tooling.
- **Long-term payoff**: This is the end-state optimization that makes Tabula orders of magnitude more efficient than zkVMs for structured state programs.

---

## 5. Improvement Direction: Literal-Key Direct Wiring

### 5.1 Problem Statement

Row expressions in Tabula can be:
- `Lit(n)`: Static, known at compile time
- `Param(p)`: Dynamic, from transaction parameters
- `Slot(s)`: Dynamic, computed at runtime

For `Lit(n)` keys, the prover knows **at compile time** exactly which cell `(t, c, n)` is accessed. Despite this, these accesses currently go through the same dynamic sorted-memory discovery as `Param(p)` keys.

### 5.2 Proposed Optimization

For literal-key accesses, wire the values **positionally** rather than through sorted-memory lookup:

```
Literal-key wiring:
1. At compile time, identify all Lit(n) accesses: {(t, c, n)}
2. For each literal key, create a dedicated "cell wire":
   - One VC opening at batch start (or one init row)
   - Direct value propagation across txs via carry columns
   - One VC update at batch end (if written)
3. Execution trace references literal-key values via fixed column positions
   (no LogUp needed — the wire is structural)
```

**Example**: Program reads cell (0, 0, 42) in every tx.

- **Current**: Init row for (0,0,42) in GlobalSortedMem + 1 access row per tx + LogUp linking per access.
- **Optimized**: One "cell_0_0_42" column in the execution trace. VC opening proves its value at batch start. All txs read from the same column. Zero memory-consistency overhead.

### 5.3 Interaction with Program Compilation (§4)

Literal-key direct wiring is a natural sub-optimization of program compilation. In a compiled circuit:
- Literal keys become fixed VC opening/update targets
- Their values are wired directly into compute constraints
- No indirection through sorted memory or LogUp

If §4 is implemented, §5 comes essentially for free.

Without §4, literal-key wiring can still be implemented as a standalone optimization within the interpreter model, by adding dedicated columns for known literal cells.

### 5.4 Estimated Impact

Depends on program structure. Programs with many literal keys (global config, counters, fixed-address state) benefit most.

- **Typical**: 30-50% of keys are literal in structured programs.
- **Savings per literal key**: Eliminates 1 init row + N access rows in GlobalSortedMem + N LogUp fingerprints. Replaced by 1 column in execution trace + 1 VC opening.
- **Roughly**: Same category of savings as §2 (read-only fast path), but applies to both read and write accesses on literal keys.

### 5.5 Risk Assessment

- **Low risk**: Structural wiring is simpler than sorted-memory. Fewer moving parts.
- **Low-medium effort**: Requires compile-time analysis of row expressions + modified trace layout.
- **Diminishing returns**: If §4 (compilation) is planned, this is subsumed. Worth implementing only if §4 is deferred.

---

## 6. Improvement Direction: Incremental VC Update

### 6.1 Problem Statement

SSMC commitment uses streaming Poseidon hash over the entire sorted key-value list:

```
Com[t,c] = Poseidon(0x00 || t || c || (k_0, v_0) || (k_1, v_1) || ... || (k_{m-1}, v_{m-1}))
```

When updating the commitment after a batch, the current approach is a **full 3-way merge**:

1. OldList (m entries) + WriteSet (w entries) → NewList (m' entries)
2. Re-hash the entire NewList to get Com_new
3. Prove merge correctness via GlobalMerge AIR chip

**Cost**: O(m + w) rows in GlobalMerge, where m = column size (total entries).

For a column with 100 entries but only 1 write, this re-processes all 100 entries just to update 1.

### 6.2 Proposed Optimization: Incremental Hash Update

Instead of re-hashing the entire list, compute the commitment update **incrementally**.

**Approach A: Algebraic commitment (product/sum based)**

Replace streaming hash with a commitment scheme that supports efficient updates:

```
Com = Π (α - encode(k_i, v_i))   (product commitment over random challenge α)
```

Update for insert/delete/modify:
```
Com_new = Com_old × (α - encode(k_new, v_new)) / (α - encode(k_old, v_old))
```

Cost: O(w) per batch, independent of m.

**Tradeoff**: Product commitments require a trusted random challenge (Fiat-Shamir from transcript). The soundness argument is different from hash-based commitment. May interact poorly with the SSMC membership/non-membership proofs that rely on sorted-list structure.

**Approach B: Incremental hash chain with skip**

Modify the streaming hash to support incremental updates by maintaining partial hashes:

```
Com = H(prefix_hash || updated_entries || suffix_hash)
```

Where `prefix_hash` and `suffix_hash` are partial hashes of the unchanged portions.

Cost: O(w × P) where P = Poseidon cost per entry. Independent of m for the hash itself, but still need to prove prefix/suffix correctness.

**Tradeoff**: Proving prefix/suffix correctness may require additional witness (partial hash values at update points). Still O(m) in the worst case for proving the partial hashes are correct, unless using a tree structure.

**Approach C: Promote SSMC to tree structure**

Replace the streaming hash with a **small Merkle tree** (depth = ceil(log2(m))):

```
Com = MerkleRoot(sorted_entries)
```

Update: O(w × depth) = O(w × log m).

**Tradeoff**: This is essentially making SSMC look more like SMT. The advantage of SSMC (lower constant factor for small m) diminishes as we add tree overhead. The crossover point with SMT shifts lower.

### 6.3 Analysis

The incremental update optimization is most impactful when:
- Column size m is large relative to write count w (m >> w)
- But m is still within SSMC range (m ≤ threshold, estimated 100-300)

For the typical case (m = 50-200, w = 1-5):
- Current: O(m) merge rows ≈ 50-200 rows
- Approach A: O(w) ≈ 1-5 operations (but different commitment scheme)
- Approach B: O(w) hash ops + O(m) prefix/suffix proof ≈ still O(m) worst case
- Approach C: O(w × log m) ≈ 7-40 operations

**Conclusion**: Approach C is most sound but reduces SSMC's advantage over SMT. Approach A is most efficient but changes the commitment scheme fundamentally. Approach B doesn't clearly save over the current full merge.

### 6.4 Recommendation

**Defer** this optimization. The current full-merge approach is acceptable for SSMC-range columns (m ≤ 300). The proving cost is dominated by other components (VC opening proofs, execution trace). If profiling shows GlobalMerge as a bottleneck, revisit Approach C (small Merkle tree for SSMC).

### 6.5 Risk Assessment

- **High risk (Approach A)**: Changes commitment scheme, affects soundness argument.
- **Medium risk (Approach C)**: Well-understood technique, but blurs SSMC/SMT boundary.
- **Low payoff**: GlobalMerge is not expected to be the bottleneck for SSMC-range columns.

---

## 7. Comparative Analysis: Current vs. Fully Optimized

### 7.1 Cost Model

Let:
- B = number of txs in batch
- I = instructions per tx
- U = unique keys touched in batch
- R = read-only keys (subset of U)
- L = literal keys (subset of U)
- A = total access events (≤ 2 × U × B, typically much less)
- G = number of (t,c) groups
- m_g = column size for group g

**Current architecture cost (per batch):**

| Component | Rows | Width | Total Cells |
|-----------|------|-------|-------------|
| Instruction trace | B × I | ~30 | 30·B·I |
| GlobalSortedMem | U + A | ~11+w | (11+w)·(U+A) |
| GlobalSSMC | Σ m_g | ~10+w | (10+w)·Σm_g |
| GlobalMerge | Σ (m_g + w_g) | ~8+2w | (8+2w)·Σ(m_g+w_g) |
| ColumnMeta | G | ~25 | 25·G |

**Fully optimized cost (§2 + §3 + §4 applied):**

| Component | Rows | Width | Total Cells |
|-----------|------|-------|-------------|
| Compiled execution chip | B | ~W_custom | W_custom·B |
| Read-only opening | R | ~5+w | (5+w)·R |
| ShortRun (IRW) | U-R-fallback | ~5+3w | (5+3w)·(U-R-fb) |
| GlobalSortedMem (fallback) | fb + A_fb | ~11+w | (11+w)·(fb+A_fb) |
| GlobalSSMC | Σ m_g | ~10+w | (10+w)·Σm_g |
| GlobalMerge | Σ (m_g + w_g) | ~8+2w | (8+2w)·Σ(m_g+w_g) |
| ColumnMeta | G | ~25 | 25·G |

### 7.2 Example Scenario

Program: 20 instructions, 5 state accesses (3 reads, 2 writes), 2 literal keys, 3 param keys.
Batch: 1000 txs, 500 unique keys total, 300 read-only, 5 (t,c) groups, avg 100 entries/group.

**Current:**
- Instruction: 20,000 × 30 = 600,000
- GlobalSortedMem: (500 + 5000) × 14 = 77,000
- GlobalSSMC: 500 × 13 = 6,500
- GlobalMerge: 700 × 14 = 9,800
- ColumnMeta: 5 × 25 = 125
- **Total: ~693,000 cells**

**Fully optimized:**
- Compiled chip: 1,000 × 15 = 15,000
- Read-only opening: 300 × 8 = 2,400
- ShortRun: 180 × 14 = 2,520
- GlobalSortedMem (fallback 20 keys): 100 × 14 = 1,400
- GlobalSSMC: 500 × 13 = 6,500
- GlobalMerge: 700 × 14 = 9,800
- ColumnMeta: 5 × 25 = 125
- **Total: ~37,745 cells**

**Reduction: ~95%** (dominated by instruction trace elimination via compilation).

### 7.3 Priority Ranking

| # | Direction | Impact | Effort | Risk | Priority |
|---|-----------|--------|--------|------|----------|
| 1 | Read-only fast path (§2) | High (46% sorted-mem reduction) | Low-Medium | Low | **P0** |
| 2 | Short-run specialization (§3) | Medium (30% constraint reduction) | Medium | Medium | **P1** |
| 3 | Program compilation (§4) | Very High (95% total reduction) | Very High | High | **P2** (long-term) |
| 4 | Literal-key wiring (§5) | Medium (subsumed by §4) | Low-Medium | Low | **P1** (if §4 deferred) |
| 5 | Incremental VC update (§6) | Low (not bottleneck) | High | High | **Defer** |

---

## 8. Relationship to Existing Roadmap

The current implementation path (from MEMORY.md):

```
T1-T3 → S1 (SSA) → S2 (2-slot R/W) → S3 (NF validation) + S4 (Select) + S5 (Hash encoding)
  → Plonky3 → Poseidon → SMT/SSMC → Phase B → Phase C
```

These optimizations are **orthogonal** to the current milestone path. They can be applied after the baseline proving system is functional:

- **§2 (Read-only fast path)**: After Phase C (GlobalSortedMem exists). Low-hanging fruit.
- **§3 (Short-run)**: After Phase C. Performance tuning phase.
- **§4 (Compilation)**: After full baseline is proven correct. Separate project.
- **§5 (Literal-key)**: After Phase B (execution trace exists). Can prototype early.
- **§6 (Incremental VC)**: Only if benchmarks show GlobalMerge bottleneck.

None of these should delay the current milestone work. They are performance optimizations on top of a correct baseline.

---

## 9. Open Questions

1. **Read-only detection granularity**: Should read-only classification be per-batch (dynamic) or per-program (static analysis)? Per-program is cheaper but less precise (a key that CAN be written might not be written in a specific batch).

2. **ShortRun chip count**: How many pattern-specific chips is acceptable before the overhead of multiple commitments outweighs the per-chip savings?

3. **Compilation target**: Should compiled chips target Plonky3 AIR directly, or an intermediate representation that can target multiple backends?

4. **Verification of compiled circuits**: How to ensure a compiled circuit is equivalent to the interpreted execution? Formal verification? Differential testing against interpreter?

5. **Hybrid threshold recalibration**: If §2 or §3 change the per-key overhead, the SSMC/SMT crossover threshold shifts. Need to re-benchmark.

6. **Interaction with batching strategy**: Larger batches amortize init-row overhead but increase GlobalSortedMem size. Optimizations §2-§3 change this tradeoff — need to model.
