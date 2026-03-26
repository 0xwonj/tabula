mod body;
mod entry;
mod ids;
mod manifest;
mod ops;
mod requests;
mod schema;
mod validated;

pub use body::*;
pub use entry::*;
pub use ids::*;
pub use manifest::*;
pub use ops::*;
pub use requests::*;
pub use schema::*;
pub use validated::*;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Program {
    pub program_id: ProgramId,
    pub state: StateSchema,
    pub context: ContextSchema,
    pub const_pool: ConstantPool,
    pub relation_manifest: RelationManifest,
    pub capability_manifest: CapabilityManifest,
    pub event_manifest: EventManifest,
    pub entries: Vec<Entry>,
}
