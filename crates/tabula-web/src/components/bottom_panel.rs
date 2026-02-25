//! Bottom panel: result tabs (diagnostics, execution, proof, trace, IR).

use leptos::prelude::*;

use crate::components::results::{render_proof_display, tab_button};
use crate::state::AppSignals;

#[component]
pub(crate) fn BottomPanel(s: AppSignals) -> impl IntoView {
    view! {
        <section class="panel bottom-panel reveal-delay-3">
            <div class="tab-row">
                {tab_button(s.active_tab, s.set_active_tab, "diagnostics", "Diagnostics")}
                {tab_button(s.active_tab, s.set_active_tab, "execution", "Execution")}
                {tab_button(s.active_tab, s.set_active_tab, "proof", "Proof")}
                {tab_button(s.active_tab, s.set_active_tab, "trace", "Trace")}
                {tab_button(s.active_tab, s.set_active_tab, "ir", "IR")}
            </div>

            <div class="tab-content">
                {move || match s.active_tab.get().as_str() {
                    "diagnostics" => view! { <pre>{move || s.diagnostics_text.get()}</pre> }.into_any(),
                    "execution" => view! { <pre>{move || s.execution_json.get()}</pre> }.into_any(),
                    "proof" => {
                        view! {
                            <div class="proof-display">
                                {move || render_proof_display(s.stark_summary, s.verify_report, s.proof_log_json)}
                            </div>
                        }.into_any()
                    }
                    "trace" => view! { <pre>{move || s.trace_json.get()}</pre> }.into_any(),
                    "ir" => view! { <pre>{move || s.compiled_ir_json.get()}</pre> }.into_any(),
                    _ => view! { <pre>""</pre> }.into_any(),
                }}
            </div>
        </section>
    }
}
