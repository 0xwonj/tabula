use std::collections::BTreeSet;

use tabula_core::error::TabulaError;

use super::{RelationBinding, RelationManifestEntry, types};

pub(super) fn validate_relation_entry(entry: &RelationManifestEntry) -> Result<(), TabulaError> {
    match &entry.binding {
        RelationBinding::EnumSet { values } => {
            if entry.descriptor.inputs.len() != 1 || !entry.descriptor.outputs.is_empty() {
                return Err(TabulaError::InvalidIr(format!(
                    "enum relation {} must have exactly one input and no outputs",
                    entry.descriptor.symbol
                )));
            }
            for value in values {
                types::ensure_type(
                    value.type_id(),
                    entry.descriptor.inputs[0],
                    "enum relation value type mismatch",
                )?;
            }
        }
        RelationBinding::Map { rows } => {
            let mut seen_inputs = BTreeSet::new();
            for row in rows {
                if row.inputs.len() != entry.descriptor.inputs.len()
                    || row.outputs.len() != entry.descriptor.outputs.len()
                {
                    return Err(TabulaError::InvalidIr(format!(
                        "map relation {} row arity mismatch",
                        entry.descriptor.symbol
                    )));
                }
                for (value, expected) in row.inputs.iter().zip(&entry.descriptor.inputs) {
                    types::ensure_type(value.type_id(), *expected, "relation input type mismatch")?;
                }
                for (value, expected) in row.outputs.iter().zip(&entry.descriptor.outputs) {
                    types::ensure_type(
                        value.type_id(),
                        *expected,
                        "relation output type mismatch",
                    )?;
                }
                let input_fingerprint = row
                    .inputs
                    .iter()
                    .map(|value| (value.type_id().0, value.payload().to_vec()))
                    .collect::<Vec<_>>();
                if !seen_inputs.insert(input_fingerprint) {
                    return Err(TabulaError::InvalidIr(format!(
                        "map relation {} contains duplicate input tuple",
                        entry.descriptor.symbol
                    )));
                }
            }
        }
    }
    Ok(())
}
