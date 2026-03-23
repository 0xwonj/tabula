//! Local engine implementation.

mod helpers;
mod submit;
mod verify;

use std::collections::BTreeMap;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, RwLock};

use tabula_artifact::{Artifact, State, normalize_state};

use crate::protocol::error::ErrorCode;
use crate::service::capabilities::{Capabilities, CapabilityClientKind, CapabilityInputMode};
use crate::service::catalog::{CatalogEntry, ProgramCatalog, SINGLE_PROGRAM_ID};
use crate::service::error::{ServiceError, ServiceResult};
use crate::service::io::FileAccessPolicy;
use crate::service::receipt::{bytes_to_hex, now_ms};
use crate::service::{
    CreateInstanceCommand, InputRef, InstanceId, InstanceRecord, InstanceStatus,
    ListInstancesCommand, ListRunsCommand, ProgramId, ProgramInline, ProgramRecord,
    RegisterProgramCommand, RunId, RunRecord,
};

use helpers::{next_id, read_guard, write_guard};

/// Local engine implementation backed by in-process crates.
#[derive(Debug, Clone)]
pub struct LocalEngine {
    pub(super) files: FileAccessPolicy,
    pub(super) catalog: Arc<RwLock<ProgramCatalog>>,
    pub(super) instances: Arc<RwLock<BTreeMap<InstanceId, InstanceRecord>>>,
    pub(super) runs: Arc<RwLock<BTreeMap<RunId, RunRecord>>>,
    instance_seq: Arc<AtomicU64>,
    run_seq: Arc<AtomicU64>,
}

impl LocalEngine {
    /// Build local engine from file policy.
    pub fn new(files: FileAccessPolicy) -> Self {
        Self {
            files,
            catalog: Arc::new(RwLock::new(ProgramCatalog::default())),
            instances: Arc::new(RwLock::new(BTreeMap::new())),
            runs: Arc::new(RwLock::new(BTreeMap::new())),
            instance_seq: Arc::new(AtomicU64::new(0)),
            run_seq: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Return service capabilities.
    pub fn capabilities(&self) -> Capabilities {
        Capabilities {
            service_role: "local_control_plane",
            clients: vec![
                CapabilityClientKind::WebIde,
                CapabilityClientKind::Cli,
                CapabilityClientKind::Automation,
            ],
            register_program: true,
            create_instance: true,
            submit_run: true,
            prove: true,
            verify: true,
            list_programs: true,
            list_instances: true,
            run_history: true,
            input_modes: vec![CapabilityInputMode::Inline, CapabilityInputMode::File],
        }
    }

    /// Register and persist a artifact.
    pub fn register_program(&self, req: RegisterProgramCommand) -> ServiceResult<ProgramRecord> {
        let compiled_program = self.compile_program_input(&req.program)?;
        let program: Artifact = compiled_program.as_artifact();

        #[cfg(feature = "stark")]
        let prepared_runtime = Arc::new(
            tabula_runtime::TabulaRuntime::builder(compiled_program.clone())
                .build()
                .map_err(|e| map_runtime_registration_error(&e))?,
        );

        let mut catalog = write_guard(&self.catalog, "catalog")?;
        let record = ProgramRecord {
            program_id: ProgramId::new(SINGLE_PROGRAM_ID),
            label: req.label.filter(|label| !label.trim().is_empty()),
            created_at_ms: now_ms(),
            table_count: program.table_schemas.len(),
            tx_type_count: program.tx_types.len(),
            profile_hash: bytes_to_hex(&compiled_program.metadata_envelope().profile_hash),
            metadata_hash: bytes_to_hex(&compiled_program.metadata_envelope().canonical_hash()),
            program_hash: program.canonical_digest().map_err(|e| {
                ServiceError::internal(
                    ErrorCode::InternalError,
                    format!("failed to hash artifact: {e}"),
                )
            })?,
            contract_schema_version: compiled_program.metadata_envelope().contract_schema_version,
            binding_version: compiled_program.metadata_envelope().binding_version,
            statement_schema_version: compiled_program
                .metadata_envelope()
                .statement_schema_version,
            verifier_profile_version: compiled_program
                .metadata_envelope()
                .verifier_profile_version,
            program,
        };

        let entry = CatalogEntry {
            record: record.clone(),
            compiled_program,
            #[cfg(feature = "stark")]
            prepared_runtime,
        };
        catalog.replace_single(entry);

        Ok(record)
    }

    /// Fetch a registered program.
    pub fn get_program(&self, program_id: &str) -> ServiceResult<ProgramRecord> {
        Ok(self.get_program_store(program_id)?.record)
    }

    /// List registered programs.
    pub fn list_programs(&self) -> ServiceResult<Vec<ProgramRecord>> {
        Ok(read_guard(&self.catalog, "catalog")?.list_records())
    }

    /// Create a stateful instance from a program and initial state.
    pub fn create_instance(&self, req: CreateInstanceCommand) -> ServiceResult<InstanceRecord> {
        let program = self.get_program_store(req.program_id.as_str())?;
        let initial_state = self.files.load_json_input::<State>(&req.state, "state")?;
        let normalized_state = normalize_state(&initial_state)
            .map_err(|e| ServiceError::bad_request(ErrorCode::InvalidStateCell, e.to_string()))?;
        let ts = now_ms();
        let record = InstanceRecord {
            instance_id: self.next_instance_id(),
            program_id: program.record.program_id,
            label: req.label.filter(|label| !label.trim().is_empty()),
            created_at_ms: ts,
            updated_at_ms: ts,
            version: 0,
            status: InstanceStatus::Active,
            state_hash: normalized_state.canonical_digest().map_err(|e| {
                ServiceError::internal(
                    ErrorCode::InternalError,
                    format!("failed to hash state artifact: {e}"),
                )
            })?,
            state: normalized_state,
        };

        write_guard(&self.instances, "instance")?
            .insert(record.instance_id.clone(), record.clone());
        Ok(record)
    }

    /// Fetch a stateful instance.
    pub fn get_instance(&self, instance_id: &str) -> ServiceResult<InstanceRecord> {
        self.get_instance_record(instance_id)
    }

    /// List stateful instances.
    pub fn list_instances(&self, req: &ListInstancesCommand) -> ServiceResult<Vec<InstanceRecord>> {
        Ok(read_guard(&self.instances, "instance")?
            .values()
            .filter(|instance| {
                req.program_id
                    .as_ref()
                    .is_none_or(|program_id| instance.program_id == *program_id)
            })
            .cloned()
            .collect())
    }

    /// Fetch one run.
    pub fn get_run(&self, run_id: &str) -> ServiceResult<RunRecord> {
        let runs = read_guard(&self.runs, "run")?;
        runs.get(run_id).cloned().ok_or_else(|| {
            ServiceError::not_found(ErrorCode::RunNotFound, format!("run not found: {run_id}"))
        })
    }

    /// List runs.
    pub fn list_runs(&self, req: &ListRunsCommand) -> ServiceResult<Vec<RunRecord>> {
        let mut runs: Vec<_> = read_guard(&self.runs, "run")?
            .values()
            .filter(|run| {
                req.instance_id
                    .as_ref()
                    .is_none_or(|instance_id| run.instance_id == *instance_id)
            })
            .cloned()
            .collect();
        runs.sort_by(|a, b| b.created_at_ms.cmp(&a.created_at_ms));
        if let Some(limit) = req.limit {
            runs.truncate(limit);
        }
        Ok(runs)
    }

    // --- Internal helpers ---

    pub(super) fn get_program_store(&self, program_id: &str) -> ServiceResult<CatalogEntry> {
        read_guard(&self.catalog, "catalog")?.get(program_id)
    }

    fn get_instance_record(&self, instance_id: &str) -> ServiceResult<InstanceRecord> {
        read_guard(&self.instances, "instance")?
            .get(instance_id)
            .cloned()
            .ok_or_else(|| {
                ServiceError::not_found(
                    ErrorCode::InstanceNotFound,
                    format!("instance not found: {instance_id}"),
                )
            })
    }

    fn next_instance_id(&self) -> InstanceId {
        InstanceId::new(next_id(&self.instance_seq, "inst"))
    }

    pub(super) fn next_run_id(&self) -> RunId {
        RunId::new(next_id(&self.run_seq, "run"))
    }
}

#[cfg(feature = "stark")]
fn map_runtime_registration_error(err: &tabula_runtime::RuntimeError) -> ServiceError {
    match err {
        tabula_runtime::RuntimeError::ValidationFailed { detail } => {
            ServiceError::unprocessable(ErrorCode::ProgramSchemaError, detail.clone())
        }
        tabula_runtime::RuntimeError::MachineSetup(source) => {
            ServiceError::internal(ErrorCode::InternalError, source.to_string())
        }
        _ => ServiceError::internal(ErrorCode::InternalError, err.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    use crate::service::ErrorKind;
    use crate::service::{CreateInstanceCommand, RunStatus, SubmitRunCommand};
    use tabula_artifact::{StateEntry, Statement, merge_output_state_entries};
    use tabula_core::{ColId, RowKey, TableId};
    use tabula_testing::assertions::assert_state_cell;
    use tabula_testing::fixtures::examples::transfer_example_artifact_case;
    use tabula_types::u64_portable;

    #[test]
    fn inline_source_program_compiles() {
        let cwd = env::current_dir().expect("cwd");
        let engine = LocalEngine::new(FileAccessPolicy::new(vec![cwd]).expect("policy"));

        let result = engine.compile_program_input(&InputRef::inline(ProgramInline::source(
            "table a { v: u64 }\n tx t() {}",
        )));
        assert!(result.is_ok(), "inline source should compile");
    }

    #[test]
    fn capabilities_advertise_only_implemented_input_modes() {
        let cwd = env::current_dir().expect("cwd");
        let engine = LocalEngine::new(FileAccessPolicy::new(vec![cwd]).expect("policy"));
        let caps = engine.capabilities();
        assert_eq!(
            caps.input_modes,
            vec![CapabilityInputMode::Inline, CapabilityInputMode::File]
        );
    }

    #[test]
    fn statement_hash_is_stable() {
        let c1 = Statement {
            program_hash: "a".to_string(),
            state_hash: "b".to_string(),
            batch_hash: "c".to_string(),
            state_after_hash: "d".to_string(),
            metadata_hash: "e".to_string(),
            old_state_root: vec!["01".to_string()],
            new_state_root: vec!["02".to_string()],
        };
        let c2 = c1.clone();
        let h1 = c1.statement_hash();
        let h2 = c2.statement_hash();
        assert_eq!(h1, h2);

        let c3 = Statement {
            metadata_hash: "x".to_string(),
            ..c2
        };
        assert_ne!(h1, c3.statement_hash());
    }

    #[test]
    fn merge_output_state_entries_deduplicates_initial_entries() {
        let initial = vec![
            StateEntry {
                table: 0,
                row: 1,
                col: 2,
                value: Some(u64_portable(10)),
            },
            StateEntry {
                table: 0,
                row: 1,
                col: 2,
                value: Some(u64_portable(20)),
            },
        ];

        let merged = merge_output_state_entries(&initial, &[]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].value, Some(u64_portable(20)));
    }

    #[test]
    fn stateful_program_instance_run_flow_commits_and_records_run() {
        let cwd = env::current_dir().expect("cwd");
        let engine = LocalEngine::new(FileAccessPolicy::new(vec![cwd]).expect("policy"));
        let case = transfer_example_artifact_case();

        let registered = engine
            .register_program(RegisterProgramCommand {
                program: InputRef::inline(ProgramInline::program(case.artifact.clone())),
                label: Some("transfer".to_string()),
            })
            .expect("register program");

        let created = engine
            .create_instance(CreateInstanceCommand {
                program_id: registered.program_id.clone(),
                state: InputRef::inline(case.state.clone()),
                label: Some("demo".to_string()),
            })
            .expect("create instance");
        assert_eq!(created.version, 0);

        let submitted = engine
            .submit_run(&SubmitRunCommand {
                instance_id: created.instance_id.clone(),
                batch: InputRef::inline(case.batch.clone()),
                include_trace: false,
                prove: true,
                verify: false,
                commit: true,
                expected_instance_version: Some(0),
            })
            .expect("submit run");

        assert!(submitted.committed);
        #[cfg(feature = "stark")]
        assert!(
            submitted.stark_proof.is_some(),
            "STARK proof should be present when stark feature is enabled"
        );
        #[cfg(not(feature = "stark"))]
        assert!(
            submitted.proof.is_some(),
            "legacy proof should be present when stark feature is disabled"
        );
        assert_eq!(submitted.instance_version_before, 0);
        assert_eq!(submitted.instance_version_after, 1);

        let fetched = engine
            .get_instance(created.instance_id.as_str())
            .expect("fetch instance");
        assert_eq!(fetched.version, 1);
        assert_eq!(fetched.state_hash, submitted.state_hash_after);
        assert_state_cell(
            &fetched.state,
            TableId(0),
            ColId(0),
            RowKey(0),
            Some(&u64_portable(750)),
        );
        assert_state_cell(
            &fetched.state,
            TableId(0),
            ColId(0),
            RowKey(1),
            Some(&u64_portable(600)),
        );
        assert_state_cell(
            &fetched.state,
            TableId(0),
            ColId(0),
            RowKey(2),
            Some(&u64_portable(350)),
        );

        let fetched_run = engine
            .get_run(submitted.run_id.as_str())
            .expect("fetch run");
        assert_eq!(fetched_run.run_id, submitted.run_id);
    }

    #[test]
    fn submit_run_rejects_stale_expected_version() {
        let cwd = env::current_dir().expect("cwd");
        let engine = LocalEngine::new(FileAccessPolicy::new(vec![cwd]).expect("policy"));
        let case = transfer_example_artifact_case();

        let registered = engine
            .register_program(RegisterProgramCommand {
                program: InputRef::inline(ProgramInline::program(case.artifact.clone())),
                label: None,
            })
            .expect("register program");
        let created = engine
            .create_instance(CreateInstanceCommand {
                program_id: registered.program_id,
                state: InputRef::inline(case.state),
                label: None,
            })
            .expect("create instance");

        let err = engine
            .submit_run(&SubmitRunCommand {
                instance_id: created.instance_id,
                batch: InputRef::inline(case.batch),
                include_trace: false,
                prove: false,
                verify: false,
                commit: true,
                expected_instance_version: Some(7),
            })
            .expect_err("must reject stale expected version");
        assert!(matches!(err.kind(), ErrorKind::Conflict));
        assert!(matches!(err.code(), ErrorCode::InstanceVersionMismatch));
    }

    #[test]
    fn verify_run_transitions_status_to_verified() {
        let cwd = env::current_dir().expect("cwd");
        let engine = LocalEngine::new(FileAccessPolicy::new(vec![cwd]).expect("policy"));
        let case = transfer_example_artifact_case();

        let registered = engine
            .register_program(RegisterProgramCommand {
                program: InputRef::inline(ProgramInline::program(case.artifact.clone())),
                label: None,
            })
            .expect("register program");
        let created = engine
            .create_instance(CreateInstanceCommand {
                program_id: registered.program_id,
                state: InputRef::inline(case.state),
                label: None,
            })
            .expect("create instance");
        let submitted = engine
            .submit_run(&SubmitRunCommand {
                instance_id: created.instance_id,
                batch: InputRef::inline(case.batch),
                include_trace: false,
                prove: true,
                verify: false,
                commit: true,
                expected_instance_version: Some(0),
            })
            .expect("submit run");

        let verified = engine
            .verify_run(submitted.run_id.as_str())
            .expect("verify run");
        assert!(verified.verified);
        assert!(matches!(verified.run.status, RunStatus::Verified));
        assert_eq!(verified.run.proof_verified, Some(true));
    }
}
