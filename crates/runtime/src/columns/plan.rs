use std::collections::BTreeSet;

use tabula_core::{ColId, SchemeId, TableId, ValueType};
use tabula_ir::PropertyQueryKind;

/// Compiler/runtime-owned plan for one committed column.
#[derive(Debug, Clone)]
pub struct ColumnPlan {
    /// Table identifier.
    pub table_id: TableId,
    /// Column identifier.
    pub col_id: ColId,
    /// Portable scheme identifier selected by the compiler.
    pub scheme_id: SchemeId,
    /// Column value type from the sealed schema surface.
    pub value_type: ValueType,
    /// Whether this column participates in the root commitment.
    pub receives_commitment: bool,
    /// Exact structural property kinds required for this column by the program.
    pub required_property_query_kinds: BTreeSet<PropertyQueryKind>,
}

impl ColumnPlan {
    /// Whether this column needs any scheme-backed property support.
    pub fn requires_property_support(&self) -> bool {
        !self.required_property_query_kinds.is_empty()
    }
}
