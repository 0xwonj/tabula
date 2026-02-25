use leptos::prelude::*;
use leptos_router::components::A;

use crate::layout::nav::docs_href;

/// Closing CTA section with centered heading, feature summary, and links.
#[component]
pub fn FooterCta() -> impl IntoView {
    view! {
        <section class="cta-section">
            <h2 class="cta-heading">"Start building."</h2>
            <div class="features-row">
                <span class="feature">"No ISA overhead"</span>
                <span class="feature-dot" />
                <span class="feature">"Typed per-column VC"</span>
                <span class="feature-dot" />
                <span class="feature">"O(accesses) memory consistency"</span>
            </div>
            <div class="cta-links">
                <A href="/playground" attr:class="action filled">"Playground"</A>
                <a href=docs_href() class="action ghost">"Documentation"</a>
            </div>
        </section>
    }
}
