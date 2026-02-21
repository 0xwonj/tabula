use std::collections::BTreeMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use tabula_contract::ContractMetadataEnvelope;
use tabula_core::mock::{
    InMemoryState, InMemoryStaticTables, MockHasher, MockSigVerifier, SequentialNonce,
};
use tabula_core::{Batch, CellKey, ExecutionConsistencyStatus, Value};
use tabula_driver::{RegisteredProgram, register_program};
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_executor::consistency::check_consistency_status;

use crate::kernel::domain::{
    BatchFile, Capabilities, CapabilityClientKind, CapabilityInputMode, CheckCommand, CheckResult,
    CompileCommand, CompileResult, ExecuteCommand, ExecuteResult, ExecutionReceipt, InputRef,
    ProgramFile, ProgramInline, ProgramInputRef, ProveCommand, ProveResult, StateCell, StateFile,
    VerifyCommand, VerifyExpectedCommand, VerifyResult,
};
use crate::kernel::io::FileAccessPolicy;
use crate::protocol::error::{ApiError, ApiResult, ErrorCode};

const RECEIPT_VERSION: u32 = 1;
const RECEIPT_SCHEME: &str = "execution_receipt_v1";
const JSON_HASH_DOMAIN: &[u8] = b"tabula.daemon.json_hash.v1";
const STATEMENT_HASH_DOMAIN: &[u8] = b"tabula.daemon.statement_hash.v1";

/// Abstract engine boundary so daemon remains extensible and testable.
pub trait KernelEngine: Send + Sync {
    fn capabilities(&self) -> Capabilities;
    fn check(&self, req: CheckCommand) -> ApiResult<CheckResult>;
    fn compile(&self, req: CompileCommand) -> ApiResult<CompileResult>;
    fn execute(&self, req: ExecuteCommand) -> ApiResult<ExecuteResult>;
    fn prove(&self, req: ProveCommand) -> ApiResult<ProveResult>;
    fn verify(&self, req: VerifyCommand) -> ApiResult<VerifyResult>;
}

/// Default engine implementation backed by existing Tabula crates.
#[derive(Debug, Clone)]
pub struct TabulaEngine {
    files: FileAccessPolicy,
}

impl TabulaEngine {
    pub fn new(files: FileAccessPolicy) -> Self {
        Self { files }
    }
}

impl KernelEngine for TabulaEngine {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            service_role: "local_control_plane",
            clients: vec![
                CapabilityClientKind::WebIde,
                CapabilityClientKind::Cli,
                CapabilityClientKind::Automation,
            ],
            compile: true,
            check: true,
            execute: true,
            prove: true,
            verify: true,
            input_modes: vec![
                CapabilityInputMode::Inline,
                CapabilityInputMode::File,
                CapabilityInputMode::Artifact,
            ],
        }
    }

    fn check(&self, req: CheckCommand) -> ApiResult<CheckResult> {
        let loaded = self.load_program_sources(&req.program)?;
        register_loaded_program(&loaded)?;

        Ok(CheckResult {
            table_count: loaded.schemas.len(),
            tx_type_count: loaded.tx_types.len(),
        })
    }

    fn compile(&self, req: CompileCommand) -> ApiResult<CompileResult> {
        let loaded = self.load_program_sources(&req.program)?;
        let artifact = register_loaded_program(&loaded)?;

        Ok(CompileResult {
            table_count: loaded.schemas.len(),
            tx_type_count: loaded.tx_types.len(),
            program: ProgramFile {
                table_schemas: artifact.table_schemas,
                tx_types: artifact.tx_types,
                contract_metadata: Some(artifact.metadata_envelope),
            },
        })
    }

    fn execute(&self, req: ExecuteCommand) -> ApiResult<ExecuteResult> {
        let executed = self.execute_internal(req.program, req.state, req.batch)?;
        Ok(executed.into_execute_result(req.include_trace))
    }

    fn prove(&self, req: ProveCommand) -> ApiResult<ProveResult> {
        let executed = self.execute_internal(req.program, req.state, req.batch)?;
        let execution = executed.clone().into_execute_result(req.include_trace);
        let proof = build_receipt(&executed)?;
        Ok(ProveResult { proof, execution })
    }

    fn verify(&self, req: VerifyCommand) -> ApiResult<VerifyResult> {
        let proof: ExecutionReceipt = serde_json::from_value(req.proof).map_err(|e| {
            ApiError::bad_request(ErrorCode::ParseError, format!("invalid proof payload: {e}"))
        })?;

        let mut verified = true;
        let mut message = "receipt verified".to_string();

        if proof.version != RECEIPT_VERSION || proof.scheme != RECEIPT_SCHEME {
            verified = false;
            message = format!(
                "unsupported receipt format: expected version={}, scheme={}, got version={}, scheme={}",
                RECEIPT_VERSION, RECEIPT_SCHEME, proof.version, proof.scheme
            );
        }

        let recomputed_statement_hash = statement_hash(
            &proof.program_hash,
            &proof.state_hash,
            &proof.batch_hash,
            &proof.state_after_hash,
            &proof.metadata_hash,
        );
        if verified && proof.statement_hash != recomputed_statement_hash {
            verified = false;
            message = "receipt statement hash mismatch".to_string();
        }

        let mut expected_statement_hash = None;
        let mut matched_expected = None;
        if let Some(expected) = req.expected {
            let expected_hash = self.expected_statement_hash(expected)?;
            expected_statement_hash = Some(expected_hash.clone());
            let matched = proof.statement_hash == expected_hash;
            matched_expected = Some(matched);
            if verified && !matched {
                verified = false;
                message =
                    "proof does not match expected program/state/batch/state_after".to_string();
            }
        }

        Ok(VerifyResult {
            verified,
            message,
            statement_hash: Some(proof.statement_hash.clone()),
            expected_statement_hash,
            matched_expected,
            proof: Some(proof),
        })
    }
}

#[derive(Debug, Clone)]
struct LoadedProgram {
    schemas: Vec<tabula_core::TableSchema>,
    tx_types: Vec<tabula_ir::TxTypeDef>,
    contract_metadata: Option<ContractMetadataEnvelope>,
}

#[derive(Debug, Clone)]
struct ExecutedBatch {
    artifact: RegisteredProgram,
    state_file: StateFile,
    batch_file: BatchFile,
    tx_outcomes: Vec<tabula_core::TxOutcome>,
    read_set: Vec<(CellKey, Option<Value>)>,
    write_set: Vec<(CellKey, Option<Value>)>,
    emitted: Vec<tabula_core::EmittedEvent>,
    events: Vec<tabula_core::ExecutionEvent>,
    consistency: ExecutionConsistencyStatus,
    state_after: StateFile,
}

impl ExecutedBatch {
    fn into_execute_result(self, include_trace: bool) -> ExecuteResult {
        let trace = if include_trace {
            Some(self.events.clone())
        } else {
            None
        };

        ExecuteResult {
            tx_outcomes: self.tx_outcomes,
            read_set: self
                .read_set
                .iter()
                .map(|(k, v)| StateCell::from_cell_pair(k, v))
                .collect(),
            write_set: self
                .write_set
                .iter()
                .map(|(k, v)| StateCell::from_cell_pair(k, v))
                .collect(),
            emitted: self.emitted,
            consistency: self.consistency,
            trace,
            state_after: self.state_after,
        }
    }
}

impl TabulaEngine {
    fn execute_internal(
        &self,
        program: ProgramInputRef,
        state: InputRef<StateFile>,
        batch: InputRef<BatchFile>,
    ) -> ApiResult<ExecutedBatch> {
        let loaded = self.load_program_sources(&program)?;
        let artifact = register_loaded_program(&loaded)?;

        let state_file = self.files.load_json_input(&state, "state")?;
        let batch_file = self.files.load_json_input::<BatchFile>(&batch, "batch")?;

        let mut state_store = InMemoryState::new();
        for cell in &state_file.cells {
            let (key, value) = cell
                .to_cell_pair()
                .map_err(|e| ApiError::bad_request(ErrorCode::InvalidStateCell, e))?;
            state_store.set(key, value);
        }

        let transactions: Vec<_> = batch_file
            .transactions
            .iter()
            .map(|t| {
                t.to_transaction()
                    .map_err(|e| ApiError::bad_request(ErrorCode::InvalidBatchTx, e))
            })
            .collect::<Result<_, _>>()?;
        let batch_value = Batch { transactions };

        let st = InMemoryStaticTables::new();
        let env = BatchEnv {
            hasher: &MockHasher,
            sig_verifier: &MockSigVerifier,
            nonce_policy: &SequentialNonce,
            static_tables: &st,
        };

        let result = execute_batch(
            &batch_value,
            &artifact.program,
            &state_store,
            &env,
            &BTreeMap::new(),
        )
        .map_err(|e| ApiError::unprocessable(ErrorCode::ExecutionError, e.to_string()))?;

        let consistency = check_consistency_status(&result.events, &result.read_set_old);
        let state_after = StateFile {
            cells: merge_output_state_cells(&state_file.cells, &result.write_set_final),
        };

        Ok(ExecutedBatch {
            artifact,
            state_file,
            batch_file,
            tx_outcomes: result.tx_outcomes,
            read_set: result.read_set_old,
            write_set: result.write_set_final,
            emitted: result.emitted,
            events: result.events,
            consistency,
            state_after,
        })
    }

    fn expected_statement_hash(&self, expected: VerifyExpectedCommand) -> ApiResult<String> {
        let loaded = self.load_program_sources(&expected.program)?;
        let artifact = register_loaded_program(&loaded)?;
        let state = self
            .files
            .load_json_input::<StateFile>(&expected.state, "state")?;
        let batch = self
            .files
            .load_json_input::<BatchFile>(&expected.batch, "batch")?;
        let state_after = self
            .files
            .load_json_input::<StateFile>(&expected.state_after, "state_after")?;

        let components = statement_components(&artifact, &state, &batch, &state_after)?;
        Ok(statement_hash(
            &components.program_hash,
            &components.state_hash,
            &components.batch_hash,
            &components.state_after_hash,
            &components.metadata_hash,
        ))
    }

    fn load_program_sources(&self, input: &ProgramInputRef) -> ApiResult<LoadedProgram> {
        match input {
            InputRef::Inline(inline) => match inline {
                ProgramInline::Source(source) => compile_program_source(source),
                ProgramInline::Program(pf) => {
                    if pf.contract_metadata.is_none() {
                        return Err(ApiError::unprocessable(
                            ErrorCode::ProgramSchemaError,
                            "compiled program JSON is missing contract_metadata; recompile with current driver",
                        ));
                    }
                    Ok(LoadedProgram {
                        schemas: pf.table_schemas.clone(),
                        tx_types: pf.tx_types.clone(),
                        contract_metadata: pf.contract_metadata.clone(),
                    })
                }
            },
            InputRef::File(file_path) => self.load_program_from_file(file_path),
            InputRef::Artifact(artifact_id) => Err(ApiError::not_implemented(
                ErrorCode::ArtifactInputNotAvailable,
                format!("artifact input is not available yet: {artifact_id}"),
            )),
        }
    }

    fn load_program_from_file(&self, path: &Path) -> ApiResult<LoadedProgram> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext == "tab" {
            let source = self.files.read_utf8_file(path, "program")?;
            compile_program_source(&source)
        } else {
            let pf: ProgramFile = self.files.read_json_file(path, "program")?;
            if pf.contract_metadata.is_none() {
                return Err(ApiError::unprocessable(
                    ErrorCode::ProgramSchemaError,
                    "compiled program JSON is missing contract_metadata; recompile with current driver",
                ));
            }
            Ok(LoadedProgram {
                schemas: pf.table_schemas,
                tx_types: pf.tx_types,
                contract_metadata: pf.contract_metadata,
            })
        }
    }
}

fn compile_program_source(source: &str) -> ApiResult<LoadedProgram> {
    match tabula_lang::compile(source) {
        Ok(compiled) => Ok(LoadedProgram {
            schemas: compiled.schemas,
            tx_types: compiled.tx_types,
            contract_metadata: None,
        }),
        Err(errors) => {
            let diagnostics: Vec<_> = errors
                .iter()
                .map(|err| {
                    let (line, col) = tabula_lang::span::line_col(source, err.span.start);
                    json!({
                        "kind": format!("{:?}", err.kind),
                        "message": err.message,
                        "span_start": err.span.start,
                        "span_end": err.span.end,
                        "line": line,
                        "col": col
                    })
                })
                .collect();

            Err(
                ApiError::unprocessable(ErrorCode::CompileError, "program compilation failed")
                    .with_details(json!({ "diagnostics": diagnostics })),
            )
        }
    }
}

fn register_loaded_program(loaded: &LoadedProgram) -> ApiResult<RegisteredProgram> {
    let artifact = register_program(&loaded.schemas, &loaded.tx_types).map_err(|e| {
        ApiError::unprocessable(
            ErrorCode::ProgramValidationError,
            format!("invalid program: {e}"),
        )
    })?;

    if let Some(meta) = &loaded.contract_metadata {
        artifact
            .compatibility_policy()
            .validate(meta)
            .map_err(|e| {
                ApiError::unprocessable(
                    ErrorCode::ProgramSchemaError,
                    format!("contract metadata mismatch: {e}"),
                )
            })?;
    }

    Ok(artifact)
}

#[derive(Debug, Clone)]
struct StatementComponents {
    program_hash: String,
    state_hash: String,
    batch_hash: String,
    state_after_hash: String,
    metadata_hash: String,
}

fn build_receipt(executed: &ExecutedBatch) -> ApiResult<ExecutionReceipt> {
    let components = statement_components(
        &executed.artifact,
        &executed.state_file,
        &executed.batch_file,
        &executed.state_after,
    )?;

    Ok(ExecutionReceipt {
        version: RECEIPT_VERSION,
        scheme: RECEIPT_SCHEME.to_string(),
        statement_hash: statement_hash(
            &components.program_hash,
            &components.state_hash,
            &components.batch_hash,
            &components.state_after_hash,
            &components.metadata_hash,
        ),
        program_hash: components.program_hash,
        state_hash: components.state_hash,
        batch_hash: components.batch_hash,
        state_after_hash: components.state_after_hash,
        metadata_hash: components.metadata_hash,
        generated_at_ms: now_ms(),
        tx_count: executed.tx_outcomes.len(),
        emitted_count: executed.emitted.len(),
        consistency: executed.consistency.clone(),
    })
}

fn statement_components(
    artifact: &RegisteredProgram,
    state: &StateFile,
    batch: &BatchFile,
    state_after: &StateFile,
) -> ApiResult<StatementComponents> {
    let program_file = ProgramFile {
        table_schemas: artifact.table_schemas.clone(),
        tx_types: artifact.tx_types.clone(),
        contract_metadata: Some(artifact.metadata_envelope.clone()),
    };

    let program_hash = hash_json("program", &program_file)?;
    let state_hash = hash_json("state", state)?;
    let batch_hash = hash_json("batch", batch)?;
    let state_after_hash = hash_json("state_after", state_after)?;
    let metadata_hash = bytes_to_hex(&artifact.metadata_envelope.canonical_hash());

    Ok(StatementComponents {
        program_hash,
        state_hash,
        batch_hash,
        state_after_hash,
        metadata_hash,
    })
}

fn hash_json<T: Serialize>(label: &str, value: &T) -> ApiResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|e| {
        ApiError::internal(
            ErrorCode::InternalError,
            format!("failed to serialize {label} for hashing: {e}"),
        )
    })?;

    let mut hasher = Sha256::new();
    hasher.update(JSON_HASH_DOMAIN);
    hasher.update(label.as_bytes());
    hasher.update([0u8]);
    hasher.update(&bytes);
    Ok(bytes_to_hex(&hasher.finalize()))
}

fn statement_hash(
    program_hash: &str,
    state_hash: &str,
    batch_hash: &str,
    state_after_hash: &str,
    metadata_hash: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(STATEMENT_HASH_DOMAIN);
    hash_part(&mut hasher, b"program_hash", program_hash);
    hash_part(&mut hasher, b"state_hash", state_hash);
    hash_part(&mut hasher, b"batch_hash", batch_hash);
    hash_part(&mut hasher, b"state_after_hash", state_after_hash);
    hash_part(&mut hasher, b"metadata_hash", metadata_hash);
    bytes_to_hex(&hasher.finalize())
}

fn hash_part(hasher: &mut Sha256, label: &[u8], value: &str) {
    hasher.update(label);
    hasher.update([0u8]);
    hasher.update(value.as_bytes());
    hasher.update([0xffu8]);
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{:02x}", b);
    }
    out
}

fn merge_output_state_cells(
    initial_cells: &[StateCell],
    write_set_final: &[(CellKey, Option<Value>)],
) -> Vec<StateCell> {
    let mut merged: BTreeMap<(u32, u64, u16), Value> = BTreeMap::new();

    for cell in initial_cells {
        if let Some(value) = cell.value {
            merged.insert((cell.table, cell.row, cell.col), value);
        }
    }

    for (key, value) in write_set_final {
        let tuple_key = (key.table.0, key.row.0, key.col.0);
        match value {
            Some(v) => {
                merged.insert(tuple_key, *v);
            }
            None => {
                merged.remove(&tuple_key);
            }
        }
    }

    merged
        .into_iter()
        .map(|((table, row, col), value)| StateCell {
            table,
            row,
            col,
            value: Some(value),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    use tabula_core::Value;

    #[test]
    fn merge_output_state_cells_deduplicates_initial_cells() {
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
    fn inline_program_requires_contract_metadata() {
        let cwd = env::current_dir().expect("cwd");
        let engine = TabulaEngine::new(FileAccessPolicy::new(vec![cwd]).expect("policy"));

        let result =
            engine.load_program_sources(&InputRef::Inline(ProgramInline::Program(ProgramFile {
                table_schemas: vec![],
                tx_types: vec![],
                contract_metadata: None,
            })));
        assert!(result.is_err(), "missing metadata must fail closed");
        let err = result.expect_err("error");
        assert!(format!("{err:?}").contains("missing contract_metadata"));
    }

    #[test]
    fn statement_hash_is_stable() {
        let h1 = statement_hash("a", "b", "c", "d", "e");
        let h2 = statement_hash("a", "b", "c", "d", "e");
        assert_eq!(h1, h2);
        assert_ne!(h1, statement_hash("a", "b", "c", "d", "x"));
    }
}
