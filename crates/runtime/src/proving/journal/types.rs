use std::collections::{BTreeMap, BTreeSet};

use tabula_artifact::State;
use tabula_chips::precompile_transcript::PrecompileTranscriptCall;
use tabula_core::traits::StaticTableProvider;
use tabula_core::{Batch, ColId, TableId};
use tabula_executor::ExecutionJournal;
use tabula_ext::backend::precompile::ResolvedPrecompileCall;
use tabula_ir::PrecompileId;
use tabula_witness::stark::{LoweringOutput, TxLoweringOutput};
use tabula_witness::{
    AccessEvent, ColumnValueProfile, ColumnWrite, CommittedEntry, InitCell, PropertyReadClaim,
};

use crate::program::{ColumnProofSlot, PrecompileProofSlot, ResolvedProofProgram};

pub(crate) type ColumnPlanIndex = BTreeMap<(TableId, ColId), usize>;
pub(crate) type PrecompilePlanIndex = BTreeMap<PrecompileId, usize>;

/// Immutable input bundle for runtime-owned proof journal reduction.
#[derive(Clone, Copy)]
pub(crate) struct JournalInput<'a> {
    pub(crate) resolved_program: &'a ResolvedProofProgram,
    pub(crate) state: &'a State,
    pub(crate) batch: &'a Batch,
    pub(crate) execution_journal: &'a ExecutionJournal,
    pub(crate) static_tables: &'a dyn StaticTableProvider,
}

/// Canonical runtime-owned proof input for one batch.
#[derive(Debug, Clone)]
pub(crate) struct ProofJournal {
    pub(crate) lowering: LoweringOutput,
    pub(crate) columns: Vec<ProofColumnSlot>,
    pub(crate) precompile_calls_by_slot: Vec<Vec<ResolvedPrecompileCall>>,
    pub(crate) precompile_transcript_calls: Vec<PrecompileTranscriptCall>,
}

/// Fully reduced proof input for one planned committed column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProofColumnSlot {
    pub(crate) table: TableId,
    pub(crate) col: ColId,
    pub(crate) type_id: tabula_core::TypeId,
    pub(crate) encoding_profile_id: tabula_core::EncodingProfileId,
    pub(crate) old_entries: Vec<CommittedEntry>,
    pub(crate) init_cells: Vec<InitCell>,
    pub(crate) access_events: Vec<AccessEvent>,
    pub(crate) writes: Vec<ColumnWrite>,
    pub(crate) property_reads: Vec<PropertyReadClaim>,
}

/// Tx-local proof-relevant projection derived from one successful execution shard.
#[derive(Debug, Clone)]
pub(crate) struct TxProofProjection {
    pub(crate) tx_index: u32,
    pub(crate) lowering: TxLoweringOutput,
    pub(crate) access_events_by_slot: Vec<Vec<AccessEvent>>,
    pub(crate) property_reads_by_slot: Vec<Vec<PropertyReadClaim>>,
    pub(crate) precompile_calls_by_slot: Vec<Vec<ResolvedPrecompileCall>>,
    pub(crate) precompile_transcript_calls: Vec<PrecompileTranscriptCall>,
}

pub(super) struct TxProofProjectionContext<'a> {
    pub(super) resolved_program: &'a ResolvedProofProgram,
    pub(super) batch: &'a Batch,
    pub(super) column_profiles: &'a BTreeMap<(TableId, ColId), ColumnValueProfile>,
    pub(super) column_index: &'a ColumnPlanIndex,
    pub(super) precompile_index: &'a PrecompilePlanIndex,
    pub(super) precompile_slots: &'a [PrecompileProofSlot],
    pub(super) static_tables: &'a dyn StaticTableProvider,
    pub(super) empty_columns: &'a BTreeSet<(TableId, ColId)>,
}

pub(super) struct PreparedBatchPlanContext<'a> {
    pub(super) column_slots: &'a [ColumnProofSlot],
    pub(super) column_index: &'a ColumnPlanIndex,
    pub(super) column_profiles: &'a BTreeMap<(TableId, ColId), ColumnValueProfile>,
}
