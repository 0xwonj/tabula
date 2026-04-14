use std::sync::Arc;

use tabula_chips::shards::memory::MemoryShardChip;
use tabula_chips::shards::meta::MetaShardChip;
use tabula_chips::shards::smt_state::SmtStateShardChip;
use tabula_commitment::{PoseidonHasher, compute_column_root_binding_prefix_digest};
use tabula_core::{ColumnLayoutKind, CommittedPropertyQuery, PropertyQueryKind, SchemeId};
use tabula_ext::ExtError;
use tabula_ext::scheme::RuntimeColumn;
use tabula_ext::scheme::{
    ColumnBackendFactory, ColumnBackendSetup, ColumnVerifierContract, MaterializedColumnBackend,
    RootBindingContract,
};
use tabula_machine::SetupError;
use tabula_machine::backend::{AnyRap, ColumnChipSet, ProofColumn};
use tabula_profile::{ENCODING_BOOL_ID, ENCODING_I64_ID, ENCODING_U64_ID};
use tabula_stark::chips::ChipIdAllocator;
use tabula_stark::trace::DynChip;
use tabula_types::{CommittedColumnEntry, TypeRuntime, TypedCommittedPropertyQueryResult};
#[cfg(feature = "prove")]
use tabula_types::{EncodingRuntime, TableKeyCodec};
#[cfg(feature = "prove")]
use tabula_witness::stark::schemes::smt::{PreparedSmtProof, SmtProofInput, prepare_smt_proof};

#[cfg(feature = "prove")]
use tabula_ext::backend::column::{ColumnProofBackend, ColumnProofContext, PreparedColumnProof};

/// SMT commitment scheme factory.
pub struct SmtScheme<const W: usize>;

#[derive(Clone)]
struct SmtBackendState {
    table_id: tabula_core::TableId,
    col_id: tabula_core::ColId,
    scheme_id: SchemeId,
    #[cfg(feature = "prove")]
    type_runtime: Arc<dyn TypeRuntime>,
    #[cfg(feature = "prove")]
    encoding_runtime: Arc<dyn EncodingRuntime>,
    #[cfg(feature = "prove")]
    key_codec: Arc<TableKeyCodec>,
    receives_commitment: bool,
    root_binding_contract: RootBindingContract,
}

impl std::fmt::Debug for SmtBackendState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("SmtBackendState");
        debug
            .field("table_id", &self.table_id)
            .field("col_id", &self.col_id)
            .field("scheme_id", &self.scheme_id)
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
struct SmtRuntimeState {
    table_id: tabula_core::TableId,
    col_id: tabula_core::ColId,
    type_runtime: Arc<dyn TypeRuntime>,
}

impl std::fmt::Debug for SmtRuntimeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmtRuntimeState")
            .field("table_id", &self.table_id)
            .field("col_id", &self.col_id)
            .field("type_id", &self.type_runtime.type_id())
            .finish()
    }
}

impl<const W: usize> ColumnBackendFactory for SmtScheme<W> {
    fn scheme_id(&self) -> SchemeId {
        SchemeId::SMT
    }

    fn name(&self) -> &str {
        "smt"
    }

    fn materialize_backend(
        &self,
        setup: ColumnBackendSetup<'_>,
    ) -> Result<MaterializedColumnBackend, ExtError> {
        validate_profile_setup(&setup, self.name(), ColumnLayoutKind::SMT_V1)?;
        validate_private_tree_locator_setup(&setup, self.name())?;
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
        let state = SmtBackendState {
            table_id: setup.table.id,
            col_id: setup.column.id,
            scheme_id: setup.profile.scheme_profile.scheme_family_id,
            #[cfg(feature = "prove")]
            type_runtime: Arc::clone(&setup.type_runtime),
            #[cfg(feature = "prove")]
            encoding_runtime: Arc::clone(&setup.encoding_runtime),
            #[cfg(feature = "prove")]
            key_codec: Arc::clone(&setup.key_codec),
            receives_commitment: setup.profile.receives_commitment(),
            root_binding_contract: root_binding_contract.clone(),
        };

        Ok(MaterializedColumnBackend {
            table_id: setup.table.id,
            col_id: setup.column.id,
            required_property_query_kinds: setup.column.required_property_queries.clone(),
            runtime_column: Arc::new(SmtRuntimeColumn {
                state: SmtRuntimeState {
                    table_id: setup.table.id,
                    col_id: setup.column.id,
                    type_runtime: Arc::clone(&setup.type_runtime),
                },
            }),
            proof_column: Arc::new(SmtProofColumn::<W> {
                state: state.clone(),
            }),
            #[cfg(feature = "prove")]
            proof_backend: Arc::new(SmtProofBackend::<W> {
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
    if let Some(kind) = setup.column.required_property_queries.iter().next() {
        return Err(ExtError::validation(format!(
            "scheme '{scheme_name}' does not support property query {:?} for table {} col {}",
            kind, setup.table.id.0, setup.column.id.0,
        )));
    }
    Ok(())
}

fn validate_private_tree_locator_setup(
    setup: &ColumnBackendSetup<'_>,
    scheme_name: &str,
) -> Result<(), ExtError> {
    let unsupported = setup
        .key_codec
        .contract()
        .component_encoding_profile_ids
        .iter()
        .find(|encoding_profile_id| {
            !matches!(
                **encoding_profile_id,
                ENCODING_U64_ID | ENCODING_I64_ID | ENCODING_BOOL_ID
            )
        });
    if let Some(encoding_profile_id) = unsupported {
        return Err(ExtError::validation(format!(
            "scheme '{scheme_name}' only supports private u64-decodable key payload locators; unsupported key encoding {} for table {}",
            encoding_profile_id.0, setup.table.id.0,
        )));
    }
    Ok(())
}

#[derive(Debug)]
struct SmtRuntimeColumn {
    state: SmtRuntimeState,
}

impl RuntimeColumn for SmtRuntimeColumn {
    fn name(&self) -> &str {
        "smt"
    }

    fn resolve_property(
        &self,
        query: &CommittedPropertyQuery,
        _state: &[CommittedColumnEntry],
    ) -> Result<TypedCommittedPropertyQueryResult, tabula_core::error::TabulaError> {
        Err(tabula_core::error::TabulaError::InvalidIr(format!(
            "column scheme '{}' does not implement property query {:?} for table {} col {}",
            self.name(),
            property_query_kind(query),
            self.state.table_id.0,
            self.state.col_id.0,
        )))
    }

    fn supported_property_query_kinds(&self) -> &[PropertyQueryKind] {
        &[]
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
struct SmtProofColumn<const W: usize> {
    state: SmtBackendState,
}

impl<const W: usize> ProofColumn for SmtProofColumn<W> {
    fn name(&self) -> &str {
        "smt"
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
        let state = SmtStateShardChip::<W>::new(state_id, t, c);
        let meta = MetaShardChip::new(
            meta_id,
            t,
            c,
            self.state.root_binding_contract.binding_digest,
            self.state.receives_commitment,
        );

        Ok(ColumnChipSet {
            airs: vec![
                Box::new(mem.clone()) as Box<dyn AnyRap>,
                Box::new(state.clone()),
                Box::new(meta.clone()),
            ],
            dyn_chips: vec![
                Box::new(mem) as Box<dyn DynChip>,
                Box::new(state),
                Box::new(meta),
            ],
            bus_consumers: vec![],
        })
    }
}

#[cfg(feature = "prove")]
#[derive(Debug)]
struct SmtProofBackend<const W: usize> {
    state: SmtBackendState,
}

#[cfg(feature = "prove")]
impl<const W: usize> ColumnProofBackend for SmtProofBackend<W> {
    fn name(&self) -> &str {
        "smt"
    }

    fn scheme_id(&self) -> SchemeId {
        self.state.scheme_id
    }

    fn prepare_column(&self, context: ColumnProofContext) -> Result<PreparedColumnProof, ExtError> {
        if !context.property_reads.is_empty() {
            return Err(ExtError::proof_preparation(
                tabula_core::error::TabulaError::ProofError {
                    phase: "smt_proof",
                    detail: format!(
                        "SMT column ({}, {}) received unexpected property reads",
                        context.column.table.0, context.column.col.0
                    ),
                },
            ));
        }

        let PreparedSmtProof {
            root_binding,
            store,
        } = prepare_smt_proof::<W>(&SmtProofInput {
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
