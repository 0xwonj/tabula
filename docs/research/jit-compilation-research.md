# JIT Compilation Opportunities in Tabula

> Research document analyzing where just-in-time (or ahead-of-time) compilation
> could improve performance in the Tabula proving framework.
>
> Date: 2026-03-09
> Related: [proof-optimization-architecture.md](proof-optimization-architecture.md),
> [master-roadmap.md](master-roadmap.md)

---

## Executive Summary

Tabula has **four distinct computational phases** where dynamic/compiled code
generation could apply. After analyzing each against industry benchmarks (SP1,
RISC Zero, Stwo, Triton VM, ZisK, OpenVM), the conclusion is:

| Phase | Technique | Expected Speedup | Effort | Priority |
|-------|-----------|-------------------|--------|----------|
| Constraint evaluation | Multicircuit CSE compilation | 10–100× on eval | Medium | **High** |
| Trace generation | AOT-compiled trace builders | 3–7× | Medium | **High** |
| IR execution | Specialized interpreter / AOT | 1.5–3× | Low–Med | Medium |
| Permutation fingerprints | Batch inversion + vectorized Horner | 2–4× | Low | Medium |

**Key insight**: Tabula is NOT a general-purpose zkVM. Its chip set and IR are
fixed at compile time. This means **AOT (ahead-of-time) compilation is strictly
superior to JIT** for most scenarios — there is no need to compile at runtime
what can be compiled at build time. The exception is user-defined precompiles
(Goal 4), where JIT becomes relevant.

---

## 1. Constraint Evaluation — Multicircuit Compilation

### Current State

Each chip implements `eval<AB: AirBuilder>()` as a Rust generic function.
Plonky3 monomorphizes this at Rust compile time for each builder type
(`ProverConstraintFolder`, `VerifierConstraintFolder`, `RapProverFolder`, etc.).

Constraint evaluation is called **per row** across the entire trace domain,
making it the tightest inner loop in proving (~35–45% of total proving time).

### Opportunity: Subexpression Elimination (Triton VM's Approach)

Triton VM demonstrated a **1,790–2,000× speedup** on constraint evaluation by
treating all transition constraints as a shared DAG ("multicircuit") and
eliminating common subexpressions across constraints.

Tabula's largest chip — ExecutionChip (278 columns) — has significant
subexpression sharing:

```
Example shared subexpressions in ExecutionChip:
  - op_arith, op_cmp, op_mul flags are read in 10+ constraints each
  - src1_val[0..3] encoding is reused across arith, cmp, mul, divmod
  - is_real gating multiplied into ~50% of all constraints
  - u64 limb decomposition (x0, x1, x2) reused in carry chains and comparisons
```

### Implementation Path

**Option A: Compile-time CSE (Rust proc-macro or build script)**

1. Express constraints as an expression DAG (already implicit in `eval()`)
2. At `cargo build` time, extract the DAG via symbolic execution
3. Topologically sort, eliminate common subexpressions
4. Emit optimized Rust code as a generated `eval_optimized()` method
5. No runtime JIT needed — all happens at build time

**Option B: Runtime symbolic evaluation (like Plonky3's `SymbolicAirBuilder`)**

Plonky3 already has `SymbolicAirBuilder` that collects constraint expressions as
symbolic objects during keygen. This could be extended to:

1. Collect the symbolic expression graph
2. Apply CSE + constant folding + dead constraint elimination
3. Compile to a flat evaluation function (interpreter or native code)

**Estimated effort**: Medium (2–4 weeks for Option B using existing Plonky3 infra)

**Estimated speedup**: 10–100× on constraint evaluation alone. Even conservatively,
ExecutionChip's 278-column constraints would see significant CSE opportunities.

### Industry Reference

| System | Approach | Speedup |
|--------|----------|---------|
| Triton VM | Multicircuit CSE at build time | 2,000× on eval |
| Zirgen (RISC Zero) | MLIR-based constraint compilation to Rust/CUDA/Metal | 10% additional on top of precompiles |
| Stwo | Pluggable backend (scalar/SIMD/GPU) for eval | 940× over Stone |

---

## 2. Trace Generation — AOT-Compiled Trace Builders

### Current State

Tabula's trace pipeline has three stages:

```
ExecutionResult → WitnessGenerator → TraceBuilder → TraceMap<ChipId, Matrix>
                  (witness/generator.rs)  (witness/trace/)  (chips/*/trace.rs)
```

Each chip's `generate_*_trace()` function iterates over witness rows and
populates a `RowMajorMatrix<BabyBear>` column by column. This involves:

- Per-row field element encoding (`encode_value_with_null_flag`)
- Per-row witness computation (carry chains, borrow bits, IsZero witnesses)
- BTreeMap lookups for slot resolution
- Dynamic dispatch through `TraceContributor` trait objects

In industry benchmarks, trace generation accounts for **50–70% of total proving
time** in unoptimized systems.

### Opportunity: Specialized Trace Emitters

Since Tabula's chips are fixed at compile time, trace builders can be
aggressively specialized:

**Level 1: Monomorphized trace builders (no JIT needed)**

Replace `dyn TraceContributor` dispatch with static dispatch. Each chip's trace
builder is a known type — virtual dispatch adds branch misprediction overhead
in tight inner loops.

```rust
// Current: dynamic dispatch per chip
for (chip_id, contributor) in contributors.iter() {
    contributor.contribute(&witness, &mut trace_map)?;
}

// Optimized: static dispatch, inlined
execution_chip.contribute(&witness, &mut trace_map)?;
poseidon_chip.contribute(&witness, &mut trace_map)?;
// ... each call monomorphized and inlineable
```

**Level 2: Pre-computed layout tables**

Pre-compute column offsets, widths, and encoding functions at build time.
Currently, per-row encoding decisions (which columns to fill, what width) are
resolved dynamically. A layout descriptor compiled at build time eliminates
these branches.

**Level 3: SIMD-vectorized encoding (BabyBear-specific)**

Value encoding (`u64 → 3 BabyBear limbs`) is a fixed arithmetic operation
(30+30+4 bit split). This can be vectorized to encode 8 values simultaneously
with AVX2:

```
// Scalar: ~6 ops per value
x0 = val & 0x3FFFFFFF
x1 = (val >> 30) & 0x3FFFFFFF
x2 = (val >> 60) & 0xF

// SIMD (AVX2): 8 values in ~6 vector ops
// 4× throughput improvement
```

### Where JIT Actually Applies: User Precompiles (Goal 4)

When Tabula supports user-defined precompiles (Goal 4 in todo.md), the trace
builder for a custom chip is not known at Tabula's build time. Here, two options:

1. **Interpreted trace builder**: User provides a `TraceContributor` impl as a
   Rust library loaded at runtime (dylib). No JIT, but requires Rust compilation.

2. **JIT-compiled trace builder**: User provides a trace description in a DSL,
   which Tabula JIT-compiles to native code at machine setup time. This is the
   Zirgen model — but Zirgen uses AOT, not JIT.

**Recommendation**: Dylib approach (Option 1) is simpler and sufficient. JIT
only becomes compelling if Tabula wants to support non-Rust precompile authors
or on-the-fly chip generation.

### Industry Reference

| System | Approach | Throughput |
|--------|----------|-----------|
| ZisK | AOT RISC-V → x86 compilation | 1.5 GHz |
| OpenVM 2.0 | AOT single-pass compiler | 3.8 GHz (7.8× over interpreter) |
| SP1 | Instrumented Rust execution | ~150 MHz |
| RISC Zero | JIT RISC-V → native | ~150 MHz |

---

## 3. IR Execution — Interpreter Optimization

### Current State

Tabula's interpreter (`executor/interpreter.rs`) is a straightforward
match-dispatch loop over 13 instruction variants:

```rust
for (idx, instr) in instructions.iter().enumerate() {
    match instr {
        Instruction::Read { .. } => { ... }
        Instruction::Arith { .. } => { ... }
        // ... 11 more arms
    }
}
```

Overhead sources:
- Enum dispatch (~1–2 branches per instruction, highly predictable)
- `Value` cloning on every slot/param read
- Bounds checking on every `set_slot` / `get_slot`
- `Result` wrapping/unwrapping per instruction
- `BTreeMap` access for overlay read/write (O(log n))

### Opportunity Analysis

**Is JIT worthwhile here?**

Unlike a zkVM (SP1, RISC Zero) where the interpreter processes millions of
guest instructions per proof, Tabula's IR programs are **small** — typically
tens to low hundreds of instructions per transaction type. The interpreter runs
once per transaction, not millions of times.

**Quantitative estimate**: For a batch of 100 transactions × 50 instructions
each = 5,000 instruction dispatches. At ~10ns per dispatch (with branch
prediction), this is ~50μs — negligible compared to trace generation (ms) and
proving (seconds).

**Verdict**: JIT compilation of the interpreter is **not worthwhile**. The
execution phase is not a bottleneck. Simple optimizations suffice:

1. Replace `BTreeMap` with `HashMap` for `read_cache` (order not needed)
2. Use `Cow<Value>` or references instead of cloning
3. Pre-allocate slot vector based on `max_slots` from `ProgramBudgets`

### Exception: Template Chips (Already Planned)

The roadmap already includes "template chips" — specialized execution chips
for known transaction patterns. This is effectively **AOT compilation of IR
programs into constraint circuits**, which is the optimal form of this
optimization. No JIT needed; the template is generated at compile time from
the IR definition.

---

## 4. Permutation Fingerprints — Batch Inversion

### Current State

LogUp permutation trace generation computes, per interaction per row:

```
fingerprint = α + bus_tag + β·v₀ + β²·v₁ + ...   (Horner evaluation)
phi = multiplicity / fingerprint                    (EF4 division)
```

EF4 division is expensive (~10–20 BabyBear multiplications per division).

### Opportunity: Montgomery Batch Inversion

Instead of dividing per-interaction, collect all fingerprints, then batch-invert
using Montgomery's trick (1 inversion + 3(n-1) multiplications for n elements):

```
// Current: n divisions = n × ~20 muls = 20n muls
// Batch: 1 inversion + 3(n-1) muls ≈ 3n muls
// Speedup: ~6.7×
```

This is a well-known optimization used in SP1 and Stwo. No JIT needed —
purely algorithmic.

### Opportunity: Vectorized Horner Evaluation

Fingerprint computation (Horner polynomial eval) can be SIMD-vectorized across
rows. Since all interactions on the same bus have the same arity, the Horner
template is fixed per bus:

```
// 8 rows simultaneously (AVX2, BabyBear fits in 32-bit)
for row_batch in rows.chunks(8) {
    f_vec = alpha_vec;
    f_vec += tag_vec;
    for k in 0..arity {
        f_vec = f_vec * beta_vec + v_k_vec;
    }
}
```

**Estimated speedup**: 2–4× on fingerprint computation (combined batch
inversion + SIMD Horner).

---

## 5. Where JIT Is Genuinely Needed

After analyzing all phases, JIT compilation (as opposed to AOT) is only
compelling in scenarios where code must be generated at **runtime**:

### 5a. User-Defined Precompiles (Goal 4)

If Tabula supports a precompile DSL where users describe custom chips in a
domain-specific language, JIT compilation of that DSL to native trace builders
and constraint evaluators becomes valuable. The workflow:

```
User writes .tabula-chip file
  → Tabula parses at machine setup time
  → JIT compiles trace builder + AIR constraints
  → Registers as dynamic chip in ChipRegistry
```

This is analogous to Zirgen's compilation pipeline but executed at runtime
rather than build time.

### 5b. Adaptive Trace Sharding (Goal 7)

Full sharding (Goal 7) involves splitting traces into independent shards based
on runtime characteristics (trace height, column access patterns). The shard
boundaries and per-shard constraint specialization could benefit from JIT:

```
Runtime analysis:
  - Column X has 90% zero entries → generate sparse constraint evaluator
  - Shard 3 has no Hash instructions → elide Hash constraints entirely
  - Shard 7 has only Bool-width columns → specialize to W=1
```

This is **profile-guided constraint specialization** — the constraint evaluator
is JIT-compiled based on the actual trace data, eliding irrelevant constraints
and specializing for the data distribution.

### 5c. Recursive Proof Composition

If Tabula eventually supports recursive proofs (verifier-inside-prover), the
inner verifier circuit could be JIT-compiled based on the specific proof
structure being verified. However, this is far-future (not on current roadmap).

---

## 6. AOT vs JIT Decision Framework

| Criterion | AOT | JIT |
|-----------|-----|-----|
| Chip set known at build time | ✓ Preferred | Unnecessary overhead |
| User-defined chips at runtime | Cannot handle | ✓ Required |
| Startup latency tolerance | No impact | Adds warmup cost |
| Optimization quality | Better (LLVM full pipeline) | Limited (no time for heavy opts) |
| Debugging / profiling | Standard tooling works | Harder (generated code) |
| Cross-platform | Handled by cargo | Must support multiple backends |

**Tabula's position**: Fixed chip set → AOT is correct for core chips. JIT
is relevant only for extensibility (Goals 4, 6) and adaptive optimization
(Goal 8).

---

## 7. Concrete Recommendations

### Immediate (No JIT, high impact)

1. **Batch inversion in permutation trace** — 6× on fingerprint phase,
   pure algorithm change, ~1 day of work
2. **Static dispatch for trace builders** — remove `dyn TraceContributor`
   overhead, ~2 days
3. **HashMap for read_cache** — O(1) vs O(log n) in interpreter, ~1 hour
4. **Pre-allocate slot vector** — eliminate bounds checks, ~1 hour

### Medium-term (Build-time compilation, high impact)

5. **Constraint CSE via SymbolicAirBuilder** — extract DAG, eliminate common
   subexpressions, emit optimized evaluator. Target: ExecutionChip (278 cols),
   MergeChip (74 cols). Expected 10–100× on constraint eval. ~2–4 weeks
6. **SIMD-vectorized value encoding** — AVX2 batch encoding for u64→limbs.
   ~1 week
7. **Template chip code generation** — compile known IR patterns to specialized
   constraint circuits at build time. Already planned in optimization roadmap

### Long-term (JIT for extensibility)

8. **Precompile DSL → native compilation** — when Goal 4 (precompile framework)
   matures, provide a chip description language that compiles to native trace
   builders. Could use Cranelift as JIT backend for Rust ecosystem compatibility
9. **Profile-guided constraint specialization** — after sharding (Goal 7),
   JIT-compile per-shard constraint evaluators based on runtime trace statistics
10. **GPU kernel generation** — JIT-compile constraint evaluators to GPU
    compute shaders (WGSL/SPIR-V) for hardware acceleration. Requires Plonky3
    GPU backend

### JIT Backend Candidates (if needed)

| Backend | Language | Startup | Opt Quality | Ecosystem |
|---------|----------|---------|-------------|-----------|
| Cranelift | Rust-native | ~1ms | Medium | wasmtime, Rust compiler |
| LLVM (inkwell) | C/Rust bindings | ~10ms | High | Full LLVM optimization |
| dynasm-rs | Rust macro | <0.1ms | Manual | Lightweight, x86/ARM |
| WASM (wasmtime) | Portable | ~5ms | Medium | Sandboxed, cross-platform |

**Recommendation**: Cranelift for Rust-ecosystem JIT (if needed). It provides
good optimization quality with fast compilation, and is already used by the
Rust compiler's debug mode backend.

---

## 8. Comparison with Industry

| System | JIT/AOT Usage | Tabula Equivalent |
|--------|--------------|-------------------|
| SP1 | Instrumented Rust execution (no JIT) | Tabula interpreter (no JIT needed) |
| RISC Zero | Zirgen AOT constraint compilation (MLIR) | SymbolicAirBuilder CSE (Rec. 5) |
| ZisK | AOT RISC-V → x86 (1.5 GHz trace gen) | AOT trace builders (Rec. 2, 6) |
| OpenVM 2.0 | AOT single-pass compiler (3.8 GHz) | Template chips (Rec. 7) |
| Stwo | Multi-backend eval (CPU/SIMD/GPU) | GPU kernel gen future (Rec. 10) |
| Triton VM | Multicircuit CSE (2000× eval speedup) | Constraint CSE (Rec. 5) |

---

## Conclusion

JIT compilation is **not the primary optimization lever** for Tabula in its
current form. The fixed chip set and compile-time-known IR make AOT
optimizations strictly superior for the core proving pipeline.

The highest-impact optimizations are:
1. **Constraint CSE** (build-time, 10–100× on eval)
2. **Batch inversion** (algorithmic, 6× on permutation)
3. **Static dispatch** (compile-time, eliminates vtable overhead)

JIT becomes relevant only when Tabula's extensibility goals (precompiles,
custom types, adaptive sharding) mature — and even then, the JIT surface is
narrow (user-provided chip definitions, not core proving logic).

The project should focus on **AOT compilation techniques** (constraint
subexpression elimination, template chip generation, SIMD vectorization) before
considering JIT infrastructure.
