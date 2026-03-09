# Machine Code Quality

> Status: ✅ Complete
> Crate: `tabula-machine`
> Design: [docs/design/implementation-workplan.md](../docs/design/implementation-workplan.md)

## Goal

Improve `tabula-machine` code quality: encapsulation, error handling, function extraction, directory organization.

## Tasks

### RAP Folder Encapsulation

- [x] `prove/rap_folder.rs` — private fields + `pub(crate)` getters (`accumulator()`, `constraint_index()`)
- [x] `verify/rap_folder.rs` — private fields + `pub(crate)` getter (`accumulator()`)
- [x] All callsites updated

### Error Types

- [x] `ProveError` enum in `proof.rs` (InvalidTraceHeight, MissingKeygenInfo, LogUpImbalance, NoChips, FingerprintZero)
- [x] `VerificationError` enum (ChipVerificationFailed, LogUpImbalance, InvalidChipManifest, PcsVerificationFailed)
- [x] `prove_with_key()` returns `Result<TabulaProof, ProveError>`
- [x] `verify_with_key()` returns `Result<(), VerificationError>`
- [x] `TabulaMachine::prove()` and `verify()` return Result types
- [x] Test callsites updated (machine.rs, stark_e2e.rs, daemon)

### Function Extraction

- [x] `prove/mod.rs` — helpers: `collect_chip_infos`, `build_index_map`, `compute_chip_quotients`, `open_and_extract`
- [x] `prove/quotient.rs` — `compute_quotient_standard`, `compute_quotient_rap`, `build_alpha_powers`
- [x] `verify/mod.rs` — helpers: `reconstruct_challenges`, `build_verification_rounds`, `verify_logup_and_public_values`, `validate_chip_manifest`, `verify_chip_constraints`, `recompose_quotient`

### Directory Restructuring

- [x] `prove/` directory: `mod.rs`, `quotient.rs`, `rap_folder.rs`
- [x] `verify/` directory: `mod.rs`, `rap_folder.rs`
- [x] Old `prover.rs`, `verifier.rs`, `rap/` removed
- [x] `lib.rs` module declarations updated

## Current Structure

```
crates/machine/src/
├── lib.rs, config.rs, ef4.rs, proof.rs, keys.rs
├── machine.rs, registry.rs, any_rap.rs, chip_ref.rs, composition.rs
├── permutation/{mod,challenges,trace,tests}.rs
├── prove/{mod,quotient,rap_folder}.rs
└── verify/{mod,rap_folder}.rs
```

## Remaining

- [x] Commit all changes
- [x] Verify zero clippy regressions from this work (pre-existing warnings only: too_many_arguments, duplicate bounds)

## Verification

```bash
cargo check -p tabula-machine
cargo test -p tabula-machine
cargo clippy -p tabula-machine --all-targets
cargo test --workspace  # 979 tests passing
```
