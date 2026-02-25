use leptos::prelude::*;
use leptos_router::components::A;

use crate::layout::nav::docs_href;

/// Hero: bold tagline, subtitle, and two CTA buttons.
#[component]
pub fn HeroSection() -> impl IntoView {
    view! {
        <section class="hero">
            <h1 class="hero-title">
                "Prove state transitions,"<br />"not execution."
            </h1>
            <p class="hero-sub">
                "A co-designed IR, commitment, and constraint system for typed tabular state."
            </p>
            <div class="hero-cta">
                <A href="/playground" attr:class="action filled">"Playground"</A>
                <a href=docs_href() class="action ghost">"Documentation"</a>
            </div>
        </section>
    }
}
