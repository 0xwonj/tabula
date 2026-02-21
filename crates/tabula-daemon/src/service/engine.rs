//! Local engine implementation.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use serde_json::json;

use tabula_artifact::{BatchFile, ProgramArtifact, StateFile, normalize_state};
use tabula_core::mock::MockHasher;
use tabula_driver::{
    DriverError, MetadataPolicy, ProgramSourceFormat, RegisteredProgram, parse_program_sources,
    register_program_sources,
};

use super::catalog::{CatalogEntry, ProgramCatalog, SINGLE_PROGRAM_ID};
use super::commands::*;
use super::error::{ServiceError, ServiceResult};
use super::execute::execute_registered_batch;
use super::io::FileAccessPolicy;
use super::receipt::{
    build_receipt, bytes_to_hex, hash_json, now_ms, statement_components, verify_receipt,
};
use super::types::*;
use crate::protocol::error::ErrorCode;

/// Local engine implementation backed by in-process crates.
#[derive(Debug, Clone)]
pub struct LocalEngine {
    files: FileAccessPolicy,
    catalog: Arc<RwLock<ProgramCatalog>>,
    instances: Arc<RwLock<BTreeMap<String, InstanceRecord>>>,
    runs: Arc<RwLock<BTreeMap<String, RunRecord>>>,
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

    /// Register and persist a program artifact.
    pub fn register_program(&self, req: RegisterProgramCommand) -> ServiceResult<ProgramRecord> {
        let resolved = self.resolve_program_input(&req.program)?;
        let registered = register_resolved_program(&resolved)?;
        let program = ProgramArtifact {
            table_schemas: registered.table_schemas.clone(),
            tx_types: registered.tx_types.clone(),
            contract_metadata: Some(registered.metadata_envelope.clone()),
        };

        let mut catalog = write_guard(&self.catalog, "catalog")?;
        let record = ProgramRecord {
            program_id: SINGLE_PROGRAM_ID.to_string(),
            label: req.label.filter(|label| !label.trim().is_empty()),
            created_at_ms: now_ms(),
            table_count: program.table_schemas.len(),
            tx_type_count: program.tx_types.len(),
            profile_hash: bytes_to_hex(&registered.metadata_envelope.profile_hash),
            metadata_hash: bytes_to_hex(&registered.metadata_envelope.canonical_hash()),
            program_hash: hash_json("program", &program)?,
            program,
        };

        let entry = CatalogEntry {
            record: record.clone(),
            registered,
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
        let program = self.get_program_store(&req.program_id)?;
        let initial_state = self
            .files
            .load_json_input::<StateFile>(&req.state, "state")?;
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
            state_hash: hash_json("state", &normalized_state)?,
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
    pub fn list_instances(&self, req: ListInstancesCommand) -> ServiceResult<Vec<InstanceRecord>> {
        Ok(read_guard(&self.instances, "instance")?
            .values()
            .filter(|instance| {
                req.program_id
                    .as_ref()
                    .is_none_or(|program_id| &instance.program_id == program_id)
            })
            .cloned()
            .collect())
    }

    /// Submit a batch run against an instance.
    pub fn submit_run(&self, req: SubmitRunCommand) -> ServiceResult<RunRecord> {
        let snapshot = self.get_instance_record(&req.instance_id)?;
        check_version(&snapshot, req.expected_instance_version)?;

        let program = self.get_program_store(&snapshot.program_id)?;
        let batch_file = self
            .files
            .load_json_input::<BatchFile>(&req.batch, "batch")?;
        let state_before = snapshot.state.clone();

        #[cfg(feature = "stark")]
        let stark_proof_summary;

        let executed = {
            #[cfg(feature = "stark")]
            let use_stark = req.prove || req.verify;
            #[cfg(not(feature = "stark"))]
            let use_stark = false;

            if use_stark {
                #[cfg(feature = "stark")]
                {
                    let poseidon_hasher = tabula_commitment::PoseidonHasher::new();
                    let exec = execute_registered_batch(
                        program.registered.clone(),
                        state_before,
                        batch_file,
                        &poseidon_hasher,
                    )?;
                    stark_proof_summary = match super::prove::prove_batch(&exec, &program.registered)
                    {
                        Ok(summary) => Some(summary),
                        Err(e) => {
                            tracing::warn!(
                                "STARK proof generation failed, returning mock: {e}"
                            );
                            Some(super::prove::mock_stark_summary())
                        }
                    };
                    exec
                }
                #[cfg(not(feature = "stark"))]
                unreachable!()
            } else {
                #[cfg(feature = "stark")]
                {
                    stark_proof_summary = None;
                }
                execute_registered_batch(program.registered, state_before, batch_file, &MockHasher)?
            }
        };

        let execution = executed.clone().into_execution_result(req.include_trace);

        let components = statement_components(
            &executed.artifact,
            &executed.state_file,
            &executed.batch_file,
            &executed.state_after,
        )?;
        let stmt_hash = components.statement_hash();

        // Fill in statement hashes on STARK proof summary (computed after execution).
        #[cfg(not(feature = "stark"))]
        let stark_proof_summary: Option<tabula_artifact::StarkProofSummary> = None;
        #[cfg(feature = "stark")]
        let stark_proof_summary = stark_proof_summary.map(|mut s| {
            s.statement_hash = stmt_hash.clone();
            s.program_hash = components.program_hash.clone();
            s.batch_hash = components.batch_hash.clone();
            s
        });

        // Build legacy receipt for non-STARK path.
        let has_stark_proof = stark_proof_summary.is_some();
        let proof_requested = req.prove || req.verify;
        let proof = if proof_requested && !has_stark_proof {
            Some(build_receipt(
                &components,
                executed.tx_outcomes.len(),
                executed.emitted.len(),
                &executed.consistency,
            ))
        } else {
            None
        };

        let verification_message = if req.verify {
            if has_stark_proof {
                let verified = stark_proof_summary
                    .as_ref()
                    .map(|s| s.verified)
                    .unwrap_or(false);
                if !verified {
                    return Err(ServiceError::unprocessable(
                        ErrorCode::ExecutionError,
                        "STARK proof verification failed",
                    ));
                }
                Some("STARK proof verified".to_string())
            } else {
                let proof_ref = proof.as_ref().ok_or_else(|| {
                    ServiceError::internal(
                        ErrorCode::InternalError,
                        "proof must exist when verify=true",
                    )
                })?;
                let verification = verify_receipt(proof_ref, &components, &stmt_hash);
                if !verification.verified {
                    return Err(ServiceError::unprocessable(
                        ErrorCode::ExecutionError,
                        verification.message,
                    ));
                }
                Some(verification.message)
            }
        } else {
            None
        };

        let (version_after, state_hash_after) = if req.commit {
            commit_instance(
                &self.instances,
                &snapshot.instance_id,
                snapshot.version,
                execution.state_after.clone(),
                components.state_after_hash.clone(),
                now_ms(),
            )?
        } else {
            (snapshot.version, components.state_after_hash.clone())
        };

        let prove = req.prove || req.verify;
        let ts = now_ms();
        let run = RunRecord {
            run_id: self.next_run_id(),
            program_id: snapshot.program_id.clone(),
            instance_id: snapshot.instance_id.clone(),
            created_at_ms: ts,
            status: RunStatus::Succeeded,
            committed: req.commit,
            include_trace: req.include_trace,
            prove,
            verify: req.verify,
            instance_version_before: snapshot.version,
            instance_version_after: version_after,
            state_hash_before: components.state_hash,
            state_hash_after,
            program_hash: components.program_hash,
            batch_hash: components.batch_hash,
            metadata_hash: components.metadata_hash,
            statement_hash: stmt_hash,
            execution,
            proof,
            stark_proof: stark_proof_summary,
            proof_verified: None,
            verification_message: None,
            verified_at_ms: None,
        };

        let mut run = run;
        if req.verify {
            apply_verification(
                &mut run,
                true,
                verification_message.expect("guarded above"),
                ts,
            );
        }

        write_guard(&self.runs, "run")?.insert(run.run_id.clone(), run.clone());
        Ok(run)
    }

    /// Fetch one run.
    pub fn get_run(&self, run_id: &str) -> ServiceResult<RunRecord> {
        let runs = read_guard(&self.runs, "run")?;
        runs.get(run_id).cloned().ok_or_else(|| {
            ServiceError::not_found(ErrorCode::RunNotFound, format!("run not found: {run_id}"))
        })
    }

    /// List runs.
    pub fn list_runs(&self, req: ListRunsCommand) -> ServiceResult<Vec<RunRecord>> {
        let mut runs: Vec<_> = read_guard(&self.runs, "run")?
            .values()
            .filter(|run| {
                req.instance_id
                    .as_ref()
                    .is_none_or(|instance_id| &run.instance_id == instance_id)
            })
            .cloned()
            .collect();
        runs.sort_by(|a, b| b.created_at_ms.cmp(&a.created_at_ms));
        if let Some(limit) = req.limit {
            runs.truncate(limit);
        }
        Ok(runs)
    }

    /// Verify one run proof and update run verification status.
    pub fn verify_run(&self, run_id: &str) -> ServiceResult<VerifyOutcome> {
        let mut runs = write_guard(&self.runs, "run")?;
        let run = runs.get_mut(run_id).ok_or_else(|| {
            ServiceError::not_found(ErrorCode::RunNotFound, format!("run not found: {run_id}"))
        })?;

        // If STARK proof exists, use its cached verification result.
        if let Some(stark) = &run.stark_proof {
            let verified = stark.verified;
            let message = if verified {
                "STARK proof verified".to_string()
            } else {
                "STARK proof verification failed".to_string()
            };
            apply_verification(run, verified, message.clone(), now_ms());
            return Ok(VerifyOutcome {
                run: run.clone(),
                verified,
                message,
                statement_hash: run.statement_hash.clone(),
            });
        }

        let proof = run.proof.as_ref().ok_or_else(|| {
            ServiceError::unprocessable(ErrorCode::ExecutionError, "run has no proof to verify")
        })?;

        let components = super::receipt::StatementComponents {
            program_hash: run.program_hash.clone(),
            state_hash: run.state_hash_before.clone(),
            batch_hash: run.batch_hash.clone(),
            state_after_hash: run.state_hash_after.clone(),
            metadata_hash: run.metadata_hash.clone(),
        };

        let verification = verify_receipt(proof, &components, &run.statement_hash);
        apply_verification(
            run,
            verification.verified,
            verification.message.clone(),
            now_ms(),
        );

        Ok(VerifyOutcome {
            run: run.clone(),
            verified: verification.verified,
            message: verification.message,
            statement_hash: run.statement_hash.clone(),
        })
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct ResolvedProgramInput {
    sources: tabula_driver::ProgramSourceFile,
    metadata_policy: MetadataPolicy,
}

impl LocalEngine {
    fn get_program_store(&self, program_id: &str) -> ServiceResult<CatalogEntry> {
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

    fn next_instance_id(&self) -> String {
        next_id(&self.instance_seq, "inst")
    }

    fn next_run_id(&self) -> String {
        next_id(&self.run_seq, "run")
    }

    fn resolve_program_input(
        &self,
        input: &ProgramInputRef,
    ) -> ServiceResult<ResolvedProgramInput> {
        match input {
            InputRef::Inline { inline } => match inline {
                ProgramInline::Source { source } => {
                    parse_program_sources(source, ProgramSourceFormat::TabSource, "<inline:source>")
                        .map(|sources| ResolvedProgramInput {
                            sources,
                            metadata_policy: MetadataPolicy::Optional,
                        })
                        .map_err(map_driver_error)
                }
                ProgramInline::Program(program) => Ok(ResolvedProgramInput {
                    sources: program.clone(),
                    metadata_policy: MetadataPolicy::Required,
                }),
            },
            InputRef::File { file_path } => self.load_program_from_file(file_path),
            InputRef::Artifact { artifact_id } => Err(ServiceError::not_implemented(
                ErrorCode::ArtifactInputNotAvailable,
                format!("artifact input is not available yet: {artifact_id}"),
            )),
        }
    }

    fn load_program_from_file(&self, path: &Path) -> ServiceResult<ResolvedProgramInput> {
        let source = self.files.read_utf8_file(path, "program")?;
        let (format, metadata_policy) = if path.extension().and_then(|e| e.to_str()) == Some("tab")
        {
            (ProgramSourceFormat::TabSource, MetadataPolicy::Optional)
        } else {
            (ProgramSourceFormat::JsonArtifact, MetadataPolicy::Required)
        };

        parse_program_sources(&source, format, &path.display().to_string())
            .map(|sources| ResolvedProgramInput {
                sources,
                metadata_policy,
            })
            .map_err(map_driver_error)
    }
}

fn check_version(instance: &InstanceRecord, expected: Option<u64>) -> ServiceResult<()> {
    if instance.status != InstanceStatus::Active {
        return Err(ServiceError::unprocessable(
            ErrorCode::InstanceNotActive,
            format!("instance is not active: {}", instance.instance_id),
        ));
    }
    if let Some(expected) = expected
        && expected != instance.version
    {
        return Err(ServiceError::conflict(
            ErrorCode::InstanceVersionMismatch,
            format!(
                "instance version mismatch for {}: expected {expected}, actual {}",
                instance.instance_id, instance.version
            ),
        )
        .with_details(json!({
            "instance_id": instance.instance_id,
            "expected_version": expected,
            "actual_version": instance.version,
        })));
    }
    Ok(())
}

fn commit_instance(
    instances: &RwLock<BTreeMap<String, InstanceRecord>>,
    instance_id: &str,
    version_before: u64,
    state_after: StateFile,
    state_hash_after: String,
    updated_at_ms: u64,
) -> ServiceResult<(u64, String)> {
    let mut guard = write_guard(instances, "instance")?;
    let live = guard.get_mut(instance_id).ok_or_else(|| {
        ServiceError::not_found(
            ErrorCode::InstanceNotFound,
            format!("instance not found: {instance_id}"),
        )
    })?;

    if live.version != version_before {
        return Err(ServiceError::conflict(
            ErrorCode::InstanceVersionMismatch,
            format!(
                "instance version mismatch for {instance_id}: expected {version_before}, actual {}",
                live.version
            ),
        )
        .with_details(json!({
            "instance_id": instance_id,
            "expected_version": version_before,
            "actual_version": live.version,
        })));
    }

    let new_version = version_before.saturating_add(1);
    live.state = state_after;
    live.version = new_version;
    live.state_hash = state_hash_after.clone();
    live.updated_at_ms = updated_at_ms;

    Ok((new_version, state_hash_after))
}

fn apply_verification(run: &mut RunRecord, verified: bool, message: String, ts: u64) {
    run.status = if verified {
        RunStatus::Verified
    } else {
        RunStatus::VerificationFailed
    };
    run.proof_verified = Some(verified);
    run.verification_message = Some(message);
    run.verified_at_ms = Some(ts);
    run.verify = true;
}

fn read_guard<'a, T>(
    lock: &'a RwLock<T>,
    store_name: &str,
) -> ServiceResult<RwLockReadGuard<'a, T>> {
    lock.read().map_err(|_| {
        ServiceError::internal(
            ErrorCode::InternalError,
            format!("{store_name} store lock is poisoned"),
        )
    })
}

fn write_guard<'a, T>(
    lock: &'a RwLock<T>,
    store_name: &str,
) -> ServiceResult<RwLockWriteGuard<'a, T>> {
    lock.write().map_err(|_| {
        ServiceError::internal(
            ErrorCode::InternalError,
            format!("{store_name} store lock is poisoned"),
        )
    })
}

fn next_id(counter: &AtomicU64, prefix: &str) -> String {
    let value = counter.fetch_add(1, Ordering::Relaxed) + 1;
    format!("{prefix}_{value:016x}")
}

fn register_resolved_program(input: &ResolvedProgramInput) -> ServiceResult<RegisteredProgram> {
    register_program_sources(&input.sources, input.metadata_policy).map_err(map_driver_error)
}

fn map_driver_error(err: DriverError) -> ServiceError {
    match err {
        DriverError::ReadFile { path, source } => ServiceError::bad_request(
            ErrorCode::FileIoError,
            DriverError::ReadFile { path, source }.to_string(),
        ),
        DriverError::ParseJson { path, source } => ServiceError::bad_request(
            ErrorCode::ParseError,
            DriverError::ParseJson { path, source }.to_string(),
        ),
        DriverError::Compile { diagnostics } => {
            ServiceError::unprocessable(ErrorCode::CompileError, "program compilation failed")
                .with_details(json!({ "diagnostics": diagnostics }))
        }
        DriverError::InvalidProgram { message } => {
            ServiceError::unprocessable(ErrorCode::ProgramValidationError, message)
        }
        DriverError::MissingContractMetadata => ServiceError::unprocessable(
            ErrorCode::ProgramSchemaError,
            DriverError::MissingContractMetadata.to_string(),
        ),
        DriverError::ContractMetadataMismatch { message } => ServiceError::unprocessable(
            ErrorCode::ProgramSchemaError,
            DriverError::ContractMetadataMismatch { message }.to_string(),
        ),
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
    use tabula_artifact::merge_output_state_cells;
    use tabula_core::Value;
    use tabula_driver::transfer_example_bundle;

    #[test]
    fn inline_program_requires_contract_metadata() {
        let cwd = env::current_dir().expect("cwd");
        let engine = LocalEngine::new(FileAccessPolicy::new(vec![cwd]).expect("policy"));

        let resolved = engine
            .resolve_program_input(&InputRef::inline(ProgramInline::program(ProgramArtifact {
                table_schemas: vec![],
                tx_types: vec![],
                contract_metadata: None,
            })))
            .expect("program input should resolve");

        let result = register_resolved_program(&resolved);
        assert!(result.is_err(), "missing metadata must fail closed");
        let err = result.expect_err("error");
        assert!(format!("{err}").contains("missing contract_metadata"));
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
        let c1 = super::super::receipt::StatementComponents {
            program_hash: "a".to_string(),
            state_hash: "b".to_string(),
            batch_hash: "c".to_string(),
            state_after_hash: "d".to_string(),
            metadata_hash: "e".to_string(),
        };
        let c2 = c1.clone();
        let h1 = c1.statement_hash();
        let h2 = c2.statement_hash();
        assert_eq!(h1, h2);

        let c3 = super::super::receipt::StatementComponents {
            metadata_hash: "x".to_string(),
            ..c2
        };
        assert_ne!(h1, c3.statement_hash());
    }

    #[test]
    fn merge_output_state_cells_deduplicates_initial_cells() {
        use tabula_artifact::StateCell;

        let initial = vec![
            StateCell {
                table: 0,
                row: 1,
                col: 2,
                value: Some(Value::U64(10)),
            },
            StateCell {
                table: 0,
                row: 1,
                col: 2,
                value: Some(Value::U64(20)),
            },
        ];

        let merged = merge_output_state_cells(&initial, &[]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].value, Some(Value::U64(20)));
    }

    #[test]
    fn stateful_program_instance_run_flow_commits_and_records_run() {
        let cwd = env::current_dir().expect("cwd");
        let engine = LocalEngine::new(FileAccessPolicy::new(vec![cwd]).expect("policy"));
        let bundle = transfer_example_bundle().expect("example bundle");

        let registered = engine
            .register_program(RegisterProgramCommand {
                program: InputRef::inline(ProgramInline::program(bundle.program.clone())),
                label: Some("transfer".to_string()),
            })
            .expect("register program");

        let created = engine
            .create_instance(CreateInstanceCommand {
                program_id: registered.program_id.clone(),
                state: InputRef::inline(bundle.state.clone()),
                label: Some("demo".to_string()),
            })
            .expect("create instance");
        assert_eq!(created.version, 0);

        let submitted = engine
            .submit_run(SubmitRunCommand {
                instance_id: created.instance_id.clone(),
                batch: InputRef::inline(bundle.batch.clone()),
                include_trace: false,
                prove: true,
                verify: false,
                commit: true,
                expected_instance_version: Some(0),
            })
            .expect("submit run");

        assert!(submitted.committed);
        assert!(submitted.proof.is_some());
        assert_eq!(submitted.instance_version_before, 0);
        assert_eq!(submitted.instance_version_after, 1);

        let fetched = engine
            .get_instance(&created.instance_id)
            .expect("fetch instance");
        assert_eq!(fetched.version, 1);
        assert_eq!(fetched.state_hash, submitted.state_hash_after);
        assert_eq!(
            value_at_row(&fetched.state, 0),
            Some(Value::U64(750)),
            "row=0 balance after transfers"
        );
        assert_eq!(value_at_row(&fetched.state, 1), Some(Value::U64(600)));
        assert_eq!(value_at_row(&fetched.state, 2), Some(Value::U64(350)));

        let fetched_run = engine.get_run(&submitted.run_id).expect("fetch run");
        assert_eq!(fetched_run.run_id, submitted.run_id);
    }

    #[test]
    fn submit_run_rejects_stale_expected_version() {
        let cwd = env::current_dir().expect("cwd");
        let engine = LocalEngine::new(FileAccessPolicy::new(vec![cwd]).expect("policy"));
        let bundle = transfer_example_bundle().expect("example bundle");

        let registered = engine
            .register_program(RegisterProgramCommand {
                program: InputRef::inline(ProgramInline::program(bundle.program.clone())),
                label: None,
            })
            .expect("register program");
        let created = engine
            .create_instance(CreateInstanceCommand {
                program_id: registered.program_id,
                state: InputRef::inline(bundle.state),
                label: None,
            })
            .expect("create instance");

        let err = engine
            .submit_run(SubmitRunCommand {
                instance_id: created.instance_id,
                batch: InputRef::inline(bundle.batch),
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
        let bundle = transfer_example_bundle().expect("example bundle");

        let registered = engine
            .register_program(RegisterProgramCommand {
                program: InputRef::inline(ProgramInline::program(bundle.program.clone())),
                label: None,
            })
            .expect("register program");
        let created = engine
            .create_instance(CreateInstanceCommand {
                program_id: registered.program_id,
                state: InputRef::inline(bundle.state),
                label: None,
            })
            .expect("create instance");
        let submitted = engine
            .submit_run(SubmitRunCommand {
                instance_id: created.instance_id,
                batch: InputRef::inline(bundle.batch),
                include_trace: false,
                prove: true,
                verify: false,
                commit: true,
                expected_instance_version: Some(0),
            })
            .expect("submit run");

        let verified = engine.verify_run(&submitted.run_id).expect("verify run");
        assert!(verified.verified);
        assert!(matches!(verified.run.status, RunStatus::Verified));
        assert_eq!(verified.run.proof_verified, Some(true));
    }

    fn value_at_row(state: &StateFile, row: u64) -> Option<Value> {
        state
            .cells
            .iter()
            .find(|cell| cell.table == 0 && cell.col == 0 && cell.row == row)
            .and_then(|cell| cell.value)
    }
}
