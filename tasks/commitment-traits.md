# Commitment Traits

> Status: ⬜ Blocked (Goal 6 — depends on sharding migration)
> Depends: [sharding.md](sharding.md) §Migration (Goal 4) — design on sharded architecture
> Design: [docs/design/extensibility-architecture.md](../docs/design/extensibility-architecture.md) §6 (State Commitment Extension), §7 (State Opening Extension)
> Related: [docs/design/master-roadmap.md](../docs/design/master-roadmap.md) (layered composition), [docs/design/commitment-architecture-research.md](../docs/design/commitment-architecture-research.md)

## Goal

Make commitment schemes (SSMC, SMT) pluggable via traits. App developers can provide custom commitment schemes without modifying Tabula. StateShard becomes an SSMC implementation detail, not core infrastructure.

**Sharding context**: With full sharding as base architecture, ColumnCommitment operates within Tier 2 (per-column proofs). Each column proof independently runs its commitment scheme. The trait must be designed for per-column-proof context, not global batching.

## Architecture

Three-tier composition model (aligned with sharded proof structure):

```
Tier 1: Execution Proof (fixed)
  ExecutionChip, StaticTableChip, PoseidonLocal, RCLocal

Tier 2: Column Proofs (pluggable commitment per column)
  ColumnCommitment trait — per-column API
  "ssmc" → SsmcCommitment → MemoryShard + StateShard
  "smt"  → SmtCommitment → (lightweight)
  "custom" → user-defined

Tier 3: Root Proof (fixed)
  Cumsum balance, SMT path verification
```

## Tasks

### ColumnCommitment trait

- [ ] Define trait in `machine/src/commitment.rs`
  ```rust
  pub trait ColumnCommitment: Send + Sync {
      fn name(&self) -> &str;
      fn create_chips(&self, columns: &[ColumnPlan]) -> Vec<Box<dyn AnyRap>>;
      fn create_dyn_chips(&self, columns: &[ColumnPlan]) -> Vec<Box<dyn DynChip>>;
      fn required_buses(&self) -> Vec<BusId>;
  }
  ```
- [ ] SsmcCommitment impl (wraps existing StateColumnChip)
- [ ] SmtCommitment impl (lightweight, no extra chip)
- [ ] Tests: custom commitment registration + build

### BusConsumer trait

- [ ] Define trait
  ```rust
  pub trait BusConsumer: Send + Sync {
      fn consumed_buses(&self) -> Vec<BusId>;
      fn create_chip(&self) -> Box<dyn AnyRap>;
      fn create_dyn_chip(&self) -> Box<dyn DynChip>;
  }
  ```
- [ ] PoseidonChip as BusConsumer
- [ ] RangeCheckChip as BusConsumer
- [ ] Auto-collection in MachineBuilder

### ProofPlan + ColumnPlan

- [ ] Define per-column metadata
  ```rust
  pub struct ColumnPlan {
      pub table_id: TableId,
      pub col_id: ColId,
      pub encoding_width: EncodingWidth,
      pub commitment_scheme: String,
  }
  pub struct ProofPlan {
      pub columns: Vec<ColumnPlan>,
  }
  ```
- [ ] Wire into witness pipeline and prover

### Internal traits (pub(crate))

- [ ] RootProof — abstraction over ColumnMeta + SmtPath (single impl: GlobalSmtRootProof)

### Builder API extensions

- [ ] `with_commitment(name, impl)` — register custom commitment scheme
- [ ] `with_default_commitments()` — register SSMC + SMT
- [ ] `with_proof_plan(|plan| ...)` — configure per-column scheme

### Witness pipeline cleanup

- [ ] Revert broken shard migration artifacts in witness crate
- [ ] Ensure TraceBuilder works with ColumnCommitment-provided chips

## Completion Criteria

- StateShard owned by SsmcCommitment, not core
- SSMC/SMT registered via ColumnCommitment trait
- PoseidonChip and RangeCheckChip via BusConsumer
- Internal trait boundaries (MemoryModel, RootProof) exist
- Builder API working: `with_core_chips()` + `with_commitment()`
- All existing tests pass

## Verification

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
```
