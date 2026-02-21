use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gloo_file::{File, callbacks::FileReader, callbacks::read_as_text};
use leptos::{html, prelude::*};
use serde_json::{Value as JsonValue, json};
use tabula_core::Value as CoreValue;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;

use crate::web::api::{ApiClient, ApiClientError};
use crate::web::models::{
    BatchFile, HealthResponse, ProgramArtifact, RunRecord, StarkProofSummary, StateCell, StateFile,
    VerifyReport, WorkspaceDoc,
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
    let (_capabilities_json, set_capabilities_json) = signal(String::new());
    let (diagnostics_text, set_diagnostics_text) = signal("Ready.".to_string());
    let (compiled_ir_json, set_compiled_ir_json) = signal("".to_string());
    let (execution_json, set_execution_json) = signal("".to_string());
    let (trace_json, set_trace_json) = signal("".to_string());
    let (_rw_diff_json, set_rw_diff_json) = signal("".to_string());
    let (proof_log_json, set_proof_log_json) = signal("".to_string());

    let (busy_action, set_busy_action) = signal::<Option<String>>(None);
    let (status_line, set_status_line) = signal("Idle".to_string());
    let (active_tab, set_active_tab) = signal("diagnostics".to_string());
    let (run_history, set_run_history) = signal(Vec::<RunRecord>::new());
    let (_pending_state_after, set_pending_state_after) = signal::<Option<String>>(None);
    let (verify_report, set_verify_report) = signal::<Option<VerifyReport>>(None);
    let (_last_run_id, set_last_run_id) = signal::<Option<String>>(None);
    let (workspace_import_json, set_workspace_import_json) = signal(String::new());

    // Deploy/Submit flow state.
    let (_deployed_program_id, set_deployed_program_id) = signal::<Option<String>>(None);
    let (deployed_instance_id, set_deployed_instance_id) = signal::<Option<String>>(None);
    let (deployed_instance_version, set_deployed_instance_version) = signal::<u64>(0);
    let (program_artifact, set_program_artifact) = signal::<Option<ProgramArtifact>>(None);

    // STARK proof summary for structured rendering.
    let (stark_summary, set_stark_summary) = signal::<Option<StarkProofSummary>>(None);

    // Settings panel toggle.
    let (show_settings, set_show_settings) = signal(false);

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

    // ── Handlers ──────────────────────────────────────────────────────

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
            match client.register_program(&source).await {
                Ok(resp) => {
                    set_diagnostics_text.set(format!(
                        "CHECK OK (register)\n- program_id: {}\n- table_count: {}\n- tx_type_count: {}",
                        resp.program.program_id,
                        resp.program.table_count,
                        resp.program.tx_type_count,
                    ));
                    set_status_line.set("Check finished".to_string());
                    append_history(
                        "check",
                        true,
                        format!(
                            "{} table(s), {} tx type(s)",
                            resp.program.table_count, resp.program.tx_type_count
                        ),
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

    let run_deploy = move |_| {
        if busy_action.get().is_some() {
            return;
        }

        clear_verify_gate();

        let state = match parse_state(&state_json.get()) {
            Ok(state) => state,
            Err(e) => {
                set_diagnostics_text.set(format!("STATE JSON ERROR: {e}"));
                append_history("deploy", false, format!("state parse failed: {e}"));
                return;
            }
        };

        let base = daemon_url.get();
        let token = auth_token.get();
        let source = program_source.get();
        let client = ApiClient::new(base, opt_token(token));

        set_busy_action.set(Some("deploy".to_string()));
        set_active_tab.set("diagnostics".to_string());
        set_status_line.set("Deploying (compile + register + create instance) ...".to_string());

        spawn_local(async move {
            match client.register_program(&source).await {
                Ok(program_resp) => {
                    set_compiled_ir_json.set(pretty_json_value(&program_resp.program.program));
                    match client
                        .create_instance(&program_resp.program.program_id, state)
                        .await
                    {
                        Ok(instance_resp) => {
                            set_deployed_program_id
                                .set(Some(program_resp.program.program_id.clone()));
                            set_deployed_instance_id
                                .set(Some(instance_resp.instance.instance_id.clone()));
                            set_deployed_instance_version.set(instance_resp.instance.version);

                            // Extract schema info from program artifact.
                            let artifact: Option<ProgramArtifact> =
                                serde_json::from_value(program_resp.program.program.clone()).ok();
                            set_program_artifact.set(artifact);

                            set_diagnostics_text.set(format!(
                                "DEPLOY OK\n- program_id: {}\n- instance_id: {}\n- version: {}\n- table_count: {}\n- tx_type_count: {}",
                                program_resp.program.program_id,
                                instance_resp.instance.instance_id,
                                instance_resp.instance.version,
                                program_resp.program.table_count,
                                program_resp.program.tx_type_count,
                            ));
                            set_status_line.set(format!(
                                "Deployed: instance={}",
                                instance_resp.instance.instance_id
                            ));
                            append_history(
                                "deploy",
                                true,
                                format!("instance_id={}", instance_resp.instance.instance_id),
                            );
                        }
                        Err(e) => {
                            set_diagnostics_text.set(format_api_err("deploy:create_instance", &e));
                            set_status_line.set(format!("Deploy failed: {}", e.message));
                            append_history("deploy", false, e.message);
                        }
                    }
                }
                Err(e) => {
                    set_diagnostics_text.set(format_api_err("deploy:register_program", &e));
                    set_status_line.set(format!("Deploy failed: {}", e.message));
                    append_history("deploy", false, e.message);
                }
            }

            set_busy_action.set(None);
        });
    };

    let run_submit = move |_| {
        if busy_action.get().is_some() {
            return;
        }

        let Some(instance_id) = deployed_instance_id.get() else {
            set_status_line.set("Submit blocked: deploy first.".to_string());
            append_history("submit", false, "no deployed instance".to_string());
            return;
        };

        let batch = match parse_batch(&batch_json.get()) {
            Ok(batch) => batch,
            Err(e) => {
                set_diagnostics_text.set(format!("BATCH JSON ERROR: {e}"));
                append_history("submit", false, format!("batch parse failed: {e}"));
                return;
            }
        };

        let base = daemon_url.get();
        let token = auth_token.get();
        let include_trace_value = include_trace.get();
        let version = deployed_instance_version.get();
        let client = ApiClient::new(base, opt_token(token));

        set_busy_action.set(Some("submit".to_string()));
        set_active_tab.set("proof".to_string());
        set_status_line.set("Submitting batch (execute + prove + verify + commit) ...".to_string());

        spawn_local(async move {
            match client
                .submit_run(
                    &instance_id,
                    batch,
                    include_trace_value,
                    true,
                    true,
                    true,
                    Some(version),
                )
                .await
            {
                Ok(resp) => {
                    set_last_run_id.set(Some(resp.run.run_id.clone()));
                    set_deployed_instance_version.set(version + 1);

                    let state_after_str = pretty_json_value(&json!(resp.run.execution.state_after));

                    // Execution tab: merge execution + rw diff.
                    let execution_blob = json!({
                        "tx_outcomes": resp.run.execution.tx_outcomes,
                        "consistency": resp.run.execution.consistency,
                        "emitted": resp.run.execution.emitted,
                        "read_set": resp.run.execution.read_set,
                        "write_set": resp.run.execution.write_set,
                    });
                    set_execution_json.set(pretty_json_value(&execution_blob));

                    set_rw_diff_json.set(pretty_json_value(&json!({
                        "read_set": resp.run.execution.read_set,
                        "write_set": resp.run.execution.write_set,
                    })));

                    set_trace_json.set(pretty_json_value(&json!({
                        "trace": resp.run.execution.trace,
                    })));

                    // Update proof tab — prefer STARK summary.
                    if let Some(stark) = &resp.run.stark_proof {
                        let stark_display = pretty_json_value(&json!(stark));
                        set_proof_json.set(stark_display);
                        set_proof_log_json.set(format!(
                            "STARK PROOF\n- scheme: {}\n- verified: {}\n- chips: {}\n- prove: {}ms\n- verify: {}ms\n- old_root: {:?}\n- new_root: {:?}",
                            stark.scheme,
                            stark.verified,
                            stark.chip_count,
                            stark.prove_time_ms,
                            stark.verify_time_ms,
                            stark.old_state_root,
                            stark.new_state_root,
                        ));
                        set_stark_summary.set(Some(stark.clone()));

                        let report = VerifyReport {
                            ok: stark.verified,
                            mode: "stark_inline".to_string(),
                            message: if stark.verified {
                                "STARK proof verified".to_string()
                            } else {
                                "STARK proof verification failed".to_string()
                            },
                            statement_hash: Some(stark.statement_hash.clone()),
                            expected_statement_hash: None,
                            checked_at_ms: storage::now_ms(),
                            raw: Some(json!(stark)),
                        };
                        set_verify_result_json.set(pretty_json_value(&json!(report)));
                        set_verify_report.set(Some(report));
                    } else if let Some(proof) = &resp.run.proof {
                        let proof_json_text = pretty_json_value(&json!(proof));
                        set_proof_json.set(proof_json_text);
                        set_proof_log_json.set(format!(
                            "RECEIPT\n- scheme: {}\n- statement_hash: {}",
                            proof.scheme, proof.statement_hash
                        ));
                        set_stark_summary.set(None);
                    }

                    // Commit state.
                    set_state_json.set(state_after_str);
                    set_pending_state_after.set(None);

                    set_status_line.set(format!(
                        "Submitted: run_id={}, v{}",
                        resp.run.run_id,
                        version + 1
                    ));
                    set_diagnostics_text.set(format!(
                        "SUBMIT OK\n- run_id: {}\n- status: {}\n- statement_hash: {}",
                        resp.run.run_id, resp.run.status, resp.run.statement_hash
                    ));
                    append_history("submit", true, format!("run_id={}", resp.run.run_id));
                }
                Err(e) => {
                    set_diagnostics_text.set(format_api_err("submit", &e));
                    set_status_line.set(format!("Submit failed: {}", e.message));
                    append_history("submit", false, e.message);
                }
            }

            persist();
            set_busy_action.set(None);
        });
    };

    let load_template = move |id: &'static str| {
        if let Some(ws) = template_workspace(id) {
            set_program_source.set(ws.program_source);
            set_state_json.set(ws.state_json);
            set_batch_json.set(ws.batch_json);
            set_pending_state_after.set(None);
            set_last_run_id.set(None);
            set_deployed_program_id.set(None);
            set_deployed_instance_id.set(None);
            set_deployed_instance_version.set(0);
            set_program_artifact.set(None);
            set_stark_summary.set(None);
            clear_verify_gate();
            set_status_line.set(format!("Template loaded: {id}"));
            append_history("template", true, format!("loaded {id}"));
            persist();
        }
    };

    let add_state_row = move |_| {
        let artifact = program_artifact.get();
        let mut state = parse_state(&state_json.get()).unwrap_or(StateFile { cells: vec![] });

        if let Some(ref art) = artifact {
            // Add a row to the first table with default values for all columns.
            if let Some(schema) = art.table_schemas.first() {
                let next_row = state
                    .cells
                    .iter()
                    .filter(|c| c.table == schema.id.0)
                    .map(|c| c.row)
                    .max()
                    .map(|r| r + 1)
                    .unwrap_or(0);
                for col_def in &schema.columns {
                    state.cells.push(StateCell {
                        table: schema.id.0,
                        row: next_row,
                        col: col_def.id.0,
                        value: Some(default_value_for_type(&format!("{:?}", col_def.value_type))),
                    });
                }
            }
        } else {
            state.cells.push(StateCell {
                table: 0,
                row: 0,
                col: 0,
                value: Some(CoreValue::U64(0)),
            });
        }

        set_state_json.set(pretty_json_value(&json!(state)));
        persist();
    };

    let add_tx_row = move |_| {
        let artifact = program_artifact.get();
        let mut batch = parse_batch(&batch_json.get()).unwrap_or(BatchFile {
            transactions: vec![],
        });

        let next_nonce = batch.transactions.len() as u64;

        if let Some(ref art) = artifact {
            if let Some(tx_def) = art.tx_types.first() {
                let params: Vec<CoreValue> = tx_def
                    .param_schema
                    .iter()
                    .map(|p| default_value_for_type(&format!("{:?}", p.value_type)))
                    .collect();
                batch.transactions.push(crate::web::models::TxInput {
                    tx_type: tx_def.id.0,
                    params,
                    sender: "01".repeat(32),
                    nonce: next_nonce,
                });
            }
        } else {
            batch.transactions.push(crate::web::models::TxInput {
                tx_type: 0,
                params: vec![CoreValue::U64(0)],
                sender: "01".repeat(32),
                nonce: next_nonce,
            });
        }

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
                set_last_run_id.set(None);
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

    // ── View ──────────────────────────────────────────────────────────

    view! {
        <div class="tabula-app">
            <header class="topbar glass">
                <div class="brand">
                    <span class="kicker">"Tabula"</span>
                    <h1>"Playground"</h1>
                </div>
                <div class="topbar-center">
                    <div class="deploy-status">
                        {move || {
                            if let Some(id) = deployed_instance_id.get() {
                                let short = if id.len() > 12 {
                                    format!("{}...", &id[..12])
                                } else {
                                    id.clone()
                                };
                                view! {
                                    <span class="badge badge-ok">"Deployed"</span>
                                    <span class="instance-id">{short}</span>
                                    <span class="version-tag">{format!("v{}", deployed_instance_version.get())}</span>
                                }
                                    .into_any()
                            } else {
                                view! { <span class="badge badge-idle">"Not deployed"</span> }
                                    .into_any()
                            }
                        }}
                    </div>
                </div>
                <div class="topbar-right">
                    <button
                        class="action ghost settings-toggle"
                        on:click=move |_| set_show_settings.set(!show_settings.get())
                    >
                        {move || if show_settings.get() { "Close Settings" } else { "Settings" }}
                    </button>
                    <button class="action ghost" on:click=connect_daemon disabled=move || busy_action.get().is_some()>
                        "Connect"
                    </button>
                    <div class="status-pill">
                        <span class="dot"></span>
                        {move || status_line.get()}
                    </div>
                </div>
            </header>

            // Hidden file inputs (always in DOM, triggered by buttons).
            <input class="hidden" node_ref=workspace_input_ref type="file" accept="application/json" on:change=on_workspace_file_change />
            <input class="hidden" node_ref=proof_input_ref type="file" accept="application/json" on:change=on_proof_file_change />

            // Settings drawer (collapsible).
            <section
                class="settings-drawer glass reveal-delay-1"
                style=move || if show_settings.get() { "" } else { "display:none" }
            >
                <div class="settings-grid">
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
                        <span>"Bearer Token"</span>
                        <input
                            type="password"
                            prop:value=move || auth_token.get()
                            on:input=move |ev| {
                                set_auth_token.set(event_target_value(&ev));
                                persist();
                            }
                            placeholder="optional"
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
                        <span>"Include trace"</span>
                    </label>
                    <div class="settings-io">
                        <div class="row-btns">
                            <button class="action ghost" on:click=export_workspace>"Export"</button>
                            <button class="action ghost" on:click=open_workspace_picker>"Import File"</button>
                            <button class="action ghost" on:click=export_proof>"Export Proof"</button>
                            <button class="action ghost" on:click=open_proof_picker>"Import Proof"</button>
                        </div>
                        <textarea
                            rows="3"
                            prop:value=move || workspace_import_json.get()
                            on:input=move |ev| set_workspace_import_json.set(event_target_value(&ev))
                            placeholder="Paste workspace JSON"
                        ></textarea>
                        <button class="action ghost" on:click=import_workspace_text>"Import JSON"</button>
                    </div>
                    <div class="health-box">
                        {move || {
                            if let Some(h) = health.get() {
                                view! { <span class="muted">{format!("{} {} v{}", h.service, h.status, h.version)}</span> }.into_any()
                            } else {
                                view! { <span class="muted">"not connected"</span> }.into_any()
                            }
                        }}
                    </div>
                </div>
            </section>

            <section class="workspace-grid">
                // ── Left panel: Program + Actions + Templates ─────────
                <div class="panel glass left-panel reveal-delay-1">
                    <div class="panel-head">
                        <h2>"Program"</h2>
                        <small class="muted">"Tabula DSL"</small>
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

                    <div class="left-actions">
                        <button class="action" on:click=run_check disabled=move || busy_action.get().is_some()>
                            "Check"
                        </button>
                        <button class="action" on:click=run_deploy disabled=move || busy_action.get().is_some()>
                            "Deploy"
                        </button>
                    </div>

                    <div class="templates-row">
                        {built_in_templates()
                            .into_iter()
                            .map(|tpl| {
                                let id = tpl.id;
                                let title = tpl.title;
                                let description = tpl.description;
                                view! {
                                    <button class="template-chip" title=description on:click=move |_| load_template(id)>
                                        {title}
                                    </button>
                                }
                            })
                            .collect_view()}
                    </div>
                </div>

                // ── Right panel: State + Tx Builder ──────────────────
                <div class="panel glass right-panel reveal-delay-2">
                    // State Tables section.
                    <div class="panel-head inline">
                        <h2>"State"</h2>
                        <button class="action ghost" on:click=add_state_row>"+ Row"</button>
                    </div>
                    <div class="state-section">
                        {move || render_state_editor(state_json, set_state_json, program_artifact, persist)}
                    </div>

                    <div class="section-divider"></div>

                    // Transaction Builder section.
                    <div class="panel-head inline">
                        <h2>"Transactions"</h2>
                        <div class="row-btns">
                            <button class="action ghost" on:click=add_tx_row>"+ Tx"</button>
                            <button
                                class="action success"
                                on:click=run_submit
                                disabled=move || busy_action.get().is_some() || deployed_instance_id.get().is_none()
                            >
                                "Submit Batch"
                            </button>
                        </div>
                    </div>
                    <div class="tx-section">
                        {move || render_batch_editor(batch_json, set_batch_json, program_artifact, persist)}
                    </div>
                </div>
            </section>

            // ── Bottom panel: Results ─────────────────────────────────
            <section class="panel glass bottom-panel reveal-delay-3">
                <div class="tab-row">
                    {tab_button(active_tab, set_active_tab, "diagnostics", "Diagnostics")}
                    {tab_button(active_tab, set_active_tab, "execution", "Execution")}
                    {tab_button(active_tab, set_active_tab, "proof", "Proof")}
                    {tab_button(active_tab, set_active_tab, "trace", "Trace")}
                    {tab_button(active_tab, set_active_tab, "ir", "IR")}
                </div>

                <div class="tab-content">
                    {move || match active_tab.get().as_str() {
                        "diagnostics" => view! { <pre>{move || diagnostics_text.get()}</pre> }.into_any(),
                        "execution" => view! { <pre>{move || execution_json.get()}</pre> }.into_any(),
                        "proof" => {
                            view! {
                                <div class="proof-display">
                                    {move || render_proof_display(stark_summary, verify_report, proof_log_json)}
                                </div>
                            }.into_any()
                        }
                        "trace" => view! { <pre>{move || trace_json.get()}</pre> }.into_any(),
                        "ir" => view! { <pre>{move || compiled_ir_json.get()}</pre> }.into_any(),
                        _ => view! { <pre>""</pre> }.into_any(),
                    }}
                </div>
            </section>

            // ── Run history (floating) ────────────────────────────────
            <div class="history-float">
                {move || {
                    let history = run_history.get();
                    if history.is_empty() {
                        ().into_any()
                    } else {
                        let recent: Vec<_> = history.iter().rev().take(5).cloned().collect();
                        view! {
                            <div class="history-items">
                                {recent
                                    .into_iter()
                                    .map(|entry| {
                                        let tone = if entry.ok { "ok" } else { "err" };
                                        view! {
                                            <div class=format!("history-chip {tone}")>
                                                <strong>{entry.action}</strong>
                                                <span>{entry.summary}</span>
                                            </div>
                                        }
                                    })
                                    .collect_view()}
                            </div>
                        }
                            .into_any()
                    }
                }}
            </div>
        </div>
    }
}

// ── Helper components ─────────────────────────────────────────────────

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

// ── Schema-aware state editor ─────────────────────────────────────────

fn render_state_editor(
    state_json: ReadSignal<String>,
    set_state_json: WriteSignal<String>,
    program_artifact: ReadSignal<Option<ProgramArtifact>>,
    persist: impl Fn() + Clone + 'static,
) -> impl IntoView {
    let parsed = parse_state(&state_json.get());
    let artifact = program_artifact.get();

    match parsed {
        Ok(state) => {
            if let Some(ref art) = artifact {
                // Schema-aware rendering: one table per table schema.
                render_schema_state_tables(art, &state, state_json, set_state_json, persist)
            } else {
                // Fallback: raw flat table.
                render_flat_state_table(&state, state_json, set_state_json, persist)
            }
        }
        Err(err) => {
            view! { <p class="error-text">{format!("state parse error: {err}")}</p> }.into_any()
        }
    }
}

fn render_schema_state_tables(
    artifact: &ProgramArtifact,
    state: &StateFile,
    state_json: ReadSignal<String>,
    set_state_json: WriteSignal<String>,
    persist: impl Fn() + Clone + 'static,
) -> AnyView {
    // Group cells by (table, row) → BTreeMap<row, BTreeMap<col, value>>
    let mut views = Vec::new();

    for schema in &artifact.table_schemas {
        let table_id = schema.id.0;
        let table_name = &schema.name;

        // Build row→col→value map for this table.
        let mut rows: BTreeMap<u64, BTreeMap<u16, Option<CoreValue>>> = BTreeMap::new();
        for cell in &state.cells {
            if cell.table == table_id {
                rows.entry(cell.row)
                    .or_default()
                    .insert(cell.col, cell.value);
            }
        }

        let col_headers: Vec<(u16, String, String)> = schema
            .columns
            .iter()
            .map(|c| (c.id.0, c.name.clone(), format!("{:?}", c.value_type)))
            .collect();

        let row_entries: Vec<(u64, Vec<(u16, String)>)> = rows
            .into_iter()
            .map(|(row_key, col_map)| {
                let values: Vec<(u16, String)> = col_headers
                    .iter()
                    .map(|(col_id, _, _)| {
                        let display = col_map
                            .get(col_id)
                            .and_then(|v| v.as_ref())
                            .map(format_value)
                            .unwrap_or_else(|| "null".to_string());
                        (*col_id, display)
                    })
                    .collect();
                (row_key, values)
            })
            .collect();

        let col_headers_for_view = col_headers.clone();
        let table_name_owned = table_name.clone();

        let table_view = view! {
            <div class="schema-table-block">
                <div class="schema-table-header">
                    <span class="table-name">{table_name_owned}</span>
                    <span class="table-id-badge">{format!("table {table_id}")}</span>
                </div>
                <div class="editor-table-wrap">
                    <table class="editor-table schema-table">
                        <thead>
                            <tr>
                                <th class="row-key-col">"row"</th>
                                {col_headers_for_view
                                    .iter()
                                    .map(|(_, name, vtype)| {
                                        let name = name.clone();
                                        let vtype = vtype.clone();
                                        view! {
                                            <th>
                                                <span class="col-name">{name}</span>
                                                <span class="col-type">{vtype}</span>
                                            </th>
                                        }
                                    })
                                    .collect_view()}
                            </tr>
                        </thead>
                        <tbody>
                            {row_entries
                                .into_iter()
                                .map(|(row_key, values)| {
                                    view! {
                                        <tr>
                                            <td class="row-key-cell">{row_key}</td>
                                            {values
                                                .into_iter()
                                                .map(|(col_id, display)| {
                                                    let persist_clone = persist.clone();
                                                    let tid = table_id;
                                                    let rk = row_key;
                                                    let cid = col_id;
                                                    view! {
                                                        <td>
                                                            <input
                                                                type="text"
                                                                prop:value=display
                                                                on:change=move |ev| {
                                                                    update_state_cell(
                                                                        state_json,
                                                                        set_state_json,
                                                                        tid,
                                                                        rk,
                                                                        cid,
                                                                        &event_target_value(&ev),
                                                                    );
                                                                    persist_clone();
                                                                }
                                                            />
                                                        </td>
                                                    }
                                                })
                                                .collect_view()}
                                        </tr>
                                    }
                                })
                                .collect_view()}
                        </tbody>
                    </table>
                </div>
            </div>
        };
        views.push(table_view);
    }

    if views.is_empty() {
        view! { <p class="muted">"No table schemas defined."</p> }.into_any()
    } else {
        views.collect_view().into_any()
    }
}

fn render_flat_state_table(
    state: &StateFile,
    state_json: ReadSignal<String>,
    set_state_json: WriteSignal<String>,
    persist: impl Fn() + Clone + 'static,
) -> AnyView {
    view! {
        <div class="editor-table-wrap">
            <table class="editor-table">
                <thead>
                    <tr>
                        <th>"#"</th>
                        <th>"table"</th>
                        <th>"row"</th>
                        <th>"col"</th>
                        <th>"value"</th>
                        <th>""</th>
                    </tr>
                </thead>
                <tbody>
                    {state
                        .cells
                        .iter()
                        .enumerate()
                        .map(|(idx, cell)| {
                            let persist_t = persist.clone();
                            let persist_r = persist.clone();
                            let persist_c = persist.clone();
                            let persist_v = persist.clone();
                            let persist_x = persist.clone();
                            let val_display = cell
                                .value
                                .as_ref()
                                .map(format_value)
                                .unwrap_or_else(|| "null".to_string());
                            let cell_table = cell.table;
                            let cell_row = cell.row;
                            let cell_col = cell.col;

                            view! {
                                <tr>
                                    <td>{idx}</td>
                                    <td>
                                        <input
                                            type="number"
                                            prop:value=cell_table
                                            on:input=move |ev| {
                                                if let Ok(mut s) = parse_state(&state_json.get()) {
                                                    if let Some(target) = s.cells.get_mut(idx)
                                                        && let Ok(next) = event_target_value(&ev).parse::<u32>()
                                                    {
                                                        target.table = next;
                                                    }
                                                    set_state_json.set(pretty_json_value(&json!(s)));
                                                    persist_t();
                                                }
                                            }
                                        />
                                    </td>
                                    <td>
                                        <input
                                            type="number"
                                            prop:value=cell_row
                                            on:input=move |ev| {
                                                if let Ok(mut s) = parse_state(&state_json.get()) {
                                                    if let Some(target) = s.cells.get_mut(idx)
                                                        && let Ok(next) = event_target_value(&ev).parse::<u64>()
                                                    {
                                                        target.row = next;
                                                    }
                                                    set_state_json.set(pretty_json_value(&json!(s)));
                                                    persist_r();
                                                }
                                            }
                                        />
                                    </td>
                                    <td>
                                        <input
                                            type="number"
                                            prop:value=cell_col
                                            on:input=move |ev| {
                                                if let Ok(mut s) = parse_state(&state_json.get()) {
                                                    if let Some(target) = s.cells.get_mut(idx)
                                                        && let Ok(next) = event_target_value(&ev).parse::<u16>()
                                                    {
                                                        target.col = next;
                                                    }
                                                    set_state_json.set(pretty_json_value(&json!(s)));
                                                    persist_c();
                                                }
                                            }
                                        />
                                    </td>
                                    <td>
                                        <input
                                            type="text"
                                            prop:value=val_display
                                            on:change=move |ev| {
                                                if let Ok(mut s) = parse_state(&state_json.get()) {
                                                    if let Some(target) = s.cells.get_mut(idx) {
                                                        let raw = event_target_value(&ev);
                                                        target.value = parse_value_input(&raw);
                                                    }
                                                    set_state_json.set(pretty_json_value(&json!(s)));
                                                    persist_v();
                                                }
                                            }
                                        />
                                    </td>
                                    <td>
                                        <button
                                            class="table-remove"
                                            on:click=move |_| {
                                                if let Ok(mut s) = parse_state(&state_json.get()) {
                                                    if idx < s.cells.len() {
                                                        s.cells.remove(idx);
                                                    }
                                                    set_state_json.set(pretty_json_value(&json!(s)));
                                                    persist_x();
                                                }
                                            }
                                        >"x"</button>
                                    </td>
                                </tr>
                            }
                        })
                        .collect_view()}
                </tbody>
            </table>
        </div>
    }
    .into_any()
}

fn update_state_cell(
    state_json: ReadSignal<String>,
    set_state_json: WriteSignal<String>,
    table: u32,
    row: u64,
    col: u16,
    raw_value: &str,
) {
    if let Ok(mut state) = parse_state(&state_json.get()) {
        let new_val = parse_value_input(raw_value);
        // Find existing cell or insert.
        if let Some(cell) = state
            .cells
            .iter_mut()
            .find(|c| c.table == table && c.row == row && c.col == col)
        {
            cell.value = new_val;
        } else {
            state.cells.push(StateCell {
                table,
                row,
                col,
                value: new_val,
            });
        }
        set_state_json.set(pretty_json_value(&json!(state)));
    }
}

// ── Schema-aware batch editor ─────────────────────────────────────────

fn render_batch_editor(
    batch_json: ReadSignal<String>,
    set_batch_json: WriteSignal<String>,
    program_artifact: ReadSignal<Option<ProgramArtifact>>,
    persist: impl Fn() + Clone + 'static,
) -> impl IntoView {
    let parsed = parse_batch(&batch_json.get());
    let artifact = program_artifact.get();

    match parsed {
        Ok(batch) => {
            if let Some(ref art) = artifact {
                render_schema_batch(art, &batch, batch_json, set_batch_json, persist)
            } else {
                render_flat_batch(&batch, batch_json, set_batch_json, persist)
            }
        }
        Err(err) => {
            view! { <p class="error-text">{format!("batch parse error: {err}")}</p> }.into_any()
        }
    }
}

fn render_schema_batch(
    artifact: &ProgramArtifact,
    batch: &BatchFile,
    batch_json: ReadSignal<String>,
    set_batch_json: WriteSignal<String>,
    persist: impl Fn() + Clone + 'static,
) -> AnyView {
    let tx_type_names: BTreeMap<u32, String> = artifact
        .tx_types
        .iter()
        .map(|t| (t.id.0, t.name.clone()))
        .collect();

    let tx_type_options: Vec<(u32, String)> = artifact
        .tx_types
        .iter()
        .map(|t| (t.id.0, t.name.clone()))
        .collect();

    let param_schemas: BTreeMap<u32, Vec<(String, String)>> = artifact
        .tx_types
        .iter()
        .map(|t| {
            (
                t.id.0,
                t.param_schema
                    .iter()
                    .map(|p| (p.name.clone(), format!("{:?}", p.value_type)))
                    .collect(),
            )
        })
        .collect();

    let views: Vec<_> = batch
        .transactions
        .iter()
        .enumerate()
        .map(|(idx, tx)| {
            let tx_name = tx_type_names
                .get(&tx.tx_type)
                .cloned()
                .unwrap_or_else(|| format!("type_{}", tx.tx_type));
            let params = param_schemas.get(&tx.tx_type).cloned().unwrap_or_default();
            let tx_type_options = tx_type_options.clone();

            let tx_type_val = tx.tx_type;
            let sender_val = tx.sender.clone();
            let nonce_val = tx.nonce;
            let param_values: Vec<String> = tx.params.iter().map(format_value).collect();

            let persist_type = persist.clone();
            let persist_nonce = persist.clone();
            let persist_sender = persist.clone();
            let persist_remove = persist.clone();

            view! {
                <div class="tx-card">
                    <div class="tx-card-header">
                        <span class="tx-index">{format!("#{idx}")}</span>
                        <select
                            prop:value=tx_type_val.to_string()
                            on:change=move |ev| {
                                if let Ok(mut b) = parse_batch(&batch_json.get())
                                    && let Some(target) = b.transactions.get_mut(idx)
                                    && let Ok(next) = event_target_value(&ev).parse::<u32>()
                                {
                                    target.tx_type = next;
                                    set_batch_json.set(pretty_json_value(&json!(b)));
                                }
                                persist_type();
                            }
                        >
                            {tx_type_options
                                .iter()
                                .map(|(id, name)| {
                                    let selected = *id == tx_type_val;
                                    let id_str = id.to_string();
                                    let name = name.clone();
                                    view! { <option value=id_str selected=selected>{name}</option> }
                                })
                                .collect_view()}
                        </select>
                        <span class="tx-name">{tx_name}</span>
                        <button
                            class="table-remove"
                            on:click=move |_| {
                                if let Ok(mut b) = parse_batch(&batch_json.get()) {
                                    if idx < b.transactions.len() {
                                        b.transactions.remove(idx);
                                    }
                                    set_batch_json.set(pretty_json_value(&json!(b)));
                                    persist_remove();
                                }
                            }
                        >"x"</button>
                    </div>
                    <div class="tx-params">
                        {params
                            .into_iter()
                            .enumerate()
                            .map(|(pidx, (pname, ptype))| {
                                let value_display = param_values.get(pidx).cloned().unwrap_or_default();
                                let persist_p = persist.clone();
                                view! {
                                    <div class="tx-param-field">
                                        <label>
                                            <span class="param-name">{pname}</span>
                                            <span class="param-type">{ptype}</span>
                                        </label>
                                        <input
                                            type="text"
                                            prop:value=value_display
                                            on:change=move |ev| {
                                                if let Ok(mut b) = parse_batch(&batch_json.get()) {
                                                    if let Some(target) = b.transactions.get_mut(idx) {
                                                        let raw = event_target_value(&ev);
                                                        if let Some(val) = parse_value_input(&raw)
                                                            && pidx < target.params.len()
                                                        {
                                                            target.params[pidx] = val;
                                                        }
                                                    }
                                                    set_batch_json.set(pretty_json_value(&json!(b)));
                                                    persist_p();
                                                }
                                            }
                                        />
                                    </div>
                                }
                            })
                            .collect_view()}
                    </div>
                    <div class="tx-meta">
                        <label>
                            <span>"sender"</span>
                            <input
                                type="text"
                                class="sender-input"
                                prop:value=sender_val
                                on:change=move |ev| {
                                    if let Ok(mut b) = parse_batch(&batch_json.get()) {
                                        if let Some(target) = b.transactions.get_mut(idx) {
                                            target.sender = event_target_value(&ev);
                                        }
                                        set_batch_json.set(pretty_json_value(&json!(b)));
                                        persist_sender();
                                    }
                                }
                            />
                        </label>
                        <label>
                            <span>"nonce"</span>
                            <input
                                type="number"
                                prop:value=nonce_val
                                on:change=move |ev| {
                                    if let Ok(mut b) = parse_batch(&batch_json.get()) {
                                        if let Some(target) = b.transactions.get_mut(idx)
                                            && let Ok(next) = event_target_value(&ev).parse::<u64>()
                                        {
                                            target.nonce = next;
                                        }
                                        set_batch_json.set(pretty_json_value(&json!(b)));
                                        persist_nonce();
                                    }
                                }
                            />
                        </label>
                    </div>
                </div>
            }
        })
        .collect();

    if views.is_empty() {
        view! { <p class="muted">"No transactions. Click \"+Tx\" to add one."</p> }.into_any()
    } else {
        views.collect_view().into_any()
    }
}

fn render_flat_batch(
    batch: &BatchFile,
    batch_json: ReadSignal<String>,
    set_batch_json: WriteSignal<String>,
    persist: impl Fn() + Clone + 'static,
) -> AnyView {
    view! {
        <div class="editor-table-wrap">
            <table class="editor-table">
                <thead>
                    <tr>
                        <th>"#"</th>
                        <th>"tx_type"</th>
                        <th>"nonce"</th>
                        <th>"sender"</th>
                        <th>"params"</th>
                        <th>""</th>
                    </tr>
                </thead>
                <tbody>
                    {batch
                        .transactions
                        .iter()
                        .enumerate()
                        .map(|(idx, tx)| {
                            let persist_t = persist.clone();
                            let persist_n = persist.clone();
                            let persist_s = persist.clone();
                            let persist_p = persist.clone();
                            let persist_x = persist.clone();
                            let tx_type_val = tx.tx_type;
                            let nonce_val = tx.nonce;
                            let sender_val = tx.sender.clone();
                            let params_val = pretty_json_inline(&tx.params);

                            view! {
                                <tr>
                                    <td>{idx}</td>
                                    <td>
                                        <input type="number" prop:value=tx_type_val
                                            on:input=move |ev| {
                                                if let Ok(mut b) = parse_batch(&batch_json.get()) {
                                                    if let Some(target) = b.transactions.get_mut(idx)
                                                        && let Ok(next) = event_target_value(&ev).parse::<u32>()
                                                    {
                                                        target.tx_type = next;
                                                    }
                                                    set_batch_json.set(pretty_json_value(&json!(b)));
                                                    persist_t();
                                                }
                                            }
                                        />
                                    </td>
                                    <td>
                                        <input type="number" prop:value=nonce_val
                                            on:input=move |ev| {
                                                if let Ok(mut b) = parse_batch(&batch_json.get()) {
                                                    if let Some(target) = b.transactions.get_mut(idx)
                                                        && let Ok(next) = event_target_value(&ev).parse::<u64>()
                                                    {
                                                        target.nonce = next;
                                                    }
                                                    set_batch_json.set(pretty_json_value(&json!(b)));
                                                    persist_n();
                                                }
                                            }
                                        />
                                    </td>
                                    <td>
                                        <input type="text" prop:value=sender_val
                                            on:input=move |ev| {
                                                if let Ok(mut b) = parse_batch(&batch_json.get()) {
                                                    if let Some(target) = b.transactions.get_mut(idx) {
                                                        target.sender = event_target_value(&ev);
                                                    }
                                                    set_batch_json.set(pretty_json_value(&json!(b)));
                                                    persist_s();
                                                }
                                            }
                                        />
                                    </td>
                                    <td>
                                        <input type="text" prop:value=params_val
                                            on:change=move |ev| {
                                                if let Ok(mut b) = parse_batch(&batch_json.get()) {
                                                    if let Some(target) = b.transactions.get_mut(idx)
                                                        && let Ok(next) = serde_json::from_str::<Vec<CoreValue>>(&event_target_value(&ev))
                                                    {
                                                        target.params = next;
                                                    }
                                                    set_batch_json.set(pretty_json_value(&json!(b)));
                                                    persist_p();
                                                }
                                            }
                                        />
                                    </td>
                                    <td>
                                        <button class="table-remove"
                                            on:click=move |_| {
                                                if let Ok(mut b) = parse_batch(&batch_json.get()) {
                                                    if idx < b.transactions.len() {
                                                        b.transactions.remove(idx);
                                                    }
                                                    set_batch_json.set(pretty_json_value(&json!(b)));
                                                    persist_x();
                                                }
                                            }
                                        >"x"</button>
                                    </td>
                                </tr>
                            }
                        })
                        .collect_view()}
                </tbody>
            </table>
        </div>
    }
    .into_any()
}

// ── Structured proof display ──────────────────────────────────────────

fn render_proof_display(
    stark_summary: ReadSignal<Option<StarkProofSummary>>,
    verify_report: ReadSignal<Option<VerifyReport>>,
    proof_log_json: ReadSignal<String>,
) -> impl IntoView {
    let stark = stark_summary.get();
    let verify = verify_report.get();

    if let Some(ref stark) = stark {
        let verified_class = if stark.verified {
            "badge-ok"
        } else {
            "badge-err"
        };
        let verified_text = if stark.verified { "Verified" } else { "Failed" };

        let chips: Vec<_> = stark.chips.clone();
        let old_root: Vec<_> = stark.old_state_root.clone();
        let new_root: Vec<_> = stark.new_state_root.clone();

        let prove_ms = stark.prove_time_ms;
        let verify_ms = stark.verify_time_ms;
        let total_ms = prove_ms + verify_ms;
        let prove_pct = if total_ms > 0 {
            (prove_ms as f64 / total_ms as f64 * 100.0) as u32
        } else {
            50
        };

        let verify_msg = verify
            .map(|v| v.message)
            .unwrap_or_else(|| "pending".to_string());

        view! {
            <div class="stark-result">
                <div class="stark-header">
                    <span class=format!("badge {verified_class}")>{verified_text}</span>
                    <span class="stark-scheme">{stark.scheme.clone()}</span>
                    <span class="stark-stmt-hash" title="statement hash">{format!("stmt: {}...", &stark.statement_hash[..12.min(stark.statement_hash.len())])}</span>
                </div>

                <div class="stark-timing">
                    <div class="timing-bar">
                        <div class="timing-prove" style=format!("width: {}%", prove_pct)>
                            {format!("prove {}ms", prove_ms)}
                        </div>
                        <div class="timing-verify" style=format!("width: {}%", 100 - prove_pct)>
                            {format!("verify {}ms", verify_ms)}
                        </div>
                    </div>
                </div>

                <div class="stark-roots">
                    <div class="root-block">
                        <span class="root-label">"old root"</span>
                        <code>{old_root.join(" ")}</code>
                    </div>
                    <div class="root-block">
                        <span class="root-label">"new root"</span>
                        <code>{new_root.join(" ")}</code>
                    </div>
                </div>

                <div class="stark-chips">
                    <h4>{format!("Chips ({})", chips.len())}</h4>
                    <div class="chip-grid">
                        {chips
                            .into_iter()
                            .map(|chip| {
                                view! {
                                    <div class="chip-entry">
                                        <span class="chip-name">{chip.name}</span>
                                        <span class="chip-height">{format!("{} rows", chip.trace_height)}</span>
                                    </div>
                                }
                            })
                            .collect_view()}
                    </div>
                </div>

                <div class="verify-msg muted">{verify_msg}</div>
            </div>
        }
        .into_any()
    } else {
        let log = proof_log_json.get();
        if log.is_empty() {
            view! { <p class="muted">"No proof results yet. Deploy and submit a batch."</p> }
                .into_any()
        } else {
            view! { <pre>{log}</pre> }.into_any()
        }
    }
}

// ── Utility functions ─────────────────────────────────────────────────

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

fn pretty_json_value(value: &JsonValue) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn pretty_json_inline<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

fn opt_token(token: String) -> Option<String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn format_value(v: &CoreValue) -> String {
    match v {
        CoreValue::U64(n) => n.to_string(),
        CoreValue::I64(n) => n.to_string(),
        CoreValue::Bool(b) => b.to_string(),
        CoreValue::Bytes32(d) => format!("0x{}", hex::encode(d)),
    }
}

fn parse_value_input(raw: &str) -> Option<CoreValue> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return None;
    }

    // Try parsing as JSON value first.
    if let Ok(v) = serde_json::from_str::<CoreValue>(trimmed) {
        return Some(v);
    }

    // Try bare integer.
    if let Ok(n) = trimmed.parse::<u64>() {
        return Some(CoreValue::U64(n));
    }
    if let Ok(n) = trimmed.parse::<i64>() {
        return Some(CoreValue::I64(n));
    }

    // Try boolean.
    match trimmed {
        "true" => return Some(CoreValue::Bool(true)),
        "false" => return Some(CoreValue::Bool(false)),
        _ => {}
    }

    None
}

fn default_value_for_type(type_name: &str) -> CoreValue {
    match type_name {
        "U64" => CoreValue::U64(0),
        "I64" => CoreValue::I64(0),
        "Bool" => CoreValue::Bool(false),
        _ => CoreValue::U64(0),
    }
}
