use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::de::DeserializeOwned;
use serde_json::json;

use tabula_core::mock::{
    InMemoryState, InMemoryStaticTables, MockHasher, MockSigVerifier, SequentialNonce,
};
use tabula_core::{Batch, CellKey, ColId, TableId, TableSchema, Value};
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_executor::consistency::check_consistency;
use tabula_ir::{Instruction, Program, TxTypeDef};

use crate::error::ApiError;
use crate::model::{
    CheckRequest, CheckResponse, CompileRequest, CompileResponse, ExecuteRequest, ExecuteResponse,
    InputRef, ProgramFile, ProgramInline, ProgramInputRef, StateCell, StateFile,
};

pub fn handle_check(req: CheckRequest) -> Result<CheckResponse, ApiError> {
    let loaded = load_program_sources(&req.program)?;
    register_program(&loaded.schemas, &loaded.tx_types)?;

    Ok(CheckResponse {
        ok: true,
        table_count: loaded.schemas.len(),
        tx_type_count: loaded.tx_types.len(),
    })
}

pub fn handle_compile(req: CompileRequest) -> Result<CompileResponse, ApiError> {
    let loaded = load_program_sources(&req.program)?;
    register_program(&loaded.schemas, &loaded.tx_types)?;

    Ok(CompileResponse {
        ok: true,
        table_count: loaded.schemas.len(),
        tx_type_count: loaded.tx_types.len(),
        program: ProgramFile {
            table_schemas: loaded.schemas,
            tx_types: loaded.tx_types,
        },
    })
}

pub fn handle_execute(req: ExecuteRequest) -> Result<ExecuteResponse, ApiError> {
    let loaded = load_program_sources(&req.program)?;
    let program = register_program(&loaded.schemas, &loaded.tx_types)?;
    let state_file = load_json_input(&req.state, "state")?;
    let batch_file = load_json_input(&req.batch, "batch")?;

    let mut state = InMemoryState::new();
    for cell in &state_file.cells {
        let (key, value) = cell
            .to_cell_pair()
            .map_err(|e| ApiError::bad_request("INVALID_STATE_CELL", e))?;
        state.set(key, value);
    }

    let transactions: Vec<_> = batch_file
        .transactions
        .iter()
        .map(|t| {
            t.to_transaction()
                .map_err(|e| ApiError::bad_request("INVALID_BATCH_TX", e))
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

    let result = execute_batch(
        &batch,
        &program,
        &state,
        &env,
        &std::collections::BTreeMap::new(),
    )
    .map_err(|e| ApiError::unprocessable("EXECUTION_ERROR", e.to_string()))?;

    let consistency = match check_consistency(&result.events, &result.read_set_old) {
        Ok(()) => "PASSED".to_string(),
        Err(e) => format!("FAILED: {e}"),
    };

    let state_after = StateFile {
        cells: merge_output_state_cells(&state_file.cells, &result.write_set_final),
    };

    let trace = if req.include_trace {
        Some(result.events.clone())
    } else {
        None
    };

    Ok(ExecuteResponse {
        ok: true,
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

struct LoadedProgram {
    schemas: Vec<TableSchema>,
    tx_types: Vec<TxTypeDef>,
}

fn load_program_sources(input: &ProgramInputRef) -> Result<LoadedProgram, ApiError> {
    match input {
        InputRef::Inline { inline } => match inline {
            ProgramInline::Source { source } => compile_program_source(source),
            ProgramInline::Program(pf) => Ok(LoadedProgram {
                schemas: pf.table_schemas.clone(),
                tx_types: pf.tx_types.clone(),
            }),
        },
        InputRef::File { file_path } => load_program_from_file(file_path),
        InputRef::Artifact { artifact_id } => Err(ApiError::not_implemented(
            "ARTIFACT_INPUT_NOT_AVAILABLE",
            format!("artifact input is not available yet: {artifact_id}"),
        )),
    }
}

fn load_program_from_file(path: &Path) -> Result<LoadedProgram, ApiError> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext == "tab" {
        let source = std::fs::read_to_string(path).map_err(|e| {
            ApiError::bad_request(
                "FILE_IO_ERROR",
                format!("failed to read {}: {e}", path.display()),
            )
        })?;
        compile_program_source(&source)
    } else {
        let pf: ProgramFile = load_json_file(path, "program")?;
        Ok(LoadedProgram {
            schemas: pf.table_schemas,
            tx_types: pf.tx_types,
        })
    }
}

fn compile_program_source(source: &str) -> Result<LoadedProgram, ApiError> {
    match tabula_lang::compile(source) {
        Ok(compiled) => Ok(LoadedProgram {
            schemas: compiled.schemas,
            tx_types: compiled.tx_types,
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
                ApiError::unprocessable("COMPILE_ERROR", "program compilation failed")
                    .with_details(json!({ "diagnostics": diagnostics })),
            )
        }
    }
}

fn load_json_input<T>(input: &InputRef<T>, label: &str) -> Result<T, ApiError>
where
    T: Clone + DeserializeOwned,
{
    match input {
        InputRef::Inline { inline } => Ok(inline.clone()),
        InputRef::File { file_path } => load_json_file(file_path, label),
        InputRef::Artifact { artifact_id } => Err(ApiError::not_implemented(
            "ARTIFACT_INPUT_NOT_AVAILABLE",
            format!("artifact input is not available yet: {artifact_id}"),
        )),
    }
}

fn load_json_file<T: DeserializeOwned>(path: &Path, label: &str) -> Result<T, ApiError> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        ApiError::bad_request(
            "FILE_IO_ERROR",
            format!("failed to read {label} file {}: {e}", path.display()),
        )
    })?;

    serde_json::from_str(&content).map_err(|e| {
        ApiError::bad_request(
            "PARSE_ERROR",
            format!("failed to parse {label} file {}: {e}", path.display()),
        )
    })
}

fn register_program(schemas: &[TableSchema], tx_types: &[TxTypeDef]) -> Result<Program, ApiError> {
    validate_schema_coverage(schemas, tx_types)?;

    let mut program = Program::new();
    for schema in schemas {
        program.add_schema(schema.clone());
    }
    for def in tx_types {
        program.register(def.clone()).map_err(|e| {
            ApiError::unprocessable("PROGRAM_VALIDATION_ERROR", format!("invalid program: {e}"))
        })?;
    }
    Ok(program)
}

fn validate_schema_coverage(
    schemas: &[TableSchema],
    tx_types: &[TxTypeDef],
) -> Result<(), ApiError> {
    let mut columns_by_table: BTreeMap<TableId, BTreeSet<ColId>> = BTreeMap::new();
    for schema in schemas {
        let cols = columns_by_table.entry(schema.id).or_default();
        for col in &schema.columns {
            cols.insert(col.id);
        }
    }

    for tx in tx_types {
        for (instr_idx, instr) in tx.body.iter().enumerate() {
            match instr {
                Instruction::Read { table, col, .. } | Instruction::Write { table, col, .. } => {
                    ensure_table_col_exists(&columns_by_table, tx, instr_idx, *table, *col)?
                }
                Instruction::Lookup {
                    static_table, col, ..
                } => {
                    ensure_table_col_exists(&columns_by_table, tx, instr_idx, *static_table, *col)?
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn ensure_table_col_exists(
    columns_by_table: &BTreeMap<TableId, BTreeSet<ColId>>,
    tx: &TxTypeDef,
    instr_idx: usize,
    table: TableId,
    col: ColId,
) -> Result<(), ApiError> {
    let Some(cols) = columns_by_table.get(&table) else {
        return Err(ApiError::unprocessable(
            "PROGRAM_SCHEMA_ERROR",
            format!(
                "tx '{}' (id {}), instruction {} references table {} but no schema is declared for it",
                tx.name, tx.id.0, instr_idx, table.0
            ),
        ));
    };

    if !cols.contains(&col) {
        return Err(ApiError::unprocessable(
            "PROGRAM_SCHEMA_ERROR",
            format!(
                "tx '{}' (id {}), instruction {} references table {} col {} but that column is missing in schema",
                tx.name, tx.id.0, instr_idx, table.0, col.0
            ),
        ));
    }

    Ok(())
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
}
