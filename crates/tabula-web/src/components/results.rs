//! Result display components: tab buttons, proof display.

use leptos::prelude::*;

use crate::models::{StarkProofSummary, VerifyReport};

/// Tab button for the bottom result panel.
pub(crate) fn tab_button(
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

/// Structured STARK proof display.
pub(crate) fn render_proof_display(
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
