use tabula_chips::shards::memory::MemoryShardChip;
use tabula_chips::shards::meta::MetaShardChip;
use tabula_chips::shards::state::StateShardChip;
use tabula_core::{ColId, SchemeId, TableId};
use tabula_stark::chips::ChipIdAllocator;
use tabula_stark::trace::DynChip;

use crate::SetupError;
use crate::backend::{AnyRap, ColumnChipSet, ProofColumn};

#[derive(Debug, Clone, Copy)]
pub(crate) struct TestSsmcProofColumn {
    pub(crate) table_id: TableId,
    pub(crate) col_id: ColId,
    pub(crate) receives_commitment: bool,
}

impl ProofColumn for TestSsmcProofColumn {
    fn name(&self) -> &str {
        "test-ssmc"
    }

    fn table_id(&self) -> TableId {
        self.table_id
    }

    fn col_id(&self) -> ColId {
        self.col_id
    }

    fn scheme_id(&self) -> SchemeId {
        SchemeId::SSMC
    }

    fn create_chips(&self, alloc: &mut ChipIdAllocator) -> Result<ColumnChipSet, SetupError> {
        let t = self.table_id.0;
        let c = self.col_id.0;

        let mem_id = alloc.next();
        let state_id = alloc.next();
        let meta_id = alloc.next();

        let mem = MemoryShardChip::<3>::new(mem_id, t, c);
        let state = StateShardChip::<3>::new(state_id, t, c);
        let meta = MetaShardChip::new(
            meta_id,
            t,
            c,
            self.scheme_id().raw(),
            self.receives_commitment,
        );

        let airs: Vec<Box<dyn AnyRap>> = vec![
            Box::new(mem.clone()),
            Box::new(state.clone()),
            Box::new(meta.clone()),
        ];
        let dyn_chips: Vec<Box<dyn DynChip>> = vec![Box::new(mem), Box::new(state), Box::new(meta)];

        Ok(ColumnChipSet {
            airs,
            dyn_chips,
            bus_consumers: vec![],
        })
    }
}
