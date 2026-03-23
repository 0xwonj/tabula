//! Canonical batch executor.

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_core::traits::{Hasher, NoncePolicy, SigVerifier, StateView, StaticTableProvider};
use tabula_core::{Batch, PortableValue, TxTypeId};
use tabula_ir::ParamDef;
use tabula_types::{TypeRuntimeRegistry, TypedValue};

use crate::interpreter::{self, ExecContext};
use crate::journal::{
    ExecutionJournal, ExecutionStateSummary, FailedTxExecution, TxExecutionOutcome,
    TxJournalBuilder,
};
use crate::overlay::Overlay;
use crate::precompile::PrecompileRegistry;
use crate::property::{CommittedStateProvider, PropertyQueryRegistry};
use crate::resolved_program::{ResolvedExecutionProgram, ResolvedTxDefinition};

/// Validate transaction parameters against the schema definition.
fn validate_params(
    params: &[PortableValue],
    schema: &[ParamDef],
    type_runtimes: &TypeRuntimeRegistry,
) -> Result<Vec<TypedValue>, TabulaError> {
    if params.len() != schema.len() {
        return Err(TabulaError::ParamSchemaMismatch(format!(
            "expected {} params, got {}",
            schema.len(),
            params.len()
        )));
    }
    let mut decoded = Vec::with_capacity(params.len());
    for (i, (param, def)) in params.iter().zip(schema.iter()).enumerate() {
        if param.type_id() != def.type_id {
            return Err(TabulaError::ParamSchemaMismatch(format!(
                "param {i}: expected type_id {}, got {}",
                def.type_id.0,
                param.type_id().0,
            )));
        }
        decoded.push(type_runtimes.decode_portable(param)?);
    }
    Ok(decoded)
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
    /// Runtime type registry used for portable/typed boundary decoding.
    pub type_runtimes: &'a TypeRuntimeRegistry,
    /// Optional precompile handlers for custom instructions.
    pub precompiles: Option<&'a PrecompileRegistry>,
    /// Optional committed state for PropertyRead instructions.
    pub committed_state: Option<&'a dyn CommittedStateProvider>,
    /// Property query registry for PropertyRead resolution.
    pub property_queries: &'a PropertyQueryRegistry,
}

/// Canonical batch executor over the resolved execution contract.
pub(crate) struct BatchExecutor<'a, S: StateView> {
    batch: &'a Batch,
    program: &'a ResolvedExecutionProgram,
    snapshot: &'a S,
    env: &'a BatchEnv<'a>,
    initial_nonces: &'a BTreeMap<[u8; 32], u64>,
}

impl<'a, S: StateView> BatchExecutor<'a, S> {
    pub(crate) fn new(
        batch: &'a Batch,
        program: &'a ResolvedExecutionProgram,
        snapshot: &'a S,
        env: &'a BatchEnv<'a>,
        initial_nonces: &'a BTreeMap<[u8; 32], u64>,
    ) -> Self {
        Self {
            batch,
            program,
            snapshot,
            env,
            initial_nonces,
        }
    }

    pub(crate) fn execute(self) -> Result<ExecutionJournal, TabulaError> {
        let mut overlay = Overlay::new(self.snapshot, self.env.type_runtimes);
        let mut txs = Vec::with_capacity(self.batch.transactions.len());
        let mut nonces: BTreeMap<[u8; 32], u64> = self.initial_nonces.clone();
        let mut next_logical_time = 0;

        let ctx = ExecContext {
            hasher: self.env.hasher,
            static_tables: self.env.static_tables,
            type_runtimes: self.env.type_runtimes,
            execution_program: self.program,
            precompiles: self.env.precompiles,
            committed_state: self.env.committed_state,
            property_queries: self.env.property_queries,
        };

        for (tx_idx, tx) in self.batch.transactions.iter().enumerate() {
            let tx_index = tx_idx as u32;
            let tx_def = match self.resolve_tx_definition(tx.tx_type) {
                Ok(def) => def,
                Err(error) => {
                    txs.push(TxExecutionOutcome::Failed(pre_execution_failure(
                        tx_index, &error,
                    )));
                    continue;
                }
            };

            let decoded_params =
                match validate_params(&tx.params, &tx_def.param_schema, self.env.type_runtimes) {
                    Ok(decoded) => decoded,
                    Err(error) => {
                        txs.push(TxExecutionOutcome::Failed(pre_execution_failure(
                            tx_index, &error,
                        )));
                        continue;
                    }
                };

            let msg = tx.signable_bytes()?;
            if let Err(error) = self
                .env
                .sig_verifier
                .verify(&tx.sender, &msg, &tx.signature)
            {
                txs.push(TxExecutionOutcome::Failed(pre_execution_failure(
                    tx_index, &error,
                )));
                continue;
            }

            let current_nonce = *nonces.get(&tx.sender).unwrap_or(&0);
            if let Err(error) = self
                .env
                .nonce_policy
                .validate(&tx.sender, tx.nonce, current_nonce)
            {
                txs.push(TxExecutionOutcome::Failed(pre_execution_failure(
                    tx_index, &error,
                )));
                continue;
            }

            overlay.checkpoint();
            let mut journal = TxJournalBuilder::new(tx_index, next_logical_time);

            match interpreter::execute_with_journal(
                tx_index,
                &tx_def.body,
                &decoded_params,
                &mut overlay,
                &ctx,
                &mut journal,
            ) {
                Ok(()) => {
                    overlay.discard_checkpoint();
                    let next = self.env.nonce_policy.next_nonce(&tx.sender, current_nonce);
                    nonces.insert(tx.sender, next);
                    // The canonical batch logical clock advances only for
                    // successful semantic access effects. Failed transaction
                    // access observations are diagnostic-only and therefore do
                    // not consume canonical time.
                    next_logical_time += journal.access_effect_count() as u64;
                    txs.push(TxExecutionOutcome::Success(journal.into_success()));
                }
                Err(error) => {
                    overlay.rollback();
                    txs.push(TxExecutionOutcome::Failed(journal.into_failure(
                        error.error.to_string(),
                        Some(error.instruction_index),
                    )));
                }
            }
        }

        let result = overlay.into_result()?;
        Ok(ExecutionJournal {
            state_summary: ExecutionStateSummary {
                read_set_old: result.read_set_old,
                write_set_final: result.write_set_final,
            },
            txs,
        })
    }

    fn resolve_tx_definition(
        &self,
        tx_type: TxTypeId,
    ) -> Result<&ResolvedTxDefinition, TabulaError> {
        self.program.tx_definition(tx_type)
    }
}

fn pre_execution_failure(tx_index: u32, error: &TabulaError) -> FailedTxExecution {
    FailedTxExecution {
        tx_index,
        reason: error.to_string(),
        partial_accesses: vec![],
        failed_instruction: None,
    }
}

/// Execute a batch against the canonical resolved execution contract.
pub fn execute_batch<S: StateView>(
    batch: &Batch,
    program: &ResolvedExecutionProgram,
    snapshot: &S,
    env: &BatchEnv<'_>,
    initial_nonces: &BTreeMap<[u8; 32], u64>,
) -> Result<ExecutionJournal, TabulaError> {
    BatchExecutor::new(batch, program, snapshot, env, initial_nonces).execute()
}
