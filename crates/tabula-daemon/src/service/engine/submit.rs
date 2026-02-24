//! Batch run submission logic.

use std::collections::BTreeMap;
use std::sync::RwLock;

use serde_json::json;

use tabula_artifact::{
    BatchFile, InstanceId, InstanceRecord, InstanceStatus, RunRecord, RunStatus, StateFile,
    SubmitRunCommand,
};
use tabula_core::mock::MockHasher;

use crate::protocol::error::ErrorCode;
use crate::service::error::{ServiceError, ServiceResult};
use crate::service::execute::execute_registered_batch;
use crate::service::receipt::{build_receipt, now_ms, statement_components, verify_receipt};

use super::helpers::write_guard;

impl super::LocalEngine {
    /// Submit a batch run against an instance.
    pub fn submit_run(&self, req: SubmitRunCommand) -> ServiceResult<RunRecord> {
        let snapshot = self.get_instance_record(req.instance_id.as_str())?;
        check_version(&snapshot, req.expected_instance_version)?;

        let program = self.get_program_store(snapshot.program_id.as_str())?;
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
                    stark_proof_summary =
                        match super::super::prove::prove_batch(&exec, &program.registered) {
                            Ok(summary) => Some(summary),
                            Err(e) => {
                                tracing::warn!(
                                    "STARK proof generation failed, returning mock: {e}"
                                );
                                Some(super::super::prove::mock_stark_summary())
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

        let execution = executed.clone().into_execution_summary(req.include_trace);

        let components = statement_components(
            &executed.artifact,
            &executed.inner.state_before,
            &executed.batch_file,
            &executed.inner.state_after,
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
                executed.inner.tx_outcomes.len(),
                executed.inner.emitted.len(),
                &executed.inner.consistency,
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
        let mut run = RunRecord {
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

        if req.verify {
            super::verify::apply_verification(
                &mut run,
                true,
                verification_message.expect("guarded above"),
                ts,
            );
        }

        write_guard(&self.runs, "run")?.insert(run.run_id.clone(), run.clone());
        Ok(run)
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
            "instance_id": instance.instance_id.as_str(),
            "expected_version": expected,
            "actual_version": instance.version,
        })));
    }
    Ok(())
}

fn commit_instance(
    instances: &RwLock<BTreeMap<InstanceId, InstanceRecord>>,
    instance_id: &InstanceId,
    version_before: u64,
    state_after: StateFile,
    state_hash_after: String,
    updated_at_ms: u64,
) -> ServiceResult<(u64, String)> {
    let mut guard = write_guard(instances, "instance")?;
    let live = guard.get_mut(instance_id.as_str()).ok_or_else(|| {
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
            "instance_id": instance_id.as_str(),
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
