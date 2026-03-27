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

/// The complete canonical IR for one deployed Tabula program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Program {
    /// Unique program identifier.
    pub program_id: ProgramId,
    /// State schema (all tables and their column types).
    pub state: StateSchema,
    /// Public context schema (caller-supplied fields).
    pub context: ContextSchema,
    /// Compile-time constant pool.
    pub const_pool: ConstantPool,
    /// Static relation table manifest.
    pub relation_manifest: RelationManifest,
    /// Native capability manifest.
    pub capability_manifest: CapabilityManifest,
    /// Event manifest.
    pub event_manifest: EventManifest,
    /// Callable entries (functions, queries, transactions).
    pub entries: Vec<Entry>,
}
