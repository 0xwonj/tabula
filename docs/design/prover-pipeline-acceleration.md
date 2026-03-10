# Prover Pipeline Acceleration

> Status: Design
> Date: 2026-03-09
> Depends on: tabula-machine-architecture.md, proof-optimization-architecture.md
> Scope: Infrastructure below the chip/AIR layer — how the prover pipeline runs, not what it proves

---

## Context

Tabula's prover uses Plonky3 (p3 0.4) over BabyBear with FRI-based polynomial commitment. The proving pipeline has 11 phases: trace generation, NTT, Merkle commitment, challenge sampling, permutation trace generation, permutation NTT + commit, quotient polynomial computation, quotient NTT + commit, FRI fold, FRI query, and proof serialization.

After constraint common subexpression elimination removes `eval()` as the bottleneck, NTT and hashing become dominant, consuming 60-90% of remaining proving time. The current configuration uses Poseidon2 for both in-circuit hashing (PoseidonChip AIR constraints) and Merkle tree commitment, with `log_blowup=3`, fold-by-2, and `num_queries=2` (test-friendly, not production).

The optimizations described here are orthogonal to chip-layer improvements (template chips, KeyRoute routing) covered in `proof-optimization-architecture.md`. They target the shared proving infrastructure that all chips pass through.

---

## Non-Algebraic Merkle Hash

Poseidon2 is required for in-circuit hashing — the PoseidonChip constrains Poseidon2 permutations within the AIR. However, Merkle tree commitment is a prover/verifier-native operation with no algebraic constraint representation. The verifier recomputes Merkle hashes natively; there is no need for an arithmetic circuit over the hash function.

Switching the commitment hash from Poseidon2 to BLAKE3 yields approximately 10x faster hashing on CPU and approximately 5x net commitment speedup (accounting for the unchanged NTT cost). Plonky3 supports configurable hash backends via the `Compress` and `CryptographicHasher` traits — the PCS configuration specifies the Merkle hash independently of the field arithmetic.

Expected impact: ~30% total proving time reduction. This is a configuration change, not a structural modification.

---

## Batch Inversion for Permutation Trace

LogUp permutation trace generation computes `phi = multiplicity / fingerprint` for each interaction on each row. The fingerprint is an EF4 (BabyBear quartic extension) element, and EF4 division costs approximately 20 BabyBear multiplications per division.

Montgomery batch inversion replaces N independent inversions with a single inversion plus 3(N-1) multiplications. The technique:

1. Compute prefix products: `P_i = f_0 * f_1 * ... * f_i`
2. Invert the final product: `P_{N-1}^{-1}`
3. Backtrack: `f_i^{-1} = P_{i-1} * (product of remaining)`

This is a standard optimization already used by SP1 and Stwo. Expected speedup: ~6x on permutation trace generation.

---

## Trace Matrix Memory Optimization

The current `prove_with_key` implementation clones trace matrices when building `ChipProveInfo` entries. For a multi-chip system with 2-3 GB of total traces (e.g., 278-column ExecutionChip at 2^20 rows), cloning doubles peak memory to 4-6 GB.

The solution is to transfer ownership of trace matrices into the proving pipeline rather than cloning, or to use `Arc<RowMajorMatrix>` for shared access without duplication. Additionally, trace buffers can be pre-allocated based on `ProgramBudgets` height predictions, avoiding reallocation during trace generation.

Expected impact: 50% peak memory reduction.

---

## FRI Configuration Tuning

The current FRI parameters (`log_blowup=3`, `num_queries=2`, `pow_bits=1`) are test-friendly but not production-ready. Production configurations for 128-bit security:

| Parameter Set | log_blowup | num_queries | pow_bits | Proof Size |
|---------------|------------|-------------|----------|------------|
| A (conservative) | 3 | 43 | 0 | ~250 KB |
| B (balanced) | 4 | 32 | 0 | ~210 KB |
| C (with PoW) | 3 | 30 | 16 | ~180 KB |

Fold-by-4 (instead of fold-by-2) halves the number of FRI layers, reducing Merkle tree construction cost by approximately 50% per folding round. Each fold-by-4 step combines two radix-2 folds into a single operation, halving the number of intermediate Merkle commitments.

Expected impact: ~7% total proving time reduction from fold-by-4. Proof size at 128-bit security: ~210 KB with parameter set B.

---

## Quotient Computation Parallelism

Quotient polynomial computation (Phase 8 of the proving pipeline) iterates over chips sequentially. Each chip's quotient is independent: it depends only on that chip's committed trace, the constraint evaluations, and the random challenge point.

Parallelization via `rayon::par_iter` over chips is straightforward. The quotient computation for each chip evaluates constraints at every coset point of the LDE domain, which is already row-parallel within a single chip. Cross-chip parallelism adds a second dimension of concurrency.

Expected impact: 2-3x speedup on Phase 8, which constitutes approximately 35% of proving time after the Merkle hash optimization.

---

## NTT Optimization

Plonky3 uses `Radix2DitParallel` with a Bowers gFFT variant for number-theoretic transforms. The current implementation operates on column-major data within a row-major trace layout, causing stride-width memory access patterns that defeat cache prefetching for traces wider than L2 cache lines.

### Explicit Transpose

An explicit transpose of the trace matrix before NTT converts column-wise NTT operations into sequential memory access. The transpose cost is O(N) with cache-oblivious blocking, while the NTT speedup is 1.5-2x for large traces (those exceeding L2 cache, typically > 2^16 rows with > 64 columns).

### Radix-4 Butterflies

Radix-4 (or Radix-2-squared) butterflies process four elements per stage instead of two, halving memory traffic and reducing arithmetic by approximately 20%. This requires modification to `p3-dft` internals. The Radix-2-squared variant maintains the same twiddle factor structure as Radix-2 while processing pairs of stages simultaneously.

---

## SIMD Vectorization Gaps

Plonky3 provides `PackedBabyBear` for SIMD acceleration: 8 elements on AVX2, 16 on AVX-512, 4 on NEON. Constraint evaluation already uses SIMD via `PackedVal` — Plonky3's `eval()` framework processes multiple rows simultaneously through packed field types.

Current gaps where SIMD is not utilized:

- **Trace generation**: All chip `generate_trace()` implementations operate on scalar `BabyBear` values, processing one row at a time.
- **Permutation trace generation**: Fingerprint computation and phi accumulation are scalar.

Permutation trace vectorization computes fingerprints for 4 or 8 rows simultaneously using packed EF4 arithmetic. The fingerprint `alpha - (beta^0 * v_0 + beta^1 * v_1 + ... + beta^k * v_k)` is a polynomial evaluation that vectorizes naturally across rows.

---

## Huge Pages

A 278-column trace with 2^20 rows occupies approximately 1.1 GB. With the default 4 KB page size, this requires 275,000 TLB entries. TLB capacity on modern x86 CPUs is typically 1,536 entries (L2 TLB), causing near-100% TLB miss rates for sequential traversal of large traces.

2 MB huge pages reduce the entry count to approximately 550, fitting within L2 TLB capacity. Research on memory-intensive workloads (database systems, HPC) shows 2-3x speedup from reduced TLB pressure for sequential access patterns.

Platform support:
- **Linux**: `madvise(MADV_HUGEPAGE)` on `mmap`'d allocations, or transparent huge pages (THP) via `/sys/kernel/mm/transparent_hugepage/enabled`
- **macOS**: Limited support via `VM_FLAGS_SUPERPAGE_SIZE_2MB` in `mmap` flags. Not available for all allocation patterns.

---

## GPU Offloading Strategy

NTT and Merkle tree construction are excellent GPU candidates: they are embarrassingly parallel, have high arithmetic intensity, and dominate proving time. The ICICLE library (Ingonyama) provides GPU-accelerated NTT and Merkle tree construction for BabyBear, with Plonky3 integration via the AIR-ICICLE adapter.

### Offloading Tiers

**Tier 1 — NTT + Merkle (highest ROI)**: The PCS phases (trace commit, quotient commit, FRI) account for 60-80% of proving time. GPU NTT achieves 10-50x speedup over CPU for 2^20+ element transforms. GPU Merkle hashing (BLAKE3 or Poseidon2) achieves 5-20x speedup.

**Tier 2 — Constraint evaluation**: Quotient polynomial computation is regular and parallelizable. GPU execution requires compiling constraint functions into GPU kernels (CUDA or Metal compute shaders).

**Tier 3 — On-GPU trace generation**: Simple chips (RangeCheck, InterTxOrder) with regular access patterns can generate traces directly on GPU, avoiding CPU-GPU transfer entirely.

### Transfer Bottleneck

PCIe 4.0 x16 provides ~25 GB/s bidirectional bandwidth. A 1 GB trace transfer takes ~40 ms, which is non-trivial compared to GPU computation time. Mitigation: persistent GPU memory allocation across proving phases, and streaming (overlapping transfer with computation via CUDA streams).

Expected impact: 5-20x on PCS phases.

---

## GKR for LogUp

The current LogUp implementation accumulates permutation sums via committed polynomial traces — each chip has permutation columns (phi, cumulative sum) that are NTT'd and Merkle-committed alongside the main trace. This adds O(N log N) prover cost for the NTT and O(N) commitment cost for the additional columns.

The GKR (Goldwasser-Kalai-Rothblum) sum-check protocol replaces committed accumulation with an interactive proof of the multilinear sum. The prover cost drops to O(N) (linear scan, no NTT), and the permutation trace commitment is eliminated entirely.

GKR-based LogUp is used by Stwo (StarkWare) and is being adopted by several Plonky3-based projects. The protocol change is significant:

- The permutation trace (phi, cumulative sum columns) is removed from PCS commitment.
- A sum-check sub-protocol is added to the proof transcript.
- The verifier performs O(log N) field operations for the sum-check instead of reading committed permutation evaluations.

Expected impact: 20-30% reduction in PCS cost (fewer columns to commit). Requires protocol-level changes to the proof format, verifier, and transcript structure.

---

## Priority Ranking

**Tier 1** — Immediate, no structural dependency:

| Optimization | Proving Time Impact | Memory Impact | Effort |
|---|---|---|---|
| BLAKE3 Merkle hash | ~30% proving reduction | None | ~1 day |
| Batch inversion | ~5% proving reduction | None | ~1 day |
| Trace clone elimination | None | ~50% memory reduction | ~1 day |
| Quotient parallelism | ~10% proving reduction | None | ~1 day |

**Tier 2** — Medium-term, moderate complexity:

| Optimization | Proving Time Impact | Effort |
|---|---|---|
| FRI fold-by-4 | ~7% proving reduction | ~3 days |
| Permutation trace SIMD | ~5% proving reduction | ~1 week |

**Tier 3** — Long-term, significant engineering:

| Optimization | PCS Impact | Effort |
|---|---|---|
| GPU offloading (ICICLE) | 50-80% PCS reduction | ~1 month |
| GKR for LogUp | 20-30% PCS reduction | ~2 months |

Tier 1 optimizations are independent and composable. Their combined effect is approximately 40% proving time reduction and 50% memory reduction with roughly 4 days of engineering effort.

---

## References

- `docs/research/compiler-optimization-research.md` -- constraint CSE and evaluation cost model
- `docs/research/jit-compilation-research.md` -- runtime code generation for constraint evaluation
- Plonky3 delayed reduction (Issue #252) -- reducing modular reduction frequency in NTT
- ICICLE + Plonky3 (AIR-ICICLE, Ingonyama) -- GPU acceleration adapter
- SP1 batch inversion implementation -- `sp1-recursion-core/src/chips/`
- Stwo GKR-LogUp -- StarkWare's sum-check based LogUp protocol
