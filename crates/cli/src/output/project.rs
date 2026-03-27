//! Domain-to-output projection helpers.

use std::collections::BTreeMap;

use tabula_sdk::{Artifact, Program, QueryResult, State};

use crate::environment::EnvironmentStatus;
use crate::io::ProgramInputKind;

#[cfg(feature = "prove")]
use super::ProveOutputV1;
#[cfg(feature = "verify")]
use super::VerifyOutputV1;
use super::{
    CheckOutputV1, EntryOutputV1, EnvDoctorOutputV1, ExecutionReportV1, ExtensionBundleOutputV1,
    NamedTypeOutputV1, QueryOutputV1, QueryRunOutputV1, SchemaOutputV1, StateCellOutputV1,
    StateInspectOutputV1, TableFieldOutputV1, TableOutputV1, TxOutcomeOutputV1, TxOutcomeStatusV1,
    TypeOutputV1,
};
use crate::output::{type_name, value_output};

pub(crate) fn check_output(artifact: &Artifact, input_kind: ProgramInputKind) -> CheckOutputV1 {
    let schema = artifact.schema();
    CheckOutputV1 {
        version: "tabula.cli.check.v1".to_string(),
        input_kind: match input_kind {
            ProgramInputKind::Source => "source".to_string(),
            ProgramInputKind::Artifact => "artifact".to_string(),
        },
        artifact_digest: artifact.digest().to_string(),
        tables: schema
            .tables()
            .iter()
            .map(|table| table.symbol().to_string())
            .collect(),
        transactions: schema
            .txs()
            .iter()
            .map(|tx| tx.symbol().to_string())
            .collect(),
        queries: schema
            .queries()
            .iter()
            .map(|query| query.symbol().to_string())
            .collect(),
        context_fields: schema
            .context_fields()
            .iter()
            .map(|field| field.symbol().to_string())
            .collect(),
    }
}

pub(crate) fn schema_output(artifact: &Artifact) -> SchemaOutputV1 {
    let schema = artifact.schema();
    SchemaOutputV1 {
        version: "tabula.cli.schema.v1".to_string(),
        artifact_digest: artifact.digest().to_string(),
        tables: schema
            .tables()
            .iter()
            .map(|table| TableOutputV1 {
                id: table.id().0,
                symbol: table.symbol().to_string(),
                key_arity: table.key_arity(),
                fields: table
                    .fields()
                    .iter()
                    .map(|field| TableFieldOutputV1 {
                        id: u32::from(field.id().0),
                        symbol: field.symbol().to_string(),
                        ty: type_output(field.ty()),
                    })
                    .collect(),
            })
            .collect(),
        transactions: schema
            .txs()
            .iter()
            .map(|entry| EntryOutputV1 {
                id: entry.id().0,
                symbol: entry.symbol().to_string(),
                params: entry
                    .params()
                    .iter()
                    .map(|param| NamedTypeOutputV1 {
                        symbol: param.symbol().to_string(),
                        ty: type_output(param.ty()),
                    })
                    .collect(),
            })
            .collect(),
        queries: schema
            .queries()
            .iter()
            .map(|query| QueryOutputV1 {
                id: query.id().0,
                symbol: query.symbol().to_string(),
                params: query
                    .params()
                    .iter()
                    .map(|param| NamedTypeOutputV1 {
                        symbol: param.symbol().to_string(),
                        ty: type_output(param.ty()),
                    })
                    .collect(),
                returns: query.returns().iter().copied().map(type_output).collect(),
            })
            .collect(),
        context_fields: schema
            .context_fields()
            .iter()
            .map(|field| NamedTypeOutputV1 {
                symbol: field.symbol().to_string(),
                ty: type_output(field.ty()),
            })
            .collect(),
    }
}

pub(crate) fn query_run_output(
    artifact: &Artifact,
    query: &str,
    result: &QueryResult,
) -> QueryRunOutputV1 {
    QueryRunOutputV1 {
        version: "tabula.cli.query.v1".to_string(),
        artifact_digest: artifact.digest().to_string(),
        query: query.to_string(),
        returns: result
            .returns()
            .iter()
            .map(value_output)
            .collect::<Vec<_>>(),
    }
}

pub(crate) fn execution_report(
    program: &Program,
    receipt: &tabula_sdk::ExecutionReceipt,
    raw: bool,
) -> ExecutionReportV1 {
    let entry_names = program
        .schema()
        .txs()
        .iter()
        .map(|tx| (tx.id().0, tx.symbol().to_string()))
        .collect::<BTreeMap<_, _>>();
    ExecutionReportV1 {
        version: "tabula.cli.execute.v1".to_string(),
        artifact_digest: program.artifact().digest().to_string(),
        outcomes: receipt
            .outcomes()
            .iter()
            .map(|outcome| TxOutcomeOutputV1 {
                tx_index: outcome.tx_index(),
                entry_id: outcome.entry_id().0,
                entry: (!raw)
                    .then(|| entry_names.get(&outcome.entry_id().0).cloned())
                    .flatten(),
                status: if outcome.success() {
                    TxOutcomeStatusV1::Success
                } else {
                    TxOutcomeStatusV1::Failed {
                        reason: outcome.reason().unwrap_or("unknown failure").to_string(),
                        failed_op_index: outcome.failed_op_index(),
                    }
                },
                state_effect_count: outcome.state_effect_count(),
                event_effect_count: outcome.event_effect_count(),
                capability_effect_count: outcome.capability_effect_count(),
                relation_effect_count: outcome.relation_effect_count(),
            })
            .collect(),
        read_count: receipt.read_count(),
        write_count: receipt.write_count(),
        state_after: state_output(Some(program), &receipt.state_after(), None, raw),
    }
}

pub(crate) fn state_output(
    program: Option<&Program>,
    state: &State,
    table_filter: Option<&str>,
    raw: bool,
) -> StateInspectOutputV1 {
    let tables_by_id = program.map_or_else(SymbolMaps::default, symbol_maps);

    let requested_table_id = table_filter.and_then(|value| {
        if let Some(program) = program {
            program.table(value).ok().map(|table| table.id().0)
        } else {
            value.parse::<u32>().ok()
        }
    });

    let cells = state
        .cells()
        .filter(|(key, _)| requested_table_id.is_none_or(|id| key.table.0 == id))
        .map(|(key, value)| StateCellOutputV1 {
            table_id: key.table.0,
            table: (!raw)
                .then(|| tables_by_id.table_names.get(&key.table.0).cloned())
                .flatten(),
            row: key.row.0,
            field_id: u32::from(key.col.0),
            field: (!raw)
                .then(|| {
                    tables_by_id
                        .field_names
                        .get(&(key.table.0, u32::from(key.col.0)))
                        .cloned()
                })
                .flatten(),
            value: value_output(value),
        })
        .collect::<Vec<_>>();

    StateInspectOutputV1 {
        version: "tabula.cli.state.v1".to_string(),
        cell_count: cells.len(),
        cells,
    }
}

pub(crate) fn type_output(ty: tabula_sdk::interop::TypeRef) -> TypeOutputV1 {
    TypeOutputV1 {
        id: ty.0,
        display: type_name(ty),
    }
}

pub(crate) fn environment_status_output(status: &EnvironmentStatus) -> EnvDoctorOutputV1 {
    EnvDoctorOutputV1 {
        version: "tabula.cli.env.v1".to_string(),
        config_path: status.config_path.clone(),
        sdk_ready: status.sdk_ready,
        build_error: status.build_error.clone(),
        extensions: status
            .extensions
            .iter()
            .map(|extension| ExtensionBundleOutputV1 {
                path: extension.path.clone(),
                name: extension.name.clone(),
                capability_paths: extension.capability_paths.clone(),
                unsupported_entries: extension.unsupported_entries.clone(),
            })
            .collect(),
        verify_feature_enabled: status.verify_feature_enabled,
        prove_feature_enabled: status.prove_feature_enabled,
    }
}

#[cfg(feature = "prove")]
pub(crate) fn prove_output(
    artifact: &Artifact,
    proof: &tabula_sdk::Proof,
) -> anyhow::Result<ProveOutputV1> {
    let envelope = proof
        .to_envelope()
        .map_err(|error| anyhow::anyhow!(error))?;
    Ok(ProveOutputV1 {
        version: "tabula.cli.prove.v1".to_string(),
        artifact_digest: artifact.digest().to_string(),
        statement_hash_hex: hex_encode(
            &proof
                .statement()
                .statement_hash_bytes()
                .map_err(|error| anyhow::anyhow!(error))?,
        ),
        proof_system: envelope.proof_system.name().to_string(),
        proof_encoding: envelope.proof_encoding.name().to_string(),
        chip_count: proof.summary().chip_count,
    })
}

#[cfg(feature = "verify")]
pub(crate) fn verify_output(
    artifact: &Artifact,
    proof: &tabula_sdk::Proof,
) -> anyhow::Result<VerifyOutputV1> {
    Ok(VerifyOutputV1 {
        version: "tabula.cli.verify.v1".to_string(),
        artifact_digest: artifact.digest().to_string(),
        statement_hash_hex: hex_encode(
            &proof
                .statement()
                .statement_hash_bytes()
                .map_err(|error| anyhow::anyhow!(error))?,
        ),
        verified: true,
    })
}

#[derive(Debug, Clone, Default)]
struct SymbolMaps {
    table_names: BTreeMap<u32, String>,
    field_names: BTreeMap<(u32, u32), String>,
}

fn symbol_maps(program: &Program) -> SymbolMaps {
    let mut maps = SymbolMaps::default();
    for table in program.schema().tables() {
        maps.table_names
            .insert(table.id().0, table.symbol().to_string());
        for field in table.fields() {
            maps.field_names.insert(
                (table.id().0, u32::from(field.id().0)),
                field.symbol().to_string(),
            );
        }
    }
    maps
}

#[cfg(any(feature = "prove", feature = "verify"))]
fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::environment::EnvironmentStatus;

    use super::environment_status_output;

    #[test]
    fn projects_environment_status_into_output_contract() {
        let output = environment_status_output(&EnvironmentStatus {
            config_path: Some("/tmp/tabula.toml".to_string()),
            sdk_ready: true,
            build_error: None,
            extensions: vec![],
            verify_feature_enabled: true,
            prove_feature_enabled: false,
        });
        assert_eq!(output.version, "tabula.cli.env.v1");
        assert!(output.extensions.is_empty());
    }
}
