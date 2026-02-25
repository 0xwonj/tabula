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

/// Base URL prefix read from Trunk's `<base href="...">` tag at runtime.
/// Returns e.g. `/tabula` on GitHub Pages, empty string for local dev.
pub fn base_url() -> String {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.query_selector("base").ok().flatten())
        .and_then(|el| el.get_attribute("href"))
        .map(|h| h.trim_end_matches('/').to_string())
        .unwrap_or_default()
}

/// Internal SPA link — prepends the base so the Router's click handler
/// recognises the path and performs client-side navigation.
pub fn app_href(path: &str) -> String {
    format!("{}{path}", base_url())
}

/// Link to the mdBook documentation (static site, not SPA).
pub fn docs_href() -> String {
    format!("{}/docs", base_url())
}

/// Minimal site-wide navigation bar.
#[component]
pub fn SiteNav() -> impl IntoView {
    let is_dark = RwSignal::new(initial_is_dark());

    Effect::new(move |_| apply_theme(is_dark.get()));

    let toggle = move |_| is_dark.update(|d| *d = !*d);

    view! {
        <nav class="site-nav">
            <A href=app_href("/") attr:class="nav-brand">
                <img class="nav-logo nav-logo-dark" src="/logo-dark.svg" alt="" width="16" height="16"/>
                <img class="nav-logo nav-logo-light" src="/logo-light.svg" alt="" width="16" height="16"/>
                "tabula"
            </A>
            <div class="nav-links">
                <a href=docs_href() class="nav-link">"Docs"</a>
                <A href=app_href("/playground") attr:class="nav-link">"Playground"</A>
                <button class="nav-theme-btn" on:click=toggle title="Toggle theme">
                    {move || if is_dark.get() { "\u{2600}" } else { "\u{263E}" }}
                </button>
            </div>
        </nav>
    }
}
