# Advanced Research

> Status: 🔬 Research
> Design: [docs/design/full-sharding-research.md](../docs/design/full-sharding-research.md), [docs/design/commitment-architecture-research.md](../docs/design/commitment-architecture-research.md)

## D2+D3: Algebraic Accumulator (per-column 236→67 cols)

> Depends: D1 Poseidon delegation + security proof
> Design: [docs/design/full-sharding-research.md](../docs/design/full-sharding-research.md) §3.2

Replace Poseidon hash chain with order-independent algebraic accumulator. In sharded model, eliminates StateShard AND PoseidonLocal (for hash chains) per column.

`Com_i = sum H(encode(key_j, value_j))` over EF4

- [ ] Security analysis (birthday bound ~2^62 in EF4 — may be insufficient)
  - Alternatives: double accumulator, power-sum hash, multiplicative accumulator
- [ ] AccumulatorCommitment impl
- [ ] UnifiedMemoryShard (65 cols per column, replaces MemoryShard + StateShard)
- [ ] Integration tests

**Effect**: 72% per-column width reduction. Combined with 50-core parallelism: ~1100x wall-clock improvement.
**Risk**: Cannot proceed without formal security proof (1-2 month research effort). **Start research now — it's not blocked by anything.**

## D4: Recursive Proof Composition (O(1) proof size)

> Depends: Working sharded prover ([sharding.md](sharding.md))
> Design: [docs/design/full-sharding-research.md](../docs/design/full-sharding-research.md) §5.3

Aggregate C+2 proofs into O(1) via recursive tree.

- [ ] STARK verifier circuit in AIR (~10K cols)
- [ ] Binary aggregation tree (ceil(log2(C)) levels)
- [ ] Final proof + optional Groth16 wrapping
- [ ] Proof size: O(1), independent of C

**Effort**: 6+ months. Required for production (sharded proofs are 18-20x larger without recursion).

## Template Chip Implementations (278→~60 cols)

> Depends: [execution-templates.md](execution-templates.md) (TemplateChip trait infrastructure)
> Design: [docs/design/execution-chip-evolution.md](../docs/design/execution-chip-evolution.md)

Concrete template implementations for hot-path tx patterns. Operate in Tier 1 (execution proof), orthogonal to sharding.

- [ ] TransferTemplate (~28 cols)
- [ ] FillOrderTemplate (~60 cols)
- [ ] Identical LogUp bus fingerprints (interpreter equivalence)

**Effect**: 84% execution layer width reduction for matched patterns.

## ExecutionChip Evolution (Level 2-3)

> Depends: Composition framework
> Design: [docs/design/execution-chip-evolution.md](../docs/design/execution-chip-evolution.md)

Reduce ExecutionChip from 278 cols toward ~60-80 cols via program-aware specialization.

- [ ] Level 2: Column subsetting — dynamic `max_slot` (278→~150 for typical programs)
- [ ] Level 3: Coprocessor factoring — extract Mul/DivMod/Cmp/Hash (278→~100)
- [ ] Level 3: Opcode subsetting — dead-opcode column removal
- [ ] Level 4 (future): Template AIR generation — per-program compiled AIR (~60-80 cols)

## Execution Segmentation

> Depends: Working sharded prover
> Design: [docs/design/full-sharding-research.md](../docs/design/full-sharding-research.md) §6.3

At scale, the execution proof becomes the bottleneck. Split execution trace into segments (like SP1).

- [ ] Intermediate state commitment between segments
- [ ] Parallel segment proving
- [ ] Segment linking constraints

## GKR for LogUp

> Depends: Parallelization + batch inversion (Tier 1b), then benchmark perm cost fraction
> Design: [docs/design/prover-pipeline-acceleration.md](../docs/design/prover-pipeline-acceleration.md) §GKR
> Gate: Proceed if perm cost >10% of proving time after Tier 1b optimizations

Replace committed permutation trace with GKR sum-check protocol. Eliminates permutation trace NTT + Merkle commit entirely. The long-term optimal protocol for LogUp — Stwo (StarkWare) and other production systems are converging on this approach.

### Current LogUp cost

Per chip: `perm_width = 4 × (interactions + 1)` EF4 columns. ExecutionChip (5 interactions) → 24 perm columns. These are NTT'd, Merkle-committed, and FRI-opened alongside main traces. Estimated ~15-30% of proving time before other optimizations.

### What GKR changes

- **Removes**: perm trace generation, perm NTT, perm Merkle commit, perm FRI opening, RAP cumsum constraints (12 per chip)
- **Adds**: Sum-check protocol (O(N) prover, O(log N) verifier, ~300-400 LOC new code)
- **Unchanged**: All chip `eval()` implementations, bus topology, interaction definitions, fingerprint formula

### Code impact

~700 LOC removed (perm trace gen, RAP folders, cumsum constraints) + ~450-550 LOC added (sum-check). Net: code shrinks.

Files affected: `stark/src/permutation/trace.rs` (delete), `stark/src/rap/{prover,verifier,ef4}.rs` (refactor), `machine/src/proof_instance.rs` (remove `build_perm_traces`), `machine/src/prove/quotient.rs` (remove Phase 2 RAP), `machine/src/proof.rs` (remove `perm_commitment`).

### Implementation risk

**No FRI+BabyBear GKR-LogUp production code exists.** Stwo uses Circle STARKs (M31 field, different backend). OpenVM and SP1 still use committed perm traces. Custom implementation required — biggest risk is Fiat-Shamir transcript ordering errors (subtle soundness issues).

### Decision criteria

- [ ] Benchmark perm cost fraction after Tier 1b (parallelization + batch inversion)
- [ ] Monitor OpenVM v2 (SWIRL + multilinear) for possible reference implementation
- [ ] If perm >10%: implement sum-check as standalone `tabula-stark` module, test against brute-force verification, then integrate
- [ ] If perm <10%: defer indefinitely, focus on constraint CSE and GPU offloading

### Tasks (when gate passes)

- [ ] Sum-check protocol implementation in `stark/src/sumcheck/` (~300-400 LOC)
- [ ] Remove permutation trace pipeline (generation, commit, RAP constraints)
- [ ] Update proof format (remove `perm_commitment`, add `sumcheck_proof`)
- [ ] Fiat-Shamir transcript integration (sum-check rounds between main commit and quotient)
- [ ] Verification update (sum-check verifier replaces cumsum constraint check)
- [ ] E2E testing against existing test suite

**Effect**: 20-30% PCS cost reduction. **Effort**: ~4-5 weeks.

## GPU Offloading

> Depends: Mature prover pipeline
> Design: [docs/design/prover-pipeline-acceleration.md](../docs/design/prover-pipeline-acceleration.md) §GPU

Offload NTT + Merkle tree construction to GPU via ICICLE.

- [ ] Tier 1: NTT + Merkle GPU offloading (10-50x on PCS phases)
- [ ] Tier 2: Constraint evaluation GPU kernels
- [ ] Tier 3: On-GPU trace generation for simple chips

**Effect**: 50-80% PCS reduction. **Effort**: ~1 month.

## Compiler Redesign (Multi-IR Tower)

> Depends: Mature extensibility framework
> Design: [docs/design/compiler-research-architecture.md](../docs/design/compiler-research-architecture.md), [docs/design/final-target-architecture.md](../docs/design/final-target-architecture.md)

Full-stack compiler redesign with typed multi-IR pipeline (HIR→MIR→LIR). 36 migration packages across 8 crates.

- [ ] Feasibility assessment
- [ ] Phase 1 gate criteria

**Effort**: 11-15 weeks. Long-term initiative.

## Conditional Branching (if/else)

> Depends: IR extension
> Research: [docs/research/conditional-branching.md](../docs/research/conditional-branching.md)

- [ ] Basic block CFG in IR
- [ ] DSL if/else syntax
- [ ] AIR constraints for block transitions
