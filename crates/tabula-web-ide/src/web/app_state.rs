//! Reactive signal groups for the App component.
//!
//! All signals are grouped into a single `AppSignals` struct so that handler
//! functions and sub-components can receive a shared reference instead of
//! dozens of individual signal pairs.

use leptos::prelude::*;

use crate::web::models::{
    HealthResponse, ProgramArtifact, RunRecord, StarkProofSummary, VerifyReport, WorkspaceDoc,
};
use crate::web::storage;

/// All reactive signals used by the App component, grouped by concern.
#[derive(Clone, Copy)]
pub(crate) struct AppSignals {
    // ── Workspace (persisted) ────────────────────────────────────────
    pub daemon_url: ReadSignal<String>,
    pub set_daemon_url: WriteSignal<String>,
    pub auth_token: ReadSignal<String>,
    pub set_auth_token: WriteSignal<String>,
    pub program_source: ReadSignal<String>,
    pub set_program_source: WriteSignal<String>,
    pub state_json: ReadSignal<String>,
    pub set_state_json: WriteSignal<String>,
    pub batch_json: ReadSignal<String>,
    pub set_batch_json: WriteSignal<String>,
    pub include_trace: ReadSignal<bool>,
    pub set_include_trace: WriteSignal<bool>,
    pub proof_json: ReadSignal<String>,
    pub set_proof_json: WriteSignal<String>,
    pub verify_result_json: ReadSignal<String>,
    pub set_verify_result_json: WriteSignal<String>,

    // ── Output (transient) ───────────────────────────────────────────
    pub health: ReadSignal<Option<HealthResponse>>,
    pub set_health: WriteSignal<Option<HealthResponse>>,
    pub set_capabilities_json: WriteSignal<String>,
    pub diagnostics_text: ReadSignal<String>,
    pub set_diagnostics_text: WriteSignal<String>,
    pub compiled_ir_json: ReadSignal<String>,
    pub set_compiled_ir_json: WriteSignal<String>,
    pub execution_json: ReadSignal<String>,
    pub set_execution_json: WriteSignal<String>,
    pub trace_json: ReadSignal<String>,
    pub set_trace_json: WriteSignal<String>,
    pub set_rw_diff_json: WriteSignal<String>,
    pub proof_log_json: ReadSignal<String>,
    pub set_proof_log_json: WriteSignal<String>,

    // ── Deploy flow ──────────────────────────────────────────────────
    pub set_deployed_program_id: WriteSignal<Option<String>>,
    pub deployed_instance_id: ReadSignal<Option<String>>,
    pub set_deployed_instance_id: WriteSignal<Option<String>>,
    pub deployed_instance_version: ReadSignal<u64>,
    pub set_deployed_instance_version: WriteSignal<u64>,
    pub program_artifact: ReadSignal<Option<ProgramArtifact>>,
    pub set_program_artifact: WriteSignal<Option<ProgramArtifact>>,

    // ── STARK proof summary ──────────────────────────────────────────
    pub stark_summary: ReadSignal<Option<StarkProofSummary>>,
    pub set_stark_summary: WriteSignal<Option<StarkProofSummary>>,

    // ── UI state ─────────────────────────────────────────────────────
    pub busy_action: ReadSignal<Option<String>>,
    pub set_busy_action: WriteSignal<Option<String>>,
    pub status_line: ReadSignal<String>,
    pub set_status_line: WriteSignal<String>,
    pub active_tab: ReadSignal<String>,
    pub set_active_tab: WriteSignal<String>,
    pub run_history: ReadSignal<Vec<RunRecord>>,
    pub set_run_history: WriteSignal<Vec<RunRecord>>,
    pub set_pending_state_after: WriteSignal<Option<String>>,
    pub verify_report: ReadSignal<Option<VerifyReport>>,
    pub set_verify_report: WriteSignal<Option<VerifyReport>>,
    pub set_last_run_id: WriteSignal<Option<String>>,
    pub workspace_import_json: ReadSignal<String>,
    pub set_workspace_import_json: WriteSignal<String>,
    pub show_settings: ReadSignal<bool>,
    pub set_show_settings: WriteSignal<bool>,
}

impl AppSignals {
    /// Create all signals from an initial workspace document.
    pub fn new(initial: WorkspaceDoc) -> Self {
        let (daemon_url, set_daemon_url) = signal(initial.daemon_url);
        let (auth_token, set_auth_token) = signal(initial.auth_token);
        let (program_source, set_program_source) = signal(initial.program_source);
        let (state_json, set_state_json) = signal(initial.state_json);
        let (batch_json, set_batch_json) = signal(initial.batch_json);
        let (include_trace, set_include_trace) = signal(initial.include_trace);
        let (proof_json, set_proof_json) = signal(initial.proof_json);
        let (verify_result_json, set_verify_result_json) = signal(initial.verify_result_json);

        let (health, set_health) = signal::<Option<HealthResponse>>(None);
        let (_capabilities_json, set_capabilities_json) = signal(String::new());
        let (diagnostics_text, set_diagnostics_text) = signal("Ready.".to_string());
        let (compiled_ir_json, set_compiled_ir_json) = signal(String::new());
        let (execution_json, set_execution_json) = signal(String::new());
        let (trace_json, set_trace_json) = signal(String::new());
        let (_rw_diff_json, set_rw_diff_json) = signal(String::new());
        let (proof_log_json, set_proof_log_json) = signal(String::new());

        let (busy_action, set_busy_action) = signal::<Option<String>>(None);
        let (status_line, set_status_line) = signal("Idle".to_string());
        let (active_tab, set_active_tab) = signal("diagnostics".to_string());
        let (run_history, set_run_history) = signal(Vec::<RunRecord>::new());
        let (_pending_state_after, set_pending_state_after) = signal::<Option<String>>(None);
        let (verify_report, set_verify_report) = signal::<Option<VerifyReport>>(None);
        let (_last_run_id, set_last_run_id) = signal::<Option<String>>(None);
        let (workspace_import_json, set_workspace_import_json) = signal(String::new());

        let (_deployed_program_id, set_deployed_program_id) = signal::<Option<String>>(None);
        let (deployed_instance_id, set_deployed_instance_id) = signal::<Option<String>>(None);
        let (deployed_instance_version, set_deployed_instance_version) = signal::<u64>(0);
        let (program_artifact, set_program_artifact) = signal::<Option<ProgramArtifact>>(None);

        let (stark_summary, set_stark_summary) = signal::<Option<StarkProofSummary>>(None);

        let (show_settings, set_show_settings) = signal(false);

        Self {
            daemon_url,
            set_daemon_url,
            auth_token,
            set_auth_token,
            program_source,
            set_program_source,
            state_json,
            set_state_json,
            batch_json,
            set_batch_json,
            include_trace,
            set_include_trace,
            proof_json,
            set_proof_json,
            verify_result_json,
            set_verify_result_json,
            health,
            set_health,
            set_capabilities_json,
            diagnostics_text,
            set_diagnostics_text,
            compiled_ir_json,
            set_compiled_ir_json,
            execution_json,
            set_execution_json,
            trace_json,
            set_trace_json,
            set_rw_diff_json,
            proof_log_json,
            set_proof_log_json,
            set_deployed_program_id,
            deployed_instance_id,
            set_deployed_instance_id,
            deployed_instance_version,
            set_deployed_instance_version,
            program_artifact,
            set_program_artifact,
            stark_summary,
            set_stark_summary,
            busy_action,
            set_busy_action,
            status_line,
            set_status_line,
            active_tab,
            set_active_tab,
            run_history,
            set_run_history,
            set_pending_state_after,
            verify_report,
            set_verify_report,
            set_last_run_id,
            workspace_import_json,
            set_workspace_import_json,
            show_settings,
            set_show_settings,
        }
    }

    /// Persist the current workspace signals to local storage.
    pub fn persist(&self) {
        storage::save_workspace(&WorkspaceDoc {
            daemon_url: self.daemon_url.get(),
            auth_token: self.auth_token.get(),
            program_source: self.program_source.get(),
            state_json: self.state_json.get(),
            batch_json: self.batch_json.get(),
            include_trace: self.include_trace.get(),
            proof_json: self.proof_json.get(),
            verify_result_json: self.verify_result_json.get(),
        });
    }

    /// Append a record to the run history (capped at 40 entries).
    pub fn append_history(&self, action: &str, ok: bool, summary: String) {
        self.set_run_history.update(|history| {
            history.push(RunRecord {
                ts_ms: storage::now_ms(),
                action: action.to_string(),
                ok,
                summary,
            });
            if history.len() > 40 {
                let overflow = history.len().saturating_sub(40);
                if overflow > 0 {
                    history.drain(0..overflow);
                }
            }
        });
    }

    /// Reset verification gate signals.
    pub fn clear_verify_gate(&self) {
        self.set_verify_report.set(None);
        self.set_verify_result_json.set(String::new());
    }
}
