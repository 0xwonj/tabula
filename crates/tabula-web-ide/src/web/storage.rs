use wasm_bindgen::JsCast;
use web_sys::{Blob, BlobPropertyBag, HtmlAnchorElement, Url};

use crate::web::models::WorkspaceDoc;

const STORAGE_KEY: &str = "tabula.web_ide.workspace.v1";

pub fn load_workspace() -> Option<WorkspaceDoc> {
    let storage = window_storage()?;
    let raw = storage.get_item(STORAGE_KEY).ok()??;
    serde_json::from_str(&raw).ok()
}

pub fn save_workspace(ws: &WorkspaceDoc) {
    let Some(storage) = window_storage() else {
        return;
    };

    if let Ok(serialized) = serde_json::to_string(ws) {
        let _ = storage.set_item(STORAGE_KEY, &serialized);
    }
}

pub fn export_text_file(filename: &str, content: &str) -> Result<(), String> {
    let window = web_sys::window().ok_or_else(|| "window unavailable".to_string())?;
    let document = window
        .document()
        .ok_or_else(|| "document unavailable".to_string())?;

    let bag = BlobPropertyBag::new();
    bag.set_type("application/json;charset=utf-8");

    let parts = js_sys::Array::new();
    parts.push(&wasm_bindgen::JsValue::from_str(content));

    let blob = Blob::new_with_str_sequence_and_options(&parts, &bag)
        .map_err(|_| "failed to create blob".to_string())?;
    let url = Url::create_object_url_with_blob(&blob)
        .map_err(|_| "failed to create blob URL".to_string())?;

    let anchor: HtmlAnchorElement = document
        .create_element("a")
        .map_err(|_| "failed to create anchor".to_string())?
        .dyn_into()
        .map_err(|_| "failed to cast anchor".to_string())?;

    anchor.set_href(&url);
    anchor.set_download(filename);
    let _ = anchor.style().set_property("display", "none");

    if let Some(body) = document.body() {
        let _ = body.append_child(&anchor);
        anchor.click();
        anchor.remove();
    }

    let _ = Url::revoke_object_url(&url);
    Ok(())
}

pub fn now_ms() -> f64 {
    js_sys::Date::now()
}

fn window_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}
