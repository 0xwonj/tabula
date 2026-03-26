use tabula_core::error::TabulaError;

use crate::model::{
    CapabilityDescriptor, CapabilityId, CapabilityQueryPolicy, ConstId, ConstantEntry,
    ContextField, ContextFieldId, Entry, EntryKind, EventDescriptor, EventId, FieldId, GuardRef,
    LocalId, Op, ParamId, Program, RelationBinding, RelationId, RelationManifestEntry,
    ReturnPolicy, StatePropertyQuery, StateSchema, TableId, TypeRef, ValueRef, ValueTupleRef,
};

mod entry;
mod helpers;
mod manifest;
mod ops;
mod state;
mod types;
mod values;

use entry::validate_entry;
use helpers::unique_fields;
use manifest::validate_relation_entry;
use state::{TableValidationInfo, validate_state};

pub fn validate_program(program: &Program) -> Result<(), TabulaError> {
    let state = validate_state(&program.state)?;
    let context = unique_fields(
        &program.context.fields,
        |field| field.id,
        "duplicate context field ID",
    )?;
    let consts = unique_fields(
        &program.const_pool.entries,
        |entry| entry.id,
        "duplicate const ID",
    )?;
    for entry in &program.const_pool.entries {
        types::ensure_type(entry.value.type_id(), entry.ty, "const entry type mismatch")?;
    }
    let relations = unique_fields(
        &program.relation_manifest.entries,
        |entry| entry.id,
        "duplicate relation ID",
    )?;
    for entry in &program.relation_manifest.entries {
        validate_relation_entry(entry)?;
    }
    let capabilities = unique_fields(
        &program.capability_manifest.entries,
        |entry| entry.id,
        "duplicate capability ID",
    )?;
    let events = unique_fields(
        &program.event_manifest.entries,
        |entry| entry.id,
        "duplicate event ID",
    )?;
    let entry_ids = unique_fields(&program.entries, |entry| entry.id, "duplicate entry ID")?;
    let _ = entry_ids;
    for entry in &program.entries {
        validate_entry(
            entry,
            &state,
            &context,
            &consts,
            &relations,
            &capabilities,
            &events,
        )?;
    }
    Ok(())
}
