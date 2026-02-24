//! Run verification logic.

use crate::protocol::error::ErrorCode;
use crate::service::error::{ServiceError, ServiceResult};
use crate::service::receipt::verify_receipt;
use crate::service::receipt::{self, now_ms};
use crate::service::types::*;

use super::helpers::write_guard;

impl super::LocalEngine {
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

        let components = receipt::StatementComponents {
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

pub(super) fn apply_verification(run: &mut RunRecord, verified: bool, message: String, ts: u64) {
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
