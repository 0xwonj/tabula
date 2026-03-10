# Proving Layer Refactoring

> Status: ✅ Complete
> Design: [docs/design/proving-layer-architecture.md](../docs/design/proving-layer-architecture.md)
> Related: [sharding.md](sharding.md) (depends on this), [docs/design/sharded-protocol-design.md](../docs/design/sharded-protocol-design.md)

## Goal

Establish clean layer boundaries between STARK protocol logic (`stark`) and proof orchestration (`machine`), and introduce the `ProofInstance` abstraction. This is a prerequisite for sharding infrastructure (Goal 3).

Three deliverables:
1. Move STARK protocol math from `machine` → `stark`
2. Introduce `ProofInstance` in `machine`
3. Add witness partitioning support in `witness`

---

## Tasks

### PL-1: Move Permutation Trace Generation to `stark` ✅

- [x] Move `machine/src/permutation/` → `stark/src/permutation/`
- [x] Define `PermutationError` in stark (decoupled from `ProveError`)
- [x] Extract `EF4` type alias to `stark` (canonical definition)
- [x] Export from `stark/src/lib.rs`: `pub mod permutation;` + `pub type EF4`
- [x] Machine re-exports `EF4` from stark, thin permutation wrapper
- [x] `From<PermutationError> for ProveError` conversion in machine
- [x] All 979 tests pass, 9 permutation tests now in stark

### PL-2: Move RAP Folders to `stark` ✅

- [x] Move `machine/src/prove/rap_folder.rs` → `stark/src/rap/prover.rs`
- [x] Move `machine/src/verify/rap_folder.rs` → `stark/src/rap/verifier.rs`
- [x] Move `machine/src/ef4.rs` → `stark/src/rap/ef4.rs` (shared arithmetic helpers)
- [x] Decouple from `TabulaStarkConfig`: use `<BabyBear as Field>::Packing` and `<EF4 as ExtensionField<BabyBear>>::ExtensionPacking` directly
- [x] Export from `stark/src/lib.rs`: `pub mod rap;`
- [x] Update `machine` prove/verify/any_rap/chip_ref to import from `tabula_stark::rap`
- [x] Deleted old files: `machine/src/{ef4,prove/rap_folder,verify/rap_folder}.rs`
- [x] All 979 tests pass, zero clippy errors

### PL-3: Quotient Helpers — Design Adjustment ✅

> Original plan: move quotient functions to stark. After analysis, quotient computation
> is prover orchestration (wires RAP folders + PCS domains + constraint folders), not
> standalone protocol math. It naturally stays in machine. Extracted the pure helper.

- [x] Extracted `build_alpha_powers()` to `stark/src/rap/ef4.rs` (pure EF4 arithmetic)
- [x] Machine's `quotient.rs` imports from `tabula_stark::rap::ef4::build_alpha_powers`
- [x] Quotient functions (`compute_quotient_standard`, `compute_quotient_rap`) stay in machine — they depend on `ChipRef`, `PcsDomain`, `ProverConstraintFolder<TabulaStarkConfig>` which are machine-level types
- [x] All tests pass

### PL-4: ProofInstance Abstraction ✅

> Depends: PL-1, PL-2, PL-3 (ProofInstance uses permutation + RAP from stark)

- [x] Define `ProofInstance<'a>` in `machine/src/proof_instance.rs`
  - Owns: chip metadata, PCS prover data (accumulated across phases)
  - Methods: `new()` (Phase 0-1), `commit_main()` (Phase 2-3), `build_perm_traces()` (Phase 5), `prove()` (Phase 6-11)
- [x] Define `MainCommitment` — PCS commitments returned to orchestrator for Fiat-Shamir
- [x] Define `SubProof` — output of a single proof instance with `into_tabula_proof(statement)` conversion
- [x] Refactor `prove_with_key()` to create a single `ProofInstance` internally
  - Zero behavioral change — existing API preserved, same function signature
  - `prove/mod.rs` reduced from 554 lines to 59 lines (thin orchestrator)
  - Quotient module visibility changed to `pub(crate)` for cross-module access
- [x] All 979 E2E STARK tests pass unchanged
- [x] VerifyInstance deferred — verification is already clean; sharding will add `VerifyInstance` when needed

### PL-5: Witness Partitioning ✅

> Depends: PL-4 (partitioning serves ProofInstance)

- [x] Define `WitnessPartition` in `witness/src/trace/partition.rs`
  - Thin wrapper: `from_store()`, `into_store()`, `store()` accessor
- [x] `single_partition()` — default non-sharded strategy (wraps full store)
- [x] `build_traces_for()` — variant of `build_all_traces()` accepting `WitnessPartition`
- [x] Existing `build_all_traces()` delegates to shared `build_traces_core()` (no behavioral change)
- [x] All 979 existing tests pass, 2 new partition unit tests
- [x] 981 total tests passing
- [x] Tier-based `partition_witness()` deferred to Goal 3 (requires `ProofPlan` type from sharding)

---

## Completion Criteria

- [x] `stark` crate owns STARK protocol math (permutation, RAP folders, EF4 helpers)
- [x] `machine` crate owns orchestration (quotient computation, ProofInstance, prove/verify pipelines, registry)
- [x] `witness` crate can produce per-proof-instance traces (WitnessPartition + build_traces_for)
- [x] All 981 tests pass (979 original + 2 new)
- [x] No circular dependencies introduced
- [x] `prove_with_key()` and `verify_with_key()` behavior identical (refactor, not rewrite)

## Verification

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
```
