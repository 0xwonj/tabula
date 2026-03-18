//! Batch executor: iterates transactions, orchestrates interpretation
//! with per-tx rollback on failure.

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_core::traits::{Hasher, NoncePolicy, SigVerifier, StateSnapshot, StaticTableProvider};
use tabula_core::{Batch, BatchResult, TxResult, Value};

use tabula_ir::{ParamDef, Program};

use crate::interpreter;
use crate::overlay::Overlay;
use crate::precompile::PrecompileRegistry;
use crate::property::{CommittedStateProvider, PropertyQueryRegistry};

/// Validate transaction parameters against the schema definition.
fn validate_params(params: &[Value], schema: &[ParamDef]) -> Result<(), TabulaError> {
    if params.len() != schema.len() {
        return Err(TabulaError::ParamSchemaMismatch(format!(
            "expected {} params, got {}",
            schema.len(),
            params.len()
        )));
    }
    for (i, (param, def)) in params.iter().zip(schema.iter()).enumerate() {
        if !param.matches_type(def.value_type) {
            return Err(TabulaError::ParamSchemaMismatch(format!(
                "param {i}: expected {}, got {}",
                def.value_type,
                param.type_name()
            )));
        }
    }
    Ok(())
}

/// Pluggable trait implementations needed by the batch executor.
pub struct BatchEnv<'a> {
    /// Cryptographic hash function.
    pub hasher: &'a dyn Hasher,
    /// Signature verification.
    pub sig_verifier: &'a dyn SigVerifier,
    /// Nonce validation and advancement.
    pub nonce_policy: &'a dyn NoncePolicy,
    /// Static (read-only) table lookups.
    pub static_tables: &'a dyn StaticTableProvider,
    /// Optional precompile handlers for custom instructions.
    pub precompiles: Option<&'a PrecompileRegistry>,
    /// Optional committed state for PropertyRead instructions.
    pub committed_state: Option<&'a dyn CommittedStateProvider>,
    /// Property query registry for PropertyRead resolution.
    pub property_queries: &'a PropertyQueryRegistry,
}

/// Execute a batch of transactions against a state snapshot.
///
/// Returns a `BatchResult` containing the read set, write set, and per-tx results
/// (each carrying its own access trace and emitted events).
pub fn execute_batch<S: StateSnapshot>(
    batch: &Batch,
    program: &Program,
    snapshot: &S,
    env: &BatchEnv<'_>,
    initial_nonces: &BTreeMap<[u8; 32], u64>,
) -> Result<BatchResult, TabulaError> {
    let mut overlay = Overlay::new(snapshot);
    let mut txs: Vec<TxResult> = Vec::new();
    let mut nonces: BTreeMap<[u8; 32], u64> = initial_nonces.clone();

    let ctx = interpreter::ExecContext {
        hasher: env.hasher,
        static_tables: env.static_tables,
        schemas: program.schemas(),
        precompiles: env.precompiles,
        committed_state: env.committed_state,
        property_queries: env.property_queries,
    };

    for (tx_idx, tx) in batch.transactions.iter().enumerate() {
        overlay.set_tx_index(tx_idx as u32);
        // Resolve tx type
        let tx_def = match program.resolve(tx.tx_type) {
            Ok(def) => def,
            Err(e) => {
                txs.push(TxResult::Failed {
                    reason: e.to_string(),
                    partial_events: vec![],
                    failed_instruction: None,
                });
                continue;
            }
        };

        // Validate param count and types against schema
        if let Err(e) = validate_params(&tx.params, &tx_def.param_schema) {
            txs.push(TxResult::Failed {
                reason: e.to_string(),
                partial_events: vec![],
                failed_instruction: None,
            });
            continue;
        }

        // Verify signature (message excludes the signature field itself)
        let msg = tx.signable_bytes()?;
        if let Err(e) = env.sig_verifier.verify(&tx.sender, &msg, &tx.signature) {
            txs.push(TxResult::Failed {
                reason: e.to_string(),
                partial_events: vec![],
                failed_instruction: None,
            });
            continue;
        }

        // Verify nonce
        let current_nonce = *nonces.get(&tx.sender).unwrap_or(&0);
        if let Err(e) = env
            .nonce_policy
            .validate(&tx.sender, tx.nonce, current_nonce)
        {
            txs.push(TxResult::Failed {
                reason: e.to_string(),
                partial_events: vec![],
                failed_instruction: None,
            });
            continue;
        }

        // Checkpoint before execution
        let events_before = overlay.events_len();
        overlay.checkpoint();

        // Execute
        match interpreter::execute(&tx_def.body, &tx.params, &mut overlay, &ctx) {
            Ok(output) => {
                overlay.discard_checkpoint();
                let next = env.nonce_policy.next_nonce(&tx.sender, current_nonce);
                nonces.insert(tx.sender, next);
                let access_trace = overlay.events_since(events_before);
                txs.push(TxResult::Success {
                    emitted: output.emitted,
                    access_trace,
                    precompile_ios: output.precompile_ios,
                    property_reads: output.property_reads,
                });
            }
            Err(interp_err) => {
                let partial_events = overlay.events_since(events_before);
                overlay.rollback();
                txs.push(TxResult::Failed {
                    reason: interp_err.error.to_string(),
                    partial_events,
                    failed_instruction: Some(interp_err.instruction_index),
                });
            }
        }
    }

    let result = overlay.into_result();
    Ok(BatchResult {
        read_set_old: result.read_set_old,
        write_set_final: result.write_set_final,
        txs,
    })
}
