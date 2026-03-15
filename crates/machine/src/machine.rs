//! High-level machine interface for multi-proof STARK proving and verification.
//!
//! [`TabulaMachine`] orchestrates the C+2 proof architecture:
//! 1 execution proof + C column proofs + 1 root proof.
//!
//! ```ignore
//! let machine = TabulaMachine::new(&col_configs)?;
//! let traces = machine.build_traces(stores)?;
//! let proof = machine.prove(traces, &column_identities, statement)?;
//! machine.verify(&proof)?;
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use p3_challenger::{CanObserve, CanSample};
use p3_field::PrimeCharacteristicRing;
use p3_uni_stark::StarkGenericConfig;
use rayon::prelude::*;

use tabula_core::error::TabulaError;
use tabula_ir::PrecompileId;
use tabula_stark::air::interaction::BusId;
use tabula_stark::air::statement::PublicStatement;

use crate::builder::MachineBuilder;
use crate::config::{Challenger, EF4, TabulaStarkConfig};
use crate::keys::{TabulaVerifyingKey, compute_external_buses};
use crate::proof::{
    ColumnIdentity, ColumnProofEntry, ProofTier, ProveError, SubProofEnvelope, TabulaProof,
    VerificationError, check_cross_proof_bus_balance,
};
use crate::proof_instance::{MainCommitment, ProofInstance};
use crate::property::PropertyOpening;
use crate::registry::{ChipRegistry, SetupError};
use crate::setup::{ColumnSetupConfig, ProofSetups, ProofTraces, build_proof_traces};

/// A configured STARK machine for multi-proof proving and verification.
///
/// Owns per-tier setups (registries, keys, chips) for the C+2 proof
/// architecture. Created from column configuration, then used to build
/// traces, generate proofs, and verify proofs.
pub struct TabulaMachine {
    config: TabulaStarkConfig,
    setups: ProofSetups,
    property_openings: Vec<Box<dyn PropertyOpening>>,
    precompile_ids: BTreeSet<PrecompileId>,
}

impl fmt::Debug for TabulaMachine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TabulaMachine")
            .field("exec_chips", &self.setups.execution.registry.chip_ids())
            .field("num_columns", &self.setups.columns.len())
            .finish_non_exhaustive()
    }
}

impl TabulaMachine {
    /// Create a machine for the given column configuration.
    ///
    /// Builds per-tier setups (execution, per-column, root) with registries,
    /// keys, and chip sets. Uses the default STARK configuration.
    ///
    /// For custom configurations or extensions, use [`builder()`](Self::builder).
    pub fn new(col_configs: &[ColumnSetupConfig]) -> Result<Self, SetupError> {
        MachineBuilder::new()
            .with_columns(col_configs.to_vec())
            .build()
    }

    /// Create a machine with a custom STARK configuration.
    ///
    /// For full control (extensions, custom root proof), use [`builder()`](Self::builder).
    pub fn with_config(
        col_configs: &[ColumnSetupConfig],
        config: TabulaStarkConfig,
    ) -> Result<Self, SetupError> {
        MachineBuilder::new()
            .with_columns(col_configs.to_vec())
            .with_config(config)
            .build()
    }

    /// Create a builder for customized machine construction.
    ///
    /// The builder allows registering extensions, custom root proofs,
    /// and per-column commitment schemes.
    ///
    /// ```ignore
    /// let machine = TabulaMachine::builder()
    ///     .with_columns(col_configs)
    ///     .with_extension(MyExtension)
    ///     .build()?;
    /// ```
    pub fn builder() -> MachineBuilder {
        MachineBuilder::new()
    }

    /// Construct from pre-built parts (used by [`MachineBuilder`]).
    pub(crate) fn from_parts(
        config: TabulaStarkConfig,
        setups: ProofSetups,
        property_openings: Vec<Box<dyn PropertyOpening>>,
        precompile_ids: BTreeSet<PrecompileId>,
    ) -> Self {
        Self {
            config,
            setups,
            property_openings,
            precompile_ids,
        }
    }

    /// Build per-tier traces from partitioned witness stores.
    ///
    /// Each tier's traces are built independently via phase-ordered chip dispatch.
    pub fn build_traces(
        &self,
        stores: tabula_witness::trace::PartitionedStores,
    ) -> Result<ProofTraces, TabulaError> {
        build_proof_traces(&self.setups, stores)
    }

    /// Generate a multi-proof from traces and column identities.
    ///
    /// Consumes `traces` by value, transferring ownership of trace matrices
    /// into PCS commit calls without cloning.
    pub fn prove(
        &self,
        traces: ProofTraces,
        column_identities: &[ColumnIdentity],
        statement: PublicStatement,
    ) -> Result<TabulaProof, ProveError> {
        prove_impl(
            &self.config,
            &self.setups,
            traces,
            column_identities,
            statement,
        )
    }

    /// Verify a multi-proof.
    ///
    /// Reconstructs shared LogUp challenges, verifies each sub-proof
    /// independently, then checks cross-proof bus balance.
    pub fn verify(&self, proof: &TabulaProof) -> Result<(), VerificationError> {
        verify_impl(&self.config, &self.setups, proof)
    }

    /// The STARK configuration.
    pub fn config(&self) -> &TabulaStarkConfig {
        &self.config
    }

    /// The per-tier proof setups.
    pub fn setups(&self) -> &ProofSetups {
        &self.setups
    }

    /// Registered property openings for structural queries.
    ///
    /// Used by the executor to resolve `PropertyRead` instructions against
    /// the correct opening implementation based on commitment compatibility.
    pub fn property_openings(&self) -> &[Box<dyn PropertyOpening>] {
        &self.property_openings
    }

    /// Precompile IDs registered for proving.
    ///
    /// Useful for verifying consistency with the executor's
    /// [`PrecompileRegistry`] at application setup time.
    pub fn precompile_ids(&self) -> &BTreeSet<PrecompileId> {
        &self.precompile_ids
    }
}

// ── Proving ──────────────────────────────────────────────────────────────────

/// A proof instance tagged with its tier identity and column metadata.
///
/// Enables uniform parallel operations across all C+2 instances while
/// preserving tier identity for Fiat-Shamir ordering and output reconstruction.
struct LabeledInstance<'a> {
    tier: ProofTier,
    identity: Option<ColumnIdentity>,
    instance: ProofInstance<'a>,
}

fn prove_impl(
    config: &TabulaStarkConfig,
    setups: &ProofSetups,
    traces: ProofTraces,
    column_identities: &[ColumnIdentity],
    statement: PublicStatement,
) -> Result<TabulaProof, ProveError> {
    // ── Derive external buses from tier metadata ──────────────────────────
    let external_buses = compute_external_buses(
        std::iter::once(&setups.execution.proving_key)
            .chain(setups.columns.iter().map(|(_, s)| &s.proving_key))
            .chain(std::iter::once(&setups.root.proving_key)),
    );

    // ── Phase 0-1: Create all C+2 ProofInstances in parallel ─────────────
    //
    // Build input descriptors sequentially (cheap pointer moves), then
    // construct ProofInstances in parallel (CPU-bound interaction eval).
    let ProofTraces {
        execution: exec_traces,
        columns: col_traces,
        root: root_traces,
    } = traces;

    let num_cols = col_traces.len();
    type InstanceInput<'a> = (
        ProofTier,
        Option<ColumnIdentity>,
        &'a ChipRegistry,
        &'a crate::keys::TabulaProvingKey,
        tabula_witness::trace::TraceMap,
    );
    let mut inputs: Vec<InstanceInput<'_>> = Vec::with_capacity(2 + num_cols);

    inputs.push((
        ProofTier::Execution,
        None,
        &setups.execution.registry,
        &setups.execution.proving_key,
        exec_traces,
    ));
    for (((_, trace_map), (_, setup)), identity) in col_traces
        .into_iter()
        .zip(setups.columns.iter())
        .zip(column_identities.iter())
    {
        inputs.push((
            ProofTier::Column {
                table_id: identity.table_id,
                col_id: identity.col_id,
            },
            Some(*identity),
            &setup.registry,
            &setup.proving_key,
            trace_map,
        ));
    }
    inputs.push((
        ProofTier::Root,
        None,
        &setups.root.registry,
        &setups.root.proving_key,
        root_traces,
    ));

    let mut instances: Vec<LabeledInstance<'_>> = inputs
        .into_par_iter()
        .map(|(tier, identity, registry, pk, trace_map)| {
            let instance = ProofInstance::new(config, registry, pk, trace_map)?;
            Ok(LabeledInstance {
                tier,
                identity,
                instance,
            })
        })
        .collect::<Result<Vec<_>, ProveError>>()?;

    // ── Phase 2-3: Commit main traces in parallel ────────────────────────
    let commitments: Vec<MainCommitment> = instances
        .par_iter_mut()
        .map(|li| li.instance.commit_main())
        .collect::<Result<Vec<_>, ProveError>>()?;

    // ── Phase 4: Fiat-Shamir barrier (sequential, deterministic) ─────────
    //
    // All commitments must be observed in canonical order [exec, col_0, ..., col_{C-1}, root]
    // before sampling shared LogUp challenges. The Vec preserves this order.
    let mut challenger = config.initialise_challenger();
    let statement_felts = statement.to_field_elements();
    challenger.observe_slice(&statement_felts);
    for c in &commitments {
        observe_commitment(&mut challenger, c);
    }

    let logup_alpha: EF4 = challenger.sample();
    let logup_beta: EF4 = challenger.sample();
    let challenges = [logup_alpha, logup_beta];

    // ── Phase 5: Build permutation traces in parallel ────────────────────
    instances
        .par_iter_mut()
        .try_for_each(|li| li.instance.build_perm_traces(challenges).map(|_| ()))?;

    // ── Phase 5b: Verify internal bus balance in parallel ────────────────
    instances
        .par_iter()
        .try_for_each(|li| check_internal_balance(&li.instance, li.tier, &external_buses))?;

    // ── Phase 5c: Verify cross-proof bus balance (sequential, cheap) ─────
    let all_external: Vec<BTreeMap<BusId, EF4>> = instances
        .iter()
        .map(|li| extract_external_cumsums(&li.instance, &external_buses))
        .collect();
    check_cross_proof_bus_balance(all_external.iter())
        .map_err(|(bus_id, total)| ProveError::CrossProofBusImbalance { bus_id, total })?;

    // ── Phases 6-11: Prove all C+2 sub-proofs in parallel ────────────────
    //
    // Each sub-proof gets an independent Fiat-Shamir fork (challenger clone)
    // from the same transcript state. Internal FRI queries are independent
    // across sub-proofs — the shared prefix ensures consistent LogUp challenges.
    let challengers: Vec<Challenger> = (0..instances.len()).map(|_| challenger.clone()).collect();

    let all_results: Vec<_> = instances
        .into_par_iter()
        .zip(all_external.into_par_iter())
        .zip(challengers.into_par_iter())
        .map(|((li, exported), mut ch)| {
            let sub = li.instance.prove(&mut ch)?;
            Ok((li.tier, li.identity, exported, sub))
        })
        .collect::<Result<Vec<_>, ProveError>>()?;

    // ── Reconstruct TabulaProof from ordered results ─────────────────────
    let mut results = all_results.into_iter();

    let (_, _, exec_cumsums, exec_sub) = results.next().unwrap();
    let exec_envelope = make_envelope(ProofTier::Execution, exec_sub, exec_cumsums);

    let col_entries: Vec<ColumnProofEntry> = results
        .by_ref()
        .take(num_cols)
        .map(|(tier, identity, exported, sub)| ColumnProofEntry {
            proof: make_envelope(tier, sub, exported),
            identity: identity.expect("column instance must have identity"),
        })
        .collect();

    let (_, _, root_cumsums, root_sub) = results.next().unwrap();
    let root_envelope = make_envelope(ProofTier::Root, root_sub, root_cumsums);

    Ok(TabulaProof {
        execution: exec_envelope,
        columns: col_entries,
        root: root_envelope,
        statement,
    })
}

// ── Verification ─────────────────────────────────────────────────────────────

fn verify_impl(
    config: &TabulaStarkConfig,
    setups: &ProofSetups,
    proof: &TabulaProof,
) -> Result<(), VerificationError> {
    // ── Phase 1: Reconstruct shared Fiat-Shamir challenges ───────────────
    let mut challenger = config.initialise_challenger();
    let statement_felts = proof.statement.to_field_elements();
    challenger.observe_slice(&statement_felts);

    observe_sub_proof_commitment(&mut challenger, &proof.execution);
    for col in &proof.columns {
        observe_sub_proof_commitment(&mut challenger, &col.proof);
    }
    observe_sub_proof_commitment(&mut challenger, &proof.root);

    let logup_alpha: EF4 = challenger.sample();
    let logup_beta: EF4 = challenger.sample();
    let logup_challenges = [logup_alpha, logup_beta];

    // ── Phase 2: Verify all sub-proofs in parallel ───────────────────────
    //
    // Each sub-proof gets an independent Fiat-Shamir fork. Build a unified
    // list of (registry, vk, envelope) for parallel dispatch.

    // Build index for O(1) setup lookup by (table, col) identity.
    let setup_index: BTreeMap<(u32, u16), usize> = setups
        .columns
        .iter()
        .enumerate()
        .map(|(i, ((t, c), _))| ((t.0, c.0), i))
        .collect();

    // Collect all verification tasks: (registry, vk, envelope, tier_index).
    let mut verify_tasks: Vec<(&ChipRegistry, &TabulaVerifyingKey, &SubProofEnvelope)> =
        Vec::with_capacity(2 + proof.columns.len());

    verify_tasks.push((
        &setups.execution.registry,
        &setups.execution.verifying_key,
        &proof.execution,
    ));

    for (i, col) in proof.columns.iter().enumerate() {
        let key = (col.identity.table_id, col.identity.col_id);
        let setup_idx = setup_index
            .get(&key)
            .ok_or(VerificationError::ColumnIdentityMismatch {
                index: i,
                proof_table: col.identity.table_id,
                proof_col: col.identity.col_id,
            })?;
        let setup = &setups.columns[*setup_idx].1;
        verify_tasks.push((&setup.registry, &setup.verifying_key, &col.proof));
    }

    verify_tasks.push((
        &setups.root.registry,
        &setups.root.verifying_key,
        &proof.root,
    ));

    verify_tasks
        .par_iter()
        .try_for_each(|(registry, vk, envelope)| {
            verify_sub_proof(
                config,
                registry,
                vk,
                envelope,
                logup_challenges,
                &mut challenger.clone(),
            )
        })?;

    // ── Phase 3: Verify cross-proof bus balance ──────────────────────────
    let all_maps = std::iter::once(&proof.execution.exported_cumsums)
        .chain(proof.columns.iter().map(|c| &c.proof.exported_cumsums))
        .chain(std::iter::once(&proof.root.exported_cumsums));

    check_cross_proof_bus_balance(all_maps)
        .map_err(|(bus_id, total)| VerificationError::CrossProofBusImbalance { bus_id, total })
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Build a [`SubProofEnvelope`] from a completed sub-proof.
fn make_envelope(
    tier: ProofTier,
    sub: crate::proof_instance::SubProof,
    exported_cumsums: BTreeMap<BusId, EF4>,
) -> SubProofEnvelope {
    SubProofEnvelope {
        tier,
        preprocessed_commitment: sub.preprocessed_commitment,
        main_commitment: sub.main_commitment,
        perm_commitment: sub.perm_commitment,
        quotient_commitment: sub.quotient_commitment,
        opening_proof: sub.opening_proof,
        chip_openings: sub.chip_openings,
        exported_cumsums,
    }
}

/// Observe a [`MainCommitment`] into the Fiat-Shamir transcript (proving).
fn observe_commitment(challenger: &mut Challenger, commitment: &MainCommitment) {
    if let Some(ref pp_c) = commitment.preprocessed {
        challenger.observe(pp_c);
    }
    challenger.observe(&commitment.main);
}

/// Observe a sub-proof's commitments into the Fiat-Shamir transcript (verification).
fn observe_sub_proof_commitment(challenger: &mut Challenger, envelope: &SubProofEnvelope) {
    if let Some(ref pp_c) = envelope.preprocessed_commitment {
        challenger.observe(pp_c);
    }
    challenger.observe(&envelope.main_commitment);
}

/// Verify a single sub-proof using pre-computed LogUp challenges.
fn verify_sub_proof(
    config: &TabulaStarkConfig,
    registry: &ChipRegistry,
    vk: &TabulaVerifyingKey,
    envelope: &SubProofEnvelope,
    logup_challenges: [EF4; 2],
    challenger: &mut Challenger,
) -> Result<(), VerificationError> {
    crate::verify::verify_sub_proof_with_challenges(
        config,
        registry,
        vk,
        &envelope.chip_openings,
        envelope.preprocessed_commitment.clone(),
        envelope.main_commitment.clone(),
        envelope.perm_commitment.clone(),
        envelope.quotient_commitment.clone(),
        &envelope.opening_proof,
        logup_challenges,
        challenger,
    )
}

/// Check that all internal buses balance within a proof instance.
fn check_internal_balance(
    instance: &ProofInstance<'_>,
    tier: ProofTier,
    external_buses: &BTreeSet<BusId>,
) -> Result<(), ProveError> {
    let cumsums = instance.cumsums_by_bus();
    for (&bus_id, &cumsum) in &cumsums {
        if external_buses.contains(&bus_id) {
            continue;
        }
        if cumsum != EF4::ZERO {
            return Err(ProveError::InternalBusImbalance {
                tier,
                bus_id,
                cumsum: tabula_stark::rap::ef4::ef4_coeffs(cumsum),
            });
        }
    }
    Ok(())
}

/// Extract external bus cumsums from a proof instance.
fn extract_external_cumsums(
    instance: &ProofInstance<'_>,
    external_buses: &BTreeSet<BusId>,
) -> BTreeMap<BusId, EF4> {
    let all = instance.cumsums_by_bus();
    all.into_iter()
        .filter(|(bus, _)| external_buses.contains(bus))
        .collect()
}
