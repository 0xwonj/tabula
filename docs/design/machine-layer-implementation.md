# Machine Layer Implementation Plan

> **Status**: Design — not yet implemented
> **Prerequisite**: [tabula-machine-architecture.md](tabula-machine-architecture.md) §10, §11, §12
> **Scope**: Replace `stark/` with `machine/` — the proving protocol itself

---

## §1 Problem Statement

The current `stark/` module wraps `p3-uni-stark` for per-chip proving. This creates five structural problems that cannot be fixed incrementally:

| ID | Problem | File:Line | Impact |
|----|---------|-----------|--------|
| **F1** | Per-chip FRI | `prover.rs:105-111` — 9× `p3_uni_stark::prove()` | 9 separate FRI proofs; no shared PCS |
| **F2** | Unsound cumsums (C1) | `proof.rs:41` — `cumsum_final: EF4` bare number | Malicious prover can forge LogUp balance |
| **F3** | Duplicate constraint eval | `prover.rs:168-189` — `compute_chip_records()` | Re-evaluates all chips via DebugConstraintBuilder after proving |
| **F4** | Dead keygen | `keygen.rs` → `InteractionDescriptor` unused by prover | Prover ignores extracted column references |
| **F5** | Hardcoded statement | `verifier.rs:84` — `if chip_id == SMT_TABLE_PATH` | Not extensible to apps |

**Root cause**: `p3_uni_stark::prove()` is a self-contained per-chip proof pipeline. It creates its own FRI, manages its own challenger, and has no hook for additional trace columns (permutation traces). All five problems stem from this API constraint.

---

## §2 Current Architecture Analysis

### 2.1 Data Flow (stark/)

```
TraceMap ──→ prover::prove()
              │
              ├── Phase 1: For each chip:
              │     p3_uni_stark::prove_with_preprocessed()  ← per-chip FRI
              │     → ChipProofEntry { proof, cumsum_final: ZERO }
              │
              ├── Phase 2: derive_challenges(chip_proofs)
              │     DuplexChallenger observes heights + PVs (NOT commitments)
              │     → [α, β]: [EF4; 2]
              │
              └── Phase 3: compute_chip_records()  ← DUPLICATE EVAL
                    For each chip: evaluate_chip_with_preprocessed_and_public_values()
                    → RecordedInteraction[]
                    compute_cumsums_ef4(records, challenges)
                    → cumsum_final per chip (BARE EF4, not committed)
```

### 2.2 Key Files

| File | Lines | Purpose | Fate |
|------|-------|---------|------|
| `stark/prover.rs` | 190 | Per-chip prove + LogUp recording | **Replace** |
| `stark/verifier.rs` | 128 | Per-chip verify + cumsum check | **Replace** |
| `stark/proof.rs` | 83 | TabulaProof, ChipProofEntry | **Replace** |
| `stark/permutation.rs` | 321 | Challenges, fingerprints, cumsums | **Reuse** fingerprint math; replace cumsum logic |
| `stark/config.rs` | ~40 | Type aliases, FRI config | **Move** to machine/ |
| `stark/bridge.rs` | 26 | EmptyMessageBuilder impls | **Move** to machine/ |
| `stark/mod.rs` | 41 | Module declarations | **Replace** |
| `air/keygen.rs` | 576 | Column-scanning extraction | **Reuse** as-is (becomes consumed by machine/) |
| `air/descriptor.rs` | 26 | InteractionDescriptor type | **Reuse** as-is |
| `air/chip_instance.rs` | 114 | ChipInstance wrapper | **Reuse** |

### 2.3 What's Already Generic (Steps 1-4 Complete)

| Component | Status | File |
|-----------|--------|------|
| `BusId(u16)` open newtype | Done | `air/interaction.rs` |
| `ChipId(u16)` open newtype | Done | `chips/mod.rs` |
| `TraceContributor` trait + dispatch | Done | `trace/contributor.rs`, `air/chip_set.rs` |
| `WitnessStore` typed key-value | Done | `trace/contributor.rs` |
| Generic `build_all_traces::<CS>()` | Done | `trace/orchestration.rs` |
| Generic `debug_validate_trace_map::<CS>()` | Done | `trace/validation.rs` |
| `bus_manifest()` on ChipSet | Done | `air/chip_set.rs` |

These are **not affected** by the Machine Layer change. The trace pipeline feeds into the prover; only the prover/verifier/proof change.

---

## §3 Target Architecture

### 3.1 Module Structure

```
crates/proof/src/machine/
├── mod.rs              # Module declarations + re-exports
├── config.rs           # TabulaStarkConfig, EF4, default_config() (moved from stark/)
├── bridge.rs           # EmptyMessageBuilder impls (moved from stark/)
├── keys.rs             # ProvingKey, VerifyingKey, ChipProvingKey
├── prover.rs           # prove(pk, traces, statement) → MachineProof
├── verifier.rs         # verify(vk, proof) → Result<(), VerificationError>
├── proof.rs            # MachineProof, ChipProofData, VerificationError
├── permutation.rs      # Descriptor-based perm trace generation + fingerprints
├── quotient.rs         # Per-chip quotient polynomial computation
└── rap.rs              # RAP wrapper (main + perm constraint evaluation)
```

### 3.2 Two-Round Protocol

```
┌──────────────────────────────────────────────────────────────────┐
│ Round 1: Main Traces                                             │
│   For each chip in ProvingKey.chips:                             │
│     commit(main_trace[chip]) → commitment_main[chip]             │
│     challenger.observe(commitment_main[chip])                    │
│   challenger.observe(public_values)                              │
│                                                                  │
│ Round 2: Permutation Traces                                      │
│   [α, β] = challenger.sample()   ← bound to main commitments    │
│   For each chip:                                                 │
│     build_perm_trace(descriptor[chip], main[chip], α, β)        │
│       → (perm_trace[chip], cumsum_final[chip])                  │
│     commit(perm_trace[chip]) → commitment_perm[chip]            │
│     challenger.observe(commitment_perm[chip])                    │
│   Assert Σ cumsum_final = 0                                      │
│                                                                  │
│ Round 3: Quotient + FRI                                          │
│   zeta = challenger.sample()                                     │
│   For each chip:                                                 │
│     evaluate RAP constraints(main + perm) at zeta               │
│     → quotient_chunks[chip]                                      │
│     commit(quotient_chunks[chip])                                │
│   Single FRI opening proof across ALL committed polynomials      │
└──────────────────────────────────────────────────────────────────┘
```

### 3.3 Key Insight: Descriptor-Based Permutation Traces

`air/keygen.rs` already extracts `InteractionDescriptor<BabyBear>` per chip via column-scanning. Each descriptor contains `Vec<Interaction<BabyBear>>` where `Interaction.values` are `Vec<VirtualPairCol>` and `Interaction.multiplicity` is a `VirtualPairCol`.

`VirtualPairCol` has an `eval(&[F], &[F]) -> F` method that computes interaction field values directly from `(local_row, next_row)` pairs. This means:

```
Current (F3):
  prover.rs → p3_uni_stark::prove() [eval #1]
  prover.rs → compute_chip_records() → DebugConstraintBuilder [eval #2]

Target:
  machine/prover.rs → commit main traces [no eval]
  machine/permutation.rs → VirtualPairCol::eval() on raw trace data [descriptor eval]
  machine/quotient.rs → RAP eval for quotient [eval #1 — the only eval]
```

**Zero re-evaluation**: The permutation trace is built from descriptors, not from re-running `eval()`. Constraint evaluation happens exactly once — during quotient polynomial computation.

---

## §4 Design Details

### 4.1 ProvingKey / VerifyingKey (`machine/keys.rs`)

```rust
/// Per-chip metadata for the prover.
pub struct ChipProvingKey {
    pub chip_id: ChipId,
    pub main_width: usize,
    pub preprocessed_width: usize,
    pub num_public_values: usize,
    /// Column-scanned interaction descriptors.
    /// Used for permutation trace generation (no re-eval needed).
    pub interactions: InteractionDescriptor<BabyBear>,
    /// Preprocessed trace data (e.g., Poseidon round constants).
    pub preprocessed_trace: Option<RowMajorMatrix<BabyBear>>,
}

/// Complete proving key for the machine layer.
pub struct ProvingKey {
    pub chip_keys: Vec<ChipProvingKey>,
    pub bus_manifest: Vec<BusId>,
}

/// Verifier-side key (no trace data, just metadata).
pub struct VerifyingKey {
    pub chips: Vec<ChipVerifyingSpec>,
    pub bus_manifest: Vec<BusId>,
}

pub struct ChipVerifyingSpec {
    pub chip_id: ChipId,
    pub main_width: usize,
    pub preprocessed_width: usize,
    pub num_public_values: usize,
    pub num_sends_per_row: usize,
    pub num_receives_per_row: usize,
}

/// Generate ProvingKey + VerifyingKey from a chip set.
///
/// Wraps `air/keygen.rs::keygen::<CS>()` + adds preprocessed traces + bus manifest.
pub fn machine_keygen<CS>() -> (ProvingKey, VerifyingKey)
where
    CS: ChipSet + for<'a> Air<DebugConstraintBuilder<'a, BabyBear>>
```

### 4.2 Permutation Trace Generation (`machine/permutation.rs`)

```rust
/// Build the permutation trace for one chip from its interaction descriptors.
///
/// For each row, evaluates all VirtualPairCol references against the raw trace
/// to compute fingerprints, then accumulates the running LogUp sum.
///
/// Returns (permutation_trace, cumsum_final).
///
/// The permutation trace has `4 * (num_sends + num_receives)` columns
/// (each EF4 element = 4 BabyBear columns).
pub fn build_perm_trace(
    cpk: &ChipProvingKey,
    main_trace: &RowMajorMatrix<BabyBear>,
    alpha: EF4,
    beta: EF4,
) -> (RowMajorMatrix<BabyBear>, EF4)
```

**Algorithm** (per row `i`):
1. Extract `local = main_trace.row(i)`, `next = main_trace.row((i+1) % height)`
2. For each send descriptor: `vpc.eval(local, next)` → field values, `mult.eval()` → multiplicity
3. Compute EF4 fingerprint: `f = α + bus_tag + β·v[0] + β²·v[1] + …` (reuse existing `compute_fingerprint_ef4`)
4. `cumsum[i] = cumsum[i-1] + m/f`
5. Store cumsum as 4 BabyBear columns in perm trace

### 4.3 MachineProof (`machine/proof.rs`)

```rust
pub struct MachineProof {
    /// Per-chip proof data.
    pub chip_proofs: Vec<ChipProofData>,
    /// Fiat-Shamir-derived LogUp challenges [α, β] in EF4.
    pub logup_challenges: [EF4; 2],
    /// Public statement (state root transition).
    pub statement: PublicStatement,
    // NOTE: Commitments and FRI proof are embedded in the p3 Proof structures.
    // The exact layout depends on how we batch the PCS operations.
}

pub struct ChipProofData {
    pub chip_id: ChipId,
    /// Per-chip main trace proof (from shared PCS).
    pub main_proof: Proof<TabulaStarkConfig>,
    /// Per-chip permutation trace proof.
    pub perm_proof: Proof<TabulaStarkConfig>,  // or: shared commitment
    /// Cumulative sum — NOW PCS-committed via perm trace.
    pub cumsum_final: EF4,
    pub trace_height: usize,
    pub public_values: Vec<BabyBear>,
}
```

### 4.4 RAP Wrapper (`machine/rap.rs`)

During quotient computation, the AIR must include both main constraints AND permutation transition constraints:

```rust
/// RAP (Randomized AIR with Preprocessing) wrapper.
///
/// Evaluates the chip's main constraints via `inner.eval(builder)`,
/// then adds permutation trace transition constraints:
///   cumsum[i] - cumsum[i-1] = Σ(±m/f)  for each interaction
pub struct RapChip<'a, CS> {
    inner: &'a ChipInstance<CS>,
    num_interactions: usize,
    alpha: EF4,
    beta: EF4,
}

impl<CS, AB> Air<AB> for RapChip<'_, CS>
where
    CS: ChipSet + Air<AB>,
    AB: AirBuilder<F = BabyBear> + ...,
{
    fn eval(&self, builder: &mut AB) {
        // 1. Main constraints
        self.inner.eval(builder);
        // 2. Permutation transition constraints
        //    (computed from the permutation trace columns)
    }
}
```

### 4.5 Statement Binding (`machine/verifier.rs`)

```rust
pub fn verify<CS: StarkAir>(
    vk: &VerifyingKey,
    proof: &MachineProof,
) -> Result<(), VerificationError>
{
    // 1. Validate chip manifest against VerifyingKey
    // 2. Re-derive Fiat-Shamir challenges
    // 3. Verify per-chip main + perm proofs
    // 4. Check Σ cumsum_final = 0
    // 5. Verify statement binding:
    //    - Match public values to chips via vk.chips[].num_public_values
    //    - No hardcoded chip ID checks
}
```

**F5 fix**: Statement binding is resolved by matching `ChipVerifyingSpec.num_public_values > 0` and distributing statement field elements. No chip-specific conditionals.

---

## §5 p3 API Dependencies

The machine layer bypasses `p3_uni_stark` and uses lower-level p3 crates:

| p3 Crate | API | Used For |
|----------|-----|----------|
| `p3-commit` | `Pcs::commit()`, `Pcs::open()` | Polynomial commitment |
| `p3-challenger` | `DuplexChallenger` | Fiat-Shamir transcript |
| `p3-dft` | `Radix2DitParallel` | NTT for quotient computation |
| `p3-fri` | `FriProver`, `FriVerifier` | FRI proof generation/verification |
| `p3-matrix` | `RowMajorMatrix` | Trace data |
| `p3-field` | `BabyBear`, `BinomialExtensionField` | Field arithmetic |

**Open question**: Whether p3's `StarkGenericConfig` can be reused for the PCS type bundle, or if we need our own config trait. SP1 defines its own `StarkGenericConfig` that extends p3's.

**Reference**: SP1's `sp1-stark/src/machine/prove.rs` and `sp1-stark/src/machine/verify.rs` implement this exact pattern (shared PCS, single FRI, committed cumsums) on top of p3.

---

## §6 Migration Strategy

### Phase 1: Keys + Descriptor-Based Permutation (~200 LOC)

**Goal**: Prove that keygen descriptors can generate correct permutation traces.

- Create `machine/keys.rs` — `machine_keygen::<CS>()` wrapping `air/keygen.rs`
- Create `machine/permutation.rs` — `build_perm_trace()` from descriptors
- **Test**: For each chip, verify that descriptor-based cumsums match existing `compute_cumsums_ef4()` output
- **No changes** to `stark/` — old prover still works

### Phase 2: Two-Round Prover + MachineProof (~400 LOC)

**Goal**: Working shared-PCS prover that produces `MachineProof`.

- Create `machine/prover.rs` — two-round protocol
- Create `machine/proof.rs` — MachineProof type
- Create `machine/quotient.rs` — quotient computation
- Create `machine/rap.rs` — RAP constraint wrapper
- Move `config.rs` and `bridge.rs` from `stark/` to `machine/`
- **Test**: E2E prove through machine layer (verifier still uses stark/)

### Phase 3: Verifier + Statement Binding (~200 LOC)

**Goal**: Complete prove→verify cycle through machine layer.

- Create `machine/verifier.rs` — generic verification
- **Test**: Full round-trip prove→verify with valid and invalid proofs
- Migrate E2E tests (`tests/stark_e2e.rs`) to machine layer

### Phase 4: Cleanup (~100 LOC net)

**Goal**: Remove old infrastructure.

- Delete `stark/` module entirely
- Update `machine/mod.rs` re-exports to preserve public API
- Update benchmarks (`benches/prover.rs`)
- Update MEMORY.md

**Coexistence**: `stark/` and `machine/` exist side-by-side during Phases 1-3. Both `TabulaProof` (old) and `MachineProof` (new) compile. `stark/` deleted only in Phase 4 after all tests pass through `machine/`.

---

## §7 Soundness Analysis

### Before (stark/)

| Property | Status |
|----------|--------|
| Per-chip main constraints | Sound (via p3-uni-stark FRI) |
| Cross-chip LogUp | **Unsound** — cumsums are bare EF4 values in proof |
| Fiat-Shamir binding | Partial — observes heights/PVs, NOT PCS commitments |
| Statement binding | Fragile — hardcoded chip ID check |

### After (machine/)

| Property | Status |
|----------|--------|
| Per-chip main constraints | Sound (via shared-PCS FRI) |
| Cross-chip LogUp | **Sound** — cumsums PCS-committed via permutation trace |
| Fiat-Shamir binding | Full — observes all PCS commitments |
| Statement binding | Generic — derived from VerifyingKey metadata |

### Permutation Trace Constraints

The permutation trace must satisfy, for each row `i`:

```
cumsum[i] = cumsum[i-1] + Σ_sends(m_j/f_j) - Σ_receives(m_k/f_k)
```

where `f_j = α + bus_tag_j + β·v_0 + β²·v_1 + …` and `m_j` is multiplicity.

At the last row: `cumsum[height-1] = cumsum_final`.

Cross-chip: `Σ_chips cumsum_final[chip] = 0`.

Since `cumsum_final` is the last row of a PCS-committed column, it cannot be forged.

---

## §8 Verification Checklist

1. `cargo test -p tabula-proof` — all 339+ existing tests pass at each phase
2. `cargo clippy --all-targets` — zero new warnings
3. E2E tests (3 in `tests/stark_e2e.rs`) pass through machine layer
4. Benchmark comparison: single FRI vs 9× per-chip FRI
5. Cumsums are PCS-committed (not bare field elements)
6. `air/keygen.rs` descriptors are consumed by `machine/permutation.rs`
7. No `SmtTablePath` or chip-specific hardcoding in verifier
8. Constraint evaluation occurs exactly once (quotient computation only)
9. Fiat-Shamir transcript observes PCS commitments (not just heights)
