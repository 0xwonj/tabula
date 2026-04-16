//! CLI presentation boundary.

mod human;
mod models;
mod project;
mod values;

#[cfg(feature = "verify")]
pub(crate) use human::render_inspect_proof;
#[cfg(feature = "prove")]
pub(crate) use human::render_prove;
#[cfg(feature = "verify")]
pub(crate) use human::render_verify;
pub(crate) use human::{render_check, render_execution, render_query, render_schema, render_state};
#[cfg(feature = "verify")]
pub(crate) use models::InspectProofOutput;
#[cfg(feature = "prove")]
pub(crate) use models::ProveOutput;
#[cfg(feature = "verify")]
pub(crate) use models::VerifyOutput;
pub(crate) use models::{
    CheckOutput, EntryOutput, ExecutionReport, NamedTypeOutput, QueryOutput, QueryRunOutput,
    SchemaOutput, StateCellOutput, StateInspectOutput, TableFieldOutput, TableOutput,
    TxOutcomeOutput, TxOutcomeStatus, TypeOutput, ValueOutput,
};
#[cfg(feature = "verify")]
pub(crate) use project::inspect_proof_output;
#[cfg(feature = "prove")]
pub(crate) use project::prove_output;
#[cfg(feature = "verify")]
pub(crate) use project::verify_output;
pub(crate) use project::{
    check_output, execution_report, query_run_output, schema_output, state_output,
};
pub(crate) use values::{type_name, value_output};
