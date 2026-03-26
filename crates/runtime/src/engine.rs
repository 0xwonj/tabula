//! Native execution and proving runtime built on `tabula_ir`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use tabula_chips::relation_table::RELATION_TABLE_CHIP_ID;
use tabula_commitment::NativeDigest;
#[cfg(feature = "verify")]
use tabula_commitment::PoseidonHasher;
use tabula_compiler::RegisteredProgram;
#[cfg(feature = "prove")]
use tabula_contract::TupleEncodingDefaults;
use tabula_contract::{ProgramBinding, StaticTableArtifact};
use tabula_core::error::TabulaError;
use tabula_core::traits::StateView;
use tabula_core::{CellKey, ColId, Digest, PortableValue, RootProfileId, RowKey, TableId};
use tabula_executor as exec;
use tabula_ext::backend::execution::{IrHashExecutionBackend, RelationExecutionBackend};
#[cfg(feature = "prove")]
use tabula_ext::backend::scheme::{ColumnProofContext, PreparedColumnDelta, PreparedColumnProof};
#[cfg(feature = "prove")]
use tabula_ext::root::{RootBackendBundle, RootWitnessContext};
#[cfg(all(feature = "verify", not(feature = "prove")))]
use tabula_ext::root::{RootProofBackend, SmtRootProofBackend};
use tabula_ext::scheme::{ColumnBackendSetup, MaterializedColumnBackend};
use tabula_ir as ir;
#[cfg(feature = "prove")]
use tabula_machine::{
    ColumnSlotKey, PreparedColumnInput, PreparedMachineInput, PreparedTierInput, PublicStatement,
};
use tabula_machine::{TabulaMachine, TabulaProof, TabulaStarkConfig};
use tabula_profile::ResolvedColumnProfileRef;
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry, TypedColumnEntry, TypedValue};
#[cfg(feature = "prove")]
use tabula_witness::stark::prepare_execution_store;
#[cfg(feature = "prove")]
use tabula_witness::stark::{LowerSuccessfulTxInput, lower_successful_tx, merge_lowering_outputs};
#[cfg(feature = "prove")]
use tabula_witness::{AccessEvent, ColumnWrite, CommittedEntry, InitCell, prepare_relation_proof};

use crate::bootstrap::machine::{
    attach_execution_backend, build_machine_builder, supported_root_binding_families,
};
use crate::error::RuntimeError;
use crate::host::{HostEnvironment, SchemeFactoryMap, V1PropertyReads};
#[cfg(feature = "prove")]
use crate::proof_summary::ProofSummary;
use crate::semantics as runtime_ir;

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct SnapshotCellRecord {
    key: CellKey,
    value: PortableValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SnapshotCellJson {
    table: ir::TableId,
    row: RowKey,
    field: ir::FieldId,
    value: PortableValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StateSnapshotJson {
    known_tables: Vec<ir::TableId>,
    cells: Vec<SnapshotCellJson>,
}

/// Proof-capable committed state input for the native runtime.
#[derive(Debug, Clone, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StateSnapshot {
    known_tables: BTreeSet<TableId>,
    cells: BTreeMap<CellKey, PortableValue>,
}

impl StateSnapshot {
    /// Create an empty state snapshot for one validated program.
    pub fn empty(program: &ir::Program) -> Self {
        Self {
            known_tables: program
                .state
                .tables
                .iter()
                .map(|table| table.id.into())
                .collect(),
            cells: BTreeMap::new(),
        }
    }

    /// Build one state snapshot from explicit committed cells.
    pub fn from_cells<I>(program: &ir::Program, cells: I) -> Result<Self, RuntimeError>
    where
        I: IntoIterator<Item = (ir::TableId, RowKey, ir::FieldId, PortableValue)>,
    {
        let mut snapshot = Self::empty(program);
        for (table, row, field, value) in cells {
            snapshot.insert(program, table, row, field, value)?;
        }
        Ok(snapshot)
    }

    /// Insert one committed cell after validating it against the sealed program schema.
    pub fn insert(
        &mut self,
        program: &ir::Program,
        table: ir::TableId,
        row: RowKey,
        field: ir::FieldId,
        value: PortableValue,
    ) -> Result<(), RuntimeError> {
        let field_schema = program
            .state
            .tables
            .iter()
            .find(|schema| schema.id == table)
            .and_then(|schema| schema.fields.iter().find(|candidate| candidate.id == field))
            .ok_or_else(|| RuntimeError::ValidationFailed {
                detail: format!("unknown state field {}.{}", table.0, field.0),
            })?;
        if value.type_id() != field_schema.ty {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "state cell {}.{} row {} stores type {} but field expects {}",
                    table.0,
                    field.0,
                    row.0,
                    value.type_id().0,
                    field_schema.ty.0,
                ),
            });
        }
        self.known_tables.insert(table.into());
        self.cells.insert(
            CellKey {
                table: table.into(),
                col: field.into(),
                row,
            },
            value,
        );
        Ok(())
    }

    /// Iterate committed cells in canonical `(table, col, row)` order.
    pub fn cells(&self) -> impl Iterator<Item = (&CellKey, &PortableValue)> {
        self.cells.iter()
    }

    /// Remove one committed cell from the snapshot.
    pub fn remove(&mut self, table: ir::TableId, row: RowKey, field: ir::FieldId) {
        self.cells.remove(&CellKey {
            table: table.into(),
            col: field.into(),
            row,
        });
    }

    /// Serialize the snapshot canonically for transcript or external binding.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, RuntimeError> {
        let records = self
            .cells
            .iter()
            .map(|(key, value)| SnapshotCellRecord {
                key: *key,
                value: value.clone(),
            })
            .collect::<Vec<_>>();
        let mut bytes = b"tabula.runtime.state_snapshot.v1".to_vec();
        bytes.extend(
            borsh::to_vec(&records).map_err(|error| RuntimeError::ValidationFailed {
                detail: format!("failed to encode state snapshot: {error}"),
            })?,
        );
        Ok(bytes)
    }

    /// Canonical digest of the committed state snapshot.
    pub fn canonical_digest(&self) -> Result<Digest, RuntimeError> {
        let bytes = self.canonical_bytes()?;
        Ok(sha2::Sha256::digest(bytes).into())
    }

    fn typed_column_entries(
        &self,
        table: TableId,
        col: ColId,
        type_runtimes: &TypeRuntimeRegistry,
    ) -> Result<Vec<TypedColumnEntry>, RuntimeError> {
        self.cells
            .iter()
            .filter(|(key, _)| key.table == table && key.col == col)
            .map(|(key, value)| {
                type_runtimes
                    .decode_portable(value)
                    .map(|typed| TypedColumnEntry {
                        row_key: key.row,
                        value: typed,
                        is_null: false,
                    })
                    .map_err(|error| RuntimeError::ValidationFailed {
                        detail: format!(
                            "failed to decode committed cell ({}, {}, {}): {error}",
                            key.table.0, key.col.0, key.row.0
                        ),
                    })
            })
            .collect()
    }

    #[cfg(feature = "prove")]
    fn committed_entries(
        &self,
        table: TableId,
        col: ColId,
        type_runtimes: &TypeRuntimeRegistry,
    ) -> Result<Vec<CommittedEntry>, RuntimeError> {
        self.typed_column_entries(table, col, type_runtimes)?
            .into_iter()
            .map(|entry| {
                Ok(CommittedEntry {
                    row: entry.row_key,
                    value: entry.value,
                    is_null: entry.is_null,
                })
            })
            .collect()
    }
}

impl Serialize for StateSnapshot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let json = StateSnapshotJson {
            known_tables: self
                .known_tables
                .iter()
                .copied()
                .map(|table| ir::TableId(table.0))
                .collect(),
            cells: self
                .cells
                .iter()
                .map(|(key, value)| SnapshotCellJson {
                    table: ir::TableId(key.table.0),
                    row: key.row,
                    field: ir::FieldId(key.col.0),
                    value: value.clone(),
                })
                .collect(),
        };
        json.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for StateSnapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let json = StateSnapshotJson::deserialize(deserializer)?;
        Ok(Self {
            known_tables: json
                .known_tables
                .into_iter()
                .map(|table| TableId(table.0))
                .collect(),
            cells: json
                .cells
                .into_iter()
                .map(|cell| {
                    (
                        CellKey {
                            table: TableId(cell.table.0),
                            col: ColId(cell.field.0),
                            row: cell.row,
                        },
                        cell.value,
                    )
                })
                .collect(),
        })
    }
}

impl StateView for StateSnapshot {
    fn read(&self, key: &CellKey) -> Result<Option<PortableValue>, TabulaError> {
        Ok(self.cells.get(key).cloned())
    }

    fn table_exists(&self, table: TableId) -> bool {
        self.known_tables.contains(&table)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct ProofPublicContextBinding {
    field: ir::ContextFieldId,
    value: PortableValue,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct CanonicalProofStatement {
    program_hash: String,
    metadata_hash: String,
    program_id: ir::ProgramId,
    public_context: Vec<ProofPublicContextBinding>,
    event_digest: Digest,
    applied_tx_digest: Digest,
    static_table_root: Digest,
    old_state_root: Digest,
    new_state_root: Digest,
}

/// Transcript-bound semantic proof statement for the native runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ProofStatement {
    /// Compiler-sealed program binding.
    pub binding: ProgramBinding,
    /// Semantic public statement visible to native callers.
    pub public: runtime_ir::PublicStatement,
    /// Canonical digest of the applied transaction batch.
    pub applied_tx_digest: Digest,
    /// Transcript-bound root of the sealed static relation table set.
    pub static_table_root: Digest,
    /// Root before batch execution.
    pub old_state_root: Digest,
    /// Root after batch execution.
    pub new_state_root: Digest,
}

impl ProofStatement {
    /// Serialize the statement canonically for transcript binding.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, RuntimeError> {
        let canonical = CanonicalProofStatement {
            program_hash: self.binding.program_hash().to_string(),
            metadata_hash: self.binding.metadata_hash().to_string(),
            program_id: self.public.program_id,
            public_context: self
                .public
                .public_context
                .iter()
                .map(|binding| ProofPublicContextBinding {
                    field: binding.field,
                    value: binding.value.clone(),
                })
                .collect(),
            event_digest: self.public.event_digest,
            applied_tx_digest: self.applied_tx_digest,
            static_table_root: self.static_table_root,
            old_state_root: self.old_state_root,
            new_state_root: self.new_state_root,
        };
        let mut bytes = b"tabula.runtime.proof_statement.v1".to_vec();
        bytes.extend(
            borsh::to_vec(&canonical).map_err(|error| RuntimeError::StatementBuild {
                detail: format!("failed to encode proof statement: {error}"),
            })?,
        );
        Ok(bytes)
    }

    /// Canonical transcript-bound digest.
    pub fn statement_hash_bytes(&self) -> Result<[u8; 32], RuntimeError> {
        Ok(sha2::Sha256::digest(self.canonical_bytes()?).into())
    }
}

/// Inputs for native proving.
#[cfg(feature = "prove")]
pub struct ProveInput<'a> {
    /// Committed pre-state.
    pub snapshot: &'a StateSnapshot,
    /// Applied transactions.
    pub batch: &'a ir::EntryBatch,
    /// Public context values.
    pub context: &'a ir::ContextInput,
    /// Execution journal returned by [`TabulaRuntime::execute_batch`].
    pub executed: &'a exec::ExecutionJournal,
}

/// Result of native proof generation.
#[cfg(feature = "prove")]
pub struct ProveResult {
    /// Generated STARK proof.
    pub proof: TabulaProof,
    /// Transcript-bound native proof statement.
    pub statement: ProofStatement,
    /// Human-readable machine summary.
    pub summary: ProofSummary,
}

/// Result of prove + verify.
#[cfg(feature = "prove")]
pub struct VerifiedResult {
    /// Generated STARK proof.
    pub proof: TabulaProof,
    /// Transcript-bound native proof statement.
    pub statement: ProofStatement,
    /// Whether verification passed.
    pub verified: bool,
    /// Human-readable machine summary.
    pub summary: ProofSummary,
}

/// Runtime-owned execution result including exact inputs and post-state.
#[cfg(feature = "verify")]
#[derive(Debug, Clone)]
pub struct ExecutionReceipt {
    /// The committed pre-state used for execution.
    pub snapshot: StateSnapshot,
    /// The exact portable entry batch that was executed.
    pub batch: ir::EntryBatch,
    /// The exact portable context input used for execution.
    pub context: ir::ContextInput,
    /// The committed post-state after applying the journal's final writes.
    pub state_after: StateSnapshot,
    /// The underlying native execution journal.
    pub journal: exec::ExecutionJournal,
}

#[cfg(feature = "prove")]
#[derive(Clone)]
struct ColumnProofSlot {
    table: TableId,
    col: ColId,
    proof_backend: Arc<dyn tabula_ext::backend::scheme::ColumnProofBackend>,
}

#[derive(Clone)]
struct RuntimeCoreProgram {
    semantic: runtime_ir::RuntimeProgram,
    #[cfg(feature = "prove")]
    column_backends: BTreeMap<(TableId, ColId), MaterializedColumnBackend>,
    #[cfg(feature = "prove")]
    column_slots: Vec<ColumnProofSlot>,
    binding: ProgramBinding,
    uses_relations: bool,
    static_table_artifact: StaticTableArtifact,
    #[cfg(feature = "prove")]
    tuple_encoding_defaults: TupleEncodingDefaults,
    type_runtimes: TypeRuntimeRegistry,
    encoding_runtimes: EncodingRuntimeRegistry,
}

/// Fluent builder for the native execution/proving runtime.
pub struct RuntimeBuilder {
    registered_program: RegisteredProgram,
    host_environment: HostEnvironment,
    machine_stark_config: TabulaStarkConfig,
    #[cfg(feature = "prove")]
    root_backend_bundle: RootBackendBundle,
    #[cfg(not(feature = "prove"))]
    root_proof_backend: Arc<dyn RootProofBackend>,
}

/// Verifier built once per registered native program.
pub struct Verifier {
    binding: ProgramBinding,
    uses_relations: bool,
    static_table_artifact: StaticTableArtifact,
    machine: TabulaMachine,
}

/// Fluent builder for [`Verifier`].
pub struct VerifierBuilder {
    registered_program: RegisteredProgram,
    host_environment: HostEnvironment,
    machine_stark_config: TabulaStarkConfig,
    #[cfg(feature = "prove")]
    root_backend_bundle: RootBackendBundle,
    #[cfg(not(feature = "prove"))]
    root_proof_backend: Arc<dyn RootProofBackend>,
}

impl RuntimeBuilder {
    fn new(registered_program: RegisteredProgram) -> Self {
        Self {
            registered_program,
            host_environment: HostEnvironment::standard(),
            machine_stark_config: tabula_machine::default_config(),
            #[cfg(feature = "prove")]
            root_backend_bundle: RootBackendBundle::standard(),
            #[cfg(not(feature = "prove"))]
            root_proof_backend: Arc::new(SmtRootProofBackend),
        }
    }

    /// Replace the host-owned runtime registries and scheme factories.
    pub fn with_host_environment(mut self, host_environment: HostEnvironment) -> Self {
        self.host_environment = host_environment;
        self
    }

    /// Override the machine STARK configuration.
    pub fn with_machine_stark_config(mut self, machine_stark_config: TabulaStarkConfig) -> Self {
        self.machine_stark_config = machine_stark_config;
        self
    }

    /// Override the root proof backend bundle.
    #[cfg(feature = "prove")]
    pub fn with_root_backend_bundle(mut self, root_backend_bundle: RootBackendBundle) -> Self {
        self.root_backend_bundle = root_backend_bundle;
        self
    }

    /// Override the proof-side root backend.
    #[cfg(not(feature = "prove"))]
    pub fn with_root_proof_backend(
        mut self,
        root_proof_backend: impl RootProofBackend + 'static,
    ) -> Self {
        self.root_proof_backend = Arc::new(root_proof_backend);
        self
    }

    /// Override the proof-side root backend using a shared backend object.
    #[cfg(not(feature = "prove"))]
    pub fn with_root_proof_backend_arc(
        mut self,
        root_proof_backend: Arc<dyn RootProofBackend>,
    ) -> Self {
        self.root_proof_backend = root_proof_backend;
        self
    }

    /// Build the native runtime.
    pub fn build(self) -> Result<TabulaRuntime, RuntimeError> {
        validate_core_first_program(self.registered_program.program())?;
        let type_runtimes = self
            .host_environment
            .runtime_registries()
            .type_runtimes()
            .clone();
        let encoding_runtimes = self
            .host_environment
            .runtime_registries()
            .encoding_runtimes()
            .clone();
        #[cfg(feature = "prove")]
        let proof_backend = self.root_backend_bundle.proof_backend();
        #[cfg(not(feature = "prove"))]
        let proof_backend = Arc::clone(&self.root_proof_backend);
        let resolved_columns = materialize_registered_column_backends(
            &self.registered_program,
            self.host_environment.schemes().factories(),
            &type_runtimes,
            &encoding_runtimes,
            supported_root_binding_families(&proof_backend),
        )?;
        #[cfg(feature = "prove")]
        let column_slots = resolved_columns
            .column_backends
            .values()
            .map(|backend| ColumnProofSlot {
                table: backend.table_id,
                col: backend.col_id,
                proof_backend: Arc::clone(&backend.proof_backend),
            })
            .collect::<Vec<_>>();
        let proof_columns = resolved_columns
            .column_backends
            .values()
            .map(|backend| Arc::clone(&backend.proof_column))
            .collect::<Vec<_>>();

        let semantic = runtime_ir::RuntimeProgram::from_validated_program(
            self.registered_program.validated_program().clone(),
        )
        .map_err(|error| RuntimeError::ValidationFailed {
            detail: error.to_string(),
        })?;

        let uses_relations = program_uses_relations(self.registered_program.program());
        let mut machine_builder = build_machine_builder(&self.machine_stark_config, proof_backend)
            .with_columns(proof_columns);
        if program_uses_hash(self.registered_program.program()) {
            machine_builder =
                attach_execution_backend(machine_builder, Arc::new(IrHashExecutionBackend));
        }
        if uses_relations {
            machine_builder =
                attach_execution_backend(machine_builder, Arc::new(RelationExecutionBackend));
        }
        let machine = machine_builder
            .build()
            .map_err(RuntimeError::MachineSetup)?;

        let runtime_program = RuntimeCoreProgram {
            semantic,
            #[cfg(feature = "prove")]
            column_backends: resolved_columns.column_backends,
            #[cfg(feature = "prove")]
            column_slots,
            binding: self.registered_program.binding().clone(),
            uses_relations,
            static_table_artifact: self.registered_program.static_table_artifact().clone(),
            #[cfg(feature = "prove")]
            tuple_encoding_defaults: self.registered_program.tuple_encoding_defaults().clone(),
            type_runtimes,
            encoding_runtimes,
        };

        Ok(TabulaRuntime {
            runtime_program,
            #[cfg(feature = "prove")]
            root_backend_bundle: self.root_backend_bundle,
            machine,
        })
    }
}

impl Verifier {
    /// Create a builder for one registered native program.
    pub fn builder(registered_program: RegisteredProgram) -> VerifierBuilder {
        VerifierBuilder::new(registered_program)
    }

    /// Borrow the transcript-bound program binding.
    pub fn binding(&self) -> &ProgramBinding {
        &self.binding
    }

    /// The STARK machine backing this verifier.
    pub fn machine(&self) -> &TabulaMachine {
        &self.machine
    }

    /// Verify one native proof against this verifier's binding.
    pub fn verify(
        &self,
        proof: &TabulaProof,
        statement: &ProofStatement,
    ) -> Result<(), RuntimeError> {
        if statement.binding != self.binding {
            return Err(RuntimeError::ValidationFailed {
                detail: "proof statement binding does not match the verifier binding".to_string(),
            });
        }
        let expected_digest = statement.statement_hash_bytes()?;
        if proof.statement_digest != expected_digest {
            return Err(RuntimeError::ValidationFailed {
                detail: "proof statement digest does not match the proof transcript binding"
                    .to_string(),
            });
        }
        if proof.statement.old_root.to_bytes() != statement.old_state_root
            || proof.statement.new_root.to_bytes() != statement.new_state_root
        {
            return Err(RuntimeError::ValidationFailed {
                detail: "AIR roots do not match the proof statement".to_string(),
            });
        }
        if statement.static_table_root != self.static_table_artifact.root {
            return Err(RuntimeError::ValidationFailed {
                detail: "static table root does not match the verifier's registered program"
                    .to_string(),
            });
        }
        match relation_table_root_from_proof(proof)? {
            Some(root) if self.uses_relations => {
                if root != statement.static_table_root {
                    return Err(RuntimeError::ValidationFailed {
                        detail: "relation table chip root does not match the proof statement"
                            .to_string(),
                    });
                }
            }
            None if self.uses_relations => {
                return Err(RuntimeError::ValidationFailed {
                    detail: "relation table chip opening is missing from the execution proof"
                        .to_string(),
                });
            }
            _ => {}
        }
        self.machine
            .verify(proof)
            .map_err(RuntimeError::Verification)
    }
}

impl VerifierBuilder {
    fn new(registered_program: RegisteredProgram) -> Self {
        Self {
            registered_program,
            host_environment: HostEnvironment::standard(),
            machine_stark_config: tabula_machine::default_config(),
            #[cfg(feature = "prove")]
            root_backend_bundle: RootBackendBundle::standard(),
            #[cfg(not(feature = "prove"))]
            root_proof_backend: Arc::new(SmtRootProofBackend),
        }
    }

    /// Replace the host-owned runtime registries and scheme factories.
    pub fn with_host_environment(mut self, host_environment: HostEnvironment) -> Self {
        self.host_environment = host_environment;
        self
    }

    /// Override the machine STARK configuration.
    pub fn with_machine_stark_config(mut self, machine_stark_config: TabulaStarkConfig) -> Self {
        self.machine_stark_config = machine_stark_config;
        self
    }

    /// Override the root proof backend bundle.
    #[cfg(feature = "prove")]
    pub fn with_root_backend_bundle(mut self, root_backend_bundle: RootBackendBundle) -> Self {
        self.root_backend_bundle = root_backend_bundle;
        self
    }

    /// Override the proof-side root backend.
    #[cfg(not(feature = "prove"))]
    pub fn with_root_proof_backend(
        mut self,
        root_proof_backend: impl RootProofBackend + 'static,
    ) -> Self {
        self.root_proof_backend = Arc::new(root_proof_backend);
        self
    }

    /// Override the proof-side root backend using a shared backend object.
    #[cfg(not(feature = "prove"))]
    pub fn with_root_proof_backend_arc(
        mut self,
        root_proof_backend: Arc<dyn RootProofBackend>,
    ) -> Self {
        self.root_proof_backend = root_proof_backend;
        self
    }

    /// Build the native verifier.
    pub fn build(self) -> Result<Verifier, RuntimeError> {
        validate_core_first_program(self.registered_program.program())?;
        #[cfg(feature = "prove")]
        let proof_backend = self.root_backend_bundle.proof_backend();
        #[cfg(not(feature = "prove"))]
        let proof_backend = Arc::clone(&self.root_proof_backend);
        let resolved_columns = materialize_registered_column_backends(
            &self.registered_program,
            self.host_environment.schemes().factories(),
            self.host_environment.runtime_registries().type_runtimes(),
            self.host_environment
                .runtime_registries()
                .encoding_runtimes(),
            supported_root_binding_families(&proof_backend),
        )?;
        let proof_columns = resolved_columns
            .column_backends
            .into_values()
            .map(|backend| backend.proof_column)
            .collect::<Vec<_>>();
        let uses_relations = program_uses_relations(self.registered_program.program());
        let mut machine_builder = build_machine_builder(&self.machine_stark_config, proof_backend)
            .with_columns(proof_columns);
        if program_uses_hash(self.registered_program.program()) {
            machine_builder =
                attach_execution_backend(machine_builder, Arc::new(IrHashExecutionBackend));
        }
        if uses_relations {
            machine_builder =
                attach_execution_backend(machine_builder, Arc::new(RelationExecutionBackend));
        }
        let machine = machine_builder
            .build()
            .map_err(RuntimeError::MachineSetup)?;
        Ok(Verifier {
            binding: self.registered_program.binding().clone(),
            uses_relations,
            static_table_artifact: self.registered_program.static_table_artifact().clone(),
            machine,
        })
    }
}

fn relation_table_root_from_proof(proof: &TabulaProof) -> Result<Option<Digest>, RuntimeError> {
    let Some(values) = proof.execution_chip_public_values(RELATION_TABLE_CHIP_ID) else {
        return Ok(None);
    };
    let public_values: [p3_koala_bear::KoalaBear; 8] =
        values
            .try_into()
            .map_err(|_| RuntimeError::ValidationFailed {
                detail: format!(
                    "relation table chip exposed {} public values; expected 8",
                    values.len()
                ),
            })?;
    Ok(Some(NativeDigest(public_values).to_bytes()))
}

/// Native execution and proving runtime.
pub struct TabulaRuntime {
    runtime_program: RuntimeCoreProgram,
    #[cfg(feature = "prove")]
    root_backend_bundle: RootBackendBundle,
    machine: TabulaMachine,
}

impl TabulaRuntime {
    /// Create a builder for one registered native program.
    pub fn builder(registered_program: RegisteredProgram) -> RuntimeBuilder {
        RuntimeBuilder::new(registered_program)
    }

    /// Borrow the semantic runtime contract.
    pub fn runtime_program(&self) -> &runtime_ir::RuntimeProgram {
        &self.runtime_program.semantic
    }

    /// Borrow the canonical execution contract.
    pub fn execution_program(&self) -> &exec::ResolvedExecutionProgram {
        self.runtime_program.semantic.execution()
    }

    /// Borrow the canonical semantic proof contract.
    pub fn proof_program(&self) -> &runtime_ir::ResolvedProofProgram {
        self.runtime_program.semantic.proof()
    }

    /// Borrow the transcript-bound program binding.
    pub fn binding(&self) -> &ProgramBinding {
        &self.runtime_program.binding
    }

    /// Borrow the transcript-bound static relation table root.
    pub fn static_table_root(&self) -> Digest {
        self.runtime_program.static_table_artifact.root
    }

    /// The machine backing native proving and verification.
    pub fn machine(&self) -> &TabulaMachine {
        &self.machine
    }

    /// Installed type runtimes.
    pub fn type_runtimes(&self) -> &TypeRuntimeRegistry {
        &self.runtime_program.type_runtimes
    }

    /// Installed encoding runtimes.
    pub fn encoding_runtimes(&self) -> &EncodingRuntimeRegistry {
        &self.runtime_program.encoding_runtimes
    }

    /// Create an empty committed state snapshot for this runtime's program.
    pub fn empty_state_snapshot(&self) -> StateSnapshot {
        StateSnapshot::empty(self.execution_program().program())
    }

    /// Execute a canonical tx batch.
    pub fn execute_batch(
        &self,
        snapshot: &StateSnapshot,
        batch: &ir::EntryBatch,
        context: &ir::ContextInput,
    ) -> Result<exec::ExecutionJournal, RuntimeError> {
        let txs = self.decode_entry_batch(batch)?;
        let context = self.decode_context_input(context)?;
        self.execute_batch_typed(snapshot, &txs, &context)
    }

    /// Execute a canonical tx batch and return a runtime-owned receipt.
    pub fn execute_batch_receipt(
        &self,
        snapshot: &StateSnapshot,
        batch: &ir::EntryBatch,
        context: &ir::ContextInput,
    ) -> Result<ExecutionReceipt, RuntimeError> {
        let journal = self.execute_batch(snapshot, batch, context)?;
        let state_after = materialize_post_state(
            self.execution_program().program(),
            snapshot,
            &journal,
            self.type_runtimes(),
        )?;
        Ok(ExecutionReceipt {
            snapshot: snapshot.clone(),
            batch: batch.clone(),
            context: context.clone(),
            state_after,
            journal,
        })
    }

    fn execute_batch_typed(
        &self,
        snapshot: &StateSnapshot,
        txs: &[exec::TxCall],
        context: &exec::ContextValues,
    ) -> Result<exec::ExecutionJournal, RuntimeError> {
        let property_reads = self.property_reads(snapshot)?;
        exec::execute_batch(
            self.execution_program(),
            txs,
            context,
            snapshot,
            &exec::ExecContext {
                hasher: &PoseidonHasher::new(),
                type_runtimes: self.type_runtimes(),
                capability_executor: None,
                property_reads: property_reads
                    .as_ref()
                    .map(|reads| reads as &dyn exec::PropertyReadExecutor),
            },
        )
        .map_err(|source| RuntimeError::Execution {
            source,
            instruction_index: None,
            tx_index: None,
        })
    }

    /// Execute one query entry. Query proving remains intentionally absent.
    pub fn execute_query(
        &self,
        snapshot: &StateSnapshot,
        entry_id: ir::EntryId,
        params: &[PortableValue],
        context: &ir::ContextInput,
    ) -> Result<exec::QueryExecutionResult, RuntimeError> {
        let params = self.decode_query_params(entry_id, params)?;
        let context = self.decode_context_input(context)?;
        self.execute_query_typed(snapshot, entry_id, &params, &context)
    }

    fn execute_query_typed(
        &self,
        snapshot: &StateSnapshot,
        entry_id: ir::EntryId,
        params: &[TypedValue],
        context: &exec::ContextValues,
    ) -> Result<exec::QueryExecutionResult, RuntimeError> {
        let property_reads = self.property_reads(snapshot)?;
        exec::execute_query(
            self.execution_program(),
            entry_id,
            params,
            context,
            snapshot,
            &exec::ExecContext {
                hasher: &PoseidonHasher::new(),
                type_runtimes: self.type_runtimes(),
                capability_executor: None,
                property_reads: property_reads
                    .as_ref()
                    .map(|reads| reads as &dyn exec::PropertyReadExecutor),
            },
        )
        .map_err(|error| RuntimeError::Execution {
            source: error.error,
            instruction_index: Some(error.op_index),
            tx_index: None,
        })
    }

    /// Build the semantic native public statement from executed batch truth.
    pub fn build_public_statement(
        &self,
        context: &ir::ContextInput,
        execution_journal: &exec::ExecutionJournal,
    ) -> Result<runtime_ir::PublicStatement, RuntimeError> {
        let context = self.decode_context_input(context)?;
        self.build_public_statement_typed(&context, execution_journal)
    }

    fn build_public_statement_typed(
        &self,
        context: &exec::ContextValues,
        execution_journal: &exec::ExecutionJournal,
    ) -> Result<runtime_ir::PublicStatement, RuntimeError> {
        runtime_ir::build_public_statement(
            self.proof_program(),
            context,
            execution_journal,
            &PoseidonHasher::new(),
            self.type_runtimes(),
        )
        .map_err(|error| RuntimeError::StatementBuild {
            detail: error.to_string(),
        })
    }

    fn property_reads(
        &self,
        snapshot: &StateSnapshot,
    ) -> Result<Option<V1PropertyReads>, RuntimeError> {
        if !program_uses_property_reads(self.execution_program().program()) {
            return Ok(None);
        }

        let mut reads = V1PropertyReads::new();
        for table in &self.execution_program().program().state.tables {
            for field in &table.fields {
                reads = reads.with_column(
                    table.id,
                    field.id,
                    snapshot.typed_column_entries(
                        table.id.into(),
                        field.id.into(),
                        self.type_runtimes(),
                    )?,
                );
            }
        }
        Ok(Some(reads))
    }

    /// Generate a proof for one already-executed tx batch.
    #[cfg(feature = "prove")]
    pub fn prove(&self, input: &ProveInput<'_>) -> Result<ProveResult, RuntimeError> {
        let (statement, machine_input) = self.prepare_proof_request(input)?;
        let proof = self
            .machine
            .prove(machine_input)
            .map_err(RuntimeError::Proving)?;
        let summary = ProofSummary::from_proof(&proof);
        Ok(ProveResult {
            proof,
            statement,
            summary,
        })
    }

    /// Verify a native proof against this runtime's machine and binding.
    pub fn verify(
        &self,
        proof: &TabulaProof,
        statement: &ProofStatement,
    ) -> Result<(), RuntimeError> {
        if statement.binding != self.runtime_program.binding {
            return Err(RuntimeError::ValidationFailed {
                detail: "proof statement binding does not match the runtime binding".to_string(),
            });
        }
        let expected_digest = statement.statement_hash_bytes()?;
        if proof.statement_digest != expected_digest {
            return Err(RuntimeError::ValidationFailed {
                detail: "proof statement digest does not match the proof transcript binding"
                    .to_string(),
            });
        }
        if proof.statement.old_root.to_bytes() != statement.old_state_root
            || proof.statement.new_root.to_bytes() != statement.new_state_root
        {
            return Err(RuntimeError::ValidationFailed {
                detail: "AIR roots do not match the proof statement".to_string(),
            });
        }
        if statement.static_table_root != self.runtime_program.static_table_artifact.root {
            return Err(RuntimeError::ValidationFailed {
                detail: "static table root does not match the runtime's registered program"
                    .to_string(),
            });
        }
        match relation_table_root_from_proof(proof)? {
            Some(root) if self.runtime_program.uses_relations => {
                if root != statement.static_table_root {
                    return Err(RuntimeError::ValidationFailed {
                        detail: "relation table chip root does not match the proof statement"
                            .to_string(),
                    });
                }
            }
            None if self.runtime_program.uses_relations => {
                return Err(RuntimeError::ValidationFailed {
                    detail: "relation table chip opening is missing from the execution proof"
                        .to_string(),
                });
            }
            _ => {}
        }
        self.machine
            .verify(proof)
            .map_err(RuntimeError::Verification)
    }

    /// Generate and verify a proof in one call.
    #[cfg(feature = "prove")]
    pub fn prove_and_verify(&self, input: &ProveInput<'_>) -> Result<VerifiedResult, RuntimeError> {
        let prove_result = self.prove(input)?;
        self.verify(&prove_result.proof, &prove_result.statement)?;
        Ok(VerifiedResult {
            proof: prove_result.proof,
            statement: prove_result.statement,
            verified: true,
            summary: prove_result.summary,
        })
    }

    /// Execute, prove, and verify one tx batch in one call.
    #[cfg(feature = "prove")]
    pub fn execute_and_prove(
        &self,
        snapshot: &StateSnapshot,
        batch: &ir::EntryBatch,
        context: &ir::ContextInput,
    ) -> Result<VerifiedResult, RuntimeError> {
        let executed = self.execute_batch(snapshot, batch, context)?;
        self.prove_and_verify(&ProveInput {
            snapshot,
            batch,
            context,
            executed: &executed,
        })
    }

    #[cfg(feature = "prove")]
    fn prepare_proof_request(
        &self,
        input: &ProveInput<'_>,
    ) -> Result<(ProofStatement, PreparedMachineInput), RuntimeError> {
        let typed_context = self.decode_context_input(input.context)?;
        let typed_txs = self.decode_entry_batch(input.batch)?;
        let public = self.build_public_statement_typed(&typed_context, input.executed)?;
        let applied_tx_digest = digest_entry_batch(input.batch)?;
        let proof_artifacts = prepare_proof_artifacts(
            &self.runtime_program,
            &self.root_backend_bundle,
            input.snapshot,
            &typed_txs,
            &typed_context,
            input.executed,
        )?;
        let statement = ProofStatement {
            binding: self.runtime_program.binding.clone(),
            public,
            applied_tx_digest,
            static_table_root: self.runtime_program.static_table_artifact.root,
            old_state_root: proof_artifacts.air_statement.old_root.to_bytes(),
            new_state_root: proof_artifacts.air_statement.new_root.to_bytes(),
        };
        let machine_input =
            proof_artifacts.into_prepared_machine_input(statement.statement_hash_bytes()?);
        Ok((statement, machine_input))
    }

    fn decode_entry_batch(
        &self,
        batch: &ir::EntryBatch,
    ) -> Result<Vec<exec::TxCall>, RuntimeError> {
        batch
            .calls
            .iter()
            .map(|call| self.decode_entry_call(call))
            .collect()
    }

    fn decode_entry_call(&self, call: &ir::EntryCall) -> Result<exec::TxCall, RuntimeError> {
        let entry = self
            .execution_program()
            .entry_definition(call.entry_id)
            .map_err(|error| RuntimeError::ValidationFailed {
                detail: error.to_string(),
            })?;
        if entry.kind != ir::EntryKind::Tx {
            return Err(RuntimeError::ValidationFailed {
                detail: format!("entry {} is not a tx entry", call.entry_id.0),
            });
        }
        let params = self.decode_params(&entry.params, &call.params)?;
        Ok(exec::TxCall {
            entry_id: call.entry_id,
            params,
        })
    }

    fn decode_query_params(
        &self,
        entry_id: ir::EntryId,
        params: &[PortableValue],
    ) -> Result<Vec<TypedValue>, RuntimeError> {
        let entry = self
            .execution_program()
            .entry_definition(entry_id)
            .map_err(|error| RuntimeError::ValidationFailed {
                detail: error.to_string(),
            })?;
        if entry.kind != ir::EntryKind::Query {
            return Err(RuntimeError::ValidationFailed {
                detail: format!("entry {} is not a query entry", entry_id.0),
            });
        }
        self.decode_params(&entry.params, params)
    }

    fn decode_params(
        &self,
        expected: &[ir::ParamDecl],
        params: &[PortableValue],
    ) -> Result<Vec<TypedValue>, RuntimeError> {
        if expected.len() != params.len() {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "expected {} params but received {}",
                    expected.len(),
                    params.len()
                ),
            });
        }
        expected
            .iter()
            .zip(params)
            .map(|(param, value)| {
                if value.type_id() != param.ty {
                    return Err(RuntimeError::ValidationFailed {
                        detail: format!(
                            "param {} expects type {} but received {}",
                            param.symbol,
                            param.ty.0,
                            value.type_id().0
                        ),
                    });
                }
                self.type_runtimes()
                    .decode_portable(value)
                    .map_err(|error| RuntimeError::ValidationFailed {
                        detail: error.to_string(),
                    })
            })
            .collect()
    }

    fn decode_context_input(
        &self,
        context: &ir::ContextInput,
    ) -> Result<exec::ContextValues, RuntimeError> {
        let mut typed = exec::ContextValues::new();
        for (field_id, value) in &context.fields {
            let field = self
                .execution_program()
                .context_field(*field_id)
                .map_err(|error| RuntimeError::ValidationFailed {
                    detail: error.to_string(),
                })?;
            if value.type_id() != field.ty {
                return Err(RuntimeError::ValidationFailed {
                    detail: format!(
                        "context field {} expects type {} but received {}",
                        field.symbol,
                        field.ty.0,
                        value.type_id().0
                    ),
                });
            }
            let decoded = self
                .type_runtimes()
                .decode_portable(value)
                .map_err(|error| RuntimeError::ValidationFailed {
                    detail: error.to_string(),
                })?;
            typed.insert(*field_id, decoded);
        }
        Ok(typed)
    }
}

fn materialize_post_state(
    program: &ir::Program,
    snapshot: &StateSnapshot,
    journal: &exec::ExecutionJournal,
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<StateSnapshot, RuntimeError> {
    let mut state_after = snapshot.clone();
    for write in &journal.state_summary.write_set_final {
        let table = ir::TableId(write.key.table.0);
        let field = ir::FieldId(write.key.col.0);
        match &write.value {
            Some(value) => {
                let portable = type_runtimes.encode_typed(value).map_err(|source| {
                    RuntimeError::ValidationFailed {
                        detail: source.to_string(),
                    }
                })?;
                state_after.insert(program, table, write.key.row, field, portable)?;
            }
            None => state_after.remove(table, write.key.row, field),
        }
    }
    Ok(state_after)
}

struct ResolvedRuntimeColumns {
    column_backends: BTreeMap<(TableId, ColId), MaterializedColumnBackend>,
}

fn materialize_registered_column_backends(
    registered_program: &RegisteredProgram,
    backend_factories: &SchemeFactoryMap,
    type_runtimes: &TypeRuntimeRegistry,
    encoding_runtimes: &EncodingRuntimeRegistry,
    accepted_root_binding_families: &[RootProfileId],
) -> Result<ResolvedRuntimeColumns, RuntimeError> {
    let mut column_backends = BTreeMap::new();
    let required_property_query_kinds = BTreeSet::new();

    for schema in registered_program.table_schemas() {
        for column in &schema.columns {
            let resolved = registered_program
                .resolve_field_profile(ir::TableId(schema.id.0), ir::FieldId(column.id.0))
                .map_err(|detail| RuntimeError::ValidationFailed { detail })?;
            let scheme_id = resolved.scheme_profile.scheme_family_id;
            let type_runtime = type_runtimes
                .resolve(resolved.type_descriptor.type_id)
                .map_err(|detail| RuntimeError::ValidationFailed {
                    detail: detail.to_string(),
                })?
                .clone();
            let encoding_runtime = encoding_runtimes
                .resolve(resolved.encoding_profile.encoding_profile_id)
                .map_err(|detail| RuntimeError::ValidationFailed {
                    detail: detail.to_string(),
                })?
                .clone();
            let Some(factory) = backend_factories.get(&scheme_id) else {
                return Err(RuntimeError::ValidationFailed {
                    detail: format!(
                        "no canonical backend factory registered for scheme id {}",
                        scheme_id.0
                    ),
                });
            };
            let backend = factory
                .materialize_backend(ColumnBackendSetup {
                    table_id: schema.id,
                    col_id: column.id,
                    profile: resolved,
                    type_runtime,
                    encoding_runtime,
                    required_property_query_kinds: &required_property_query_kinds,
                })
                .map_err(RuntimeError::from_extension_setup)?;
            validate_materialized_backend(&backend, resolved, accepted_root_binding_families)?;
            let key = (schema.id, column.id);
            column_backends.insert(key, backend);
        }
    }

    Ok(ResolvedRuntimeColumns { column_backends })
}

fn validate_materialized_backend(
    backend: &MaterializedColumnBackend,
    resolved: ResolvedColumnProfileRef<'_>,
    accepted_root_binding_families: &[RootProfileId],
) -> Result<(), RuntimeError> {
    if backend.verifier_contract.scheme_id != resolved.scheme_profile.scheme_family_id {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "materialized backend for scheme {} reported verifier contract scheme {}",
                resolved.scheme_profile.scheme_family_id.0, backend.verifier_contract.scheme_id.0
            ),
        });
    }
    if backend.verifier_contract.proof_layout_family != resolved.proof_layout_family() {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "materialized backend proof layout mismatch: profile={} backend={}",
                resolved.proof_layout_family().0,
                backend.verifier_contract.proof_layout_family.0
            ),
        });
    }
    if backend.verifier_contract.verifier_digest_format != resolved.verifier_digest_format() {
        return Err(RuntimeError::ValidationFailed {
            detail: "materialized backend verifier digest format does not match scheme profile"
                .to_string(),
        });
    }
    if backend.root_binding_contract.root_binding_family != resolved.root_binding_family() {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "materialized backend root binding family mismatch: profile={} backend={}",
                resolved.root_binding_family().0,
                backend.root_binding_contract.root_binding_family.0
            ),
        });
    }
    if backend.root_binding_contract.column_profile_hash != resolved.column_profile.profile_hash {
        return Err(RuntimeError::ValidationFailed {
            detail: "materialized backend root binding contract does not match column profile hash"
                .to_string(),
        });
    }
    if !accepted_root_binding_families.contains(&backend.root_binding_contract.root_binding_family)
    {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "root backend does not support binding family {} for table {} col {}",
                backend.root_binding_contract.root_binding_family.0,
                backend.table_id.0,
                backend.col_id.0,
            ),
        });
    }
    Ok(())
}

fn validate_core_first_program(program: &ir::Program) -> Result<(), RuntimeError> {
    for entry in &program.entries {
        for op in &entry.body.ops {
            match op {
                ir::Op::ReadStateProperty { .. } => {
                    return Err(RuntimeError::ValidationFailed {
                        detail:
                            "native proving core-first cutover does not yet support property-read proving"
                                .to_string(),
                    });
                }
                ir::Op::CallCapability { .. } => {
                    return Err(RuntimeError::ValidationFailed {
                        detail:
                            "native proving core-first cutover encountered a deferred proof feature"
                                .to_string(),
                    });
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn program_uses_hash(program: &ir::Program) -> bool {
    program
        .entries
        .iter()
        .flat_map(|entry| entry.body.ops.iter())
        .any(|op| matches!(op, ir::Op::Hash { .. }))
}

fn program_uses_property_reads(program: &ir::Program) -> bool {
    program
        .entries
        .iter()
        .flat_map(|entry| entry.body.ops.iter())
        .any(|op| matches!(op, ir::Op::ReadStateProperty { .. }))
}

fn program_uses_relations(program: &ir::Program) -> bool {
    program
        .entries
        .iter()
        .flat_map(|entry| entry.body.ops.iter())
        .any(|op| {
            matches!(
                op,
                ir::Op::AssertRelation { .. } | ir::Op::EvalRelation { .. }
            )
        })
}

#[cfg(feature = "prove")]
#[derive(Clone)]
struct PreparedColumnSlot {
    table: TableId,
    col: ColId,
    old_entries: Vec<CommittedEntry>,
    init_cells: Vec<InitCell>,
    access_events: Vec<AccessEvent>,
    writes: Vec<ColumnWrite>,
}

#[cfg(feature = "prove")]
struct PreparedColumnArtifacts {
    input: PreparedColumnInput,
}

#[cfg(feature = "prove")]
struct PreparedArtifacts {
    air_statement: PublicStatement,
    execution: PreparedTierInput,
    columns: Vec<PreparedColumnArtifacts>,
    root: PreparedTierInput,
}

#[cfg(feature = "prove")]
impl PreparedArtifacts {
    fn into_prepared_machine_input(
        self,
        semantic_statement_digest: [u8; 32],
    ) -> PreparedMachineInput {
        PreparedMachineInput {
            execution: self.execution,
            columns: self
                .columns
                .into_iter()
                .map(|column| column.input)
                .collect(),
            root: self.root,
            air_statement: self.air_statement,
            semantic_statement_digest,
        }
    }
}

#[cfg(feature = "prove")]
fn prepare_proof_artifacts(
    runtime_program: &RuntimeCoreProgram,
    root_backend_bundle: &RootBackendBundle,
    snapshot: &StateSnapshot,
    txs: &[exec::TxCall],
    context: &exec::ContextValues,
    executed: &exec::ExecutionJournal,
) -> Result<PreparedArtifacts, RuntimeError> {
    let mut column_slots = Vec::with_capacity(runtime_program.column_slots.len());
    for slot in &runtime_program.column_slots {
        column_slots.push(PreparedColumnSlot {
            table: slot.table,
            col: slot.col,
            old_entries: snapshot.committed_entries(
                slot.table,
                slot.col,
                &runtime_program.type_runtimes,
            )?,
            init_cells: Vec::new(),
            access_events: Vec::new(),
            writes: Vec::new(),
        });
    }
    let column_index = runtime_program
        .column_slots
        .iter()
        .enumerate()
        .map(|(index, slot)| ((slot.table, slot.col), index))
        .collect::<BTreeMap<_, _>>();
    let empty_columns = runtime_program
        .column_slots
        .iter()
        .zip(column_slots.iter())
        .filter_map(|(slot, prepared)| {
            prepared
                .old_entries
                .is_empty()
                .then_some((ir::TableId(slot.table.0), ir::FieldId(slot.col.0)))
        })
        .collect::<BTreeSet<_>>();

    for entry in &executed.state_summary.read_set_old {
        let slot = *column_index
            .get(&(entry.key.table, entry.key.col))
            .ok_or_else(|| RuntimeError::ValidationFailed {
                detail: format!(
                    "read-set column ({}, {}) missing from the proof plan",
                    entry.key.table.0, entry.key.col.0
                ),
            })?;
        let value = match &entry.value {
            Some(value) => value.clone(),
            None => runtime_program
                .type_runtimes
                .zero_of(entry.type_id)
                .map_err(|source| RuntimeError::WitnessGeneration {
                    detail: source.to_string(),
                })?,
        };
        column_slots[slot].init_cells.push(InitCell {
            key: entry.key,
            value,
            is_null: entry.value.is_none(),
        });
    }
    for entry in &executed.state_summary.write_set_final {
        let slot = *column_index
            .get(&(entry.key.table, entry.key.col))
            .ok_or_else(|| RuntimeError::ValidationFailed {
                detail: format!(
                    "write-set column ({}, {}) missing from the proof plan",
                    entry.key.table.0, entry.key.col.0
                ),
            })?;
        column_slots[slot].writes.push(ColumnWrite {
            row: entry.key.row,
            value: entry.value.clone(),
        });
    }

    let mut lowered_txs = Vec::new();
    for tx in executed.successful_txs() {
        for effect in &tx.state_effects {
            let slot = *column_index
                .get(&(effect.key.table, effect.key.col))
                .ok_or_else(|| RuntimeError::ValidationFailed {
                    detail: format!(
                        "state effect column ({}, {}) missing from the proof plan",
                        effect.key.table.0, effect.key.col.0
                    ),
                })?;
            let value = match &effect.value {
                Some(value) => value.clone(),
                None => runtime_program
                    .type_runtimes
                    .zero_of(effect.type_id)
                    .map_err(|source| RuntimeError::WitnessGeneration {
                        detail: source.to_string(),
                    })?,
            };
            column_slots[slot].access_events.push(AccessEvent {
                key: effect.key,
                time: effect.logical_time,
                is_write: matches!(
                    effect.kind,
                    exec::StateEffectKind::Write | exec::StateEffectKind::Delete
                ),
                value,
                is_null: effect.value.is_none(),
                tx_index: tx.tx_index,
                effect_ordinal_in_tx: effect.effect_ordinal_in_entry,
            });
        }

        let call = txs
            .get(tx.tx_index as usize)
            .ok_or_else(|| RuntimeError::ValidationFailed {
                detail: format!("missing tx call {} during witness lowering", tx.tx_index),
            })?;
        let entry = runtime_program
            .semantic
            .execution()
            .entry_definition(tx.entry_id)
            .map_err(|error| RuntimeError::ValidationFailed {
                detail: error.to_string(),
            })?;
        lowered_txs.push(
            lower_successful_tx::<3>(LowerSuccessfulTxInput {
                tx_index: tx.tx_index,
                program: runtime_program.semantic.execution().program(),
                call,
                entry,
                context,
                state_effects: &tx.state_effects,
                event_effects: &tx.event_effects,
                relation_effects: &tx.relation_effects,
                empty_columns: &empty_columns,
                type_runtimes: &runtime_program.type_runtimes,
                encoding_runtimes: &runtime_program.encoding_runtimes,
                tuple_encoding_defaults: &runtime_program.tuple_encoding_defaults,
                hasher: &PoseidonHasher::new(),
            })
            .map_err(RuntimeError::TraceBuild)?,
        );
    }

    let lowered = merge_lowering_outputs(lowered_txs.iter());
    let relation_proof = prepare_relation_proof(
        runtime_program.semantic.execution().program(),
        &runtime_program.static_table_artifact,
        &lowered.relation_claims,
    )
    .map_err(|source| RuntimeError::WitnessGeneration {
        detail: source.to_string(),
    })?;
    if relation_proof.root() != runtime_program.static_table_artifact.root {
        return Err(RuntimeError::WitnessGeneration {
            detail: "prepared relation proof root diverged from the registered static table root"
                .to_string(),
        });
    }

    let execution_store =
        prepare_execution_store(&lowered, &relation_proof).map_err(RuntimeError::TraceBuild)?;

    let prepared_columns = runtime_program
        .column_slots
        .iter()
        .zip(column_slots.into_iter())
        .map(|(slot, mut prepared)| {
            synthesize_missing_init_cells(runtime_program, slot, &mut prepared)?;
            prepare_column_slot(runtime_program, slot, prepared)
        })
        .collect::<Result<Vec<_>, RuntimeError>>()?;

    let root_bindings = prepared_columns
        .iter()
        .filter_map(|(_, _, proof)| proof.root_binding.clone())
        .collect::<Vec<_>>();
    let witness_preparer = root_backend_bundle.witness_preparer();
    let prepared_root = witness_preparer
        .prepare_root_witness(RootWitnessContext::new(&root_bindings))
        .map_err(RuntimeError::from_extension_proof)
        .map_err(|error| match error {
            RuntimeError::WitnessGeneration { detail } => RuntimeError::WitnessGeneration {
                detail: format!(
                    "root witness preparer '{}': {detail}",
                    witness_preparer.name(),
                ),
            },
            other => other,
        })?;
    let (air_statement, root_store) = prepared_root.into_parts();

    Ok(PreparedArtifacts {
        air_statement,
        execution: PreparedTierInput {
            store: execution_store,
        },
        columns: prepared_columns
            .into_iter()
            .map(|(table, col, proof)| PreparedColumnArtifacts {
                input: PreparedColumnInput {
                    key: ColumnSlotKey { table, col },
                    store: proof.store,
                },
            })
            .collect(),
        root: PreparedTierInput { store: root_store },
    })
}

#[cfg(feature = "prove")]
fn synthesize_missing_init_cells(
    runtime_program: &RuntimeCoreProgram,
    slot: &ColumnProofSlot,
    prepared: &mut PreparedColumnSlot,
) -> Result<(), RuntimeError> {
    let mut present_rows = prepared
        .init_cells
        .iter()
        .map(|cell| cell.key.row)
        .collect::<BTreeSet<_>>();
    let touched_rows = prepared
        .access_events
        .iter()
        .map(|event| event.key.row)
        .chain(prepared.writes.iter().map(|write| write.row))
        .collect::<BTreeSet<_>>();
    let old_entries = prepared
        .old_entries
        .iter()
        .map(|entry| (entry.row, (entry.value.clone(), entry.is_null)))
        .collect::<BTreeMap<_, _>>();
    let required_rows = old_entries
        .keys()
        .copied()
        .chain(touched_rows.iter().copied())
        .collect::<BTreeSet<_>>();
    if required_rows.is_empty() {
        return Ok(());
    }
    let field_ty = runtime_program
        .semantic
        .execution()
        .program()
        .state
        .tables
        .iter()
        .find(|table| table.id.0 == slot.table.0)
        .and_then(|table| {
            table
                .fields
                .iter()
                .find(|field| field.id.0 == slot.col.0)
                .map(|field| field.ty)
        })
        .ok_or_else(|| RuntimeError::ValidationFailed {
            detail: format!(
                "missing state field schema for touched proof column ({}, {})",
                slot.table.0, slot.col.0
            ),
        })?;

    for row in required_rows {
        if present_rows.contains(&row) {
            continue;
        }
        let (value, is_null) = match old_entries.get(&row) {
            Some((value, is_null)) => (value.clone(), *is_null),
            None => (
                runtime_program
                    .type_runtimes
                    .zero_of(field_ty)
                    .map_err(|source| RuntimeError::WitnessGeneration {
                        detail: source.to_string(),
                    })?,
                true,
            ),
        };
        prepared.init_cells.push(InitCell {
            key: tabula_core::CellKey {
                table: slot.table,
                col: slot.col,
                row,
            },
            value,
            is_null,
        });
        present_rows.insert(row);
    }
    prepared.init_cells.sort_by_key(|cell| cell.key.row);
    Ok(())
}

#[cfg(feature = "prove")]
fn prepare_column_slot(
    runtime_program: &RuntimeCoreProgram,
    slot: &ColumnProofSlot,
    prepared: PreparedColumnSlot,
) -> Result<(TableId, ColId, PreparedColumnProof), RuntimeError> {
    let backend = runtime_program
        .column_backends
        .get(&(slot.table, slot.col))
        .ok_or_else(|| RuntimeError::ValidationFailed {
            detail: format!(
                "missing materialized backend for table {} col {}",
                slot.table.0, slot.col.0
            ),
        })?;
    let proof = slot
        .proof_backend
        .prepare_column(ColumnProofContext {
            column: PreparedColumnDelta {
                table: prepared.table,
                col: prepared.col,
                init_cells: prepared.init_cells,
                access_events: prepared.access_events,
                writes: prepared.writes.clone(),
                is_touched: !prepared.writes.is_empty(),
            },
            old_entries: prepared.old_entries,
            property_reads: Vec::new(),
        })
        .map_err(RuntimeError::from_extension_proof)?;
    match (
        &proof.root_binding,
        backend.root_binding_contract.receives_commitment,
    ) {
        (Some(binding), true) => {
            if binding.table != slot.table
                || binding.col != slot.col
                || binding.root_binding_family != backend.root_binding_contract.root_binding_family
                || binding.column_profile_hash != backend.root_binding_contract.column_profile_hash
                || binding.binding_digest != backend.root_binding_contract.binding_digest
            {
                return Err(RuntimeError::ValidationFailed {
                    detail: format!(
                        "prepared column proof ({}, {}) returned a root binding that does not match the sealed backend contract",
                        slot.table.0, slot.col.0,
                    ),
                });
            }
        }
        (None, true) => {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "prepared column proof ({}, {}) omitted a required root binding",
                    slot.table.0, slot.col.0,
                ),
            });
        }
        (Some(_), false) => {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "prepared column proof ({}, {}) returned an unexpected root binding",
                    slot.table.0, slot.col.0,
                ),
            });
        }
        (None, false) => {}
    }
    Ok((slot.table, slot.col, proof))
}

#[cfg(feature = "prove")]
fn digest_entry_batch(batch: &ir::EntryBatch) -> Result<Digest, RuntimeError> {
    let mut bytes = b"tabula.runtime.applied_txs.v1".to_vec();
    bytes.extend(
        borsh::to_vec(batch).map_err(|error| RuntimeError::StatementBuild {
            detail: format!("failed to encode entry batch: {error}"),
        })?,
    );
    Ok(sha2::Sha256::digest(bytes).into())
}

#[cfg(all(test, feature = "prove"))]
mod relation_proof_tests {
    use super::*;

    use std::cmp::Ordering;

    use p3_field::PrimeCharacteristicRing;
    use p3_koala_bear::KoalaBear;
    use tabula_chips::execution::EXECUTION_STANDARD_VALUE_WIDTH;
    use tabula_chips::execution::trace::{InstructionRecord, Opcode};
    use tabula_chips::relation_table::{RELATION_TABLE_WITNESS_LABEL, RelationTableWitnessRow};
    use tabula_chips::relation_transcript::{
        RELATION_TRANSCRIPT_WITNESS_LABEL, RelationTranscriptCall,
    };
    use tabula_contract::format::typed_tuple::{TypedTupleRole, compute_typed_tuple_digest};
    use tabula_core::{EncodingProfileId, PortableValue, TypeId};
    use tabula_profile::{
        CanonicalNullEncoding, EncodingClass, EncodingProfile, FieldFamily, GenericIrFamily,
        HostValueFamily, NullSemantics, TranscriptSerialization, TypeCapabilities, TypeDescriptor,
        ZeroValueSpec,
    };
    use tabula_stark::trace::witness_labels;
    use tabula_testing::exec::{context_input, register_program_from_source, tx_batch};
    use tabula_types::{
        ArithmeticOp, EncodingRuntime, TypeRuntime, TypedValue, bool_portable, u64_portable,
        u64_typed,
    };
    use tabula_witness::stark::{LowerSuccessfulTxInput, lower_successful_tx};
    use tabula_witness::{RelationClaim, RelationClaimKind, prepare_relation_proof};

    const TEST_EXTRA_TYPE_ID: TypeId = TypeId(90_001);
    const TEST_EXTRA_ENCODING_ID: EncodingProfileId = EncodingProfileId(90_001);

    fn relation_source() -> &'static str {
        r#"
program RelationProof

context {
  caller: u64;
  epoch: u64;
}

state {
  table accounts(key id: u64) {
    tier: u64 @ssmc;
  }
}

relation AllowedTier(tier: u64) = enum { 0, 1, 2, 3 };
relation ValidEpoch(epoch: u64) = range(10, 13);
relation PreferredCaller(actor: u64) = set { 7, 8 };
relation PromoteTier(tier: u64) -> promoted: u64 = map {
  0 => 1,
  1 => 2,
  2 => 3,
  3 => 3,
};

tx enroll(flag: bool, id: u64, tier: u64) {
  assert relation AllowedTier(tier);
  assert relation ValidEpoch(epoch);
  if flag {
    let promoted = eval relation PromoteTier(tier);
    accounts[id].tier = promoted;
  } else {
    assert relation PreferredCaller(caller);
  }
  return;
}
"#
    }

    fn guarded_relation_source() -> &'static str {
        r#"
program GuardedRelation

context {
  caller: u64;
}

state {
  table accounts(key id: u64) {
    tier: u64 @ssmc;
  }
}

relation PromoteTier(tier: u64) -> promoted: u64 = map {
  1 => 2,
  2 => 3,
  3 => 3,
};

tx maybe_promote(flag: bool, id: u64, tier: u64) {
  if flag {
    let promoted = eval relation PromoteTier(tier);
    accounts[id].tier = promoted;
  } else {
    assert true;
  }
  return;
}
"#
    }

    fn relation_context(caller: u64, epoch: u64) -> ir::ContextInput {
        context_input([
            (ir::ContextFieldId(0), u64_portable(caller)),
            (ir::ContextFieldId(1), u64_portable(epoch)),
        ])
    }

    fn guarded_context(caller: u64) -> ir::ContextInput {
        context_input([(ir::ContextFieldId(0), u64_portable(caller))])
    }

    fn relation_snapshot(registered: &RegisteredProgram) -> StateSnapshot {
        StateSnapshot::from_cells(
            registered.program(),
            [
                (ir::TableId(0), RowKey(0), ir::FieldId(0), u64_portable(0)),
                (ir::TableId(0), RowKey(1), ir::FieldId(0), u64_portable(0)),
            ],
        )
        .expect("build relation snapshot")
    }

    fn runtime_for_source(source: &str) -> (RegisteredProgram, TabulaRuntime) {
        let registered = register_program_from_source(source);
        let runtime = TabulaRuntime::builder(registered.clone())
            .build()
            .expect("build runtime");
        (registered, runtime)
    }

    fn entry_id(runtime: &TabulaRuntime, symbol: &str) -> ir::EntryId {
        runtime
            .execution_program()
            .program()
            .entries
            .iter()
            .find(|entry| entry.symbol == symbol)
            .map_or_else(|| panic!("missing entry '{symbol}'"), |entry| entry.id)
    }

    fn prove_input<'a>(
        snapshot: &'a StateSnapshot,
        batch: &'a ir::EntryBatch,
        context: &'a ir::ContextInput,
        executed: &'a exec::ExecutionJournal,
    ) -> ProveInput<'a> {
        ProveInput {
            snapshot,
            batch,
            context,
            executed,
        }
    }

    #[derive(Clone)]
    struct ExtraTypeRuntime {
        descriptor: TypeDescriptor,
    }

    impl ExtraTypeRuntime {
        fn new() -> Self {
            let descriptor = TypeDescriptor::new(
                TEST_EXTRA_TYPE_ID,
                "test-extra-u64",
                Some("extra runtime used only to prove host overrides do not affect static relation roots".to_string()),
                HostValueFamily::UnsignedInt { bits: 64 },
                GenericIrFamily::UnsignedInteger,
                TypeCapabilities {
                    equality: true,
                    ordering: true,
                    arithmetic: true,
                },
                ZeroValueSpec::IntegerZero,
                NullSemantics::NullableWithCanonicalZero,
            )
            .expect("build extra type descriptor");
            Self { descriptor }
        }
    }

    impl TypeRuntime for ExtraTypeRuntime {
        fn type_id(&self) -> TypeId {
            self.descriptor.type_id
        }

        fn descriptor(&self) -> &TypeDescriptor {
            &self.descriptor
        }

        fn zero_typed(&self) -> TypedValue {
            TypedValue::new(self.type_id(), 0u64.to_le_bytes().to_vec())
        }

        fn encode_portable(&self, value: &TypedValue) -> Result<PortableValue, TabulaError> {
            Ok(value.clone().into_portable())
        }

        fn decode_portable(&self, value: &PortableValue) -> Result<TypedValue, TabulaError> {
            Ok(TypedValue::new(value.type_id(), value.payload().to_vec()))
        }

        fn validate(&self, value: &TypedValue) -> Result<(), TabulaError> {
            if value.type_id() != self.type_id() {
                return Err(TabulaError::Custom(
                    "unexpected type id for extra runtime".to_string(),
                ));
            }
            Ok(())
        }

        fn eq_value(&self, lhs: &TypedValue, rhs: &TypedValue) -> Result<bool, TabulaError> {
            self.validate(lhs)?;
            self.validate(rhs)?;
            Ok(lhs.payload() == rhs.payload())
        }

        fn cmp_value(&self, lhs: &TypedValue, rhs: &TypedValue) -> Result<Ordering, TabulaError> {
            self.validate(lhs)?;
            self.validate(rhs)?;
            Ok(lhs.payload().cmp(rhs.payload()))
        }

        fn apply_arithmetic(
            &self,
            _op: ArithmeticOp,
            _lhs: &TypedValue,
            _rhs: &TypedValue,
        ) -> Result<TypedValue, TabulaError> {
            Err(TabulaError::Custom(
                "extra runtime arithmetic is not used in this test".to_string(),
            ))
        }

        fn divmod(
            &self,
            _lhs: &TypedValue,
            _rhs: &TypedValue,
        ) -> Result<(TypedValue, TypedValue), TabulaError> {
            Err(TabulaError::Custom(
                "extra runtime divmod is not used in this test".to_string(),
            ))
        }

        fn debug_display(&self, value: &TypedValue) -> Result<String, TabulaError> {
            self.validate(value)?;
            Ok(format!("extra({:?})", value.payload()))
        }
    }

    #[derive(Clone)]
    struct ExtraEncodingRuntime {
        descriptor: EncodingProfile,
    }

    impl ExtraEncodingRuntime {
        fn new(type_descriptor: &TypeDescriptor) -> Self {
            let descriptor = EncodingProfile::new(
                TEST_EXTRA_ENCODING_ID,
                "test-extra-u64-encoding",
                Some("extra encoding used only to prove host overrides do not affect static relation roots".to_string()),
                type_descriptor,
                EncodingClass::FieldElementArray,
                FieldFamily::KoalaBear31,
                2,
                CanonicalNullEncoding::SeparateNullFlagWithZeroValue,
                TranscriptSerialization::FieldElementsWithNullFlag,
                true,
            )
            .expect("build extra encoding profile");
            Self { descriptor }
        }
    }

    impl EncodingRuntime for ExtraEncodingRuntime {
        fn encoding_profile_id(&self) -> EncodingProfileId {
            self.descriptor.encoding_profile_id
        }

        fn descriptor(&self) -> &EncodingProfile {
            &self.descriptor
        }

        fn encode_field_elements(&self, value: &TypedValue) -> Result<Vec<KoalaBear>, TabulaError> {
            if value.type_id() != self.descriptor.type_id {
                return Err(TabulaError::Custom(
                    "unexpected type id for extra encoding runtime".to_string(),
                ));
            }
            Ok(vec![KoalaBear::ZERO, KoalaBear::ZERO])
        }

        fn decode_field_elements(
            &self,
            _field_elements: &[KoalaBear],
        ) -> Result<TypedValue, TabulaError> {
            Ok(TypedValue::new(
                self.descriptor.type_id,
                0u64.to_le_bytes().to_vec(),
            ))
        }

        fn encode_transcript_atoms(
            &self,
            value: &TypedValue,
        ) -> Result<Vec<KoalaBear>, TabulaError> {
            self.encode_field_elements(value)
        }

        fn trace_width(&self) -> usize {
            self.descriptor.width as usize
        }
    }

    #[test]
    fn relation_table_rows_reject_claims_missing_from_manifest() {
        let (registered, runtime) = runtime_for_source(relation_source());
        let error = prepare_relation_proof(
            runtime.execution_program().program(),
            registered.static_table_artifact(),
            &[RelationClaim {
                relation: ir::RelationId(0),
                kind: RelationClaimKind::Assert,
                inputs: vec![u64_typed(9)],
                input_digest: [9; 8],
                outputs: vec![],
                output_digest: [0; 8],
                tx_index: 0,
                effect_ordinal_in_tx: 0,
                op_index: 0,
            }],
        )
        .expect_err("manifest mismatch must fail");

        assert!(
            error
                .to_string()
                .contains("was not present in the sealed manifest"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn lowering_rejects_duplicate_relation_effect_origins() {
        let (_registered, runtime) = runtime_for_source(relation_source());
        let enroll = entry_id(&runtime, "enroll");
        let batch = tx_batch(vec![ir::EntryCall {
            entry_id: enroll,
            params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
        }]);
        let context = relation_context(7, 11);
        let snapshot = runtime.empty_state_snapshot();
        let executed = runtime
            .execute_batch(&snapshot, &batch, &context)
            .expect("execute batch");
        let tx = executed
            .successful_txs()
            .next()
            .expect("successful tx")
            .clone();

        let mut duplicated_effects = tx.relation_effects.clone();
        duplicated_effects.push(
            tx.relation_effects
                .first()
                .expect("relation effect")
                .clone(),
        );

        let typed_context = runtime
            .decode_context_input(&context)
            .expect("typed context");
        let typed_txs = runtime.decode_entry_batch(&batch).expect("typed batch");
        let entry = runtime
            .execution_program()
            .entry_definition(enroll)
            .expect("resolved entry");

        let error = lower_successful_tx::<EXECUTION_STANDARD_VALUE_WIDTH>(LowerSuccessfulTxInput {
            tx_index: tx.tx_index,
            program: runtime.execution_program().program(),
            call: &typed_txs[0],
            entry,
            context: &typed_context,
            state_effects: &tx.state_effects,
            event_effects: &tx.event_effects,
            relation_effects: &duplicated_effects,
            empty_columns: &BTreeSet::new(),
            type_runtimes: runtime.type_runtimes(),
            encoding_runtimes: runtime.encoding_runtimes(),
            tuple_encoding_defaults: &runtime.runtime_program.tuple_encoding_defaults,
            hasher: &PoseidonHasher::new(),
        })
        .expect_err("duplicate relation effects must fail");

        assert!(
            error.to_string().contains("duplicate relation effect"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn untaken_relation_branches_emit_no_relation_claims_or_positive_lookup_counts() {
        let (registered, runtime) = runtime_for_source(guarded_relation_source());
        let batch = tx_batch(vec![ir::EntryCall {
            entry_id: entry_id(&runtime, "maybe_promote"),
            params: vec![bool_portable(false), u64_portable(0), u64_portable(2)],
        }]);
        let context = guarded_context(7);
        let snapshot = relation_snapshot(&registered);
        let executed = runtime
            .execute_batch(&snapshot, &batch, &context)
            .expect("execute guarded batch");

        let (_statement, machine_input) = runtime
            .prepare_proof_request(&prove_input(&snapshot, &batch, &context, &executed))
            .expect("prepare proof request");

        let transcript_calls = machine_input
            .execution
            .store
            .get::<Vec<RelationTranscriptCall>>(RELATION_TRANSCRIPT_WITNESS_LABEL)
            .expect("relation transcript calls");
        let lookup_rows = machine_input
            .execution
            .store
            .get::<Vec<RelationTableWitnessRow>>(RELATION_TABLE_WITNESS_LABEL)
            .expect("relation lookup rows");

        assert!(transcript_calls.is_empty());
        assert!(
            lookup_rows.iter().all(|row| row.lookup_mult == 0),
            "untaken branches must not contribute positive relation lookup multiplicities",
        );
    }

    #[test]
    fn tampering_relation_table_rows_breaks_proving() {
        let (registered, runtime) = runtime_for_source(relation_source());
        let batch = tx_batch(vec![ir::EntryCall {
            entry_id: entry_id(&runtime, "enroll"),
            params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
        }]);
        let context = relation_context(7, 11);
        let snapshot = relation_snapshot(&registered);
        let executed = runtime
            .execute_batch(&snapshot, &batch, &context)
            .expect("execute batch");

        let (_statement, mut machine_input) = runtime
            .prepare_proof_request(&prove_input(&snapshot, &batch, &context, &executed))
            .expect("prepare proof request");

        let mut rows = machine_input
            .execution
            .store
            .get::<Vec<RelationTableWitnessRow>>(RELATION_TABLE_WITNESS_LABEL)
            .expect("relation lookup rows")
            .clone();
        assert!(!rows.is_empty(), "expected relation lookup rows");
        let tampered = rows
            .iter_mut()
            .find(|row| row.lookup_mult > 0)
            .expect("expected at least one consumed relation lookup row");
        tampered.output_digest[0] = tampered.output_digest[0].wrapping_add(1);
        machine_input
            .execution
            .store
            .put(RELATION_TABLE_WITNESS_LABEL, rows);

        assert!(
            runtime.machine.prove(machine_input).is_err(),
            "tampered relation lookup rows must fail proving"
        );
    }

    #[test]
    fn tampering_execution_bound_relation_outputs_breaks_proving() {
        let (registered, runtime) = runtime_for_source(relation_source());
        let batch = tx_batch(vec![ir::EntryCall {
            entry_id: entry_id(&runtime, "enroll"),
            params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
        }]);
        let context = relation_context(7, 11);
        let snapshot = relation_snapshot(&registered);
        let executed = runtime
            .execute_batch(&snapshot, &batch, &context)
            .expect("execute batch");

        let (_statement, mut machine_input) = runtime
            .prepare_proof_request(&prove_input(&snapshot, &batch, &context, &executed))
            .expect("prepare proof request");

        let mut records = machine_input
            .execution
            .store
            .get::<Vec<InstructionRecord>>(witness_labels::EXECUTION_RECORDS)
            .expect("execution records")
            .clone();
        let eval_record = records
            .iter_mut()
            .find(|record| record.opcode == Opcode::RelationProof && record.relation_is_eval)
            .expect("relation eval execution record");
        eval_record.relation_output_vals[0][0] += KoalaBear::ONE;

        machine_input
            .execution
            .store
            .put(witness_labels::EXECUTION_RECORDS, records);

        assert!(
            runtime.machine.prove(machine_input).is_err(),
            "tampered relation output binding must fail proving"
        );
    }

    #[test]
    fn tampering_relation_effect_identity_breaks_proving() {
        let (registered, runtime) = runtime_for_source(relation_source());
        let batch = tx_batch(vec![ir::EntryCall {
            entry_id: entry_id(&runtime, "enroll"),
            params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
        }]);
        let context = relation_context(7, 11);
        let snapshot = relation_snapshot(&registered);
        let executed = runtime
            .execute_batch(&snapshot, &batch, &context)
            .expect("execute batch");

        let (_statement, mut machine_input) = runtime
            .prepare_proof_request(&prove_input(&snapshot, &batch, &context, &executed))
            .expect("prepare proof request");

        let mut calls = machine_input
            .execution
            .store
            .get::<Vec<RelationTranscriptCall>>(RELATION_TRANSCRIPT_WITNESS_LABEL)
            .expect("relation transcript calls")
            .clone();
        assert!(
            calls.len() >= 4,
            "expected multiple relation transcript calls"
        );
        calls[2].effect_ordinal_in_tx = calls[0].effect_ordinal_in_tx;
        machine_input
            .execution
            .store
            .put(RELATION_TRANSCRIPT_WITNESS_LABEL, calls);

        assert!(
            runtime.machine.prove(machine_input).is_err(),
            "tampered relation effect identity must fail proving"
        );
    }

    #[test]
    fn relation_table_rows_use_empty_output_digest_for_enum_relations() {
        let (registered, runtime) = runtime_for_source(relation_source());
        let empty_digest = compute_typed_tuple_digest(TypedTupleRole::RelationOutput, &[])
            .expect("empty tuple digest");
        let allowed_rows = registered
            .static_table_artifact()
            .rows
            .iter()
            .filter(|row| row.relation_id == 0)
            .collect::<Vec<_>>();
        assert_eq!(allowed_rows.len(), 4);
        assert!(
            allowed_rows
                .iter()
                .all(|row| row.output_digest == empty_digest)
        );

        let chosen = allowed_rows[2];
        let proof_rows = prepare_relation_proof(
            runtime.execution_program().program(),
            registered.static_table_artifact(),
            &[RelationClaim {
                relation: ir::RelationId(0),
                kind: RelationClaimKind::Assert,
                inputs: vec![u64_typed(2)],
                input_digest: chosen.input_digest,
                outputs: vec![],
                output_digest: chosen.output_digest,
                tx_index: 0,
                effect_ordinal_in_tx: 0,
                op_index: 0,
            }],
        )
        .expect("prepare relation proof rows");
        let rows = proof_rows
            .table_rows()
            .iter()
            .filter(|row| row.relation_id == 0)
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 4);
        assert!(rows.iter().all(|row| row.output_digest == empty_digest));
        assert_eq!(rows.iter().map(|row| row.lookup_mult).sum::<u32>(), 1);
    }

    #[test]
    fn relation_proof_root_matches_registered_artifact_and_chip_public_values() {
        let (registered, runtime) = runtime_for_source(relation_source());
        let batch = tx_batch(vec![ir::EntryCall {
            entry_id: entry_id(&runtime, "enroll"),
            params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
        }]);
        let context = relation_context(7, 11);
        let snapshot = relation_snapshot(&registered);
        let executed = runtime
            .execute_batch(&snapshot, &batch, &context)
            .expect("execute batch");
        let proved = runtime
            .execute_and_prove(&snapshot, &batch, &context)
            .expect("prove relation batch");
        let chip_root =
            relation_table_root_from_proof(&proved.proof).expect("extract relation chip root");

        assert_eq!(
            proved.statement.static_table_root,
            registered.static_table_artifact().root
        );
        assert_eq!(
            chip_root,
            Some(registered.static_table_artifact().root),
            "relation table chip root must match the registered artifact root",
        );
        assert_eq!(
            digest_entry_batch(&batch).expect("batch digest"),
            proved.statement.applied_tx_digest
        );
        assert_eq!(
            executed.successful_txs().count(),
            1,
            "sanity-check proof came from the expected execution batch",
        );
    }

    #[test]
    fn host_runtime_overrides_do_not_change_compiler_sealed_static_table_root() {
        let registered = register_program_from_source(relation_source());
        let extra_type = ExtraTypeRuntime::new();
        let extra_encoding = ExtraEncodingRuntime::new(extra_type.descriptor());
        let host_environment = HostEnvironment::standard().with_runtime_registries(
            crate::host::RuntimeRegistries::standard()
                .with_type_runtime(extra_type.clone())
                .expect("register extra type runtime")
                .with_encoding_runtime(extra_encoding)
                .expect("register extra encoding runtime"),
        );

        let runtime = TabulaRuntime::builder(registered.clone())
            .with_host_environment(host_environment.clone())
            .build()
            .expect("build runtime with extra host runtimes");
        let verifier = Verifier::builder(registered.clone())
            .with_host_environment(host_environment)
            .build()
            .expect("build verifier with extra host runtimes");

        let batch = tx_batch(vec![ir::EntryCall {
            entry_id: entry_id(&runtime, "enroll"),
            params: vec![bool_portable(true), u64_portable(0), u64_portable(2)],
        }]);
        let context = relation_context(7, 11);
        let snapshot = relation_snapshot(&registered);
        let proved = runtime
            .execute_and_prove(&snapshot, &batch, &context)
            .expect("prove relation batch under custom host environment");

        assert_eq!(
            proved.statement.static_table_root,
            registered.static_table_artifact().root
        );
        verifier
            .verify(&proved.proof, &proved.statement)
            .expect("verify proof under custom host environment");
    }
}
