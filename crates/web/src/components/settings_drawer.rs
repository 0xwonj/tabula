//! Collapsible settings panel: daemon URL, auth token, trace toggle, import/export.

use leptos::prelude::*;

use crate::state::AppSignals;

#[component]
pub(crate) fn SettingsDrawer(
    s: AppSignals,
    export_workspace: impl Fn(web_sys::MouseEvent) + Clone + 'static,
    open_workspace_picker: impl Fn(web_sys::MouseEvent) + Clone + 'static,
    export_proof: impl Fn(web_sys::MouseEvent) + Clone + 'static,
    open_proof_picker: impl Fn(web_sys::MouseEvent) + Clone + 'static,
    import_workspace_text: impl Fn(web_sys::MouseEvent) + Clone + 'static,
) -> impl IntoView {
    view! {
        <section
            class="settings-drawer reveal-delay-1"
            style=move || if s.show_settings.get() { "" } else { "display:none" }
        >
            <div class="settings-grid">
                <label>
                    <span>"Daemon URL"</span>
                    <input
                        type="text"
                        prop:value=move || s.daemon_url.get()
                        on:input=move |ev| {
                            s.set_daemon_url.set(event_target_value(&ev));
                            s.persist();
                        }
                        placeholder="http://127.0.0.1:4317"
                    />
                </label>
                <label>
                    <span>"Bearer Token"</span>
                    <input
                        type="password"
                        prop:value=move || s.auth_token.get()
                        on:input=move |ev| {
                            s.set_auth_token.set(event_target_value(&ev));
                            s.persist();
                        }
                        placeholder="optional"
                    />
                </label>
                <label class="checkbox-row">
                    <input
                        type="checkbox"
                        prop:checked=move || s.include_trace.get()
                        on:change=move |ev| {
                            s.set_include_trace.set(event_target_checked(&ev));
                            s.persist();
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
                        prop:value=move || s.workspace_import_json.get()
                        on:input=move |ev| s.set_workspace_import_json.set(event_target_value(&ev))
                        placeholder="Paste workspace JSON"
                    ></textarea>
                    <button class="action ghost" on:click=import_workspace_text>"Import JSON"</button>
                </div>
                <div class="health-box">
                    {move || {
                        if let Some(h) = s.health.get() {
                            view! { <span class="muted">{format!("{} {} v{}", h.service, h.status, h.version)}</span> }.into_any()
                        } else {
                            view! { <span class="muted">"not connected"</span> }.into_any()
                        }
                    }}
                </div>
            </div>
        </section>
    }
}
