# State Traits

> Status: 🔵 Ready
> Depends: None
> Design: [docs/design/extensibility-architecture.md](../docs/design/extensibility-architecture.md) §6 (VectorCommitment trait, §6.2-6.6) and §7 (PropertyOpening trait, §7.2-7.5)

## Goal

Extract standard interfaces for state commitment and opening proofs from existing SSMC/SMT implementations. The VectorCommitment trait enables pluggable column-level commitment strategies; the PropertyOpening trait enables structural queries (min, max, successor) against committed state.

Full design rationale, trait signatures, bus integration, and examples (including Orderbook Tree VC): see [extensibility-architecture.md §6-7](../docs/design/extensibility-architecture.md#6-axis-4-state-commitment-extension).

## Tasks

### VectorCommitment trait (~100 LOC)

- [ ] Define trait in `commitment/src/vc.rs` (see extensibility-architecture.md §6.2 for full signature)
  - `vc_id()`, `name()`, `commit()`, `prove_transition()`, `chip_name()`
- [ ] `VcWitness` opaque witness data trait
- [ ] Refactor existing SSMC as `SsmcCommitment` (trait implementation)
- [ ] Refactor existing SMT as `SmtCommitment` (trait implementation)
- [ ] Test: trait-mediated commit + transition

### PropertyOpening trait (~100 LOC)

- [ ] Define trait in `commitment/src/property.rs` (see extensibility-architecture.md §7.2)
  - `compatible_vc()`, `supported_queries()`, `prove_property()`, `chip_name()`
- [ ] `PropertyQuery` enum (Minimum, Maximum, Successor, Predecessor, NonExistenceRange, Aggregate)
- [ ] `PropertyRead` IR instruction variant (§7.3)
- [ ] Test: trait-mediated verification

## Verification

```bash
cargo check --workspace
cargo test --workspace
```
