//! Root-tier SMT witness-store assembly for the current STARK backend.

use p3_koala_bear::KoalaBear;

use tabula_commitment::{ColumnRootBinding, FieldHasher, NativeDigest};
use tabula_core::error::TabulaError;

use tabula_stark::trace::{WitnessStore, witness_labels};

use super::paths::{build_smt_paths, smt_table_public_values, validate_smt_path_shapes};

/// SMT-specific lowering context required to build the current root-tier traces.
///
/// The runtime-facing root witness surface is generic, but the built-in root
/// witness lowering still materializes SMT path witnesses here.
#[derive(Clone, Copy)]
pub struct SmtRootStoreContext<'a> {
    column_root_bindings: &'a [ColumnRootBinding],
    old_state_root: &'a NativeDigest,
    new_state_root: &'a NativeDigest,
}

impl<'a> SmtRootStoreContext<'a> {
    /// Create a new SMT root witness context.
    pub fn new(
        column_root_bindings: &'a [ColumnRootBinding],
        old_state_root: &'a NativeDigest,
        new_state_root: &'a NativeDigest,
    ) -> Self {
        Self {
            column_root_bindings,
            old_state_root,
            new_state_root,
        }
    }

    /// Column metadata for all planned columns.
    pub fn column_root_bindings(&self) -> &'a [ColumnRootBinding] {
        self.column_root_bindings
    }

    /// State root before the batch.
    pub fn old_state_root(&self) -> &'a NativeDigest {
        self.old_state_root
    }

    /// State root after the batch.
    pub fn new_state_root(&self) -> &'a NativeDigest {
        self.new_state_root
    }
}

/// Build the SMT root-tier witness store from the current batch proof context.
pub fn prepare_smt_root_store<H>(
    context: SmtRootStoreContext<'_>,
    hasher: H,
) -> Result<WitnessStore, TabulaError>
where
    H: FieldHasher<F = KoalaBear, Digest = NativeDigest> + Clone,
{
    let (smt_col_paths, smt_table_paths) = build_smt_paths(
        context.column_root_bindings(),
        context.old_state_root(),
        context.new_state_root(),
        hasher,
    )?;
    validate_smt_path_shapes(&smt_col_paths, &smt_table_paths)?;

    let smt_table_pvs = smt_table_public_values(context.old_state_root(), context.new_state_root());

    let mut store = WitnessStore::new();
    store.put(witness_labels::SMT_COL_PATHS, smt_col_paths);
    store.put(witness_labels::SMT_TABLE_PATHS, smt_table_paths);
    store.put(witness_labels::SMT_TABLE_PVS, smt_table_pvs);
    Ok(store)
}
