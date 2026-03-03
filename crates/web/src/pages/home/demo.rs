use leptos::prelude::*;

/// Animated split-pane demo: code on the left, state transition on the right.
/// Replays the staggered animation on hover.
#[component]
pub fn Demo() -> impl IntoView {
    let demo_ref = NodeRef::<leptos::html::Section>::new();

    let on_mouseenter = move |_| {
        if let Some(el) = demo_ref.get() {
            let el: &web_sys::HtmlElement = el.as_ref();
            let class_list = el.class_list();
            let _ = class_list.remove_1("demo-replay");
            // Force reflow to restart the animation
            let _ = el.offset_width();
            let _ = class_list.add_1("demo-replay");
        }
    };

    view! {
        <section class="demo" node_ref=demo_ref on:mouseenter=on_mouseenter>
            // ── Left: code ──
            <div class="demo-code anim-d1">
                <div class="demo-bar">
                    <span class="demo-bar-dot" />
                    <span class="demo-bar-dot" />
                    <span class="demo-bar-dot" />
                    <span class="demo-bar-name">"transfer.tab"</span>
                </div>
                <pre class="demo-src">{
"table balances {
    balance: u64,
}

tx transfer(from, to, amount) {
    let a = balances[from].balance
    let b = balances[to].balance
    assert a >= amount
    balances[from].balance = a - amount
    balances[to].balance   = b + amount
}"
                }</pre>
            </div>

            // ── Right: state transition ──
            <div class="demo-right">
                // Before state
                <div class="demo-table-wrap anim-d2">
                    <table class="demo-table">
                        <thead><tr><th>"row"</th><th>"balance"</th></tr></thead>
                        <tbody>
                            <tr><td class="row-id">"0"</td><td>"100"</td></tr>
                            <tr><td class="row-id">"1"</td><td>"50"</td></tr>
                            <tr><td class="row-id">"2"</td><td>"25"</td></tr>
                        </tbody>
                    </table>
                </div>

                // Execution badge
                <div class="demo-exec anim-d3">
                    <span class="demo-exec-arrow">"\u{2193}"</span>
                    <span class="demo-exec-label">"transfer(0, 2, 30)"</span>
                </div>

                // After state
                <div class="demo-table-wrap anim-d4">
                    <table class="demo-table">
                        <thead><tr><th>"row"</th><th>"balance"</th></tr></thead>
                        <tbody>
                            <tr><td class="row-id">"0"</td><td class="val-changed">"70"</td></tr>
                            <tr><td class="row-id">"1"</td><td>"50"</td></tr>
                            <tr><td class="row-id">"2"</td><td class="val-changed">"55"</td></tr>
                        </tbody>
                    </table>
                </div>

                // Proof result
                <div class="demo-proof anim-d5">
                    <span class="proof-check">"\u{2713}"</span>
                    " proved \u{00b7} 9 chips \u{00b7} 670 columns"
                </div>
            </div>
        </section>
    }
}
