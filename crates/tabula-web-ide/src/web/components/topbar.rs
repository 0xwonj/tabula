//! Top bar: brand, deploy status, connect button, settings toggle, status pill.

use leptos::prelude::*;

use crate::web::app_state::AppSignals;

#[component]
pub(crate) fn Topbar(
    s: AppSignals,
    connect_daemon: impl Fn(web_sys::MouseEvent) + Clone + 'static,
) -> impl IntoView {
    view! {
        <header class="topbar glass">
            <div class="brand">
                <span class="kicker">"Tabula"</span>
                <h1>"Playground"</h1>
            </div>
            <div class="topbar-center">
                <div class="deploy-status">
                    {move || {
                        if let Some(id) = s.deployed_instance_id.get() {
                            let short = if id.len() > 12 {
                                format!("{}...", &id[..12])
                            } else {
                                id.clone()
                            };
                            view! {
                                <span class="badge badge-ok">"Deployed"</span>
                                <span class="instance-id">{short}</span>
                                <span class="version-tag">{format!("v{}", s.deployed_instance_version.get())}</span>
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
                    on:click=move |_| s.set_show_settings.set(!s.show_settings.get())
                >
                    {move || if s.show_settings.get() { "Close Settings" } else { "Settings" }}
                </button>
                <button class="action ghost" on:click=connect_daemon disabled=move || s.busy_action.get().is_some()>
                    "Connect"
                </button>
                <div class="status-pill">
                    <span class="dot"></span>
                    {move || s.status_line.get()}
                </div>
            </div>
        </header>
    }
}
