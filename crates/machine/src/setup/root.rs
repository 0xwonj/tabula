//! Root-tier proof composition.

use tabula_chips::smt_path::{SmtColPathChip, SmtTablePathChip};
use tabula_core::RootProfileId;
use tabula_stark::air::interaction::BusId;
use tabula_stark::trace::DynChip;

use crate::backend::AnyRap;

/// How column commitments are aggregated into a state root.
pub trait RootProof: Send + Sync {
    /// Compatibility profile identifier exposed to artifact-bound column schemes.
    fn profile_id(&self) -> RootProfileId {
        RootProfileId::SMT_V1
    }

    /// Produce the AIR(s) that implement this root proof (for proving/verifying).
    fn airs(&self) -> Vec<Box<dyn AnyRap>>;

    /// Produce the chip(s) that implement this root proof (for trace building and debug validation).
    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>>;

    /// Buses this root proof's chips interact with.
    fn buses(&self) -> Vec<BusId> {
        vec![]
    }
}

/// SMT root proof (standard per-tier architecture).
#[derive(Debug)]
pub struct SmtRootProof;

impl RootProof for SmtRootProof {
    fn profile_id(&self) -> RootProfileId {
        RootProfileId::SMT_V1
    }

    fn airs(&self) -> Vec<Box<dyn AnyRap>> {
        vec![Box::new(SmtColPathChip), Box::new(SmtTablePathChip)]
    }

    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>> {
        vec![Box::new(SmtColPathChip), Box::new(SmtTablePathChip)]
    }
}
