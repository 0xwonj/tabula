use std::collections::BTreeMap;
use std::sync::Arc;

use tabula_core::SchemeId;

#[cfg(feature = "prove")]
use crate::columns::ColumnSchemeFactory;
use crate::proof_extensions::ProofSchemeFactory;

mod smt;
mod ssmc;

pub use smt::SmtScheme;
pub use ssmc::SsmcScheme;

#[cfg(feature = "prove")]
pub(crate) fn default_factories() -> BTreeMap<SchemeId, Arc<dyn ColumnSchemeFactory>> {
    let mut schemes: BTreeMap<SchemeId, Arc<dyn ColumnSchemeFactory>> = BTreeMap::new();
    schemes.insert(SchemeId::SSMC, Arc::new(SsmcScheme::<3>));
    schemes.insert(SchemeId::SMT, Arc::new(SmtScheme::<3>));
    schemes
}

pub(crate) fn default_proof_factories() -> BTreeMap<SchemeId, Arc<dyn ProofSchemeFactory>> {
    let mut schemes: BTreeMap<SchemeId, Arc<dyn ProofSchemeFactory>> = BTreeMap::new();
    schemes.insert(SchemeId::SSMC, Arc::new(SsmcScheme::<3>));
    schemes.insert(SchemeId::SMT, Arc::new(SmtScheme::<3>));
    schemes
}
