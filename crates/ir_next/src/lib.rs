//! Next-generation canonical IR for the program-first rewrite.

mod model;
mod validate;

pub use model::{
    AggregateKind, ArithOp, Body, CapabilityDescriptor, CapabilityId, CapabilityManifest,
    CapabilityProofVisibility, CapabilityQueryPolicy, CapabilityTotality, CmpOp, ConstId,
    ConstantEntry, ConstantPool, ContextField, ContextFieldId, ContextSchema, Entry, EntryId,
    EntryKind, EventDescriptor, EventId, EventManifest, FieldId, FieldSchema, GuardRef, HashFamily,
    LocalDecl, LocalId, Op, ParamDecl, ParamId, Program, ProgramId, RelationBinding,
    RelationDescriptor, RelationId, RelationManifest, RelationManifestEntry, RelationRow,
    ReturnPolicy, StatePropertyQuery, StateSchema, TableId, TableSchema, TypeRef, ValidatedProgram,
    ValueRef, ValueTupleRef,
};
pub use validate::validate_program;

use tabula_core::error::TabulaError;

impl TryFrom<Program> for ValidatedProgram {
    type Error = TabulaError;

    fn try_from(program: Program) -> Result<Self, Self::Error> {
        validate_program(&program)?;
        Ok(ValidatedProgram(program))
    }
}
