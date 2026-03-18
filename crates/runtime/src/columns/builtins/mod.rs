use std::collections::BTreeMap;
use std::sync::Arc;

use tabula_core::SchemeId;

use crate::columns::ColumnSchemeFactory;

mod smt;
mod ssmc;

pub use smt::SmtScheme;
pub use ssmc::SsmcScheme;

pub(crate) fn default_factories() -> BTreeMap<SchemeId, Arc<dyn ColumnSchemeFactory>> {
    let mut schemes: BTreeMap<SchemeId, Arc<dyn ColumnSchemeFactory>> = BTreeMap::new();
    schemes.insert(SchemeId::SSMC, Arc::new(SsmcScheme::<3>));
    schemes.insert(SchemeId::SMT, Arc::new(SmtScheme::<3>));
    schemes
}
