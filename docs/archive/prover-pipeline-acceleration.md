# Prover Pipeline Acceleration

> Status: BLAKE3 + Trace Ownership implemented. Parallelization + Batch Inversion next.
> Date: 2026-03-11
> Depends on: tabula-machine-architecture.md, proof-optimization-architecture.md
> Scope: Infrastructure below the chip/AIR layer — how the prover pipeline runs, not what it proves

---

## Context

Tabula's prover uses Plonky3 (p3 0.4) over KoalaBear with FRI-based polynomial commitment. The proving pipeline has 11 phases: trace generation, NTT, Merkle commitment, challenge sampling, permutation trace generation, permutation NTT + commit, quotient polynomial computation, quotient NTT + commit, FRI fold, FRI query, and proof serialization.

After constraint common subexpression elimination removes `eval()` as the bottleneck, NTT and hashing become dominant, consuming 60-90% of remaining proving time. The current configuration uses Poseidon2 for both in-circuit hashing (PoseidonChip AIR constraints) and Merkle tree commitment, with `log_blowup=3`, fold-by-2, and `num_queries=2` (test-friendly, not production).

The optimizations described here are orthogonal to chip-layer improvements (template chips, KeyRoute routing) covered in `proof-optimization-architecture.md`. They target the shared proving infrastructure that all chips pass through.

---

## Non-Algebraic Merkle Hash

Poseidon2 is required for in-circuit hashing — the PoseidonChip constrains Poseidon2 permutations within the AIR. However, Merkle tree commitment is a prover/verifier-native operation with no algebraic constraint representation. The verifier recomputes Merkle hashes natively; there is no need for an arithmetic circuit over the hash function.

Switching the commitment hash from Poseidon2 to BLAKE3 yields approximately 10x faster hashing on CPU and approximately 5x net commitment speedup (accounting for the unchanged NTT cost). Plonky3 supports configurable hash backends via the `Compress` and `CryptographicHasher` traits — the PCS configuration specifies the Merkle hash independently of the field arithmetic.

Expected impact: ~30% total proving time reduction. This is a configuration change, not a structural modification.

### Implementation

`Blake3FieldHasher` and `Blake3FieldCompressor` in `machine/src/blake3_pcs.rs` wrap BLAKE3 to operate in KoalaBear field-element space, producing `[KoalaBear; 8]` digests. Each 4-byte chunk of the 32-byte BLAKE3 output is read as little-endian `u32` and reduced mod p. This keeps compatibility with `DuplexChallenger<Perm16, 16, 4>` which only observes `Hash<F, F, N>` commitments.

The MMCS type uses scalar `KoalaBear` packing (not `PackedKoalaBear`) — BLAKE3's native speed compensates for the loss of SIMD leaf hashing. Poseidon2 remains for Fiat-Shamir (DuplexChallenger) and in-circuit hashing (PoseidonChip).

---

## Batch Inversion for Permutation Trace

LogUp permutation trace generation computes `phi = multiplicity / fingerprint` for each interaction on each row. The fingerprint is an EF4 (KoalaBear quartic extension) element, and EF4 division costs approximately 20 KoalaBear multiplications per division.

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

### Implementation

`ProofInstance::new()` takes `TraceMap` by value. `collect_chip_infos()` calls `traces.remove(chip_id)` to transfer ownership of each `TraceEntry` (main trace, preprocessed, public values) into `ChipProveInfo` without cloning. `TabulaMachine::prove()` takes `ProofTraces` by value and destructures it into per-tier `TraceMap`s that are moved into each `ProofInstance`. `ProofTraces` derives `Clone` for benchmark use cases that need repeated proving.

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

## Pipeline Parallelization

The proving pipeline has parallelism opportunities at two levels: cross-proof (C+2 sub-proofs) and within-proof (per-chip operations). Both use rayon with adaptive work-stealing, which automatically scales to available cores without manual thread management.

### Cross-Proof Parallelism

The C+2 proof architecture (1 execution + C column + 1 root) creates natural parallelism. A hard synchronization barrier exists at Fiat-Shamir challenge sampling (Phase 4): all main trace commitments must be observed before sampling LogUp challenges (alpha, beta). After that barrier, all subsequent phases are independent across sub-proofs.

Parallelizable phases after challenge sampling:
- Phase 5: Permutation trace generation (exec || cols || root)
- Phases 6-11: Sub-proof execution (exec || cols || root) — the dominant cost

### Within-Proof Chip-Level Parallelism

Within each ProofInstance, several per-chip loops are parallelizable:

**Quotient computation** (`compute_chip_quotients`, Phase 8): Each chip's quotient polynomial is independent — depends only on that chip's committed trace, constraint evaluations, and challenge point. This is the highest-ROI parallelization target: Phase 8 constitutes approximately 35% of proving time after BLAKE3.

**Interaction evaluation** (Phase 1): Each chip's interaction evaluation reads only its own trace rows. Parallelizable via `par_iter_mut` over chips.

**Permutation trace generation** (Phase 5): Each chip's perm trace is independent given shared challenges. The per-chip cumulative sums are computed independently, then aggregated.

### Synergy Between Levels

Cross-proof and chip-level parallelism are complementary, not competing. With rayon's work-stealing scheduler:
- Cross-proof parallelism uses C+2 threads (typically 2-10)
- Each thread spawns chip-level parallelism for 4-5 chips
- Total effective parallelism: (C+2) × chips_per_proof
- Rayon automatically balances load across all available cores

### Trace Building Parallelism

Before proving, `build_proof_traces()` builds per-tier traces sequentially. Each tier's traces are built from independent witness stores — per-column trace building is naturally parallel. This is orthogonal to proving parallelism.

### Verification Parallelism

`verify_impl()` verifies C+2 sub-proofs sequentially after reconstructing shared challenges. Since verification uses the same pre-computed challenges, all sub-proof verifications are independent and parallelizable.

---

## NTT Optimization

Plonky3 uses `Radix2DitParallel` with a Bowers gFFT variant for number-theoretic transforms. The current implementation operates on column-major data within a row-major trace layout, causing stride-width memory access patterns that defeat cache prefetching for traces wider than L2 cache lines.

### Explicit Transpose

An explicit transpose of the trace matrix before NTT converts column-wise NTT operations into sequential memory access. The transpose cost is O(N) with cache-oblivious blocking, while the NTT speedup is 1.5-2x for large traces (those exceeding L2 cache, typically > 2^16 rows with > 64 columns).

### Radix-4 Butterflies

Radix-4 (or Radix-2-squared) butterflies process four elements per stage instead of two, halving memory traffic and reducing arithmetic by approximately 20%. This requires modification to `p3-dft` internals. The Radix-2-squared variant maintains the same twiddle factor structure as Radix-2 while processing pairs of stages simultaneously.

---

## SIMD Vectorization Gaps

Plonky3 provides `PackedKoalaBear` for SIMD acceleration: 8 elements on AVX2, 16 on AVX-512, 4 on NEON. Constraint evaluation already uses SIMD via `PackedVal` — Plonky3's `eval()` framework processes multiple rows simultaneously through packed field types.

Current gaps where SIMD is not utilized:

- **Trace generation**: All chip `generate_trace()` implementations operate on scalar `KoalaBear` values, processing one row at a time.
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

NTT and Merkle tree construction are excellent GPU candidates: they are embarrassingly parallel, have high arithmetic intensity, and dominate proving time. The ICICLE library (Ingonyama) provides GPU-accelerated NTT and Merkle tree construction for KoalaBear, with Plonky3 integration via the AIR-ICICLE adapter.

### Offloading Tiers

**Tier 1 — NTT + Merkle (highest ROI)**: The PCS phases (trace commit, quotient commit, FRI) account for 60-80% of proving time. GPU NTT achieves 10-50x speedup over CPU for 2^20+ element transforms. GPU Merkle hashing (BLAKE3 or Poseidon2) achieves 5-20x speedup.

**Tier 2 — Constraint evaluation**: Quotient polynomial computation is regular and parallelizable. GPU execution requires compiling constraint functions into GPU kernels (CUDA or Metal compute shaders).

**Tier 3 — On-GPU trace generation**: Simple chips (RangeCheck, InterTxOrder) with regular access patterns can generate traces directly on GPU, avoiding CPU-GPU transfer entirely.

### Transfer Bottleneck

PCIe 4.0 x16 provides ~25 GB/s bidirectional bandwidth. A 1 GB trace transfer takes ~40 ms, which is non-trivial compared to GPU computation time. Mitigation: persistent GPU memory allocation across proving phases, and streaming (overlapping transfer with computation via CUDA streams).

Expected impact: 5-20x on PCS phases.

---

## GKR for LogUp

The current LogUp implementation accumulates permutation sums via committed polynomial traces — each chip has permutation columns (phi, cumulative sum) that are NTT'd and Merkle-committed alongside the main trace. Per-chip permutation width is `4 × (interactions + 1)` KoalaBear columns (EF4 representation). This adds O(N log N) prover cost for the NTT and O(N) commitment cost for the additional columns.

The GKR (Goldwasser-Kalai-Rothblum) sum-check protocol replaces committed accumulation with an interactive proof of the multilinear sum. The prover cost drops to O(N) (linear scan, no NTT), and the permutation trace commitment is eliminated entirely.

### Protocol change

| Aspect | Current (committed LogUp) | GKR-LogUp |
|--------|--------------------------|-----------|
| Prover cost | O(N log N) NTT + O(N) Merkle | O(N) linear scan |
| Permutation columns | 4 × (interactions + 1) per chip | None |
| PCS commitment | Main + Perm + Quotient | Main + Quotient only |
| Proof transcript | — | Sum-check rounds (O(log N)) |
| Verifier cost | O(k) field ops (cumsum check) | O(log N) field ops (sum-check) |

### Ecosystem status

GKR-based LogUp is used by Stwo (StarkWare) on Circle STARKs with M31 field. However, **no FRI+KoalaBear production implementation exists**. OpenVM and SP1 (the two major Plonky3-based systems) still use committed permutation traces. Plonky3 v0.4 has no built-in sum-check or GKR support — custom implementation required.

### Code impact

The change removes more code than it adds (~700 LOC removed, ~500 LOC added):
- **Removed**: `permutation/trace.rs` (perm trace generation), `rap/prover.rs` and `rap/verifier.rs` (cumsum constraints), `perm_commitment` from proof structure
- **Added**: `sumcheck/` module (protocol prover + verifier), sum-check proof in transcript
- **Unchanged**: All chip `eval()` implementations, `InteractionAirBuilder` trait, bus topology, fingerprint formula

### Decision gate

GKR implementation is deferred until after parallelization + batch inversion (Tier 1b). After those optimizations, permutation cost fraction should be re-measured. If permutation phases still exceed 10% of total proving time, GKR proceeds. OpenVM v2 (SWIRL + multilinear) may also provide a reference implementation by that point.

### Interaction with recursive aggregation

GKR's sum-check protocol adds verifier complexity. For future recursive proof aggregation (D4), the STARK verifier circuit must include sum-check verification logic — approximately O(log N) additional field operations per sub-proof verification. This is tractable but should be considered in the verifier circuit design.

Expected impact: 20-30% reduction in PCS cost. Estimated effort: 4-5 weeks.

---

## Priority Ranking

**Tier 1a** — Complete:

| Optimization | Impact | Status |
|---|---|---|
| BLAKE3 Merkle hash | ~30% proving reduction | **Done** |
| Trace ownership transfer | ~50% memory reduction | **Done** |

**Tier 1b** — Next (rayon parallelization + batch inversion):

| Optimization | Proving Time Impact | Effort |
|---|---|---|
| Quotient parallelism (per-chip) | ~10% proving reduction | ~50 LOC |
| Cross-proof parallelism (C+2) | ~C× speedup on sub-proofs | ~100 LOC |
| Perm trace / trace building parallelism | ~2-3× on affected phases | ~90 LOC |
| Verification parallelism | ~C× on verification | ~30 LOC |
| Batch inversion (Montgomery) | ~6× on perm trace generation | ~1 day |

**Tier 2** — Medium-term, moderate complexity:

| Optimization | Proving Time Impact | Effort |
|---|---|---|
| FRI fold-by-4 | ~7% proving reduction | ~3 days |
| Permutation trace SIMD | ~5% proving reduction | ~1 week |

**Tier 3** — Long-term, protocol-level:

| Optimization | PCS Impact | Effort | Gate |
|---|---|---|---|
| GKR for LogUp | 20-30% PCS reduction | ~4-5 weeks | Perm cost >10% after Tier 1b |
| GPU offloading (ICICLE) | 50-80% PCS reduction | ~1 month | Mature prover pipeline |

Tier 1a + 1b combined effect: approximately 40% proving time reduction, 50% memory reduction, and C× speedup on parallelizable phases.

---

## References

- `docs/research/compiler-optimization-research.md` -- constraint CSE and evaluation cost model
- `docs/research/jit-compilation-research.md` -- runtime code generation for constraint evaluation
- Plonky3 delayed reduction (Issue #252) -- reducing modular reduction frequency in NTT
- ICICLE + Plonky3 (AIR-ICICLE, Ingonyama) -- GPU acceleration adapter
- SP1 batch inversion implementation -- `sp1-recursion-core/src/chips/`
- Stwo GKR-LogUp -- StarkWare's sum-check based LogUp protocol
