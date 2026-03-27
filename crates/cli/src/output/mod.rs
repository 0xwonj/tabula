//! CLI presentation boundary.

mod human;
mod models;
mod project;
mod values;

#[cfg(feature = "prove")]
pub(crate) use human::render_prove;
#[cfg(feature = "verify")]
pub(crate) use human::render_verify;
pub(crate) use human::{
    render_check, render_env_doctor, render_execution, render_query, render_schema, render_state,
};
#[cfg(feature = "prove")]
pub(crate) use models::ProveOutputV1;
#[cfg(feature = "verify")]
pub(crate) use models::VerifyOutputV1;
pub(crate) use models::{
    CheckOutputV1, EntryOutputV1, EnvDoctorOutputV1, ExecutionReportV1, ExtensionBundleOutputV1,
    NamedTypeOutputV1, QueryOutputV1, QueryRunOutputV1, SchemaOutputV1, StateCellOutputV1,
    StateInspectOutputV1, TableFieldOutputV1, TableOutputV1, TxOutcomeOutputV1, TxOutcomeStatusV1,
    TypeOutputV1, ValueOutputV1,
};
#[cfg(feature = "prove")]
pub(crate) use project::prove_output;
#[cfg(feature = "verify")]
pub(crate) use project::verify_output;
pub(crate) use project::{
    check_output, environment_status_output, execution_report, query_run_output, schema_output,
    state_output,
};
pub(crate) use values::{type_name, value_output};
