# Proving Layer Architecture

> Defines the boundary between STARK protocol logic (`tabula-stark`) and proof orchestration (`tabula-machine`), and the abstractions needed for multi-proof architectures.
> Related: [full-sharding-research.md](full-sharding-research.md), [sharded-protocol-design.md](sharded-protocol-design.md)

---

## Layer Boundaries

The crate structure separates two concerns:

1. **STARK protocol math** (`tabula-stark`) — permutation trace generation, RAP constraint evaluation, LogUp fingerprinting, EF4 helpers
2. **Proof orchestration** (`tabula-machine`) — chip grouping, PCS batching, Fiat-Shamir sequencing, prove/verify API, `ProofInstance` phasing

---

## Architecture

```
tabula-stark (Layer 1: STARK Protocol)
├── air/              — constraint framework, bus types, keygen
├── chips.rs          — ChipId, ChipSpec, ChipIdAllocator
├── debug/            — constraint checker, LogUp balance
├── gadgets.rs        — U64Limbs
├── trace/            — TraceContributor, TraceMap, DynChip, WitnessStore
├── permutation/      — LogUp permutation trace infrastructure
│   ├── challenges.rs — Fiat-Shamir challenge derivation (test-only)
│   ├── trace.rs      — generate_permutation_trace_from_interactions(), compute_fingerprint_ef4()
│   └── mod.rs        — PermutationError
├── EF4               — pub type alias (BinomialExtensionField<KoalaBear, 4>)
└── rap/              — RAP constraint evaluation and EF4 helpers
    ├── ef4.rs        — ef4_coeffs(), ef4_mul(), RowSelectors, fingerprint/cumsum helpers, build_alpha_powers()
    ├── prover.rs     — RapProverFolder (AirBuilder wrapper for perm constraints)
    └── verifier.rs   — RapVerifierFolder (AirBuilder wrapper for perm verification)

tabula-witness (Layer 3: Witness Pipeline)
├── witness/          — WitnessGenerator, BatchWitness
└── trace/
    ├── builder.rs    — TraceBuilder, prepare_witness_store()
    ├── orchestration.rs — build_all_traces(), build_traces_for()
    └── partition.rs  — WitnessPartition, single_partition()

tabula-machine (Layer 4: Proof Orchestration)
├── config.rs         — TabulaStarkConfig (re-exports EF4 from stark)
├── registry.rs       — ChipRegistry
├── composition.rs    — CommitmentScheme, MemoryModel, RootProof
├── machine.rs        — TabulaMachine, MachineBuilder
├── proof_instance.rs — ProofInstance (phased prover), MainCommitment, SubProof
├── prove/            — prove_with_key() (thin orchestrator over ProofInstance)
│   └── quotient.rs   — compute_quotient_standard(), compute_quotient_rap()
└── verify/           — verify_with_key()
```

---

## Permutation Trace Generation (in `stark`)

Permutation trace math (EF4 fingerprints, phi columns, cumsum accumulation) is pure STARK protocol — independent of how chips are grouped into proofs. Any `ProofInstance` needs this without importing `machine`.

```rust
pub fn generate_permutation_trace_from_interactions(
    interactions: &[RecordedInteraction<KoalaBear>],
    height: usize,
    challenges: [EF4; 2],
) -> Result<(RowMajorMatrix<KoalaBear>, EF4), PermutationError>;
```

`PermutationError` is defined in `stark` (decoupled from `ProveError`). Machine's `ProveError` has `From<PermutationError>`.

## RAP Folders (in `stark`)

`RapProverFolder` and `RapVerifierFolder` implement `AirBuilder` — they define how LogUp constraints are evaluated during quotient computation and verification. Decoupled from `TabulaStarkConfig` using direct type expressions:

```rust
type PV = <KoalaBear as Field>::Packing;
type PC = <EF4 as ExtensionField<KoalaBear>>::ExtensionPacking;
```

## Quotient Computation (in `machine`)

Quotient computation wires together RAP folders (from stark), constraint folders (from p3), PCS domains, and chip references — all machine-level types. The pure arithmetic helper `build_alpha_powers()` lives in `stark/src/rap/ef4.rs`.

---

## ProofInstance Abstraction (in `machine`)

A `ProofInstance` encapsulates a chip set with phase-level methods. The monolithic `prove_with_key()` creates a single instance; future sharded provers create multiple instances sharing a synchronized Fiat-Shamir transcript.

```rust
pub(crate) struct ProofInstance<'a> {
    config: &'a TabulaStarkConfig,
    chip_infos: Vec<ChipProveInfo<'a>>,
    // PCS state accumulated across phases...
}

impl<'a> ProofInstance<'a> {
    /// Phase 0-1: Collect chip metadata, evaluate interactions.
    pub fn new(config, registry, pk, traces) -> Result<Self, ProveError>;

    /// Phase 2-3: Commit preprocessed + main traces.
    pub fn commit_main(&mut self) -> Result<MainCommitment, ProveError>;

    /// Phase 5: Generate permutation traces using shared challenges.
    pub fn build_perm_traces(&mut self, challenges: [EF4; 2]) -> Result<EF4, ProveError>;

    /// Phase 6-11: Commit perm, quotients, open all.
    pub fn prove(self, challenger: &mut Challenger) -> Result<SubProof, ProveError>;
}
```

The orchestrator (`prove_with_key()`) manages the Fiat-Shamir transcript between phases:

```rust
pub fn prove_with_key(...) -> Result<TabulaProof, ProveError> {
    let mut instance = ProofInstance::new(config, registry, pk, traces)?;
    let commitment = instance.commit_main()?;

    let mut challenger = config.initialise_challenger();
    // ... observe commitment, sample challenges ...
    let cumsum = instance.build_perm_traces(challenges)?;
    // ... check cumsum == 0 ...
    let sub_proof = instance.prove(&mut challenger)?;

    Ok(sub_proof.into_tabula_proof(statement))
}
```

### Sync Points for Sharding

The phase boundaries naturally support multi-instance sync:

1. After `commit_main()` — all instances contribute commitments to transcript
2. After shared challenger samples LogUp challenges — all instances use same challenges
3. After `build_perm_traces()` — orchestrator checks cross-instance cumsum balance
4. `prove()` — each instance independently completes FRI

---

## Witness Partitioning (in `witness`)

`WitnessPartition` wraps a `WitnessStore` containing witness data for one proof instance. The current monolithic prover uses `single_partition()` (all data in one partition). `build_traces_for()` accepts a partition for chip-subset trace building.

```rust
pub struct WitnessPartition {
    store: WitnessStore,
}

pub fn build_traces_for(
    chips: &[Box<dyn DynChip>],
    bus_consumers: &[Box<dyn BusConsumer>],
    partition: WitnessPartition,
) -> Result<TraceMap, TabulaError>;
```

Tier-based partitioning (`partition_witness()` with a `ProofPlan`) is deferred to Goal 3 (Sharding Infrastructure).
