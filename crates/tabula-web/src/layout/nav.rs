use leptos::prelude::*;
use leptos_router::components::A;

fn initial_is_dark() -> bool {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return true,
    };
    if let Ok(Some(storage)) = window.local_storage() {
        if let Ok(Some(saved)) = storage.get_item("tabula.theme") {
            return saved != "light";
        }
    }
    if let Ok(Some(mq)) = window.match_media("(prefers-color-scheme: light)") {
        if mq.matches() {
            return false;
        }
    }
    true
}

fn apply_theme(dark: bool) {
    let theme = if dark { "dark" } else { "light" };
    let Some(window) = web_sys::window() else {
        return;
    };
    if let Some(el) = window.document().and_then(|d| d.document_element()) {
        let _ = el.set_attribute("data-theme", theme);
    }
    if let Ok(Some(storage)) = window.local_storage() {
        let _ = storage.set_item("tabula.theme", theme);
    }
}

/// Base URL prefix (e.g. `/tabula` on GitHub Pages, empty for local dev).
pub fn base_url() -> &'static str {
    option_env!("BASE_URL").unwrap_or_default()
}

/// Resolve an asset path against the base URL.
pub fn asset_href(path: &str) -> String {
    let base = base_url();
    format!("{base}/{path}")
}

/// Link to the mdBook documentation (static, outside SPA).
pub fn docs_href() -> String {
    let base = base_url();
    format!("{base}/docs")
}

/// Minimal site-wide navigation bar.
#[component]
pub fn SiteNav() -> impl IntoView {
    let is_dark = RwSignal::new(initial_is_dark());

    Effect::new(move |_| apply_theme(is_dark.get()));

    let toggle = move |_| is_dark.update(|d| *d = !*d);

    view! {
        <nav class="site-nav">
            <A href="/" attr:class="nav-brand">
                <img class="nav-logo nav-logo-dark" src=asset_href("logo-dark.svg") alt="" width="16" height="16"/>
                <img class="nav-logo nav-logo-light" src=asset_href("logo-light.svg") alt="" width="16" height="16"/>
                "tabula"
            </A>
            <div class="nav-links">
                <a href=docs_href() class="nav-link">"Docs"</a>
                <A href="/playground" attr:class="nav-link">"Playground"</A>
                <button class="nav-theme-btn" on:click=toggle title="Toggle theme">
                    {move || if is_dark.get() { "\u{2600}" } else { "\u{263E}" }}
                </button>
            </div>
        </nav>
    }
}
