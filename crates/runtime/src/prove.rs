//! Proving pipeline: witness generation, trace building, and state reconstruction.
//!
//! Composes the witness and machine crates into a streamlined pipeline.
//! The primary API is [`TabulaRuntime`](crate::TabulaRuntime), but individual
//! steps are public for callers that need fine-grained control.

use std::collections::BTreeMap;

use p3_field::PrimeField32;
use p3_koala_bear::KoalaBear;

use tabula_artifact::{BatchFile, ChipSummary, StateFile};
use tabula_commitment::{ColumnState, KoalaBearCodec, NativeDigest, PoseidonHasher, scheme_tags};
use tabula_core::traits::ValueCodec;
use tabula_core::{Batch, BatchResult, ColId, InMemoryStaticTables, RowKey, TableId, TableSchema};
use tabula_ir::Program;
use tabula_machine::{ColumnIdentity, ProofTraces, PublicStatement, TabulaMachine, TabulaProof};
use tabula_witness::trace::{partition_by_tier, prepare_shard_witness};
use tabula_witness::{BatchWitness, TraceBuilder, WitnessGenerator};

use crate::error::RuntimeError;
use crate::execute::ExecutedBatch;

// ── Public input/output types ────────────────────────────────────────────────

/// Inputs for the proving pipeline.
///
/// Bundles the execution result with the original state and batch files
/// needed for witness generation and column state reconstruction.
pub struct ProveInput<'a> {
    /// Pre-execution state (for building `old_column_states`).
    pub state: &'a StateFile,
    /// Transaction batch (for witness store preparation).
    pub batch: &'a BatchFile,
    /// Executed batch result (from [`run_batch`](crate::run_batch)).
    pub executed: &'a ExecutedBatch,
}

/// Result of STARK proof generation.
pub struct ProveResult {
    /// The generated STARK proof.
    pub proof: TabulaProof,
    /// Summary of chips contributing to the proof.
    pub summary: ProofSummary,
}

/// Result of prove + verify.
pub struct VerifiedResult {
    /// The generated STARK proof.
    pub proof: TabulaProof,
    /// Whether verification passed.
    pub verified: bool,
    /// Summary of chips contributing to the proof.
    pub summary: ProofSummary,
}

/// Summary of chip contributions in a STARK proof.
#[derive(Debug, Clone)]
pub struct ProofSummary {
    /// Per-chip summaries (name + trace height).
    pub chips: Vec<ChipSummary>,
    /// Total number of chips.
    pub chip_count: usize,
}

impl ProofSummary {
    /// Extract chip summaries from a [`TabulaProof`].
    pub fn from_proof(proof: &TabulaProof) -> Self {
        let chips = collect_chip_summaries(proof);
        Self {
            chip_count: chips.len(),
            chips,
        }
    }
}

fn collect_chip_summaries(proof: &TabulaProof) -> Vec<ChipSummary> {
    let mut chips = Vec::new();
    for opening in &proof.execution.chip_openings {
        chips.push(ChipSummary {
            name: opening.chip_id.to_string(),
            trace_height: 1 << opening.degree_bits,
        });
    }
    for col in &proof.columns {
        for opening in &col.proof.chip_openings {
            chips.push(ChipSummary {
                name: opening.chip_id.to_string(),
                trace_height: 1 << opening.degree_bits,
            });
        }
    }
    for opening in &proof.root.chip_openings {
        chips.push(ChipSummary {
            name: opening.chip_id.to_string(),
            trace_height: 1 << opening.degree_bits,
        });
    }
    chips
}

// ── Pipeline steps ───────────────────────────────────────────────────────────

/// Build `old_column_states` from state file cells and schemas.
///
/// Enumerates ALL schema columns (not just those with data) to ensure
/// empty columns get proper commitments.
#[tracing::instrument(skip_all, fields(col_count))]
pub fn build_old_column_states(
    schemas: &BTreeMap<TableId, TableSchema>,
    state_file: &StateFile,
) -> Result<BTreeMap<(TableId, ColId), ColumnState<PoseidonHasher>>, RuntimeError> {
    let codec = KoalaBearCodec;
    let hasher = PoseidonHasher::new();

    // Group state cells by (table, col).
    let mut entries_by_col: BTreeMap<(TableId, ColId), Vec<(RowKey, Vec<KoalaBear>)>> =
        BTreeMap::new();

    for cell in &state_file.cells {
        if let Some(value) = &cell.value {
            let encoded = codec.encode(value).map_err(|e| RuntimeError::ColumnState {
                detail: format!(
                    "encode cell ({},{},{}): {e}",
                    cell.table, cell.col, cell.row
                ),
            })?;
            entries_by_col
                .entry((TableId(cell.table), ColId(cell.col)))
                .or_default()
                .push((RowKey(cell.row), encoded));
        }
    }

    let mut result = BTreeMap::new();
    for schema in schemas.values() {
        for col_def in &schema.columns {
            let mut entries = entries_by_col
                .remove(&(schema.id, col_def.id))
                .unwrap_or_default();
            entries.sort_by_key(|(row, _)| *row);
            let (state, _com) =
                ColumnState::commit(&hasher, schema.id, col_def.id, entries, scheme_tags::SSMC)
                    .map_err(|e| RuntimeError::ColumnState {
                        detail: e.to_string(),
                    })?;
            result.insert((schema.id, col_def.id), state);
        }
    }

    tracing::Span::current().record("col_count", result.len());
    Ok(result)
}

/// Generate a `BatchWitness` from execution results and column states.
#[tracing::instrument(skip_all)]
pub fn generate_witness(
    batch_result: &BatchResult,
    schemas: &BTreeMap<TableId, TableSchema>,
    old_column_states: &BTreeMap<(TableId, ColId), ColumnState<PoseidonHasher>>,
) -> Result<BatchWitness<PoseidonHasher>, RuntimeError> {
    let hasher = PoseidonHasher::new();
    let wg = WitnessGenerator::new(hasher);
    wg.generate(batch_result, schemas, old_column_states)
        .map_err(|e| RuntimeError::WitnessGeneration {
            detail: e.to_string(),
        })
}

/// Extract column identities from witness metadata.
pub fn extract_column_identities(witness: &BatchWitness<PoseidonHasher>) -> Vec<ColumnIdentity> {
    witness
        .columns
        .iter()
        .map(|col| ColumnIdentity {
            table_id: col.table.0,
            col_id: col.col.0,
            com_old: col.meta.com_old.0,
            com_new: col.meta.com_new.0,
        })
        .collect()
}

/// Build the public statement from witness state roots.
pub fn extract_statement(witness: &BatchWitness<PoseidonHasher>) -> PublicStatement {
    PublicStatement {
        old_root: witness.old_state_root,
        new_root: witness.new_state_root,
    }
}

/// Build traces from witness through the full pipeline:
/// witness store -> shard witness -> partition -> machine.build_traces.
#[tracing::instrument(skip_all)]
pub fn build_traces(
    machine: &TabulaMachine,
    witness: &BatchWitness<PoseidonHasher>,
    program: &Program,
    batch: &Batch,
    batch_result: &BatchResult,
    schemas: &BTreeMap<TableId, TableSchema>,
) -> Result<ProofTraces, RuntimeError> {
    let store = TraceBuilder::<PoseidonHasher, 3>::new(witness)
        .prepare_witness_store(
            program,
            batch,
            batch_result,
            schemas,
            &InMemoryStaticTables::new(),
            PoseidonHasher::new(),
        )
        .map_err(RuntimeError::TraceBuild)?;

    let shard_witness =
        prepare_shard_witness::<PoseidonHasher, 3>(witness).map_err(RuntimeError::TraceBuild)?;

    let stores = partition_by_tier(store, shard_witness);
    machine
        .build_traces(stores)
        .map_err(RuntimeError::TraceBuild)
}

/// Convert a `BatchFile` into a `Batch`.
pub fn convert_batch(batch_file: &BatchFile) -> Result<Batch, RuntimeError> {
    let transactions = batch_file
        .transactions
        .iter()
        .map(|t| t.to_transaction().map_err(RuntimeError::InvalidBatch))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Batch { transactions })
}

/// Convert `ExecutedBatch` fields into a `BatchResult`.
pub fn to_batch_result(executed: &ExecutedBatch) -> BatchResult {
    BatchResult {
        read_set_old: executed.read_set.clone(),
        write_set_final: executed.write_set.clone(),
        txs: executed.txs.clone(),
    }
}

/// Convert a `NativeDigest` (8 KoalaBear elements) to hex strings.
pub fn digest_to_hex(digest: &NativeDigest) -> Vec<String> {
    digest
        .0
        .iter()
        .map(|fe| format!("{:08x}", fe.as_canonical_u32()))
        .collect()
}
