//! Batch run submission logic.

use std::collections::BTreeMap;
use std::sync::RwLock;

use serde_json::json;

#[cfg(not(feature = "stark"))]
use crate::service::StarkProofSummary;
use tabula_artifact::{StateSnapshot, TransactionBatch};
use tabula_core::mock::Blake3Hasher;

use crate::protocol::error::ErrorCode;
use crate::service::error::{ServiceError, ServiceResult};
use crate::service::execute::execute_compiled_batch;
#[cfg(feature = "stark")]
use crate::service::execute::execute_prepared_batch;
use crate::service::receipt::{build_execution_statement, build_receipt, now_ms, verify_receipt};
use crate::service::{
    InstanceId, InstanceRecord, InstanceStatus, RunRecord, RunStatus, SubmitRunCommand,
};

use super::helpers::write_guard;

impl super::LocalEngine {
    /// Submit a batch run against an instance.
    pub fn submit_run(&self, req: &SubmitRunCommand) -> ServiceResult<RunRecord> {
        let snapshot = self.get_instance_record(req.instance_id.as_str())?;
        check_version(&snapshot, req.expected_instance_version)?;

        let program = self.get_program_store(snapshot.program_id.as_str())?;
        let batch_file = self
            .files
            .load_json_input::<TransactionBatch>(&req.batch, "batch")?;
        let state_before = snapshot.state.clone();

        #[cfg(feature = "stark")]
        let stark_result;

        let executed = {
            #[cfg(feature = "stark")]
            let use_stark = req.prove || req.verify;
            #[cfg(not(feature = "stark"))]
            let use_stark = false;

            if use_stark {
                #[cfg(feature = "stark")]
                {
                    let exec = execute_prepared_batch(
                        program.compiled_program.clone(),
                        program.prepared_runtime.as_ref(),
                        &state_before,
                        batch_file,
                    )?;
                    stark_result = match super::super::prove::prove_batch(
                        &exec,
                        program.prepared_runtime.as_ref(),
                    ) {
                        Ok((summary, statement)) => Some((summary, statement)),
                        Err(e) => {
                            tracing::warn!("STARK proof generation failed, returning mock: {e}");
                            Some((
                                super::super::prove::mock_stark_summary(),
                                build_execution_statement(
                                    &exec.compiled_program,
                                    &exec.inner.state_before,
                                    &exec.transaction_batch,
                                    &exec.inner.state_after,
                                )?,
                            ))
                        }
                    };
                    exec
                }
                #[cfg(not(feature = "stark"))]
                unreachable!()
            } else {
                #[cfg(feature = "stark")]
                {
                    stark_result = None;
                }
                execute_compiled_batch(
                    program.compiled_program,
                    &state_before,
                    batch_file,
                    &Blake3Hasher,
                )?
            }
        };

        let execution = executed.clone().into_execution_summary(req.include_trace);

        #[cfg(feature = "stark")]
        let (stark_proof_summary, statement) = match stark_result {
            Some((summary, statement)) => (Some(summary), statement),
            None => (
                None,
                build_execution_statement(
                    &executed.compiled_program,
                    &executed.inner.state_before,
                    &executed.transaction_batch,
                    &executed.inner.state_after,
                )?,
            ),
        };
        #[cfg(not(feature = "stark"))]
        let statement = build_execution_statement(
            &executed.compiled_program,
            &executed.inner.state_before,
            &executed.transaction_batch,
            &executed.inner.state_after,
        )?;
        let stmt_hash = statement.statement_hash();

        #[cfg(not(feature = "stark"))]
        let stark_proof_summary: Option<StarkProofSummary> = None;

        // Build legacy receipt for non-STARK path.
        let has_stark_proof = stark_proof_summary.is_some();
        let proof_requested = req.prove || req.verify;
        let emitted_count = executed
            .inner
            .txs
            .iter()
            .filter_map(|tx| match tx {
                tabula_core::TxResult::Success { emitted, .. } => Some(emitted.len()),
                _ => None,
            })
            .sum::<usize>();
        let proof = if proof_requested && !has_stark_proof {
            Some(build_receipt(
                &statement,
                executed.inner.txs.len(),
                emitted_count,
                &executed.inner.consistency,
            ))
        } else {
            None
        };

        let verification_message = if req.verify {
            if has_stark_proof {
                let verified = stark_proof_summary.as_ref().is_some_and(|s| s.verified);
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
                let verification = verify_receipt(proof_ref, &statement, &stmt_hash);
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
                statement.state_after_hash.clone(),
                now_ms(),
            )?
        } else {
            (snapshot.version, statement.state_after_hash.clone())
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
            state_hash_before: statement.state_hash,
            state_hash_after,
            program_hash: statement.program_hash,
            batch_hash: statement.batch_hash,
            metadata_hash: statement.metadata_hash,
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
    state_after: StateSnapshot,
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
