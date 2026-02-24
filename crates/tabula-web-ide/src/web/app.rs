use std::cell::RefCell;
use std::rc::Rc;

use gloo_file::callbacks::FileReader;
use leptos::{html, prelude::*};

use crate::web::app_state::AppSignals;
use crate::web::components::bottom_panel::BottomPanel;
use crate::web::components::run_history::RunHistory;
use crate::web::components::settings_drawer::SettingsDrawer;
use crate::web::components::topbar::Topbar;
use crate::web::components::workspace_panels::WorkspacePanels;
use crate::web::handlers;
use crate::web::storage;
use crate::web::templates::default_workspace;

#[component]
pub fn App() -> impl IntoView {
    let mut initial = storage::load_workspace().unwrap_or_else(default_workspace);
    if initial.program_source.trim().is_empty() {
        initial = default_workspace();
    }

    let s = AppSignals::new(initial);

    // File reader holders for async file imports.
    let proof_reader: Rc<RefCell<Option<FileReader>>> = Rc::new(RefCell::new(None));
    let workspace_reader: Rc<RefCell<Option<FileReader>>> = Rc::new(RefCell::new(None));
    let proof_input_ref: NodeRef<html::Input> = NodeRef::new();
    let workspace_input_ref: NodeRef<html::Input> = NodeRef::new();

    // Handler factories.
    let connect_daemon = handlers::connect_daemon(s);
    let run_check = handlers::run_check(s);
    let run_deploy = handlers::run_deploy(s);
    let run_submit = handlers::run_submit(s);
    let load_template = handlers::load_template(s);
    let add_state_row = handlers::add_state_row(s);
    let add_tx_row = handlers::add_tx_row(s);
    let export_workspace = handlers::export_workspace(s);
    let export_proof = handlers::export_proof(s);
    let import_workspace_text = handlers::import_workspace_text(s);
    let open_proof_picker = handlers::open_file_picker(proof_input_ref);
    let open_workspace_picker = handlers::open_file_picker(workspace_input_ref);
    let on_proof_file_change = handlers::on_proof_file_change(s, proof_reader.clone());
    let on_workspace_file_change = handlers::on_workspace_file_change(s, workspace_reader.clone());

    view! {
        <div class="tabula-app">
            <Topbar s=s connect_daemon=connect_daemon />

            // Hidden file inputs (always in DOM, triggered by buttons).
            <input class="hidden" node_ref=workspace_input_ref type="file" accept="application/json" on:change=on_workspace_file_change />
            <input class="hidden" node_ref=proof_input_ref type="file" accept="application/json" on:change=on_proof_file_change />

            <SettingsDrawer
                s=s
                export_workspace=export_workspace
                open_workspace_picker=open_workspace_picker
                export_proof=export_proof
                open_proof_picker=open_proof_picker
                import_workspace_text=import_workspace_text
            />

            <WorkspacePanels
                s=s
                run_check=run_check
                run_deploy=run_deploy
                run_submit=run_submit
                load_template=load_template
                add_state_row=add_state_row
                add_tx_row=add_tx_row
            />

            <BottomPanel s=s />
            <RunHistory s=s />
        </div>
    }
}
