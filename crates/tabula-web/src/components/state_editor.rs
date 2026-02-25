//! Schema-aware state editor component.

use std::collections::BTreeMap;

use leptos::prelude::*;
use serde_json::json;
use tabula_core::Value as CoreValue;

use crate::models::{ProgramArtifact, StateCell, StateFile};
use crate::utils::{format_value, parse_state, parse_value_input, pretty_json_value};

/// Top-level state editor: dispatches to schema-aware or flat view.
pub(crate) fn render_state_editor(
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
                render_schema_state_tables(art, &state, state_json, set_state_json, persist)
            } else {
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
    let mut views = Vec::new();

    for schema in &artifact.table_schemas {
        let table_id = schema.id.0;
        let table_name = &schema.name;

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
