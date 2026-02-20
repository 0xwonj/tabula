use std::collections::BTreeMap;
use std::path::Path;

use serde_json::json;

use tabula_contract::ContractMetadataEnvelope;
use tabula_core::mock::{
    InMemoryState, InMemoryStaticTables, MockHasher, MockSigVerifier, SequentialNonce,
};
use tabula_core::{Batch, CellKey, Value};
use tabula_driver::{RegisteredProgram, register_program};
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_executor::consistency::check_consistency_status;

use crate::kernel::domain::{
    BatchFile, Capabilities, CapabilityClientKind, CapabilityInputMode, CheckCommand, CheckResult,
    CompileCommand, CompileResult, ExecuteCommand, ExecuteResult, InputRef, ProgramFile,
    ProgramInline, ProgramInputRef, StateCell, StateFile,
};
use crate::kernel::io::FileAccessPolicy;
use crate::protocol::error::{ApiError, ApiResult, ErrorCode};

/// Abstract engine boundary so daemon remains extensible and testable.
pub trait KernelEngine: Send + Sync {
    fn capabilities(&self) -> Capabilities;
    fn check(&self, req: CheckCommand) -> ApiResult<CheckResult>;
    fn compile(&self, req: CompileCommand) -> ApiResult<CompileResult>;
    fn execute(&self, req: ExecuteCommand) -> ApiResult<ExecuteResult>;

    fn prove_stub(&self) -> ApiResult<serde_json::Value> {
        Err(ApiError::not_implemented(
            ErrorCode::ProofNotAvailable,
            "proof generation is not available yet",
        ))
    }

    fn verify_stub(&self) -> ApiResult<serde_json::Value> {
        Err(ApiError::not_implemented(
            ErrorCode::ProofNotAvailable,
            "proof verification is not available yet",
        ))
    }
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
            prove: false,
            verify: false,
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
        let loaded = self.load_program_sources(&req.program)?;
        let artifact = register_loaded_program(&loaded)?;
        let state_file = self.files.load_json_input(&req.state, "state")?;
        let batch_file = self
            .files
            .load_json_input::<BatchFile>(&req.batch, "batch")?;

        let mut state = InMemoryState::new();
        for cell in &state_file.cells {
            let (key, value) = cell
                .to_cell_pair()
                .map_err(|e| ApiError::bad_request(ErrorCode::InvalidStateCell, e))?;
            state.set(key, value);
        }

        let transactions: Vec<_> = batch_file
            .transactions
            .iter()
            .map(|t| {
                t.to_transaction()
                    .map_err(|e| ApiError::bad_request(ErrorCode::InvalidBatchTx, e))
            })
            .collect::<Result<_, _>>()?;
        let batch = Batch { transactions };

        let st = InMemoryStaticTables::new();
        let env = BatchEnv {
            hasher: &MockHasher,
            sig_verifier: &MockSigVerifier,
            nonce_policy: &SequentialNonce,
            static_tables: &st,
        };

        let result = execute_batch(&batch, &artifact.program, &state, &env, &BTreeMap::new())
            .map_err(|e| ApiError::unprocessable(ErrorCode::ExecutionError, e.to_string()))?;

        let consistency = check_consistency_status(&result.events, &result.read_set_old);

        let state_after = StateFile {
            cells: merge_output_state_cells(&state_file.cells, &result.write_set_final),
        };

        let trace = if req.include_trace {
            Some(result.events.clone())
        } else {
            None
        };

        Ok(ExecuteResult {
            tx_outcomes: result.tx_outcomes,
            read_set: result
                .read_set_old
                .iter()
                .map(|(k, v)| StateCell::from_cell_pair(k, v))
                .collect(),
            write_set: result
                .write_set_final
                .iter()
                .map(|(k, v)| StateCell::from_cell_pair(k, v))
                .collect(),
            emitted: result.emitted,
            consistency,
            trace,
            state_after,
        })
    }
}

struct LoadedProgram {
    schemas: Vec<tabula_core::TableSchema>,
    tx_types: Vec<tabula_ir::TxTypeDef>,
    contract_metadata: Option<ContractMetadataEnvelope>,
}

impl TabulaEngine {
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
        let err = result.err().expect("error");
        assert!(format!("{err:?}").contains("missing contract_metadata"));
    }
}
