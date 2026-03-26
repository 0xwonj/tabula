use std::collections::{BTreeMap, BTreeSet};

use tabula_core::error::TabulaError;

use super::{
    CapabilityDescriptor, CapabilityId, ConstId, ConstantEntry, ContextField, ContextFieldId,
    Entry, EntryKind, EventDescriptor, EventId, Op, RelationId, RelationManifestEntry,
    ReturnPolicy, TableId, TableValidationInfo, ops, unique_fields,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_entry(
    entry: &Entry,
    state: &BTreeMap<TableId, TableValidationInfo>,
    context: &BTreeMap<ContextFieldId, &ContextField>,
    consts: &BTreeMap<ConstId, &ConstantEntry>,
    relations: &BTreeMap<RelationId, &RelationManifestEntry>,
    capabilities: &BTreeMap<CapabilityId, &CapabilityDescriptor>,
    events: &BTreeMap<EventId, &EventDescriptor>,
) -> Result<(), TabulaError> {
    unique_fields(&entry.params, |param| param.id, "duplicate param ID")?;
    unique_fields(&entry.body.locals, |local| local.id, "duplicate local ID")?;

    if entry.kind == EntryKind::Tx && entry.return_policy != ReturnPolicy::Unit {
        return Err(TabulaError::InvalidIr(format!(
            "tx entry {} must use unit return policy",
            entry.symbol
        )));
    }
    if entry.kind == EntryKind::Tx && !entry.returns.is_empty() {
        return Err(TabulaError::InvalidIr(format!(
            "tx entry {} must not return values",
            entry.symbol
        )));
    }

    let params = entry
        .params
        .iter()
        .map(|param| (param.id, param.ty))
        .collect::<BTreeMap<_, _>>();
    let locals = entry
        .body
        .locals
        .iter()
        .map(|local| (local.id, local.ty))
        .collect::<BTreeMap<_, _>>();
    let mut assigned = BTreeSet::new();
    let mut seen_return = false;

    for (index, op) in entry.body.ops.iter().enumerate() {
        if matches!(op, Op::Return { .. }) {
            if seen_return {
                return Err(TabulaError::InvalidIr(format!(
                    "entry {} contains multiple Return ops",
                    entry.symbol
                )));
            }
            seen_return = true;
            if index + 1 != entry.body.ops.len() {
                return Err(TabulaError::InvalidIr(format!(
                    "entry {} has Return before end of body",
                    entry.symbol
                )));
            }
        } else if seen_return {
            return Err(TabulaError::InvalidIr(format!(
                "entry {} contains op after Return",
                entry.symbol
            )));
        }
        ops::validate_op(
            op,
            entry,
            state,
            context,
            consts,
            relations,
            capabilities,
            events,
            &params,
            &locals,
            &mut assigned,
        )?;
    }

    if !seen_return {
        return Err(TabulaError::InvalidIr(format!(
            "entry {} is missing Return",
            entry.symbol
        )));
    }
    Ok(())
}
