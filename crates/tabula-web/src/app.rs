use leptos::prelude::*;
use leptos_router::components::{A, Route, Router, Routes};
use leptos_router::path;

use crate::layout::nav::SiteNav;
use crate::pages::home::HomePage;
use crate::pages::playground::PlaygroundPage;

/// Root application component with client-side routing.
#[component]
pub fn App() -> impl IntoView {
    view! {
        <Router>
            <SiteNav />
            <main class="site-main">
                <Routes fallback=|| view! { <NotFound /> }>
                    <Route path=path!("/") view=HomePage />
                    <Route path=path!("/playground") view=PlaygroundPage />
                </Routes>
            </main>
        </Router>
    }
}

/// 404 fallback page.
#[component]
fn NotFound() -> impl IntoView {
    view! {
        <div class="not-found">
            <h1>"404"</h1>
            <p>"page not found"</p>
            <A href="/" attr:class="action ghost">"back to home"</A>
        </div>
    }
}
