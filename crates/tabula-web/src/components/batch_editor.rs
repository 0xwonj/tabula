//! Schema-aware batch (transaction) editor component.

use std::collections::BTreeMap;

use leptos::prelude::*;
use serde_json::json;
use tabula_core::Value as CoreValue;

use crate::models::{BatchFile, ProgramArtifact};
use crate::utils::{
    format_value, parse_batch, parse_value_input, pretty_json_inline, pretty_json_value,
};

/// Top-level batch editor: dispatches to schema-aware or flat view.
pub(crate) fn render_batch_editor(
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
