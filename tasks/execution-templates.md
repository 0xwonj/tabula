# Execution Templates

> Status: ⬜ Blocked on [composition.md](composition.md) (needs BusId, ChipExtension, WitnessStore)
> Design: [docs/design/extensibility-architecture.md](../docs/design/extensibility-architecture.md) §8 (Execution Strategy Extension), [docs/design/proof-optimization-architecture.md](../docs/design/proof-optimization-architecture.md) §3 (Template Chips)

## Goal

TemplateChip trait for hot-path tx pattern specialization. A template produces the same LogUp bus fingerprints as the generic ExecutionChip but with fewer columns (~60 vs 278).

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
