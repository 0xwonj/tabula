# Constraint Subexpression Elimination (CSE)

Confirmed design for compile-time constraint optimization in Tabula's STARK proving system.

---

## Context

Tabula's 9 AIR chips implement `eval<AB: AirBuilder>()` as Rust generics, monomorphized by Plonky3 at compile time. Constraint evaluation is called per-row across the FRI evaluation domain, making it approximately 35-45% of total proving time.

Plonky3 already provides `SymbolicAirBuilder`, which collects constraint expressions as symbolic objects during keygen. However, this symbolic representation is discarded after keygen --- the prover evaluates constraints by re-invoking the generic `eval()` function with concrete field-element types, relying on rustc/LLVM for optimization.

rustc/LLVM performs excellent local optimization (inlining, constant folding, SIMD vectorization) but cannot perform cross-trait-boundary or cross-generic-instantiation CSE. When the same subexpression (e.g., `is_real * op_add`) appears in 30 different constraints, LLVM may or may not eliminate the redundancy depending on register pressure, inlining depth, and optimization heuristics. A dedicated CSE pass operating on the constraint DAG guarantees elimination.

## Triton VM Reference

Triton VM achieved a 1,790x speedup on transition constraint evaluation via multicircuit CSE. Their approach: express constraints as a shared DAG (`CircuitExpression<II>` with `Rc<RefCell<...>>` node sharing), then generate optimized Rust code at build time.

Tabula's situation differs in two ways:

1. **No HashMap-based polynomial representation.** Triton VM's baseline was an interpreted symbolic evaluator. Tabula's baseline is already monomorphized Rust code, so the starting point is much faster.
2. **Fewer constraints per chip.** ExecutionChip has ~110 constraints; Triton VM has ~400 transition constraints.

Expected gain: 5-15x on constraint evaluation (not 1,790x), because the baseline is compiled code rather than interpreted symbolic evaluation.

The key shared subexpressions in Tabula's ExecutionChip (278 columns):

- `is_real` gating: ~50% of constraints multiply by `is_real`
- Opcode selector combinations: `is_real * op_add`, `is_real * op_sub`, etc.
- Source/destination limb encodings: `src1_val_0 + src1_val_1 * 2^30 + src1_val_2 * 2^60`
- U64 limb decomposition patterns: carry chain expressions reused across Mul, DivMod, Cmp

## Target Architecture

### Symbolic DAG Extraction

The first step extends Plonky3's `SymbolicAirBuilder` to collect all constraint expressions from `eval()` into a unified DAG.

Each `assert_zero()` call registers a root node. Internal arithmetic (`+`, `*`, `-`) builds the DAG incrementally. The DAG uses hash-consing: structurally identical subexpressions share the same node.

Node types:

| Type | Description |
|------|-------------|
| `Constant(F)` | A field element literal |
| `Input(col_idx, RowOffset)` | A trace cell reference (current or next row) |
| `Challenge(idx)` | A verifier challenge value |
| `Add(left, right)` | Addition of two subexpressions |
| `Mul(left, right)` | Multiplication of two subexpressions |
| `Sub(left, right)` | Subtraction (represented as `Add(left, Mul(Constant(-1), right))` internally) |

Node identity uses structural equality with commutativity normalization: `Add(a, b)` and `Add(b, a)` hash to the same node, as do `Mul(a, b)` and `Mul(b, a)`. Operands are canonically ordered by a deterministic node ID to ensure uniqueness.

Reference counting: each node tracks how many parents depend on it (directly or transitively). This count determines whether a subexpression is worth extracting into a `let` binding.

### CSE Algorithm

The CSE algorithm operates in four stages:

1. **Symbolic collection.** Run `eval()` with a `SymbolicAirBuilder` to collect all constraint roots. Each root is a tree of symbolic operations.

2. **Hash-consed DAG construction.** Walk each root tree, inserting nodes into a hash-cons table. When a structurally identical node already exists, reuse it. This converts the forest of trees into a shared DAG.

3. **Topological sort.** Sort nodes leaves-first (constants and inputs before their consumers). This determines evaluation order in the generated code.

4. **Extraction decision.** For each node in topological order:
   - If `refcount > 1`: mark as "extracted" --- this node becomes a `let` binding in generated code.
   - If `refcount == 1`: mark as "inline" --- this node is substituted directly into its single parent.

The extraction threshold can be tuned. A simple `refcount > 1` policy is the baseline; a cost-weighted policy (accounting for expression depth and register pressure) is a future refinement.

### Code Generation

The code generator emits a standalone Rust function using `proc-macro2` and `quote`:

```rust
fn eval_cse<AB: InteractionAirBuilder>(
    builder: &mut AB,
    local: &ExecutionCols<AB::Var>,
    next: &ExecutionCols<AB::Var>,
    public: &PublicStatement<AB::Expr>,
) {
    // Extracted shared subexpressions
    let node_0 = local.is_real.clone();
    let node_1 = local.op_add.clone();
    let node_2 = node_0.clone() * node_1.clone();
    // ...

    // Constraint roots (each becomes an assert_zero)
    builder.assert_zero(node_2.clone() * (/* ... */));
    // ...
}
```

Design choices for the generated code:

- **Shared nodes become `let` bindings.** The name `node_N` is deterministic (topological index). Each binding is computed exactly once.
- **Inline nodes become direct expressions.** Single-use subexpressions are substituted at their use site, avoiding unnecessary stack slots.
- **No runtime overhead.** The generated code is compiled by rustc, which applies register allocation, SIMD vectorization, and instruction scheduling. The CSE pass handles only cross-constraint sharing that LLVM cannot see.
- **Build-time generation.** The code generator runs in `build.rs` or as a proc-macro. No runtime JIT, no `unsafe`, no manual SIMD intrinsics.

### Integration with Plonky3

The CSE-optimized evaluator slots into the existing Plonky3 pipeline at a single point: the quotient computation phase of the prover.

- **Keygen** continues to use the original `eval()`. Symbolic collection requires the trait-based `AirBuilder` interface, and keygen is not performance-critical.
- **Quotient computation** uses the CSE-optimized evaluator. This is where per-row constraint evaluation dominates runtime.
- **Verification** uses the original `eval()`. The verifier evaluates constraints at a single random point, so CSE provides negligible benefit.

The optimized evaluator satisfies the same type constraints as `eval()`. It takes the same `AirBuilder` and column references, producing the same `assert_zero()` calls. The constraint semantics are identical; only the evaluation order and intermediate storage differ.

Correctness verification: the CSE pass includes a debug mode that evaluates both the original `eval()` and the generated `eval_cse()` on random inputs, asserting identical outputs.

## Estimated Impact

### ExecutionChip (278 columns, ~110 constraints)

This chip dominates constraint evaluation time due to its width and constraint count.

- ~40% of expression nodes are shared (`is_real`, opcode selectors, limb patterns)
- Estimated 5-20x speedup on constraint evaluation for this chip
- Overall proving time reduction from this chip alone: 15-30%

### Other Chips

| Chip | Columns | Estimated Speedup | Notes |
|------|---------|-------------------|-------|
| GlobalMerge | 74 | 2-5x | Source encoding selectors shared |
| StateColumn (SortedMem) | 67 | 2-5x | Lex ordering gadget shared |
| InterTxOrder | 67 | 2-5x | Same lex gadget patterns |
| GlobalSSMC | 66 | 2-4x | Hash chain + boundary flags |
| ColumnMeta | 56 | 2-3x | Moderate sharing |
| Poseidon | 93 | 1.5-2x | Already structured (round-based) |
| RangeCheck | 2 | 1x | Trivial chip, no benefit |

### Aggregate

Across all chips, the expected reduction in total proving time is 20-35%, assuming constraint evaluation is 35-45% of the total and the weighted average speedup across chips is 4-8x.

## Relationship to Other Optimizations

**Independent of ExecutionChip evolution.** Whether the ExecutionChip remains monolithic (Level 0), splits into per-opcode template chips (Level 3), or compiles to program-specific AIR (Level 4), CSE applies to whatever `eval()` function exists. Fewer constraints produce a smaller DAG and faster CSE, so the optimizations are synergistic.

**Independent of prover pipeline acceleration.** Optimizations like BLAKE3 Merkle hashing, batch field inversion, and FRI query parallelism operate on different phases of the prover pipeline. CSE targets only constraint evaluation. These optimizations compose multiplicatively.

**Synergistic with program-specific AIR.** A program-specific AIR eliminates dead opcode branches, producing fewer constraints. CSE then optimizes the remaining constraints. The combination is more effective than either alone.

**Does not affect soundness.** CSE is a pure performance optimization. The constraint polynomial evaluated at each domain point is identical before and after CSE. The proof structure, FRI parameters, and verification logic are unchanged.

## References

- `docs/research/compiler-optimization-research.md` -- Section 2
- `docs/research/triton-codesign-analysis.md` -- Section 2
- Neptune Cash, "Speed Up STARK Provers with Multicircuits"
- Triton VM `triton-constraint-circuit` crate
