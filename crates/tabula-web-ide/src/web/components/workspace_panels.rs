//! Left (Program) and Right (State + Tx Builder) workspace panels.

use leptos::prelude::*;

use crate::web::app_state::AppSignals;
use crate::web::components::batch_editor::render_batch_editor;
use crate::web::components::state_editor::render_state_editor;
use crate::web::templates::built_in_templates;

#[component]
pub(crate) fn WorkspacePanels(
    s: AppSignals,
    run_check: impl Fn(web_sys::MouseEvent) + Clone + 'static,
    run_deploy: impl Fn(web_sys::MouseEvent) + Clone + 'static,
    run_submit: impl Fn(web_sys::MouseEvent) + Clone + 'static,
    load_template: impl Fn(&'static str) + Clone + 'static,
    add_state_row: impl Fn(web_sys::MouseEvent) + Clone + 'static,
    add_tx_row: impl Fn(web_sys::MouseEvent) + Clone + 'static,
) -> impl IntoView {
    view! {
        <section class="workspace-grid">
            // Left panel: Program + Actions + Templates
            <div class="panel glass left-panel reveal-delay-1">
                <div class="panel-head">
                    <h2>"Program"</h2>
                    <small class="muted">"Tabula DSL"</small>
                </div>
                <textarea
                    class="code-editor"
                    prop:value=move || s.program_source.get()
                    on:input=move |ev| {
                        s.set_program_source.set(event_target_value(&ev));
                        s.persist();
                    }
                    spellcheck="false"
                ></textarea>

                <div class="left-actions">
                    <button class="action" on:click=run_check disabled=move || s.busy_action.get().is_some()>
                        "Check"
                    </button>
                    <button class="action" on:click=run_deploy disabled=move || s.busy_action.get().is_some()>
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
                            let load = load_template.clone();
                            view! {
                                <button class="template-chip" title=description on:click=move |_| load(id)>
                                    {title}
                                </button>
                            }
                        })
                        .collect_view()}
                </div>
            </div>

            // Right panel: State + Tx Builder
            <div class="panel glass right-panel reveal-delay-2">
                // State Tables section.
                <div class="panel-head inline">
                    <h2>"State"</h2>
                    <button class="action ghost" on:click=add_state_row>"+ Row"</button>
                </div>
                <div class="state-section">
                    {move || render_state_editor(s.state_json, s.set_state_json, s.program_artifact, move || s.persist())}
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
                            disabled=move || s.busy_action.get().is_some() || s.deployed_instance_id.get().is_none()
                        >
                            "Submit Batch"
                        </button>
                    </div>
                </div>
                <div class="tx-section">
                    {move || render_batch_editor(s.batch_json, s.set_batch_json, s.program_artifact, move || s.persist())}
                </div>
            </div>
        </section>
    }
}
