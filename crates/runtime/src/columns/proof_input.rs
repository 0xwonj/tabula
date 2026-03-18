#[cfg(feature = "prove")]
use tabula_commitment::PoseidonHasher;
#[cfg(feature = "prove")]
use tabula_core::error::TabulaError;
use tabula_core::{ColId, SchemeId, TableId};
#[cfg(feature = "prove")]
use tabula_stark::trace::WitnessStore;
#[cfg(feature = "prove")]
use tabula_witness::ColumnWitness;
#[cfg(feature = "prove")]
use tabula_witness::trace::builtin::PropertyReadRecord;

/// Runtime-owned per-column proof-input assembler.
pub trait ProofInputBuilder: Send + Sync {
    /// Human-readable scheme name.
    fn name(&self) -> &str;

    /// Table identifier of this committed column.
    fn table_id(&self) -> TableId;

    /// Column identifier of this committed column.
    fn col_id(&self) -> ColId;

    /// Portable scheme identifier.
    fn scheme_id(&self) -> SchemeId;

    /// Build the per-column witness store required by this scheme's proof chips.
    #[cfg(feature = "prove")]
    fn build_witness_store(
        &self,
        _column: &ColumnWitness<PoseidonHasher>,
        _property_reads: &[PropertyReadRecord],
    ) -> Result<WitnessStore, TabulaError> {
        Ok(WitnessStore::new())
    }
}
