use leptos::prelude::*;

/// Three numbered differentiator cards in a clean grid.
#[component]
pub fn FeatureCards() -> impl IntoView {
    view! {
        <section class="features">
            <div class="feature-card">
                <p class="feature-card-number">"01"</p>
                <h3 class="feature-card-title">"No ISA Overhead"</h3>
                <p class="feature-card-desc">
                    "IR maps directly to AIR chip rows. No fetch-decode-execute cycle."
                </p>
            </div>
            <div class="feature-card">
                <p class="feature-card-number">"02"</p>
                <h3 class="feature-card-title">"Typed State"</h3>
                <p class="feature-card-desc">
                    "Per-type chips, per-column commitment. Untouched columns incur zero proof cost."
                </p>
            </div>
            <div class="feature-card">
                <p class="feature-card-number">"03"</p>
                <h3 class="feature-card-title">"True SSA"</h3>
                <p class="feature-card-desc">
                    "Variables are trace columns, not memory rows. Consistency is O(accesses), not O(instructions)."
                </p>
            </div>
        </section>
    }
}
