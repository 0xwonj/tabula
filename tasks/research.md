# Advanced Research

> Status: 🔬 Research
> Design: [docs/design/commitment-architecture-research.md](../docs/design/commitment-architecture-research.md), [docs/design/full-sharding-research.md](../docs/design/full-sharding-research.md)

## D2+D3: Algebraic Accumulator (163 → 73 cols)

> Depends: D1 Poseidon delegation ([optimization.md](optimization.md)) + security proof

Replace Poseidon hash chain with order-independent algebraic accumulator.

`Com_i = sum H(encode(key_j, value_j))` over EF4

- [ ] Security analysis (birthday bound ~2^62 in EF4 — may be insufficient)
  - Alternatives: double accumulator, power-sum hash, multiplicative accumulator
- [ ] AccumulatorCommitment impl (ColumnCommitment trait)
- [ ] Unified memory chip (73 cols, C-independent)
- [ ] Integration tests

**Risk**: Cannot proceed without formal security proof (1-2 month research effort).

## D4: Recursive Proof Composition (O(1) proof size)

> Depends: mature verifier (already available)
> Research can start independently

Per-column inner STARK proofs + recursive tree aggregation.

- [ ] STARK verifier circuit in AIR (~10K cols)
- [ ] Per-column inner proof generation
- [ ] Binary aggregation tree (ceil(log2(C)) levels)
- [ ] Final proof + optional Groth16 wrapping

**Tradeoff**: At C=50, recursive is ~60s vs global 2-5s. Crossover: C > ~1000.
**Effort**: 6+ months.

## Template Chips (278 → ~60 cols)

> Depends: [execution-templates.md](execution-templates.md) (TemplateChip trait)

Specialized execution chips for hot-path tx patterns.

- [ ] TransferTemplate (~28 cols)
- [ ] FillOrderTemplate (~60 cols)
- [ ] Identical LogUp bus fingerprints (interpreter equivalence)

**Effect**: 84% execution layer width reduction for matched patterns.

## Compiled Per-Program AIR

> Depends: NF-aware constraint elision ([optimization.md](optimization.md))

Generate program-specific AIR at compile time. Entire instruction sequence becomes a fixed constraint system.

- [ ] IR → constraint system compiler
- [ ] Program-specific preprocessed trace

## Distributed Proving

> Depends: D4 (recursive composition)

Distribute column proofs across machines.

- [ ] Network protocol design
- [ ] Work distribution + result aggregation

## Cross-Batch State Caching

> Depends: [commitment-traits.md](commitment-traits.md) (ColumnCommitment)

Persist per-column commitments between batches. Only re-prove changed columns.

- [ ] Column commitment persistence
- [ ] Incremental state update

## Conditional Branching (if/else)

> Depends: IR extension

- [ ] Basic block CFG in IR
- [ ] DSL if/else syntax
- [ ] AIR constraints for block transitions
- [ ] Reference: `docs/research/conditional-branching.md`
