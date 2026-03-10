# Proving Layer Refactoring

> Status: 🔵 Ready (no blockers)
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

### PL-4: ProofInstance Abstraction (~1.5 days)

> Depends: PL-1, PL-2, PL-3 (ProofInstance uses permutation + RAP from stark)

- [ ] Define `ProofInstance<'a>` in `machine/src/proof_instance.rs`
  - Owns: chip metadata, PCS prover data, challenger state
  - Methods: `new()`, `commit_main()`, `build_perm_trace()`, `prove_quotient_fri()`
- [ ] Define `SubProof` — output of a single proof instance (commitments + openings + cumsum)
- [ ] Refactor `prove_with_key()` to create a single `ProofInstance` internally
  - Zero behavioral change — existing API preserved
- [ ] Refactor `verify_with_key()` to use corresponding `VerifyInstance`
- [ ] All E2E STARK tests pass unchanged
- [ ] Add unit test: ProofInstance with subset of chips produces valid sub-proof

### PL-5: Witness Partitioning (~1 day)

> Depends: PL-4 (partitioning serves ProofInstance)

- [ ] Define `WitnessPartition` in `witness/src/trace/partition.rs`
  - Thin wrapper: subset of `WitnessStore` entries for one proof instance
- [ ] `partition_witness()` — splits `BatchWitness` by proof tier
  - Execution partition: InstructionRecords, StaticTableRows
  - Column partition[i]: per-(t,c) memory accesses, SSMC witness, SMT paths
  - Root partition: all Com_old/Com_new, SMT table paths
- [ ] `build_traces_for()` — variant of `build_all_traces()` accepting chip subset + partition
- [ ] Existing `build_all_traces()` delegates to `build_traces_for()` with full chip set + no partitioning
- [ ] All existing tests pass
- [ ] Add unit test: partition round-trip (partition → build → merge = original)

---

## Completion Criteria

- `stark` crate owns STARK protocol math (permutation, RAP folders, EF4 helpers)
- `machine` crate owns orchestration (quotient computation, ProofInstance, prove/verify pipelines, registry)
- `witness` crate can produce per-proof-instance traces
- All 979+ existing tests pass unchanged
- No circular dependencies introduced
- `prove_with_key()` and `verify_with_key()` behavior identical (refactor, not rewrite)

## Verification

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
```
