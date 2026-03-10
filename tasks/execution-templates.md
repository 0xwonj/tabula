# Execution Templates

> Status: ⬜ Blocked on [composition.md](composition.md) (Goal 6 — needs BusId, ChipExtension, WitnessStore)
> Design: [docs/design/extensibility-architecture.md](../docs/design/extensibility-architecture.md) §8 (Execution Strategy Extension), [docs/design/proof-optimization-architecture.md](../docs/design/proof-optimization-architecture.md) §3 (Template Chips), [docs/design/execution-chip-evolution.md](../docs/design/execution-chip-evolution.md)
> Related: [research.md](research.md) "Template Chip Implementations" — concrete templates (TransferTemplate, FillOrderTemplate) depend on this trait infrastructure

## Goal

TemplateChip trait for hot-path tx pattern specialization. A template produces the same LogUp bus fingerprints as the generic ExecutionChip but with fewer columns (~60 vs 278).

**Sharding context**: Templates operate in Tier 1 (execution proof) — orthogonal to column sharding. The execution proof is a single global proof regardless of sharding. Templates reduce its width, which is independent of per-column proof structure.

## Tasks

### TemplateChip trait (~100 LOC)

- [ ] Define trait in `machine/src/template.rs`
  ```rust
  pub trait TemplateChip: AnyRap + DynChip {
      fn matches(&self, tx_type: &TxTypeDef) -> bool;
      fn tx_type_name(&self) -> &str;
  }
  ```
- [ ] `MachineBuilder::with_template()` method
- [ ] Build-time tx_type → template matching logic

### Equivalence harness (~100 LOC)

- [ ] Test framework comparing interpreter vs template bus fingerprints
  ```rust
  fn assert_template_equivalent(
      template: &dyn TemplateChip,
      program: &Program,
      batch: &Batch,
  ) -> Result<(), EquivalenceError>;
  ```
- [ ] Memory bus fingerprint comparison (LogUp)
- [ ] CommitmentVerification bus fingerprint comparison
- [ ] Test: dummy template registration + equivalence pass

## Design Principles

- Templates add no new buses — same Memory bus, same CommitmentVerif bus
- Soundness: identical LogUp fingerprints → identical multiset → equivalent from verifier's perspective
- Templates are optimizers — fewer columns expressing the same constraints

## Verification

```bash
cargo check -p tabula-machine
cargo test -p tabula-machine
```
