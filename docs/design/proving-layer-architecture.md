# Proving Layer Architecture

> Defines the boundary between STARK protocol logic (`tabula-stark`) and proof orchestration (`tabula-machine`), and the abstractions needed for multi-proof architectures.
> Related: [full-sharding-research.md](full-sharding-research.md), [sharded-protocol-design.md](sharded-protocol-design.md)

---

## Problem

The current crate boundaries conflate two concerns:

1. **STARK protocol math** — permutation trace generation, RAP constraint evaluation, LogUp fingerprinting
2. **Proof orchestration** — chip grouping, PCS batching, Fiat-Shamir sequencing, prove/verify API

Protocol math lives in `tabula-machine` today because it was developed alongside the monolithic prover. This prevents `ProofInstance` (a subset of chips with independent PCS) from being self-contained — it would need to duplicate or re-import protocol logic from the machine layer.

Additionally, the witness pipeline (`tabula-witness`) produces a single global `TraceMap` with no concept of partitioning traces across proof instances.

---

## Target Architecture

```
tabula-stark (Layer 1: STARK Protocol)
├── air/              — constraint framework, bus types, keygen (existing, unchanged)
├── chips.rs          — ChipId, ChipSpec, ChipIdAllocator (existing, unchanged)
├── debug/            — constraint checker, LogUp balance (existing, unchanged)
├── gadgets.rs        — U64Limbs (existing, unchanged)
├── trace/            — TraceContributor, TraceMap, DynChip, WitnessStore (existing, unchanged)
├── permutation/      — LogUp permutation trace infrastructure
│   ├── challenges.rs — Fiat-Shamir challenge derivation (test-only)
│   ├── trace.rs      — generate_permutation_trace_from_interactions(), compute_fingerprint_ef4()
│   └── mod.rs        — PermutationError
├── EF4               — pub type alias (BinomialExtensionField<BabyBear, 4>)
└── rap/              — RAP constraint evaluation and EF4 helpers
    ├── ef4.rs        — ef4_coeffs(), ef4_mul(), RowSelectors, fingerprint/cumsum helpers, build_alpha_powers()
    ├── prover.rs     — RapProverFolder (AirBuilder wrapper for perm constraints)
    └── verifier.rs   — RapVerifierFolder (AirBuilder wrapper for perm verification)

tabula-witness (Layer 3: Witness Pipeline)
├── witness/          — WitnessGenerator, BatchWitness (existing)
└── trace/
    ├── builder.rs    — TraceBuilder (existing)
    ├── orchestration.rs — build_all_traces() (existing)
    └── partition.rs  — ← NEW: WitnessPartition, partition_for_proof_instance()

tabula-machine (Layer 4: Proof Orchestration)
├── config.rs         — TabulaStarkConfig (existing, unchanged)
├── registry.rs       — ChipRegistry (existing, unchanged)
├── composition.rs    — CommitmentScheme, MemoryModel, RootProof (existing, unchanged)
├── machine.rs        — TabulaMachine, MachineBuilder (existing, adapted)
├── proof_instance.rs — ← NEW: ProofInstance (chip subset + independent PCS)
├── prove/            — prove_with_key() (existing, refactored to use ProofInstance)
└── verify/           — verify_with_key() (existing, refactored to use ProofInstance)
```

---

## What Moves

### 1. Permutation Trace Generation → `stark`

**Current**: `machine/src/permutation/` (3 files: mod.rs, challenges.rs, trace.rs)

**Reason**: Permutation trace math (EF4 fingerprints, phi columns, cumsum accumulation) is pure STARK protocol — independent of how chips are grouped into proofs. Any `ProofInstance` needs this without importing `machine`.

**Public API** (in `stark`):
```rust
pub fn generate_permutation_trace_from_interactions(
    interactions: &[RecordedInteraction<BabyBear>],
    height: usize,
    challenges: [EF4; 2],
) -> Result<(RowMajorMatrix<BabyBear>, EF4), TabulaError>;
```

**Migration**: Move files, update `use` paths. No logic changes.

### 2. RAP Folders → `stark`

**Current**: `machine/src/prove/rap_folder.rs`, `machine/src/verify/rap_folder.rs`

**Reason**: `RapProverFolder` and `RapVerifierFolder` implement `AirBuilder` — they define how LogUp constraints are evaluated during quotient computation and verification. This is protocol-level, not orchestration-level.

**Public API** (in `stark`):
```rust
pub struct RapProverFolder<'a, SC: StarkGenericConfig> { ... }
impl<SC> AirBuilder for RapProverFolder<'a, SC> { ... }

pub struct RapVerifierFolder<'a, SC: StarkGenericConfig> { ... }
impl<SC> AirBuilder for RapVerifierFolder<'a, SC> { ... }
```

**Migration**: Move files, update `use` paths. No logic changes. The folders currently reference `machine::config` types — abstract over `StarkGenericConfig` generic.

### 3. Quotient Computation — Stays in `machine`

**Location**: `machine/src/prove/quotient.rs` — `compute_quotient_rap()`, `compute_quotient_standard()`

**Rationale**: Quotient computation is prover orchestration, not standalone protocol math. It wires together RAP folders (from stark), constraint folders (from p3), PCS domains, and chip references — all machine-level types. The pure arithmetic helper `build_alpha_powers()` was extracted to `stark/src/rap/ef4.rs`.

**No migration needed**: The quotient module imports protocol math from `tabula-stark::rap` and stays in machine.

---

## What's New

### 4. ProofInstance Abstraction (in `machine`)

A `ProofInstance` encapsulates a subset of chips with independent PCS. The current monolithic `prove_with_key()` becomes a thin orchestrator over one or more `ProofInstance`s.

```rust
/// A self-contained proving unit: chip subset + independent PCS.
pub struct ProofInstance<'a> {
    config: &'a TabulaStarkConfig,
    chips: Vec<ChipProveInfo<'a>>,
}

impl<'a> ProofInstance<'a> {
    /// Collect chip metadata from registry subset + traces.
    pub fn new(
        config: &'a TabulaStarkConfig,
        registry: &'a ChipRegistry,
        pk: &TabulaProvingKey,
        traces: &TraceMap,
    ) -> Result<Self, ProveError>;

    /// Phase 1: Evaluate interactions + commit main traces.
    /// Returns the PCS commitment (for Fiat-Shamir).
    pub fn commit_main(&mut self) -> Result<MainCommitment, ProveError>;

    /// Phase 3: Build permutation traces using shared challenges.
    /// Returns internal cumsum (for cross-proof balance).
    pub fn build_perm_trace(&mut self, challenges: [EF4; 2]) -> Result<EF4, ProveError>;

    /// Phase 4: Commit perm traces, compute quotients, run FRI.
    /// Produces a standalone sub-proof.
    pub fn prove_quotient_fri(
        self,
        challenger: &mut Challenger,
    ) -> Result<SubProof, ProveError>;
}
```

The existing `prove_with_key()` becomes:

```rust
pub fn prove_with_key(...) -> Result<TabulaProof, ProveError> {
    let mut instance = ProofInstance::new(config, registry, pk, traces)?;
    let commitment = instance.commit_main()?;

    let mut challenger = config.initialise_challenger();
    // ... observe commitment, sample challenges ...
    let cumsum = instance.build_perm_trace(challenges)?;
    // ... check cumsum == 0 ...
    let sub_proof = instance.prove_quotient_fri(&mut challenger)?;

    Ok(sub_proof.into_tabula_proof(statement))
}
```

No behavioral change — the refactoring preserves the exact same prove/verify semantics. The `ShardedProver` (Goal 3, G2) later creates C+2 `ProofInstance`s with a shared sync point between `commit_main()` and `build_perm_trace()`.

### 5. Witness Partitioning (in `witness`)

The current `build_all_traces()` takes all chips and produces one `TraceMap`. For sharding, we need per-proof-instance partitions.

```rust
/// A partition of witness data for a single proof instance.
pub struct WitnessPartition {
    store: WitnessStore,
}

/// Partition a BatchWitness into per-proof-instance stores.
pub fn partition_witness(
    witness: &BatchWitness<impl FieldHasher>,
    proof_plan: &ProofPlan,
) -> Vec<WitnessPartition>;
```

This is a thin layer over the existing `WitnessStore` — each partition holds a subset of labeled data. `build_all_traces()` gains an overload that accepts a chip subset + partition.

---

## Verification Strategy

Each change is independently verifiable:

1. **Permutation + RAP move**: All existing tests pass (imports change, logic identical)
2. **ProofInstance**: Existing `prove_with_key()` reimplemented atop ProofInstance — E2E tests unchanged
3. **Witness partitioning**: New unit tests for partition correctness; existing tests use "single partition" path

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
```
