use std::collections::{BTreeMap, BTreeSet};

use tabula_core::{
    CommittedKeyLayout, KeyOrderingFamily, MachineCapabilities, ProgramExecutionContract,
    ProgramMachineShape, PropertyQueryKind, StateColumnContract, StateContract, StateTableContract,
    TableKeyContract,
};
use tabula_ir as ir;
use tabula_profile::{CommitmentRole, ProfileCatalog, SemanticRegistry, TypeDescriptor};

use crate::pipeline::StateFieldSchemeBinding;
use crate::registration::profiles::{
    DEFAULT_COLUMN_SCHEME_ID, register_reused_profile_definitions,
};

fn property_query_kind(query: &ir::StatePropertyQuery) -> PropertyQueryKind {
    match query {
        ir::StatePropertyQuery::Minimum => PropertyQueryKind::Minimum,
        ir::StatePropertyQuery::Maximum => PropertyQueryKind::Maximum,
        ir::StatePropertyQuery::Successor { .. } => PropertyQueryKind::Successor,
        ir::StatePropertyQuery::Predecessor { .. } => PropertyQueryKind::Predecessor,
        ir::StatePropertyQuery::NonExistenceRange { .. } => PropertyQueryKind::NonExistenceRange,
        ir::StatePropertyQuery::Aggregate { .. } => PropertyQueryKind::Aggregate,
    }
}

fn collect_required_property_queries(
    program: &ir::Program,
) -> BTreeMap<(ir::TableId, ir::FieldId), BTreeSet<PropertyQueryKind>> {
    let mut required = BTreeMap::new();
    for entry in &program.entries {
        for op in &entry.body.ops {
            if let ir::Op::ReadStateProperty {
                table,
                field,
                query,
                ..
            } = op
            {
                required
                    .entry((*table, *field))
                    .or_insert_with(BTreeSet::new)
                    .insert(property_query_kind(query));
            }
        }
    }
    required
}

fn requires_ordered_queries(required: &BTreeSet<PropertyQueryKind>) -> bool {
    required.iter().any(|kind| {
        matches!(
            kind,
            PropertyQueryKind::Minimum
                | PropertyQueryKind::Maximum
                | PropertyQueryKind::Successor
                | PropertyQueryKind::Predecessor
                | PropertyQueryKind::NonExistenceRange
        )
    })
}

fn compute_program_machine_shape(
    program: &ir::Program,
    state_tables: &[StateTableContract],
) -> Result<ProgramMachineShape, String> {
    let max_slots = program
        .entries
        .iter()
        .map(|entry| {
            entry
                .body
                .locals
                .iter()
                .map(|local| local.id.0 as usize)
                .max()
                .map_or(0, |slot| slot + 1)
        })
        .max()
        .unwrap_or(0);
    let max_key_components = state_tables
        .iter()
        .map(|table| table.key.components.len())
        .max()
        .unwrap_or(0);
    let max_key_fes = state_tables
        .iter()
        .map(|table| usize::from(table.key.committed_layout.fe_width))
        .max()
        .unwrap_or(0);

    Ok(ProgramMachineShape {
        max_slots: u16::try_from(max_slots).map_err(|_| {
            format!("program requires {max_slots} slots, exceeds u16 machine shape")
        })?,
        max_key_components: u16::try_from(max_key_components).map_err(|_| {
            format!(
                "program requires {max_key_components} key components, exceeds u16 machine shape"
            )
        })?,
        max_key_fes: u16::try_from(max_key_fes).map_err(|_| {
            format!("program requires {max_key_fes} key field elements, exceeds u16 machine shape")
        })?,
    })
}

fn validate_machine_shape(
    shape: ProgramMachineShape,
    capabilities: MachineCapabilities,
) -> Result<(), String> {
    if capabilities.supports(shape) {
        return Ok(());
    }
    Err(format!(
        "program machine shape {shape:?} exceeds native machine capabilities {capabilities:?}"
    ))
}

fn resolve_key_encoding(
    registry: &SemanticRegistry,
    table_symbol: &str,
    component: &tabula_core::KeyComponentSchema,
) -> Result<(tabula_profile::EncodingProfile, TypeDescriptor), String> {
    let type_descriptor = registry
        .catalog()
        .type_descriptor(component.ty)
        .cloned()
        .map_err(|err| {
            format!(
                "state table {table_symbol} key component {} references unknown type id {}: {err}",
                component.symbol, component.ty.0
            )
        })?;
    let encoding_profile_id = registry
        .resolve_default_key_encoding(component.ty)
        .map_err(|err| {
            format!(
                "state table {table_symbol} key component {} type {} has no default key encoding: {err}",
                component.symbol, component.ty.0
            )
        })?;
    let encoding_profile = registry
        .catalog()
        .encoding_profile(encoding_profile_id)
        .cloned()
        .map_err(|err| {
            format!(
                "state table {table_symbol} key encoding {} is missing from the catalog: {err}",
                encoding_profile_id.0
            )
        })?;
    if encoding_profile.type_id != component.ty {
        return Err(format!(
            "state table {table_symbol} key component {} type {} resolved key encoding {} for type {}",
            component.symbol, component.ty.0, encoding_profile_id.0, encoding_profile.type_id.0
        ));
    }
    if !encoding_profile.key_eligible {
        return Err(format!(
            "state table {table_symbol} key component {} uses encoding {} that is not key-eligible",
            component.symbol, encoding_profile.encoding_profile_id.0
        ));
    }
    if encoding_profile.fixed_byte_width.is_none() {
        return Err(format!(
            "state table {table_symbol} key component {} uses encoding {} without a fixed committed byte width",
            component.symbol, encoding_profile.encoding_profile_id.0
        ));
    }
    Ok((encoding_profile, type_descriptor))
}

pub(crate) fn seal_execution_contract(
    program: &ir::Program,
    field_schemes: &[StateFieldSchemeBinding],
    registry: &SemanticRegistry,
    machine_capabilities: MachineCapabilities,
) -> Result<(ProgramExecutionContract, ProfileCatalog), String> {
    let required_queries = collect_required_property_queries(program);
    let mut scheme_by_key = field_schemes
        .iter()
        .map(|binding| ((binding.table, binding.field), binding.scheme_id))
        .collect::<BTreeMap<_, _>>();
    let catalog = registry.catalog();
    let mut sealed_catalog = ProfileCatalog::new();
    let mut next_column_profile_id = 0u32;
    let mut state_tables = Vec::with_capacity(program.state.tables.len());

    for table in &program.state.tables {
        let mut component_encoding_profile_ids = Vec::with_capacity(table.keys.len());
        let mut key_width_bytes = 0u32;
        let mut key_width_fes = 0u32;
        let mut requires_ordering = false;

        for field in &table.fields {
            let required = required_queries
                .get(&(table.id, field.id))
                .cloned()
                .unwrap_or_default();
            requires_ordering |= requires_ordered_queries(&required);
        }

        for component in &table.keys {
            let (encoding_profile, type_descriptor) =
                resolve_key_encoding(registry, &table.symbol, component)?;
            if requires_ordering {
                if !type_descriptor.capabilities.ordering {
                    return Err(format!(
                        "state table {} requires ordered property queries, but key component {} type {} is not orderable",
                        table.symbol, component.symbol, component.ty.0
                    ));
                }
                if !encoding_profile.ordering_preserving {
                    return Err(format!(
                        "state table {} requires ordered property queries, but key encoding {} for component {} is not ordering-preserving",
                        table.symbol, encoding_profile.encoding_profile_id.0, component.symbol
                    ));
                }
            }
            component_encoding_profile_ids.push(encoding_profile.encoding_profile_id);
            key_width_bytes += u32::from(encoding_profile.fixed_byte_width.unwrap_or(0));
            key_width_fes += u32::from(encoding_profile.width);
            if !sealed_catalog
                .types
                .iter()
                .any(|descriptor| descriptor.type_id == type_descriptor.type_id)
            {
                sealed_catalog
                    .register_type(type_descriptor)
                    .map_err(|err| err.to_string())?;
            }
            if !sealed_catalog
                .encodings
                .iter()
                .any(|profile| profile.encoding_profile_id == encoding_profile.encoding_profile_id)
            {
                sealed_catalog
                    .register_encoding(encoding_profile)
                    .map_err(|err| err.to_string())?;
            }
        }

        let key = TableKeyContract {
            components: table.keys.clone(),
            component_encoding_profile_ids,
            ordering_family: KeyOrderingFamily::LexicographicByComponent,
            committed_layout: CommittedKeyLayout {
                byte_width: u16::try_from(key_width_bytes).map_err(|_| {
                    format!(
                        "state table {} committed-key width {} bytes exceeds u16",
                        table.symbol, key_width_bytes
                    )
                })?,
                fe_width: u16::try_from(key_width_fes).map_err(|_| {
                    format!(
                        "state table {} committed-key width {} field elements exceeds u16",
                        table.symbol, key_width_fes
                    )
                })?,
            },
        };

        let mut columns = Vec::with_capacity(table.fields.len());
        for field in &table.fields {
            let type_descriptor = catalog.type_descriptor(field.ty).cloned().map_err(|_| {
                format!(
                    "state field {}.{} references unknown type id {}",
                    table.symbol, field.symbol, field.ty.0
                )
            })?;
            let encoding_profile_id = registry
                .resolve_default_encoding(field.ty)
                .map_err(|err| err.to_string())?;
            let encoding_profile = catalog
                .encoding_profile(encoding_profile_id)
                .cloned()
                .map_err(|_| {
                    format!(
                        "type id {} resolved default encoding {} that is missing from the registry catalog",
                        field.ty.0, encoding_profile_id.0
                    )
                })?;
            let scheme_family_id = scheme_by_key
                .remove(&(table.id, field.id))
                .unwrap_or(DEFAULT_COLUMN_SCHEME_ID);
            let scheme_profile_id = registry
                .resolve_default_scheme_profile(scheme_family_id, encoding_profile_id)
                .map_err(|err| err.to_string())?;
            let scheme_profile =
                catalog
                    .scheme_profile(scheme_profile_id)
                    .cloned()
                    .map_err(|_| {
                        format!(
                            "scheme family {} + encoding {} resolved missing scheme profile {}",
                            scheme_family_id.0, encoding_profile_id.0, scheme_profile_id.0
                        )
                    })?;

            register_reused_profile_definitions(
                &mut sealed_catalog,
                &type_descriptor,
                &encoding_profile,
                &scheme_profile,
            )
            .map_err(|err| err.to_string())?;

            let column_profile = tabula_profile::ColumnProfile::new(
                tabula_core::ColumnProfileId(next_column_profile_id),
                format!("{}.{}", table.symbol, field.symbol),
                None,
                &type_descriptor,
                &encoding_profile,
                &scheme_profile,
                CommitmentRole::IncludedInRoot,
            )
            .map_err(|err| err.to_string())?;
            let column_profile_id = column_profile.column_profile_id;
            next_column_profile_id += 1;
            sealed_catalog
                .register_column(column_profile)
                .map_err(|err| err.to_string())?;

            let required_property_queries = required_queries
                .get(&(table.id, field.id))
                .cloned()
                .unwrap_or_default();
            let resolved = sealed_catalog
                .resolve_column_profile(column_profile_id)
                .map_err(|err| {
                    format!(
                        "failed to resolve field profile for property-read validation on {}.{}: {err}",
                        table.id.0, field.id.0
                    )
                })?;
            for kind in &required_property_queries {
                if !resolved.supports_property_query(*kind) {
                    return Err(format!(
                        "field {}.{} uses property query {:?}, but scheme profile {} does not support it",
                        table.id.0, field.id.0, kind, resolved.scheme_profile.scheme_profile_id.0
                    ));
                }
            }

            columns.push(StateColumnContract {
                id: field.id.into(),
                name: field.symbol.clone(),
                ty: field.ty,
                column_profile_id,
                required_property_queries,
            });
        }

        state_tables.push(StateTableContract {
            id: table.id.into(),
            name: table.symbol.clone(),
            key,
            columns,
        });
    }

    if let Some(((table, field), _)) = scheme_by_key.first_key_value() {
        return Err(format!(
            "field scheme selection references unknown table {} field {}",
            table.0, field.0
        ));
    }

    state_tables.sort_by_key(|table| table.id);
    for table in &mut state_tables {
        table.columns.sort_by_key(|column| column.id);
    }

    let machine_shape = compute_program_machine_shape(program, &state_tables)?;
    validate_machine_shape(machine_shape, machine_capabilities)?;

    sealed_catalog.validate().map_err(|err| err.to_string())?;
    Ok((
        ProgramExecutionContract {
            state: StateContract {
                tables: state_tables,
            },
            machine_shape,
        },
        sealed_catalog,
    ))
}

#[cfg(test)]
mod tests {
    use super::compute_program_machine_shape;
    use tabula_core::ProgramMachineShape;
    use tabula_ir::{
        Body, ConstantPool, ContextSchema, Entry, EntryId, EntryKind, EventManifest, LocalDecl,
        LocalId, Op, Program, ProgramId, RelationManifest, ReturnPolicy, StateSchema,
        ValueTupleRef,
    };
    use tabula_profile::TYPE_U64_ID;

    fn program_with_sparse_local_ids() -> Program {
        Program {
            program_id: ProgramId(7),
            state: StateSchema { tables: vec![] },
            context: ContextSchema { fields: vec![] },
            const_pool: ConstantPool { entries: vec![] },
            relation_manifest: RelationManifest { entries: vec![] },
            capability_manifest: tabula_ir::CapabilityManifest { entries: vec![] },
            event_manifest: EventManifest { entries: vec![] },
            entries: vec![Entry {
                id: EntryId(0),
                symbol: "sparse".into(),
                kind: EntryKind::Tx,
                params: vec![],
                returns: vec![],
                return_policy: ReturnPolicy::Unit,
                body: Body {
                    locals: vec![
                        LocalDecl {
                            id: LocalId(0),
                            ty: TYPE_U64_ID,
                        },
                        LocalDecl {
                            id: LocalId(3),
                            ty: TYPE_U64_ID,
                        },
                    ],
                    ops: vec![Op::Return {
                        values: ValueTupleRef(vec![]),
                    }],
                },
            }],
        }
    }

    #[test]
    fn machine_shape_counts_slots_by_highest_local_id() {
        let shape = compute_program_machine_shape(&program_with_sparse_local_ids(), &[])
            .expect("shape should compute");

        assert_eq!(
            shape,
            ProgramMachineShape {
                max_slots: 4,
                max_key_components: 0,
                max_key_fes: 0,
            }
        );
    }
}
