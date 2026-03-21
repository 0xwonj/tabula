use std::sync::Arc;

use tabula_core::{ColId, SchemeId, TableId};
use tabula_machine::SetupError;
use tabula_machine::backend::{ColumnChipSet, ProofColumn};
use tabula_stark::chips::ChipIdAllocator;

struct DummyProofColumn {
    table_id: TableId,
    col_id: ColId,
}

impl ProofColumn for DummyProofColumn {
    fn name(&self) -> &str {
        "dummy"
    }

    fn table_id(&self) -> TableId {
        self.table_id
    }

    fn col_id(&self) -> ColId {
        self.col_id
    }

    fn scheme_id(&self) -> SchemeId {
        SchemeId(0x1000)
    }

    fn create_chips(&self, _alloc: &mut ChipIdAllocator) -> Result<ColumnChipSet, SetupError> {
        Ok(ColumnChipSet {
            airs: vec![],
            dyn_chips: vec![],
            bus_consumers: vec![],
        })
    }
}

pub fn dummy_proof_column(table: u32, col: u16) -> Arc<dyn ProofColumn> {
    Arc::new(DummyProofColumn {
        table_id: TableId(table),
        col_id: ColId(col),
    })
}
