use std::collections::BTreeMap;
use std::sync::Arc;

use tabula_core::TypeId;
use tabula_core::error::TabulaError;
use tabula_ir as ir;
use tabula_types::TypedValue;

#[derive(Debug, Clone)]
pub(crate) struct ResolvedEntry {
    pub(crate) definition: ir::Entry,
    params_by_id: BTreeMap<ir::ParamId, usize>,
    local_slots: BTreeMap<ir::LocalId, usize>,
    local_tys: Vec<TypeId>,
}

impl ResolvedEntry {
    pub(crate) fn param_value(
        &self,
        id: ir::ParamId,
        params: &[TypedValue],
    ) -> Result<TypedValue, TabulaError> {
        let index = self.params_by_id.get(&id).ok_or_else(|| {
            TabulaError::InvalidIr(format!(
                "entry {} references unknown param {}",
                self.definition.symbol, id.0
            ))
        })?;
        Ok(params[*index].clone())
    }

    pub(crate) fn local_slot(&self, id: ir::LocalId) -> Result<usize, TabulaError> {
        self.local_slots.get(&id).copied().ok_or_else(|| {
            TabulaError::InvalidIr(format!(
                "entry {} references unknown local {}",
                self.definition.symbol, id.0
            ))
        })
    }

    pub(crate) fn local_type(&self, id: ir::LocalId) -> Result<TypeId, TabulaError> {
        let slot = self.local_slot(id)?;
        self.local_tys.get(slot).copied().ok_or_else(|| {
            TabulaError::InvalidIr(format!(
                "entry {} references unknown local {}",
                self.definition.symbol, id.0
            ))
        })
    }

    pub(crate) fn local_count(&self) -> usize {
        self.local_tys.len()
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedTable {
    pub schema: ir::TableSchema,
    pub(crate) fields: BTreeMap<ir::FieldId, ir::FieldSchema>,
}

#[derive(Debug, Clone)]
pub struct ResolvedExecutionProgram {
    program: Arc<ir::ValidatedProgram>,
    context_fields: BTreeMap<ir::ContextFieldId, ir::ContextField>,
    consts: BTreeMap<ir::ConstId, ir::ConstantEntry>,
    entries: BTreeMap<ir::EntryId, ResolvedEntry>,
    tables: BTreeMap<ir::TableId, ResolvedTable>,
    relations: BTreeMap<ir::RelationId, ir::RelationManifestEntry>,
    capabilities: BTreeMap<ir::CapabilityId, ir::CapabilityDescriptor>,
    events: BTreeMap<ir::EventId, ir::EventDescriptor>,
}

impl ResolvedExecutionProgram {
    pub fn from_validated_program(program: ir::ValidatedProgram) -> Result<Self, TabulaError> {
        Self::from_shared_program(Arc::new(program))
    }

    pub fn from_shared_program(program: Arc<ir::ValidatedProgram>) -> Result<Self, TabulaError> {
        let raw = program.as_program();
        let context_fields = raw
            .context
            .fields
            .iter()
            .cloned()
            .map(|field| (field.id, field))
            .collect();
        let consts = raw
            .const_pool
            .entries
            .iter()
            .cloned()
            .map(|entry| (entry.id, entry))
            .collect();
        let entries = raw
            .entries
            .iter()
            .map(|entry| {
                let params_by_id = entry
                    .params
                    .iter()
                    .enumerate()
                    .map(|(index, param)| (param.id, index))
                    .collect();
                let local_slots = entry
                    .body
                    .locals
                    .iter()
                    .enumerate()
                    .map(|(slot, local)| (local.id, slot))
                    .collect();
                let local_tys = entry.body.locals.iter().map(|local| local.ty).collect();
                (
                    entry.id,
                    ResolvedEntry {
                        definition: entry.clone(),
                        params_by_id,
                        local_slots,
                        local_tys,
                    },
                )
            })
            .collect();
        let tables = raw
            .state
            .tables
            .iter()
            .cloned()
            .map(|schema| {
                let fields = schema
                    .fields
                    .iter()
                    .cloned()
                    .map(|field| (field.id, field))
                    .collect();
                (schema.id, ResolvedTable { schema, fields })
            })
            .collect();
        let relations = raw
            .relation_manifest
            .entries
            .iter()
            .cloned()
            .map(|entry| (entry.id, entry))
            .collect();
        let capabilities = raw
            .capability_manifest
            .entries
            .iter()
            .cloned()
            .map(|entry| (entry.id, entry))
            .collect();
        let events = raw
            .event_manifest
            .entries
            .iter()
            .cloned()
            .map(|entry| (entry.id, entry))
            .collect();
        Ok(Self {
            program,
            context_fields,
            consts,
            entries,
            tables,
            relations,
            capabilities,
            events,
        })
    }

    pub fn validated_program(&self) -> &ir::ValidatedProgram {
        self.program.as_ref()
    }

    pub fn program(&self) -> &ir::Program {
        self.program.as_program()
    }

    pub(crate) fn entry(&self, id: ir::EntryId) -> Result<&ResolvedEntry, TabulaError> {
        self.entries
            .get(&id)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown entry ID {}", id.0)))
    }

    pub fn entry_definition(&self, id: ir::EntryId) -> Result<&ir::Entry, TabulaError> {
        Ok(&self.entry(id)?.definition)
    }

    pub fn table(&self, id: ir::TableId) -> Result<&ResolvedTable, TabulaError> {
        self.tables
            .get(&id)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown table {}", id.0)))
    }

    pub fn field_type(
        &self,
        table: ir::TableId,
        field: ir::FieldId,
    ) -> Result<TypeId, TabulaError> {
        self.table(table)?
            .fields
            .get(&field)
            .map(|field| field.ty)
            .ok_or_else(|| {
                TabulaError::InvalidIr(format!("unknown table/field {}.{}", table.0, field.0))
            })
    }

    pub(crate) fn relation(
        &self,
        id: ir::RelationId,
    ) -> Result<&ir::RelationManifestEntry, TabulaError> {
        self.relations
            .get(&id)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown relation {}", id.0)))
    }

    pub(crate) fn capability(
        &self,
        id: ir::CapabilityId,
    ) -> Result<&ir::CapabilityDescriptor, TabulaError> {
        self.capabilities
            .get(&id)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown capability {}", id.0)))
    }

    pub(crate) fn event(&self, id: ir::EventId) -> Result<&ir::EventDescriptor, TabulaError> {
        self.events
            .get(&id)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown event {}", id.0)))
    }

    pub(crate) fn const_entry(&self, id: ir::ConstId) -> Result<&ir::ConstantEntry, TabulaError> {
        self.consts
            .get(&id)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown const {}", id.0)))
    }

    pub fn context_field(&self, id: ir::ContextFieldId) -> Result<&ir::ContextField, TabulaError> {
        self.context_fields
            .get(&id)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown context field {}", id.0)))
    }
}
