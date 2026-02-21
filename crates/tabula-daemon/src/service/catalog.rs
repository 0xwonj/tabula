//! Single-program registry.

use tabula_driver::RegisteredProgram;

use super::error::{ServiceError, ServiceResult};
use super::types::{ProgramId, ProgramRecord};
use crate::protocol::error::ErrorCode;

pub const SINGLE_PROGRAM_ID: &str = "pgm_default";

#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub record: ProgramRecord,
    pub registered: RegisteredProgram,
}

/// Single-program registry.
#[derive(Debug, Default)]
pub struct ProgramCatalog {
    entry: Option<CatalogEntry>,
}

impl ProgramCatalog {
    /// Replace (or set) the single program entry.
    ///
    /// Allows re-registration for the deploy-once / re-deploy flow.
    pub fn replace_single(&mut self, entry: CatalogEntry) -> ProgramId {
        self.entry = Some(entry);
        SINGLE_PROGRAM_ID.to_string()
    }

    pub fn get(&self, program_id: &str) -> ServiceResult<CatalogEntry> {
        if program_id != SINGLE_PROGRAM_ID {
            return Err(ServiceError::not_found(
                ErrorCode::ProgramNotFound,
                format!("program not found: {program_id}"),
            ));
        }
        self.entry.clone().ok_or_else(|| {
            ServiceError::not_found(
                ErrorCode::ProgramNotFound,
                format!("program not found: {program_id}"),
            )
        })
    }

    pub fn list_records(&self) -> Vec<ProgramRecord> {
        match self.entry.as_ref() {
            Some(entry) => vec![entry.record.clone()],
            None => vec![],
        }
    }
}
