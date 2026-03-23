use std::collections::{BTreeMap, BTreeSet};

use tabula_compiler::SealedProgram;

/// Collect exact structural property requirements grouped by column slot.
pub(crate) fn required_property_queries_by_column(
    compiled_program: &SealedProgram,
) -> BTreeMap<(tabula_core::TableId, tabula_core::ColId), BTreeSet<tabula_ir::PropertyQueryKind>> {
    let mut required_property_query_kinds: BTreeMap<_, BTreeSet<_>> = BTreeMap::new();
    for requirement in compiled_program.required_property_requirements() {
        required_property_query_kinds
            .entry((requirement.table_id, requirement.col_id))
            .or_default()
            .insert(requirement.query_kind);
    }
    required_property_query_kinds
}
