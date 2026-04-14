//! Human-readable renderers for CLI outputs.

use std::fmt::Write as _;

#[cfg(feature = "verify")]
use super::InspectProofOutput;
#[cfg(feature = "prove")]
use super::ProveOutput;
#[cfg(feature = "verify")]
use super::VerifyOutput;
use super::{
    CheckOutput, EnvDoctorOutput, ExecutionReport, QueryRunOutput, SchemaOutput,
    StateInspectOutput, TxOutcomeStatus, ValueOutput,
};

/// Render `check` output for humans.
pub(crate) fn render_check(output: &CheckOutput) -> String {
    let mut text = String::new();
    let _ = writeln!(
        &mut text,
        "OK: {} table(s), {} tx(s), {} query(ies), {} context field(s)",
        output.tables.len(),
        output.transactions.len(),
        output.queries.len(),
        output.context_fields.len()
    );
    let _ = writeln!(&mut text, "Artifact digest: {}", output.artifact_digest);
    if !output.tables.is_empty() {
        let _ = writeln!(&mut text, "Tables: {}", output.tables.join(", "));
    }
    if !output.transactions.is_empty() {
        let _ = writeln!(
            &mut text,
            "Transactions: {}",
            output.transactions.join(", ")
        );
    }
    if !output.queries.is_empty() {
        let _ = writeln!(&mut text, "Queries: {}", output.queries.join(", "));
    }
    if !output.context_fields.is_empty() {
        let _ = writeln!(
            &mut text,
            "Context fields: {}",
            output.context_fields.join(", ")
        );
    }
    text.trim_end().to_string()
}

/// Render `schema` output for humans.
pub(crate) fn render_schema(output: &SchemaOutput) -> String {
    let mut text = String::new();
    let _ = writeln!(&mut text, "Artifact digest: {}", output.artifact_digest);
    let _ = writeln!(&mut text, "Tables");
    if output.tables.is_empty() {
        let _ = writeln!(&mut text, "  (none)");
    } else {
        for table in &output.tables {
            let key_components = if table.key_components.is_empty() {
                "()".to_string()
            } else {
                table
                    .key_components
                    .iter()
                    .map(|component| format!("{}: {}", component.symbol, component.ty.display))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let _ = writeln!(
                &mut text,
                "  {} (id={}, key=[{}])",
                table.symbol, table.id, key_components
            );
            for field in &table.fields {
                let _ = writeln!(
                    &mut text,
                    "    {} (id={}): {}",
                    field.symbol, field.id, field.ty.display
                );
            }
        }
    }

    let _ = writeln!(&mut text, "Transactions");
    if output.transactions.is_empty() {
        let _ = writeln!(&mut text, "  (none)");
    } else {
        for entry in &output.transactions {
            let params = entry
                .params
                .iter()
                .map(|param| format!("{}: {}", param.symbol, param.ty.display))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(
                &mut text,
                "  {} (id={}): ({params})",
                entry.symbol, entry.id
            );
        }
    }

    let _ = writeln!(&mut text, "Queries");
    if output.queries.is_empty() {
        let _ = writeln!(&mut text, "  (none)");
    } else {
        for query in &output.queries {
            let params = query
                .params
                .iter()
                .map(|param| format!("{}: {}", param.symbol, param.ty.display))
                .collect::<Vec<_>>()
                .join(", ");
            let returns = query
                .returns
                .iter()
                .map(|ty| ty.display.clone())
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(
                &mut text,
                "  {} (id={}): ({params}) -> {returns}",
                query.symbol, query.id
            );
        }
    }

    let _ = writeln!(&mut text, "Context");
    if output.context_fields.is_empty() {
        let _ = writeln!(&mut text, "  (none)");
    } else {
        for field in &output.context_fields {
            let _ = writeln!(&mut text, "  {}: {}", field.symbol, field.ty.display);
        }
    }
    text.trim_end().to_string()
}

/// Render `query` output for humans.
pub(crate) fn render_query(output: &QueryRunOutput) -> String {
    let mut text = String::new();
    let _ = writeln!(
        &mut text,
        "Query {} returned {} value(s)",
        output.query,
        output.returns.len()
    );
    for (index, value) in output.returns.iter().enumerate() {
        let _ = writeln!(&mut text, "  [{index}] {}", display_value(value));
    }
    text.trim_end().to_string()
}

/// Render `execute` output for humans.
pub(crate) fn render_execution(output: &ExecutionReport) -> String {
    let mut text = String::new();
    let _ = writeln!(&mut text, "Execution summary");
    for outcome in &output.outcomes {
        match &outcome.status {
            TxOutcomeStatus::Success => {
                let label = outcome.entry.as_deref().unwrap_or("<unknown>");
                let _ = writeln!(
                    &mut text,
                    "  tx {} [{}]: success (state={}, events={}, capabilities={}, relations={})",
                    outcome.tx_index,
                    label,
                    outcome.state_effect_count,
                    outcome.event_effect_count,
                    outcome.capability_effect_count,
                    outcome.relation_effect_count
                );
            }
            TxOutcomeStatus::Failed {
                reason,
                failed_op_index,
            } => {
                let label = outcome.entry.as_deref().unwrap_or("<unknown>");
                let suffix = failed_op_index
                    .map(|index| format!(", op={index}"))
                    .unwrap_or_default();
                let _ = writeln!(
                    &mut text,
                    "  tx {} [{}]: failed ({reason}{suffix})",
                    outcome.tx_index, label
                );
            }
        }
    }
    let _ = writeln!(&mut text, "Read set: {}", output.read_count);
    let _ = writeln!(&mut text, "Write set: {}", output.write_count);
    let _ = writeln!(&mut text, "Final state");
    append_state_cells(&mut text, &output.state_after, true);
    text.trim_end().to_string()
}

/// Render `state inspect` output for humans.
pub(crate) fn render_state(output: &StateInspectOutput) -> String {
    let mut text = String::new();
    let _ = writeln!(&mut text, "State: {} cell(s)", output.cell_count);
    append_state_cells(&mut text, output, false);
    text.trim_end().to_string()
}

/// Render `env doctor` output for humans.
pub(crate) fn render_env_doctor(output: &EnvDoctorOutput) -> String {
    let mut text = String::new();
    let _ = writeln!(
        &mut text,
        "Config: {}",
        output.config_path.as_deref().unwrap_or("(none)")
    );
    let _ = writeln!(&mut text, "SDK ready: {}", output.sdk_ready);
    if let Some(error) = &output.build_error {
        let _ = writeln!(&mut text, "Build error: {error}");
    }
    let _ = writeln!(
        &mut text,
        "Features: verify={}, prove={}",
        output.verify_feature_enabled, output.prove_feature_enabled
    );
    if output.extensions.is_empty() {
        let _ = writeln!(&mut text, "Extensions: (none)");
    } else {
        let _ = writeln!(&mut text, "Extensions");
        for extension in &output.extensions {
            let _ = writeln!(&mut text, "  {} ({})", extension.name, extension.path);
            if !extension.capability_paths.is_empty() {
                let _ = writeln!(
                    &mut text,
                    "    capabilities: {}",
                    extension.capability_paths.join(", ")
                );
            }
            if !extension.unsupported_entries.is_empty() {
                let _ = writeln!(
                    &mut text,
                    "    unsupported: {}",
                    extension.unsupported_entries.join(", ")
                );
            }
        }
    }
    text.trim_end().to_string()
}

/// Render `prove` output for humans.
#[cfg(feature = "prove")]
pub(crate) fn render_prove(output: &ProveOutput) -> String {
    format!(
        "Proof generated\nArtifact digest: {}\nBinding digest: {}\nProof system: {}\nProof encoding: {}\nChip count: {}",
        output.artifact_digest,
        output.binding_digest_hex,
        output.proof_system,
        output.proof_encoding,
        output.chip_count
    )
}

/// Render `verify` output for humans.
#[cfg(feature = "verify")]
pub(crate) fn render_verify(output: &VerifyOutput) -> String {
    format!(
        "Proof verified\nArtifact digest: {}\nBinding digest: {}",
        output.artifact_digest, output.binding_digest_hex
    )
}

/// Render `inspect-proof` output for humans.
#[cfg(feature = "verify")]
pub(crate) fn render_inspect_proof(output: &InspectProofOutput) -> String {
    format!(
        "Embedded proof statement\nBinding digest: {}\nProof system: {}\nProof encoding: {}\nPublic-context digest: {}\nEvent digest: {}",
        output.binding_digest_hex,
        output.proof_system,
        output.proof_encoding,
        output.public_statement_file.public_context_digest_hex,
        output.public_statement_file.event_digest_hex,
    )
}

fn append_state_cells(text: &mut String, output: &StateInspectOutput, leading_blank_line: bool) {
    if leading_blank_line && !output.cells.is_empty() {
        let _ = writeln!(text);
    }
    if output.cells.is_empty() {
        let _ = writeln!(text, "  (no cells)");
        return;
    }
    for cell in &output.cells {
        let table = cell.table.as_deref().unwrap_or("<unknown_table>");
        let field = cell.field.as_deref().unwrap_or("<unknown_field>");
        let key = cell
            .key
            .iter()
            .map(display_value)
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            text,
            "  {}[{}].{} = {}",
            table,
            key,
            field,
            display_value(&cell.value)
        );
    }
}

fn display_value(value: &ValueOutput) -> String {
    match value {
        ValueOutput::Bool { value } => value.to_string(),
        ValueOutput::U64 { value } => value.to_string(),
        ValueOutput::I64 { value } => value.to_string(),
        ValueOutput::Bytes32 { hex } => hex.clone(),
        ValueOutput::Portable {
            type_id,
            payload_hex,
        } => format!("portable(type#{type_id}, {payload_hex})"),
    }
}
