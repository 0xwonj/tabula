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

> Depends: Protocol-level changes
> Design: [docs/design/prover-pipeline-acceleration.md](../docs/design/prover-pipeline-acceleration.md) §GKR

Replace committed permutation trace with GKR sum-check protocol.

- [ ] Security analysis — sum-check soundness for Tabula's bus structure
- [ ] Protocol design — transcript integration
- [ ] Remove permutation trace columns from PCS commitment
- [ ] Sum-check sub-protocol implementation

**Effect**: 20-30% PCS cost reduction. **Effort**: ~2 months.

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
