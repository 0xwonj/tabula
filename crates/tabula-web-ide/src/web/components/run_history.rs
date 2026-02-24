//! Floating run history display (most recent 5 entries).

use leptos::prelude::*;

use crate::web::app_state::AppSignals;

#[component]
pub(crate) fn RunHistory(s: AppSignals) -> impl IntoView {
    view! {
        <div class="history-float">
            {move || {
                let history = s.run_history.get();
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
    }
}
