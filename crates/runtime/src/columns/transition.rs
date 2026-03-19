#[cfg(feature = "prove")]
use p3_koala_bear::KoalaBear;
#[cfg(feature = "prove")]
use tabula_commitment::{ColumnMeta, NativeDigest};
use tabula_core::ValueType;
#[cfg(feature = "prove")]
use tabula_core::error::TabulaError;
use tabula_core::{ColId, RowKey, SchemeId, TableId};
#[cfg(feature = "prove")]
use tabula_machine::{ColumnIdentity, PublicStatement};
#[cfg(feature = "prove")]
use tabula_stark::trace::WitnessStore;
#[cfg(feature = "prove")]
use tabula_witness::trace::builtin::PropertyReadRecord;
#[cfg(feature = "prove")]
use tabula_witness::{AccessRow, InitRow};

/// Final scheme-owned per-column proving seam used by the runtime.
#[cfg(feature = "prove")]
pub trait ColumnTransitionBackend: Send + Sync {
    /// Human-readable scheme name.
    fn name(&self) -> &str;

    /// Table identifier of this committed column.
    fn table_id(&self) -> TableId;

    /// Column identifier of this committed column.
    fn col_id(&self) -> ColId;

    /// Portable scheme identifier.
    fn scheme_id(&self) -> SchemeId;

    /// Build the final per-column proving input from shared execution rows and
    /// scheme-owned state transition logic.
    fn build_proof_input(
        &self,
        input: ColumnTransitionInput,
        property_reads: &[PropertyReadRecord],
    ) -> Result<ColumnProofInput, TabulaError>;
}

/// Shared execution-derived inputs for one planned column transition.
#[cfg(feature = "prove")]
#[derive(Debug, Clone)]
pub struct ColumnTransitionInput {
    /// Table identifier of the column being proved.
    pub table: TableId,
    /// Column identifier of the column being proved.
    pub col: ColId,
    /// Value type declared in the schema for this column.
    pub value_type: ValueType,
    /// Fully encoded pre-batch column entries, sorted by row key.
    pub old_entries: Vec<(RowKey, Vec<KoalaBear>)>,
    /// Shared init rows derived from the executor read-set.
    pub init_rows: Vec<InitRow>,
    /// Shared access rows derived from successful execution events.
    pub access_rows: Vec<AccessRow>,
    /// Final coalesced writes for this column.
    pub writes: Vec<(RowKey, Option<Vec<KoalaBear>>)>,
    /// Whether the column was touched by this batch.
    pub is_touched: bool,
}

/// Canonical per-column proving input produced by one transition backend.
#[cfg(feature = "prove")]
pub struct ColumnProofInput {
    /// Table identifier of the proved column.
    pub table: TableId,
    /// Column identifier of the proved column.
    pub col: ColId,
    /// Root-binding column metadata produced by the transition backend.
    pub meta: ColumnMeta,
    /// Column-tier witness store consumed by this scheme's proof chips.
    pub witness_store: WitnessStore,
}

/// Canonical batch-level proving input for the native column scheme platform.
#[cfg(feature = "prove")]
pub struct BatchProofInput {
    /// Canonical per-column proof inputs for all planned columns.
    pub columns: Vec<ColumnProofInput>,
    /// State root before the batch.
    pub old_state_root: NativeDigest,
    /// State root after the batch.
    pub new_state_root: NativeDigest,
}

#[cfg(feature = "prove")]
impl BatchProofInput {
    pub(crate) fn column_metas(&self) -> Vec<ColumnMeta> {
        self.columns
            .iter()
            .map(|column| column.meta.clone())
            .collect()
    }

    pub(crate) fn column_identities(&self) -> Vec<ColumnIdentity> {
        self.columns
            .iter()
            .map(|column| ColumnIdentity {
                table_id: column.table.0,
                col_id: column.col.0,
                com_old: column.meta.com_old.0,
                com_new: column.meta.com_new.0,
            })
            .collect()
    }

    pub(crate) fn public_statement(&self) -> PublicStatement {
        PublicStatement {
            old_root: self.old_state_root,
            new_root: self.new_state_root,
        }
    }
}
