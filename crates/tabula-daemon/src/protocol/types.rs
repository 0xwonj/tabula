use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::kernel::domain::{
    BatchFile, Capabilities, CapabilityClientKind, CapabilityInputMode, CheckCommand, CheckResult,
    CompileCommand, CompileResult, ExecuteCommand, ExecuteResult, InputRef as DomainInputRef,
    ProgramFile, ProgramInline as DomainProgramInline, ProgramInputRef as DomainProgramInputRef,
    StateCell, StateFile,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InputMode {
    Inline,
    File,
    Artifact,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    WebIde,
    Cli,
    Automation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InputRef<T> {
    Inline { inline: T },
    File { file_path: PathBuf },
    Artifact { artifact_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProgramInline {
    Source { source: String },
    Program(ProgramFile),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckRequest {
    pub program: InputRef<ProgramInline>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompileRequest {
    pub program: InputRef<ProgramInline>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecuteRequest {
    pub program: InputRef<ProgramInline>,
    pub state: InputRef<StateFile>,
    pub batch: InputRef<BatchFile>,
    #[serde(default)]
    pub include_trace: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct HealthResponse {
    pub ok: bool,
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
}

impl HealthResponse {
    pub fn ok() -> Self {
        Self {
            ok: true,
            status: "ok",
            service: "tabula-daemon",
            version: env!("CARGO_PKG_VERSION"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilitiesResponse {
    pub ok: bool,
    pub service_role: &'static str,
    pub clients: Vec<ClientKind>,
    pub compile: bool,
    pub check: bool,
    pub execute: bool,
    pub prove: bool,
    pub verify: bool,
    pub input_modes: Vec<InputMode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckResponse {
    pub ok: bool,
    pub table_count: usize,
    pub tx_type_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompileResponse {
    pub ok: bool,
    pub table_count: usize,
    pub tx_type_count: usize,
    pub program: ProgramFile,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecuteResponse {
    pub ok: bool,
    pub tx_outcomes: Vec<tabula_core::TxOutcome>,
    pub read_set: Vec<StateCell>,
    pub write_set: Vec<StateCell>,
    pub emitted: Vec<tabula_core::EmittedEvent>,
    pub consistency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<Vec<tabula_core::ExecutionEvent>>,
    pub state_after: StateFile,
}

impl From<CheckRequest> for CheckCommand {
    fn from(value: CheckRequest) -> Self {
        Self {
            program: map_program_input(value.program),
        }
    }
}

impl From<CompileRequest> for CompileCommand {
    fn from(value: CompileRequest) -> Self {
        Self {
            program: map_program_input(value.program),
        }
    }
}

impl From<ExecuteRequest> for ExecuteCommand {
    fn from(value: ExecuteRequest) -> Self {
        Self {
            program: map_program_input(value.program),
            state: map_input(value.state),
            batch: map_input(value.batch),
            include_trace: value.include_trace,
        }
    }
}

impl From<Capabilities> for CapabilitiesResponse {
    fn from(value: Capabilities) -> Self {
        Self {
            ok: true,
            service_role: value.service_role,
            clients: value.clients.into_iter().map(ClientKind::from).collect(),
            compile: value.compile,
            check: value.check,
            execute: value.execute,
            prove: value.prove,
            verify: value.verify,
            input_modes: value.input_modes.into_iter().map(InputMode::from).collect(),
        }
    }
}

impl From<CheckResult> for CheckResponse {
    fn from(value: CheckResult) -> Self {
        Self {
            ok: true,
            table_count: value.table_count,
            tx_type_count: value.tx_type_count,
        }
    }
}

impl From<CompileResult> for CompileResponse {
    fn from(value: CompileResult) -> Self {
        Self {
            ok: true,
            table_count: value.table_count,
            tx_type_count: value.tx_type_count,
            program: value.program,
        }
    }
}

impl From<ExecuteResult> for ExecuteResponse {
    fn from(value: ExecuteResult) -> Self {
        Self {
            ok: true,
            tx_outcomes: value.tx_outcomes,
            read_set: value.read_set,
            write_set: value.write_set,
            emitted: value.emitted,
            consistency: value.consistency,
            trace: value.trace,
            state_after: value.state_after,
        }
    }
}

fn map_program_input(input: InputRef<ProgramInline>) -> DomainProgramInputRef {
    match input {
        InputRef::Inline { inline } => DomainInputRef::Inline(map_program_inline(inline)),
        InputRef::File { file_path } => DomainInputRef::File(file_path),
        InputRef::Artifact { artifact_id } => DomainInputRef::Artifact(artifact_id),
    }
}

fn map_program_inline(inline: ProgramInline) -> DomainProgramInline {
    match inline {
        ProgramInline::Source { source } => DomainProgramInline::Source(source),
        ProgramInline::Program(program) => DomainProgramInline::Program(program),
    }
}

fn map_input<T>(input: InputRef<T>) -> DomainInputRef<T> {
    match input {
        InputRef::Inline { inline } => DomainInputRef::Inline(inline),
        InputRef::File { file_path } => DomainInputRef::File(file_path),
        InputRef::Artifact { artifact_id } => DomainInputRef::Artifact(artifact_id),
    }
}

impl From<CapabilityInputMode> for InputMode {
    fn from(value: CapabilityInputMode) -> Self {
        match value {
            CapabilityInputMode::Inline => InputMode::Inline,
            CapabilityInputMode::File => InputMode::File,
            CapabilityInputMode::Artifact => InputMode::Artifact,
        }
    }
}

impl From<CapabilityClientKind> for ClientKind {
    fn from(value: CapabilityClientKind) -> Self {
        match value {
            CapabilityClientKind::WebIde => ClientKind::WebIde,
            CapabilityClientKind::Cli => ClientKind::Cli,
            CapabilityClientKind::Automation => ClientKind::Automation,
        }
    }
}
