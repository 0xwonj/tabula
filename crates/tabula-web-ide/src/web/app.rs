use std::cell::RefCell;
use std::rc::Rc;

use gloo_file::{File, callbacks::FileReader, callbacks::read_as_text};
use leptos::{html, prelude::*};
use serde_json::{Value, json};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;

use crate::web::api::{ApiClient, ApiClientError};
use crate::web::models::{
    BatchFile, CheckResponse, CompileResponse, ExecuteResponse, HealthResponse, RunRecord,
    StateCell, StateFile, VerifyReport, WorkspaceDoc,
};
use crate::web::storage;
use crate::web::templates::{built_in_templates, default_workspace, template_workspace};

#[component]
pub fn App() -> impl IntoView {
    let mut initial = storage::load_workspace().unwrap_or_else(default_workspace);
    if initial.program_source.trim().is_empty() {
        initial = default_workspace();
    }

    let (daemon_url, set_daemon_url) = signal(initial.daemon_url);
    let (auth_token, set_auth_token) = signal(initial.auth_token);
    let (program_source, set_program_source) = signal(initial.program_source);
    let (state_json, set_state_json) = signal(initial.state_json);
    let (batch_json, set_batch_json) = signal(initial.batch_json);
    let (include_trace, set_include_trace) = signal(initial.include_trace);
    let (proof_json, set_proof_json) = signal(initial.proof_json);
    let (verify_result_json, set_verify_result_json) = signal(initial.verify_result_json);

    let (health, set_health) = signal::<Option<HealthResponse>>(None);
    let (capabilities_json, set_capabilities_json) = signal(String::new());
    let (diagnostics_text, set_diagnostics_text) = signal("Ready.".to_string());
    let (compiled_ir_json, set_compiled_ir_json) = signal("".to_string());
    let (execution_json, set_execution_json) = signal("".to_string());
    let (trace_json, set_trace_json) = signal("".to_string());
    let (rw_diff_json, set_rw_diff_json) = signal("".to_string());
    let (proof_log_json, set_proof_log_json) = signal("".to_string());

    let (busy_action, set_busy_action) = signal::<Option<String>>(None);
    let (status_line, set_status_line) = signal("Idle".to_string());
    let (active_tab, set_active_tab) = signal("diagnostics".to_string());
    let (run_history, set_run_history) = signal(Vec::<RunRecord>::new());
    let (pending_state_after, set_pending_state_after) = signal::<Option<String>>(None);
    let (verify_report, set_verify_report) = signal::<Option<VerifyReport>>(None);
    let (workspace_import_json, set_workspace_import_json) = signal(String::new());

    let proof_reader: Rc<RefCell<Option<FileReader>>> = Rc::new(RefCell::new(None));
    let workspace_reader: Rc<RefCell<Option<FileReader>>> = Rc::new(RefCell::new(None));
    let proof_input_ref: NodeRef<html::Input> = NodeRef::new();
    let workspace_input_ref: NodeRef<html::Input> = NodeRef::new();

    let persist = move || {
        storage::save_workspace(&WorkspaceDoc {
            daemon_url: daemon_url.get(),
            auth_token: auth_token.get(),
            program_source: program_source.get(),
            state_json: state_json.get(),
            batch_json: batch_json.get(),
            include_trace: include_trace.get(),
            proof_json: proof_json.get(),
            verify_result_json: verify_result_json.get(),
        });
    };

    let append_history = move |action: &str, ok: bool, summary: String| {
        set_run_history.update(|history| {
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
    };

    let clear_verify_gate = move || {
        set_verify_report.set(None);
        set_verify_result_json.set(String::new());
    };

    let connect_daemon = move |_| {
        if busy_action.get().is_some() {
            return;
        }

        let base = daemon_url.get();
        let token = auth_token.get();
        let client = ApiClient::new(base.clone(), opt_token(token));

        set_busy_action.set(Some("connect".to_string()));
        set_status_line.set(format!("Connecting to {base} ..."));

        spawn_local(async move {
            let health_res = client.health().await;
            let caps_res = client.capabilities().await;

            match (health_res, caps_res) {
                (Ok(health_ok), Ok(caps_ok)) => {
                    set_health.set(Some(health_ok));
                    set_capabilities_json.set(pretty_json_value(&json!(caps_ok)));
                    set_diagnostics_text.set("Connected. Health/capabilities synced.".to_string());
                    set_status_line.set("Daemon connected".to_string());
                    append_history("connect", true, "health + capabilities ok".to_string());
                }
                (Err(e), _) | (_, Err(e)) => {
                    set_status_line.set(format!("Connection failed: {}", e.message));
                    set_diagnostics_text.set(format_api_err("connect", &e));
                    append_history("connect", false, e.message);
                }
            }

            set_busy_action.set(None);
        });
    };

    let run_check = move |_| {
        if busy_action.get().is_some() {
            return;
        }

        clear_verify_gate();
        let base = daemon_url.get();
        let token = auth_token.get();
        let source = program_source.get();
        let client = ApiClient::new(base, opt_token(token));

        set_busy_action.set(Some("check".to_string()));
        set_active_tab.set("diagnostics".to_string());
        set_status_line.set("Running check ...".to_string());

        spawn_local(async move {
            match client.check(&source).await {
                Ok(CheckResponse {
                    table_count,
                    tx_type_count,
                    ..
                }) => {
                    set_diagnostics_text.set(format!(
                        "CHECK OK\n- table_count: {table_count}\n- tx_type_count: {tx_type_count}"
                    ));
                    set_status_line.set("Check finished".to_string());
                    append_history(
                        "check",
                        true,
                        format!("{table_count} table(s), {tx_type_count} tx type(s)"),
                    );
                }
                Err(e) => {
                    set_diagnostics_text.set(format_api_err("check", &e));
                    set_status_line.set(format!("Check failed: {}", e.message));
                    append_history("check", false, e.message);
                }
            }

            set_busy_action.set(None);
        });
    };

    let run_compile = move |_| {
        if busy_action.get().is_some() {
            return;
        }

        clear_verify_gate();
        let base = daemon_url.get();
        let token = auth_token.get();
        let source = program_source.get();
        let client = ApiClient::new(base, opt_token(token));

        set_busy_action.set(Some("compile".to_string()));
        set_active_tab.set("compiled".to_string());
        set_status_line.set("Compiling ...".to_string());

        spawn_local(async move {
            match client.compile(&source).await {
                Ok(CompileResponse {
                    table_count,
                    tx_type_count,
                    program,
                    ..
                }) => {
                    set_compiled_ir_json.set(pretty_json_value(&program));
                    set_status_line.set("Compile finished".to_string());
                    set_diagnostics_text.set(format!(
                        "COMPILE OK\n- table_count: {table_count}\n- tx_type_count: {tx_type_count}"
                    ));
                    append_history(
                        "compile",
                        true,
                        format!("{table_count} table(s), {tx_type_count} tx type(s)"),
                    );
                }
                Err(e) => {
                    set_diagnostics_text.set(format_api_err("compile", &e));
                    set_status_line.set(format!("Compile failed: {}", e.message));
                    append_history("compile", false, e.message);
                }
            }

            set_busy_action.set(None);
        });
    };

    let run_execute = move |_| {
        if busy_action.get().is_some() {
            return;
        }

        clear_verify_gate();

        let state = match parse_state(&state_json.get()) {
            Ok(state) => state,
            Err(e) => {
                set_diagnostics_text.set(format!("STATE JSON ERROR: {e}"));
                append_history("execute", false, format!("state parse failed: {e}"));
                return;
            }
        };

        let batch = match parse_batch(&batch_json.get()) {
            Ok(batch) => batch,
            Err(e) => {
                set_diagnostics_text.set(format!("BATCH JSON ERROR: {e}"));
                append_history("execute", false, format!("batch parse failed: {e}"));
                return;
            }
        };

        let base = daemon_url.get();
        let token = auth_token.get();
        let source = program_source.get();
        let include_trace_value = include_trace.get();
        let client = ApiClient::new(base, opt_token(token));

        set_busy_action.set(Some("execute".to_string()));
        set_active_tab.set("execution".to_string());
        set_status_line.set("Executing batch ...".to_string());

        spawn_local(async move {
            match client
                .execute(&source, state, batch, include_trace_value)
                .await
            {
                Ok(ExecuteResponse {
                    tx_outcomes,
                    read_set,
                    write_set,
                    emitted,
                    consistency,
                    trace,
                    state_after,
                    ..
                }) => {
                    let state_after_str = pretty_json_value(&json!(state_after));

                    let execution_blob = json!({
                        "tx_outcomes": tx_outcomes,
                        "consistency": consistency,
                        "emitted": emitted,
                        "state_after": serde_json::from_str::<Value>(&state_after_str).unwrap_or(json!({"raw": state_after_str})),
                    });
                    set_execution_json.set(pretty_json_value(&execution_blob));

                    let rw_blob = json!({
                        "read_set": read_set,
                        "write_set": write_set,
                    });
                    set_rw_diff_json.set(pretty_json_value(&rw_blob));

                    let trace_blob = json!({
                        "trace": trace,
                    });
                    set_trace_json.set(pretty_json_value(&trace_blob));

                    set_pending_state_after.set(Some(state_after_str));
                    set_status_line.set("Execute finished".to_string());
                    set_diagnostics_text.set(format!(
                        "EXECUTE OK\n- consistency: {}",
                        consistency_label(&execution_blob["consistency"])
                    ));
                    append_history("execute", true, "batch execution finished".to_string());
                }
                Err(e) => {
                    set_diagnostics_text.set(format_api_err("execute", &e));
                    set_status_line.set(format!("Execute failed: {}", e.message));
                    append_history("execute", false, e.message);
                }
            }

            set_busy_action.set(None);
        });
    };

    let run_prove = move |_| {
        if busy_action.get().is_some() {
            return;
        }

        let state = match parse_state(&state_json.get()) {
            Ok(state) => state,
            Err(e) => {
                set_status_line.set(format!("Prove blocked: invalid state JSON ({e})"));
                append_history("prove", false, format!("state parse failed: {e}"));
                return;
            }
        };

        let batch = match parse_batch(&batch_json.get()) {
            Ok(batch) => batch,
            Err(e) => {
                set_status_line.set(format!("Prove blocked: invalid batch JSON ({e})"));
                append_history("prove", false, format!("batch parse failed: {e}"));
                return;
            }
        };

        let base = daemon_url.get();
        let token = auth_token.get();
        let source = program_source.get();
        let include_trace_value = include_trace.get();
        let client = ApiClient::new(base, opt_token(token));

        set_busy_action.set(Some("prove".to_string()));
        set_active_tab.set("proof".to_string());
        set_status_line.set("Generating proof ...".to_string());

        spawn_local(async move {
            match client
                .prove(&source, state, batch, include_trace_value)
                .await
            {
                Ok(resp) => {
                    let proof_json_text = pretty_json_value(&json!(resp.proof));
                    let state_after_str = pretty_json_value(&json!(resp.execution.state_after));

                    set_proof_json.set(proof_json_text.clone());
                    set_proof_log_json.set(format!(
                        "PROVE OK\n- scheme: {}\n- statement_hash: {}\n- tx_count: {}\n- emitted_count: {}",
                        resp.proof.scheme,
                        resp.proof.statement_hash,
                        resp.proof.tx_count,
                        resp.proof.emitted_count
                    ));
                    set_pending_state_after.set(Some(state_after_str));

                    let execution_blob = json!({
                        "tx_outcomes": resp.execution.tx_outcomes,
                        "consistency": resp.execution.consistency,
                        "emitted": resp.execution.emitted,
                        "state_after": resp.execution.state_after,
                    });
                    set_execution_json.set(pretty_json_value(&execution_blob));
                    set_rw_diff_json.set(pretty_json_value(&json!({
                        "read_set": resp.execution.read_set,
                        "write_set": resp.execution.write_set,
                    })));
                    set_trace_json.set(pretty_json_value(&json!({
                        "trace": resp.execution.trace,
                    })));

                    set_status_line.set("Proof generated".to_string());
                    append_history(
                        "prove",
                        true,
                        format!("statement_hash={}", resp.proof.statement_hash),
                    );
                }
                Err(err) => {
                    set_status_line.set(format!("Prove failed: {}", err.message));
                    set_diagnostics_text.set(format_api_err("prove", &err));
                    append_history("prove", false, err.message);
                }
            }

            persist();
            set_busy_action.set(None);
        });
    };

    let run_verify = move |_| {
        if busy_action.get().is_some() {
            return;
        }

        let proof_text = proof_json.get();
        if proof_text.trim().is_empty() {
            set_status_line.set("No proof artifact provided".to_string());
            append_history("verify", false, "missing proof artifact".to_string());
            return;
        }

        let proof_value = match serde_json::from_str::<Value>(&proof_text) {
            Ok(value) => value,
            Err(e) => {
                set_status_line.set(format!("Invalid proof JSON: {e}"));
                append_history("verify", false, format!("invalid proof JSON: {e}"));
                return;
            }
        };

        let state = match parse_state(&state_json.get()) {
            Ok(state) => state,
            Err(e) => {
                set_status_line.set(format!("Verify blocked: invalid state JSON ({e})"));
                append_history("verify", false, format!("state parse failed: {e}"));
                return;
            }
        };

        let batch = match parse_batch(&batch_json.get()) {
            Ok(batch) => batch,
            Err(e) => {
                set_status_line.set(format!("Verify blocked: invalid batch JSON ({e})"));
                append_history("verify", false, format!("batch parse failed: {e}"));
                return;
            }
        };

        let state_after = match pending_state_after.get() {
            Some(raw) => match parse_state(&raw) {
                Ok(parsed) => parsed,
                Err(e) => {
                    set_status_line
                        .set(format!("Verify blocked: invalid pending state_after ({e})"));
                    append_history("verify", false, format!("state_after parse failed: {e}"));
                    return;
                }
            },
            None => {
                set_status_line.set(
                    "Verify blocked: no pending state_after. execute/prove first.".to_string(),
                );
                append_history("verify", false, "missing pending state_after".to_string());
                return;
            }
        };

        let source = program_source.get();
        let base = daemon_url.get();
        let token = auth_token.get();
        let client = ApiClient::new(base, opt_token(token));

        set_busy_action.set(Some("verify".to_string()));
        set_active_tab.set("verify".to_string());
        set_status_line.set("Verifying proof ...".to_string());

        spawn_local(async move {
            match client
                .verify(proof_value, &source, state, batch, state_after)
                .await
            {
                Ok(resp) => {
                    let report = VerifyReport {
                        ok: resp.verified,
                        mode: "daemon_receipt".to_string(),
                        message: resp.message.clone(),
                        statement_hash: resp.statement_hash.clone(),
                        expected_statement_hash: resp.expected_statement_hash.clone(),
                        checked_at_ms: storage::now_ms(),
                        raw: Some(json!(resp)),
                    };

                    let rendered = pretty_json_value(&json!(report));
                    set_verify_result_json.set(rendered);
                    set_verify_report.set(Some(report));
                    set_status_line.set(if resp.verified {
                        "Verify passed".to_string()
                    } else {
                        "Verify failed".to_string()
                    });
                    append_history("verify", resp.verified, resp.message);
                }
                Err(err) => {
                    set_status_line.set(format!("Verify failed: {}", err.message));
                    set_diagnostics_text.set(format_api_err("verify", &err));
                    append_history("verify", false, err.message);
                }
            }

            persist();
            set_busy_action.set(None);
        });
    };

    let apply_verified_state = move |_| {
        let report = verify_report.get();
        let Some(report) = report else {
            set_status_line.set("Verify report missing".to_string());
            return;
        };
        if !report.ok {
            set_status_line.set("Verify not passed. apply blocked.".to_string());
            return;
        }
        let Some(next_state) = pending_state_after.get() else {
            set_status_line.set("No pending state_after to apply".to_string());
            return;
        };

        set_state_json.set(next_state);
        set_pending_state_after.set(None);
        set_status_line.set("Verified state applied".to_string());
        append_history(
            "apply_state",
            true,
            "state_after merged as new state".to_string(),
        );
        persist();
    };

    let load_template = move |id: &'static str| {
        if let Some(ws) = template_workspace(id) {
            set_program_source.set(ws.program_source);
            set_state_json.set(ws.state_json);
            set_batch_json.set(ws.batch_json);
            set_pending_state_after.set(None);
            clear_verify_gate();
            set_status_line.set(format!("Template loaded: {id}"));
            append_history("template", true, format!("loaded {id}"));
            persist();
        }
    };

    let add_state_row = move |_| {
        let mut state = parse_state(&state_json.get()).unwrap_or(StateFile { cells: vec![] });
        state.cells.push(StateCell {
            table: 0,
            row: 0,
            col: 0,
            value: Some(json!({ "U64": 0 })),
        });
        set_state_json.set(pretty_json_value(&json!(state)));
        persist();
    };

    let add_tx_row = move |_| {
        let mut batch = parse_batch(&batch_json.get()).unwrap_or(BatchFile {
            transactions: vec![],
        });
        batch.transactions.push(crate::web::models::TxInput {
            tx_type: 0,
            params: vec![json!({ "U64": 0 })],
            sender: "01".repeat(32),
            nonce: batch.transactions.len() as u64,
        });
        set_batch_json.set(pretty_json_value(&json!(batch)));
        persist();
    };

    let export_workspace = move |_| {
        let doc = WorkspaceDoc {
            daemon_url: daemon_url.get(),
            auth_token: auth_token.get(),
            program_source: program_source.get(),
            state_json: state_json.get(),
            batch_json: batch_json.get(),
            include_trace: include_trace.get(),
            proof_json: proof_json.get(),
            verify_result_json: verify_result_json.get(),
        };

        match serde_json::to_string_pretty(&doc) {
            Ok(payload) => match storage::export_text_file("tabula-workspace.json", &payload) {
                Ok(()) => set_status_line.set("Workspace exported".to_string()),
                Err(e) => set_status_line.set(format!("Export failed: {e}")),
            },
            Err(e) => set_status_line.set(format!("Export failed: {e}")),
        }
    };

    let export_proof = move |_| {
        let data = proof_json.get();
        if data.trim().is_empty() {
            set_status_line.set("No proof artifact to export".to_string());
            return;
        }

        match storage::export_text_file("tabula-proof.json", &data) {
            Ok(()) => set_status_line.set("Proof artifact exported".to_string()),
            Err(e) => set_status_line.set(format!("Proof export failed: {e}")),
        }
    };

    let import_workspace_text = move |_| {
        let payload = workspace_import_json.get();
        match serde_json::from_str::<WorkspaceDoc>(&payload) {
            Ok(ws) => {
                set_daemon_url.set(ws.daemon_url);
                set_auth_token.set(ws.auth_token);
                set_program_source.set(ws.program_source);
                set_state_json.set(ws.state_json);
                set_batch_json.set(ws.batch_json);
                set_include_trace.set(ws.include_trace);
                set_proof_json.set(ws.proof_json);
                set_verify_result_json.set(ws.verify_result_json);
                set_pending_state_after.set(None);
                clear_verify_gate();
                set_status_line.set("Workspace imported from JSON text".to_string());
                append_history(
                    "workspace_import",
                    true,
                    "imported from textarea".to_string(),
                );
                persist();
            }
            Err(e) => {
                set_status_line.set(format!("Import failed: {e}"));
                append_history("workspace_import", false, e.to_string());
            }
        }
    };

    let open_proof_picker = move |_| {
        if let Some(el) = proof_input_ref.get() {
            el.click();
        }
    };

    let open_workspace_picker = move |_| {
        if let Some(el) = workspace_input_ref.get() {
            el.click();
        }
    };

    let on_proof_file_change = {
        let reader_holder = proof_reader.clone();
        move |ev: web_sys::Event| {
            let Some(target) = ev.target() else {
                return;
            };
            let Ok(input) = target.dyn_into::<HtmlInputElement>() else {
                return;
            };
            let Some(files) = input.files() else {
                return;
            };
            let Some(file) = files.get(0) else {
                return;
            };

            let gloo_file = File::from(file);
            let task = read_as_text(&gloo_file, move |result| match result {
                Ok(text) => {
                    set_proof_json.set(text);
                    set_status_line.set("Proof artifact imported from file".to_string());
                    persist();
                }
                Err(err) => {
                    set_status_line.set(format!("Failed to read proof file: {err}"));
                }
            });

            *reader_holder.borrow_mut() = Some(task);
        }
    };

    let on_workspace_file_change = {
        let reader_holder = workspace_reader.clone();
        move |ev: web_sys::Event| {
            let Some(target) = ev.target() else {
                return;
            };
            let Ok(input) = target.dyn_into::<HtmlInputElement>() else {
                return;
            };
            let Some(files) = input.files() else {
                return;
            };
            let Some(file) = files.get(0) else {
                return;
            };

            let gloo_file = File::from(file);
            let task = read_as_text(&gloo_file, move |result| match result {
                Ok(text) => {
                    set_workspace_import_json.set(text);
                    set_status_line.set("Workspace JSON loaded to import box".to_string());
                }
                Err(err) => {
                    set_status_line.set(format!("Failed to read workspace file: {err}"));
                }
            });

            *reader_holder.borrow_mut() = Some(task);
        }
    };

    view! {
        <div class="tabula-app">
            <header class="topbar glass">
                <div class="brand">
                    <span class="kicker">"Tabula"</span>
                    <h1>"Web IDE Control Plane"</h1>
                </div>
                <div class="actions-row">
                    <button class="action ghost" on:click=connect_daemon disabled=move || busy_action.get().is_some()>
                        "Connect"
                    </button>
                    <button class="action" on:click=run_check disabled=move || busy_action.get().is_some()>
                        "Check"
                    </button>
                    <button class="action" on:click=run_compile disabled=move || busy_action.get().is_some()>
                        "Compile"
                    </button>
                    <button class="action" on:click=run_execute disabled=move || busy_action.get().is_some()>
                        "Execute"
                    </button>
                    <button class="action" on:click=run_prove disabled=move || busy_action.get().is_some()>
                        "Prove"
                    </button>
                    <button class="action" on:click=run_verify disabled=move || busy_action.get().is_some()>
                        "Verify"
                    </button>
                    <button
                        class="action success"
                        on:click=apply_verified_state
                        disabled=move || {
                            if pending_state_after.get().is_none() {
                                return true;
                            }
                            !verify_report.get().map(|r| r.ok).unwrap_or(false)
                        }
                    >
                        "Apply Verified State"
                    </button>
                </div>
                <div class="status-pill">
                    <span class="dot"></span>
                    {move || status_line.get()}
                </div>
            </header>

            <section class="workspace-grid">
                <aside class="panel glass left-panel reveal-delay-1">
                    <h2>"Connection"</h2>
                    <label>
                        <span>"Daemon URL"</span>
                        <input
                            type="text"
                            prop:value=move || daemon_url.get()
                            on:input=move |ev| {
                                set_daemon_url.set(event_target_value(&ev));
                                persist();
                            }
                            placeholder="http://127.0.0.1:4317"
                        />
                    </label>
                    <label>
                        <span>"Bearer Token (optional)"</span>
                        <input
                            type="password"
                            prop:value=move || auth_token.get()
                            on:input=move |ev| {
                                set_auth_token.set(event_target_value(&ev));
                                persist();
                            }
                            placeholder="TABULA_DAEMON_TOKEN"
                        />
                    </label>
                    <label class="checkbox-row">
                        <input
                            type="checkbox"
                            prop:checked=move || include_trace.get()
                            on:change=move |ev| {
                                set_include_trace.set(event_target_checked(&ev));
                                persist();
                            }
                        />
                        <span>"Include execution trace"</span>
                    </label>

                    <div class="health-box">
                        <h3>"Health"</h3>
                        {move || {
                            if let Some(health) = health.get() {
                                view! {
                                    <pre>{format!(
                                        "service={}\nstatus={}\nversion={}",
                                        health.service, health.status, health.version
                                    )}</pre>
                                }
                                    .into_any()
                            } else {
                                view! { <p class="muted">"not connected"</p> }.into_any()
                            }
                        }}
                    </div>

                    <div class="templates-box">
                        <h3>"Scenario Templates"</h3>
                        {built_in_templates()
                            .into_iter()
                            .map(|tpl| {
                                let id = tpl.id;
                                let title = tpl.title;
                                let description = tpl.description;
                                view! {
                                    <button class="template-btn" on:click=move |_| load_template(id)>
                                        <strong>{title}</strong>
                                        <small>{description}</small>
                                    </button>
                                }
                            })
                            .collect_view()}
                    </div>

                    <div class="import-export">
                        <h3>"Workspace I/O"</h3>
                        <div class="row-btns">
                            <button class="action ghost" on:click=export_workspace>
                                "Export Workspace"
                            </button>
                            <button class="action ghost" on:click=open_workspace_picker>
                                "Load Workspace File"
                            </button>
                        </div>
                        <input
                            class="hidden"
                            node_ref=workspace_input_ref
                            type="file"
                            accept="application/json"
                            on:change=on_workspace_file_change
                        />
                        <textarea
                            rows="6"
                            prop:value=move || workspace_import_json.get()
                            on:input=move |ev| set_workspace_import_json.set(event_target_value(&ev))
                            placeholder="Paste workspace JSON here"
                        ></textarea>
                        <button class="action ghost" on:click=import_workspace_text>
                            "Import Workspace JSON"
                        </button>
                    </div>

                    <div class="history-box">
                        <h3>"Run History"</h3>
                        <div class="history-list">
                            {move || {
                                run_history
                                    .get()
                                    .into_iter()
                                    .rev()
                                    .map(|entry| {
                                        let tone = if entry.ok { "ok" } else { "err" };
                                        view! {
                                            <div class=format!("history-item {tone}")>
                                                <strong>{entry.action}</strong>
                                                <span>{entry.summary}</span>
                                            </div>
                                        }
                                    })
                                    .collect_view()
                            }}
                        </div>
                    </div>
                </aside>

                <section class="panel glass center-panel reveal-delay-2">
                    <div class="panel-head">
                        <h2>"Program IDE"</h2>
                        <small>"Tabula DSL source"</small>
                    </div>
                    <textarea
                        class="code-editor"
                        prop:value=move || program_source.get()
                        on:input=move |ev| {
                            set_program_source.set(event_target_value(&ev));
                            persist();
                        }
                        spellcheck="false"
                    ></textarea>

                    <div class="proof-section">
                        <div class="panel-head inline">
                            <h3>"Proof Artifact"</h3>
                            <div class="row-btns">
                                <button class="action ghost" on:click=open_proof_picker>
                                    "Import Proof File"
                                </button>
                                <button class="action ghost" on:click=export_proof>
                                    "Export Proof"
                                </button>
                            </div>
                        </div>

                        <input
                            class="hidden"
                            node_ref=proof_input_ref
                            type="file"
                            accept="application/json"
                            on:change=on_proof_file_change
                        />

                        <textarea
                            rows="8"
                            prop:value=move || proof_json.get()
                            on:input=move |ev| {
                                set_proof_json.set(event_target_value(&ev));
                                persist();
                            }
                            placeholder="proof artifact JSON"
                        ></textarea>
                    </div>
                </section>

                <aside class="panel glass right-panel reveal-delay-3">
                    <div class="panel-head inline">
                        <h2>"State Table"</h2>
                        <button class="action ghost" on:click=add_state_row>
                            "+ Cell"
                        </button>
                    </div>
                    <div class="editor-table-wrap">
                        {move || render_state_editor(state_json, set_state_json, persist)}
                    </div>
                    <textarea
                        rows="6"
                        prop:value=move || state_json.get()
                        on:input=move |ev| {
                            set_state_json.set(event_target_value(&ev));
                            clear_verify_gate();
                            persist();
                        }
                        placeholder="state JSON"
                    ></textarea>

                    <div class="panel-head inline">
                        <h2>"Transaction Batch"</h2>
                        <button class="action ghost" on:click=add_tx_row>
                            "+ Tx"
                        </button>
                    </div>
                    <div class="editor-table-wrap">
                        {move || render_batch_editor(batch_json, set_batch_json, persist)}
                    </div>
                    <textarea
                        rows="7"
                        prop:value=move || batch_json.get()
                        on:input=move |ev| {
                            set_batch_json.set(event_target_value(&ev));
                            clear_verify_gate();
                            persist();
                        }
                        placeholder="batch JSON"
                    ></textarea>
                </aside>
            </section>

            <section class="panel glass bottom-panel reveal-delay-4">
                <div class="tab-row">
                    {tab_button(active_tab, set_active_tab, "diagnostics", "Diagnostics")}
                    {tab_button(active_tab, set_active_tab, "compiled", "Compiled IR")}
                    {tab_button(active_tab, set_active_tab, "execution", "Execution")}
                    {tab_button(active_tab, set_active_tab, "trace", "Trace")}
                    {tab_button(active_tab, set_active_tab, "diff", "RW Diff")}
                    {tab_button(active_tab, set_active_tab, "proof", "Proof")}
                    {tab_button(active_tab, set_active_tab, "verify", "Verify")}
                    {tab_button(active_tab, set_active_tab, "capabilities", "Capabilities")}
                </div>

                <div class="tab-content">
                    {move || match active_tab.get().as_str() {
                        "diagnostics" => view! { <pre>{move || diagnostics_text.get()}</pre> }.into_any(),
                        "compiled" => view! { <pre>{move || compiled_ir_json.get()}</pre> }.into_any(),
                        "execution" => view! { <pre>{move || execution_json.get()}</pre> }.into_any(),
                        "trace" => view! { <pre>{move || trace_json.get()}</pre> }.into_any(),
                        "diff" => view! { <pre>{move || rw_diff_json.get()}</pre> }.into_any(),
                        "proof" => view! { <pre>{move || proof_log_json.get()}</pre> }.into_any(),
                        "verify" => view! { <pre>{move || verify_result_json.get()}</pre> }.into_any(),
                        "capabilities" => {
                            view! { <pre>{move || capabilities_json.get()}</pre> }.into_any()
                        }
                        _ => view! { <pre>""</pre> }.into_any(),
                    }}
                </div>
            </section>
        </div>
    }
}

fn tab_button(
    active_tab: ReadSignal<String>,
    set_active_tab: WriteSignal<String>,
    value: &'static str,
    label: &'static str,
) -> impl IntoView {
    view! {
        <button
            class=move || {
                if active_tab.get() == value {
                    "tab-btn active".to_string()
                } else {
                    "tab-btn".to_string()
                }
            }
            on:click=move |_| set_active_tab.set(value.to_string())
        >
            {label}
        </button>
    }
}

fn render_state_editor(
    state_json: ReadSignal<String>,
    set_state_json: WriteSignal<String>,
    persist: impl Fn() + Clone + 'static,
) -> impl IntoView {
    let parsed = parse_state(&state_json.get());

    match parsed {
        Ok(state) => {
            view! {
                <table class="editor-table">
                    <thead>
                        <tr>
                            <th>"#"</th>
                            <th>"table"</th>
                            <th>"row"</th>
                            <th>"col"</th>
                            <th>"value (json)"</th>
                            <th>""</th>
                        </tr>
                    </thead>
                    <tbody>
                        {state
                            .cells
                            .into_iter()
                            .enumerate()
                            .map(|(idx, cell)| {
                                let state_json_for_table = state_json;
                                let set_state_for_table = set_state_json;
                                let persist_for_table = persist.clone();

                                let state_json_for_row = state_json;
                                let set_state_for_row = set_state_json;
                                let persist_for_row = persist.clone();

                                let state_json_for_col = state_json;
                                let set_state_for_col = set_state_json;
                                let persist_for_col = persist.clone();

                                let state_json_for_val = state_json;
                                let set_state_for_val = set_state_json;
                                let persist_for_val = persist.clone();

                                let state_json_for_remove = state_json;
                                let set_state_for_remove = set_state_json;
                                let persist_for_remove = persist.clone();

                                view! {
                                    <tr>
                                        <td>{idx}</td>
                                        <td>
                                            <input
                                                type="number"
                                                prop:value=cell.table
                                                on:input=move |ev| {
                                                    if let Ok(mut state) = parse_state(&state_json_for_table.get()) {
                                                        if let Some(target) = state.cells.get_mut(idx)
                                                            && let Ok(next) = event_target_value(&ev).parse::<u32>()
                                                        {
                                                            target.table = next;
                                                        }
                                                        set_state_for_table.set(pretty_json_value(&json!(state)));
                                                        persist_for_table();
                                                    }
                                                }
                                            />
                                        </td>
                                        <td>
                                            <input
                                                type="number"
                                                prop:value=cell.row
                                                on:input=move |ev| {
                                                    if let Ok(mut state) = parse_state(&state_json_for_row.get()) {
                                                        if let Some(target) = state.cells.get_mut(idx)
                                                            && let Ok(next) = event_target_value(&ev).parse::<u64>()
                                                        {
                                                            target.row = next;
                                                        }
                                                        set_state_for_row.set(pretty_json_value(&json!(state)));
                                                        persist_for_row();
                                                    }
                                                }
                                            />
                                        </td>
                                        <td>
                                            <input
                                                type="number"
                                                prop:value=cell.col
                                                on:input=move |ev| {
                                                    if let Ok(mut state) = parse_state(&state_json_for_col.get()) {
                                                        if let Some(target) = state.cells.get_mut(idx)
                                                            && let Ok(next) = event_target_value(&ev).parse::<u16>()
                                                        {
                                                            target.col = next;
                                                        }
                                                        set_state_for_col.set(pretty_json_value(&json!(state)));
                                                        persist_for_col();
                                                    }
                                                }
                                            />
                                        </td>
                                        <td>
                                            <input
                                                type="text"
                                                prop:value=cell
                                                    .value
                                                    .as_ref()
                                                    .map(pretty_json_inline)
                                                    .unwrap_or_else(|| "null".to_string())
                                                on:change=move |ev| {
                                                    if let Ok(mut state) = parse_state(&state_json_for_val.get()) {
                                                        if let Some(target) = state.cells.get_mut(idx) {
                                                            let raw = event_target_value(&ev);
                                                            if raw.trim().is_empty() || raw.trim() == "null" {
                                                                target.value = None;
                                                            } else if let Ok(v) = serde_json::from_str::<Value>(&raw) {
                                                                target.value = Some(v);
                                                            }
                                                        }
                                                        set_state_for_val.set(pretty_json_value(&json!(state)));
                                                        persist_for_val();
                                                    }
                                                }
                                            />
                                        </td>
                                        <td>
                                            <button
                                                class="table-remove"
                                                on:click=move |_| {
                                                    if let Ok(mut state) = parse_state(&state_json_for_remove.get()) {
                                                        if idx < state.cells.len() {
                                                            state.cells.remove(idx);
                                                        }
                                                        set_state_for_remove.set(pretty_json_value(&json!(state)));
                                                        persist_for_remove();
                                                    }
                                                }
                                            >
                                                "x"
                                            </button>
                                        </td>
                                    </tr>
                                }
                            })
                            .collect_view()}
                    </tbody>
                </table>
            }
                .into_any()
        }
        Err(err) => view! { <p class="error-text">{format!("state parse error: {err}")}</p> }.into_any(),
    }
}

fn render_batch_editor(
    batch_json: ReadSignal<String>,
    set_batch_json: WriteSignal<String>,
    persist: impl Fn() + Clone + 'static,
) -> impl IntoView {
    let parsed = parse_batch(&batch_json.get());

    match parsed {
        Ok(batch) => {
            view! {
                <table class="editor-table">
                    <thead>
                        <tr>
                            <th>"#"</th>
                            <th>"tx_type"</th>
                            <th>"nonce"</th>
                            <th>"sender"</th>
                            <th>"params (json array)"</th>
                            <th>""</th>
                        </tr>
                    </thead>
                    <tbody>
                        {batch
                            .transactions
                            .into_iter()
                            .enumerate()
                            .map(|(idx, tx)| {
                                let batch_json_for_type = batch_json;
                                let set_batch_for_type = set_batch_json;
                                let persist_for_type = persist.clone();

                                let batch_json_for_nonce = batch_json;
                                let set_batch_for_nonce = set_batch_json;
                                let persist_for_nonce = persist.clone();

                                let batch_json_for_sender = batch_json;
                                let set_batch_for_sender = set_batch_json;
                                let persist_for_sender = persist.clone();

                                let batch_json_for_params = batch_json;
                                let set_batch_for_params = set_batch_json;
                                let persist_for_params = persist.clone();

                                let batch_json_for_remove = batch_json;
                                let set_batch_for_remove = set_batch_json;
                                let persist_for_remove = persist.clone();

                                view! {
                                    <tr>
                                        <td>{idx}</td>
                                        <td>
                                            <input
                                                type="number"
                                                prop:value=tx.tx_type
                                                on:input=move |ev| {
                                                    if let Ok(mut batch) = parse_batch(&batch_json_for_type.get()) {
                                                        if let Some(target) = batch.transactions.get_mut(idx)
                                                            && let Ok(next) = event_target_value(&ev).parse::<u32>()
                                                        {
                                                            target.tx_type = next;
                                                        }
                                                        set_batch_for_type.set(pretty_json_value(&json!(batch)));
                                                        persist_for_type();
                                                    }
                                                }
                                            />
                                        </td>
                                        <td>
                                            <input
                                                type="number"
                                                prop:value=tx.nonce
                                                on:input=move |ev| {
                                                    if let Ok(mut batch) = parse_batch(&batch_json_for_nonce.get()) {
                                                        if let Some(target) = batch.transactions.get_mut(idx)
                                                            && let Ok(next) = event_target_value(&ev).parse::<u64>()
                                                        {
                                                            target.nonce = next;
                                                        }
                                                        set_batch_for_nonce.set(pretty_json_value(&json!(batch)));
                                                        persist_for_nonce();
                                                    }
                                                }
                                            />
                                        </td>
                                        <td>
                                            <input
                                                type="text"
                                                prop:value=tx.sender
                                                on:input=move |ev| {
                                                    if let Ok(mut batch) = parse_batch(&batch_json_for_sender.get()) {
                                                        if let Some(target) = batch.transactions.get_mut(idx) {
                                                            target.sender = event_target_value(&ev);
                                                        }
                                                        set_batch_for_sender.set(pretty_json_value(&json!(batch)));
                                                        persist_for_sender();
                                                    }
                                                }
                                            />
                                        </td>
                                        <td>
                                            <input
                                                type="text"
                                                prop:value=pretty_json_inline(&json!(tx.params))
                                                on:change=move |ev| {
                                                    if let Ok(mut batch) = parse_batch(&batch_json_for_params.get()) {
                                                        if let Some(target) = batch.transactions.get_mut(idx)
                                                            && let Ok(next) = serde_json::from_str::<Vec<Value>>(&event_target_value(&ev))
                                                        {
                                                            target.params = next;
                                                        }
                                                        set_batch_for_params.set(pretty_json_value(&json!(batch)));
                                                        persist_for_params();
                                                    }
                                                }
                                            />
                                        </td>
                                        <td>
                                            <button
                                                class="table-remove"
                                                on:click=move |_| {
                                                    if let Ok(mut batch) = parse_batch(&batch_json_for_remove.get()) {
                                                        if idx < batch.transactions.len() {
                                                            batch.transactions.remove(idx);
                                                        }
                                                        set_batch_for_remove.set(pretty_json_value(&json!(batch)));
                                                        persist_for_remove();
                                                    }
                                                }
                                            >
                                                "x"
                                            </button>
                                        </td>
                                    </tr>
                                }
                            })
                            .collect_view()}
                    </tbody>
                </table>
            }
                .into_any()
        }
        Err(err) => view! { <p class="error-text">{format!("batch parse error: {err}")}</p> }.into_any(),
    }
}

fn parse_state(raw: &str) -> Result<StateFile, String> {
    serde_json::from_str::<StateFile>(raw).map_err(|e| e.to_string())
}

fn parse_batch(raw: &str) -> Result<BatchFile, String> {
    serde_json::from_str::<BatchFile>(raw).map_err(|e| e.to_string())
}

fn format_api_err(ctx: &str, err: &ApiClientError) -> String {
    let mut out = format!("{} failed\n- message: {}", ctx.to_uppercase(), err.message);
    if let Some(status) = err.status {
        out.push_str(&format!("\n- http_status: {status}"));
    }
    if let Some(code) = err.code.as_ref() {
        out.push_str(&format!("\n- code: {code}"));
    }
    if let Some(details) = err.details.as_ref() {
        out.push_str("\n- details:\n");
        out.push_str(&pretty_json_value(details));
    }
    out
}

fn pretty_json_value(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn pretty_json_inline(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

fn consistency_label(value: &Value) -> String {
    if let Some(s) = value.as_str() {
        return s.to_string();
    }
    if let Some(obj) = value.as_object()
        && let Some((key, _)) = obj.iter().next()
    {
        return key.clone();
    }
    "unknown".to_string()
}

fn opt_token(token: String) -> Option<String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
