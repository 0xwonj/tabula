use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::sync::Arc;

use tabula_chips::shards::memory::MemoryShardChip;
use tabula_chips::shards::meta::MetaShardChip;
use tabula_chips::shards::property::SsmcPropertyChip;
use tabula_chips::shards::state::StateShardChip;
use tabula_commitment::{PoseidonHasher, compute_column_root_binding_prefix_digest};
use tabula_core::{ColumnLayoutKind, CommittedPropertyQuery, PropertyQueryKind, SchemeId};
use tabula_ext::ExtError;
#[cfg(feature = "prove")]
use tabula_ext::backend::column::{ColumnProofBackend, ColumnProofContext, PreparedColumnProof};
use tabula_ext::scheme::RuntimeColumn;
use tabula_ext::scheme::{
    ColumnBackendFactory, ColumnBackendSetup, ColumnVerifierContract, MaterializedColumnBackend,
    RootBindingContract,
};
use tabula_machine::SetupError;
use tabula_machine::backend::{AnyRap, ColumnChipSet, ProofColumn};
use tabula_stark::chips::ChipIdAllocator;
use tabula_stark::trace::DynChip;
#[cfg(feature = "prove")]
use tabula_types::EncodingRuntime;
use tabula_types::{
    CommittedColumnEntry, TableKeyCodec, TypeRuntime, TypedCommittedPropertyQueryResult,
};
#[cfg(feature = "prove")]
use tabula_witness::stark::schemes::ssmc::{PreparedSsmcProof, SsmcProofInput, prepare_ssmc_proof};
/// SSMC commitment scheme factory.
#[non_exhaustive]
pub struct SsmcScheme<const W: usize>;

/// Query kinds currently implemented for SSMC's ordered committed state.
const SSMC_SUPPORTED_QUERY_KINDS: &[PropertyQueryKind] =
    &[PropertyQueryKind::Successor, PropertyQueryKind::Predecessor];

#[derive(Clone)]
struct SsmcBackendState {
    table_id: tabula_core::TableId,
    col_id: tabula_core::ColId,
    scheme_id: SchemeId,
    #[cfg(feature = "prove")]
    type_runtime: Arc<dyn TypeRuntime>,
    #[cfg(feature = "prove")]
    encoding_runtime: Arc<dyn EncodingRuntime>,
    #[cfg(feature = "prove")]
    key_codec: Arc<TableKeyCodec>,
    required_property_query_kinds: BTreeSet<PropertyQueryKind>,
    receives_commitment: bool,
    root_binding_contract: RootBindingContract,
}

impl std::fmt::Debug for SsmcBackendState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("SsmcBackendState");
        debug
            .field("table_id", &self.table_id)
            .field("col_id", &self.col_id)
            .field("scheme_id", &self.scheme_id)
            .field(
                "required_property_query_kinds",
                &self.required_property_query_kinds,
            )
            .field("receives_commitment", &self.receives_commitment)
            .field("root_binding_contract", &self.root_binding_contract);
        #[cfg(feature = "prove")]
        debug.field("type_id", &self.type_runtime.type_id()).field(
            "encoding_profile_id",
            &self.encoding_runtime.encoding_profile_id(),
        );
        debug.finish()
    }
}

#[derive(Clone)]
struct SsmcRuntimeState {
    table_id: tabula_core::TableId,
    col_id: tabula_core::ColId,
    type_runtime: Arc<dyn TypeRuntime>,
    key_codec: Arc<TableKeyCodec>,
}

impl std::fmt::Debug for SsmcRuntimeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SsmcRuntimeState")
            .field("table_id", &self.table_id)
            .field("col_id", &self.col_id)
            .field("type_id", &self.type_runtime.type_id())
            .field("key_contract", self.key_codec.contract())
            .finish()
    }
}

impl<const W: usize> ColumnBackendFactory for SsmcScheme<W> {
    fn scheme_id(&self) -> SchemeId {
        SchemeId::SSMC
    }

    fn name(&self) -> &str {
        "ssmc"
    }

    fn materialize_backend(
        &self,
        setup: ColumnBackendSetup<'_>,
    ) -> Result<MaterializedColumnBackend, ExtError> {
        validate_profile_setup(&setup, self.name(), ColumnLayoutKind::SSMC_V1)?;
        let root_binding_contract = RootBindingContract {
            root_binding_family: setup.profile.root_binding_family(),
            column_profile_hash: setup.profile.column_profile.profile_hash,
            binding_digest: compute_column_root_binding_prefix_digest(
                &PoseidonHasher::new(),
                setup.table.id,
                setup.column.id,
                setup.profile.root_binding_family(),
                &setup.profile.column_profile.profile_hash,
            ),
            receives_commitment: setup.profile.receives_commitment(),
        };
        let verifier_contract = ColumnVerifierContract {
            scheme_id: setup.profile.scheme_profile.scheme_family_id,
            proof_layout_family: setup.profile.proof_layout_family(),
            verifier_digest_format: setup.profile.verifier_digest_format(),
        };
        let state = SsmcBackendState {
            table_id: setup.table.id,
            col_id: setup.column.id,
            scheme_id: setup.profile.scheme_profile.scheme_family_id,
            #[cfg(feature = "prove")]
            type_runtime: Arc::clone(&setup.type_runtime),
            #[cfg(feature = "prove")]
            encoding_runtime: Arc::clone(&setup.encoding_runtime),
            #[cfg(feature = "prove")]
            key_codec: Arc::clone(&setup.key_codec),
            required_property_query_kinds: setup.column.required_property_queries.clone(),
            receives_commitment: setup.profile.receives_commitment(),
            root_binding_contract: root_binding_contract.clone(),
        };

        Ok(MaterializedColumnBackend {
            table_id: setup.table.id,
            col_id: setup.column.id,
            required_property_query_kinds: setup.column.required_property_queries.clone(),
            runtime_column: Arc::new(SsmcRuntimeColumn {
                state: SsmcRuntimeState {
                    table_id: setup.table.id,
                    col_id: setup.column.id,
                    type_runtime: Arc::clone(&setup.type_runtime),
                    key_codec: Arc::clone(&setup.key_codec),
                },
            }),
            proof_column: Arc::new(SsmcProofColumn::<W> {
                state: state.clone(),
            }),
            #[cfg(feature = "prove")]
            proof_backend: Arc::new(SsmcProofBackend::<W> {
                state: state.clone(),
            }),
            verifier_contract,
            root_binding_contract,
        })
    }
}

fn validate_profile_setup(
    setup: &ColumnBackendSetup<'_>,
    scheme_name: &str,
    expected_layout: ColumnLayoutKind,
) -> Result<(), ExtError> {
    if setup.profile.proof_layout_family() != expected_layout {
        return Err(ExtError::validation(format!(
            "scheme factory '{scheme_name}' cannot prepare column layout {}",
            setup.profile.proof_layout_family().0,
        )));
    }
    if let Some(kind) = setup
        .column
        .required_property_queries
        .iter()
        .find(|kind| !SSMC_SUPPORTED_QUERY_KINDS.contains(kind))
    {
        return Err(ExtError::validation(format!(
            "scheme '{scheme_name}' does not support property query {:?} for table {} col {}",
            kind, setup.table.id.0, setup.column.id.0,
        )));
    }
    Ok(())
}

#[derive(Debug)]
struct SsmcRuntimeColumn {
    state: SsmcRuntimeState,
}

impl RuntimeColumn for SsmcRuntimeColumn {
    fn name(&self) -> &str {
        "ssmc"
    }

    fn supported_property_query_kinds(&self) -> &[PropertyQueryKind] {
        SSMC_SUPPORTED_QUERY_KINDS
    }

    fn resolve_property(
        &self,
        query: &CommittedPropertyQuery,
        state: &[CommittedColumnEntry],
    ) -> Result<TypedCommittedPropertyQueryResult, tabula_core::error::TabulaError> {
        let non_null = || state.iter().filter(|entry| !entry.is_null);

        let resolved = match query {
            CommittedPropertyQuery::Successor { key } => {
                let mut best: Option<&CommittedColumnEntry> = None;
                for entry in non_null() {
                    if self.state.key_codec.compare(&entry.key, key)? != Ordering::Greater {
                        continue;
                    }
                    if let Some(current) = best {
                        if self.state.key_codec.compare(&entry.key, &current.key)? == Ordering::Less
                        {
                            best = Some(entry);
                        }
                    } else {
                        best = Some(entry);
                    }
                }
                best.map(|entry| TypedCommittedPropertyQueryResult {
                    value: entry.value.clone(),
                    key: Some(entry.key.clone()),
                    is_null: false,
                })
            }
            CommittedPropertyQuery::Predecessor { key } => {
                let mut best: Option<&CommittedColumnEntry> = None;
                for entry in non_null() {
                    if self.state.key_codec.compare(&entry.key, key)? != Ordering::Less {
                        continue;
                    }
                    if let Some(current) = best {
                        if self.state.key_codec.compare(&entry.key, &current.key)?
                            == Ordering::Greater
                        {
                            best = Some(entry);
                        }
                    } else {
                        best = Some(entry);
                    }
                }
                best.map(|entry| TypedCommittedPropertyQueryResult {
                    value: entry.value.clone(),
                    key: Some(entry.key.clone()),
                    is_null: false,
                })
            }
            CommittedPropertyQuery::Minimum
            | CommittedPropertyQuery::Maximum
            | CommittedPropertyQuery::NonExistenceRange { .. }
            | CommittedPropertyQuery::Aggregate { .. } => {
                return Err(tabula_core::error::TabulaError::InvalidIr(format!(
                    "column scheme '{}' does not implement property query {:?} for table {} col {}",
                    self.name(),
                    property_query_kind(query),
                    self.state.table_id.0,
                    self.state.col_id.0,
                )));
            }
        };

        Ok(resolved.unwrap_or(TypedCommittedPropertyQueryResult {
            value: self.state.type_runtime.zero_typed(),
            key: None,
            is_null: true,
        }))
    }
}

fn property_query_kind(query: &CommittedPropertyQuery) -> PropertyQueryKind {
    match query {
        CommittedPropertyQuery::Minimum => PropertyQueryKind::Minimum,
        CommittedPropertyQuery::Maximum => PropertyQueryKind::Maximum,
        CommittedPropertyQuery::Successor { .. } => PropertyQueryKind::Successor,
        CommittedPropertyQuery::Predecessor { .. } => PropertyQueryKind::Predecessor,
        CommittedPropertyQuery::NonExistenceRange { .. } => PropertyQueryKind::NonExistenceRange,
        CommittedPropertyQuery::Aggregate { .. } => PropertyQueryKind::Aggregate,
    }
}

#[derive(Debug)]
struct SsmcProofColumn<const W: usize> {
    state: SsmcBackendState,
}

impl<const W: usize> ProofColumn for SsmcProofColumn<W> {
    fn name(&self) -> &str {
        "ssmc"
    }

    fn table_id(&self) -> tabula_core::TableId {
        self.state.table_id
    }

    fn col_id(&self) -> tabula_core::ColId {
        self.state.col_id
    }

    fn scheme_id(&self) -> SchemeId {
        self.state.scheme_id
    }

    fn create_chips(&self, alloc: &mut ChipIdAllocator) -> Result<ColumnChipSet, SetupError> {
        let t = self.state.table_id.0;
        let c = self.state.col_id.0;

        let mem_id = alloc.next();
        let state_id = alloc.next();
        let meta_id = alloc.next();

        let mem = MemoryShardChip::<W>::new(mem_id, t, c);
        let state = StateShardChip::<W>::new(state_id, t, c);
        let meta = MetaShardChip::new(
            meta_id,
            t,
            c,
            self.state.root_binding_contract.binding_digest,
            self.state.receives_commitment,
        );

        let mut airs: Vec<Box<dyn AnyRap>> = vec![
            Box::new(mem.clone()),
            Box::new(state.clone()),
            Box::new(meta.clone()),
        ];
        let mut dyn_chips: Vec<Box<dyn DynChip>> =
            vec![Box::new(mem), Box::new(state), Box::new(meta)];

        if !self.state.required_property_query_kinds.is_empty() {
            let prop_id = alloc.next();
            let prop = SsmcPropertyChip::<W>::new(prop_id, t, c);
            airs.push(Box::new(prop.clone()));
            dyn_chips.push(Box::new(prop));
        }

        Ok(ColumnChipSet {
            airs,
            dyn_chips,
            bus_consumers: vec![],
        })
    }
}

#[cfg(feature = "prove")]
#[derive(Debug)]
struct SsmcProofBackend<const W: usize> {
    state: SsmcBackendState,
}

#[cfg(feature = "prove")]
impl<const W: usize> ColumnProofBackend for SsmcProofBackend<W> {
    fn name(&self) -> &str {
        "ssmc"
    }

    fn scheme_id(&self) -> SchemeId {
        self.state.scheme_id
    }

    fn prepare_column(&self, context: ColumnProofContext) -> Result<PreparedColumnProof, ExtError> {
        let PreparedSsmcProof {
            root_binding,
            store,
        } = prepare_ssmc_proof::<W>(&SsmcProofInput {
            table: context.column.table,
            col: context.column.col,
            type_runtime: self.state.type_runtime.as_ref(),
            encoding_runtime: self.state.encoding_runtime.as_ref(),
            key_codec: self.state.key_codec.as_ref(),
            old_entries: &context.old_entries,
            init_cells: &context.column.init_cells,
            access_events: &context.column.access_events,
            writes: &context.column.writes,
            is_touched: context.column.is_touched,
            property_reads: &context.property_reads,
            root_binding_family: self.state.root_binding_contract.root_binding_family,
            column_profile_hash: self.state.root_binding_contract.column_profile_hash,
            binding_digest: self.state.root_binding_contract.binding_digest,
        })
        .map_err(ExtError::proof_preparation)?;

        Ok(PreparedColumnProof {
            old_digest: root_binding.old_digest,
            new_digest: root_binding.new_digest,
            root_binding: self
                .state
                .root_binding_contract
                .receives_commitment
                .then_some(root_binding),
            store,
        })
    }
}
