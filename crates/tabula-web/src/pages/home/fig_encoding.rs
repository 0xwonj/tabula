use leptos::prelude::*;

/// Figure 2: Typed per-column commitment — SSMC vs SMT comparison.
///
/// Animation narrative (logical flow):
///  1. Schema table appears — header, column names, types
///  2. FE blocks grow to reveal per-type encoding widths
///  3. Data rows fade in — concrete values
///  4. Independence arrows drop — columns commit separately
///  5. SSMC chain slides in left-to-right, SMT tree builds top-down
///  6. Cost summaries + footer fade in
#[component]
pub fn FigEncoding() -> impl IntoView {
    let fig_ref = NodeRef::<leptos::html::Div>::new();

    let on_mouseenter = move |_| {
        if let Some(el) = fig_ref.get() {
            let el: &web_sys::HtmlElement = el.as_ref();
            let cl = el.class_list();
            let _ = cl.remove_1("fig-enc-replay");
            let _ = el.offset_width(); // force reflow
            let _ = cl.add_1("fig-enc-replay");
        }
    };

    view! {
        <div class="fig-panel fig-commit" node_ref=fig_ref on:mouseenter=on_mouseenter>

            // ── Phase 1–3: Schema table ──
            <div class="fig-commit-table-section">
                <div class="fig-commit-table-header fig-enc-fade" style="--d:0">
                    <span class="fig-commit-table-name">"balances"</span>
                </div>
                <table class="fig-commit-schema-table">
                    <thead>
                        <tr>
                            <th class="fig-commit-th-row"></th>
                            <th>
                                <div class="fig-commit-col-header fig-enc-fade" style="--d:60">
                                    <span class="fig-commit-col-name">"active"</span>
                                    <span class="fig-commit-col-type">"Bool"</span>
                                    <div class="fig-commit-fe-blocks">
                                        <span class="fig-commit-fe-block fe-bool fig-enc-grow"
                                              style="--d:200"></span>
                                    </div>
                                    <span class="fig-commit-fe-label fig-enc-fade"
                                          style="--d:300">"1 FE"</span>
                                </div>
                            </th>
                            <th>
                                <div class="fig-commit-col-header fig-enc-fade" style="--d:120">
                                    <span class="fig-commit-col-name">"balance"</span>
                                    <span class="fig-commit-col-type">"U64"</span>
                                    <div class="fig-commit-fe-blocks">
                                        <span class="fig-commit-fe-block fe-u64 fig-enc-grow"
                                              style="--d:250"></span>
                                        <span class="fig-commit-fe-block fe-u64 fig-enc-grow"
                                              style="--d:280"></span>
                                        <span class="fig-commit-fe-block fe-u64 fig-enc-grow"
                                              style="--d:310"></span>
                                    </div>
                                    <span class="fig-commit-fe-label fig-enc-fade"
                                          style="--d:380">"3 FE"</span>
                                </div>
                            </th>
                            <th>
                                <div class="fig-commit-col-header fig-enc-fade" style="--d:180">
                                    <span class="fig-commit-col-name">"owner"</span>
                                    <span class="fig-commit-col-type">"Bytes32"</span>
                                    <div class="fig-commit-fe-blocks">
                                        <span class="fig-commit-fe-block fe-digest fig-enc-grow"
                                              style="--d:300"></span>
                                        <span class="fig-commit-fe-block fe-digest fig-enc-grow"
                                              style="--d:315"></span>
                                        <span class="fig-commit-fe-block fe-digest fig-enc-grow"
                                              style="--d:330"></span>
                                        <span class="fig-commit-fe-block fe-digest fig-enc-grow"
                                              style="--d:345"></span>
                                        <span class="fig-commit-fe-block fe-digest fig-enc-grow"
                                              style="--d:360"></span>
                                        <span class="fig-commit-fe-block fe-digest fig-enc-grow"
                                              style="--d:375"></span>
                                        <span class="fig-commit-fe-block fe-digest fig-enc-grow"
                                              style="--d:390"></span>
                                        <span class="fig-commit-fe-block fe-digest fig-enc-grow"
                                              style="--d:405"></span>
                                    </div>
                                    <span class="fig-commit-fe-label fig-enc-fade"
                                          style="--d:470">"8 FE"</span>
                                </div>
                            </th>
                        </tr>
                    </thead>
                    <tbody>
                        <tr class="fig-enc-fade" style="--d:420">
                            <td class="fig-commit-row-id">"0"</td>
                            <td class="fig-commit-val-bool">"true"</td>
                            <td class="fig-commit-val-num">"1\u{202f}000"</td>
                            <td class="fig-commit-val-hash">"0xab\u{2026}3f"</td>
                        </tr>
                        <tr class="fig-enc-fade" style="--d:460">
                            <td class="fig-commit-row-id">"1"</td>
                            <td class="fig-commit-val-bool">"true"</td>
                            <td class="fig-commit-val-num">"50"</td>
                            <td class="fig-commit-val-hash">"0xcd\u{2026}91"</td>
                        </tr>
                        <tr class="fig-enc-fade" style="--d:500">
                            <td class="fig-commit-row-id">"2"</td>
                            <td class="fig-commit-val-bool">"false"</td>
                            <td class="fig-commit-val-num">"0"</td>
                            <td class="fig-commit-val-hash">"0x00\u{2026}00"</td>
                        </tr>
                    </tbody>
                </table>
            </div>

            // ── Phase 4: Independence ──
            <div class="fig-commit-divider">
                <div class="fig-commit-arrows">
                    <span class="fig-commit-arrow fig-enc-drop" style="--d:570">{"\u{2193}"}</span>
                    <span class="fig-commit-arrow fig-enc-drop" style="--d:620">{"\u{2193}"}</span>
                    <span class="fig-commit-arrow fig-enc-drop" style="--d:670">{"\u{2193}"}</span>
                </div>
                <span class="fig-commit-divider-label fig-enc-fade" style="--d:730">
                    "Each column committed independently"
                </span>
            </div>

            // ── Phase 5: SSMC vs SMT ──
            <div class="fig-commit-compare">
                // ── Left: SSMC ──
                <div class="fig-commit-card">
                    <div class="fig-commit-card-head fig-enc-fade" style="--d:800">
                        <span class="fig-commit-card-tag fig-tag-ssmc">"SSMC"</span>
                        <span class="fig-commit-card-size">"3 entries"</span>
                    </div>

                    // Hash chain — slides in left-to-right
                    <div class="fig-commit-chain">
                        <div class="fig-commit-chain-node fig-enc-chain" style="--d:870">
                            <span class="fig-chain-key">"k\u{2080}"</span>
                            <span class="fig-chain-sep">","</span>
                            <span class="fig-chain-val">"v\u{2080}"</span>
                        </div>
                        <span class="fig-commit-chain-arrow fig-enc-fade"
                              style="--d:900">{"\u{2192}"}</span>
                        <div class="fig-commit-chain-node fig-enc-chain" style="--d:930">
                            <span class="fig-chain-key">"k\u{2081}"</span>
                            <span class="fig-chain-sep">","</span>
                            <span class="fig-chain-val">"v\u{2081}"</span>
                        </div>
                        <span class="fig-commit-chain-arrow fig-enc-fade"
                              style="--d:960">{"\u{2192}"}</span>
                        <div class="fig-commit-chain-node fig-enc-chain" style="--d:990">
                            <span class="fig-chain-key">"k\u{2082}"</span>
                            <span class="fig-chain-sep">","</span>
                            <span class="fig-chain-val">"v\u{2082}"</span>
                        </div>
                        <span class="fig-commit-chain-arrow fig-enc-fade"
                              style="--d:1020">{"\u{2192}"}</span>
                        <div class="fig-commit-chain-node fig-commit-chain-com fig-enc-chain"
                             style="--d:1050">
                            "Com"
                        </div>
                    </div>

                    // Cost
                    <div class="fig-commit-card-ops">
                        <div class="fig-commit-op-cost fig-enc-fade" style="--d:1200">
                            "Cost: "
                            <span class="fig-commit-detail-big">"O(n)"</span>
                            " \u{2014} no tree overhead"
                        </div>
                    </div>
                </div>

                // ── Right: SMT ──
                <div class="fig-commit-card">
                    <div class="fig-commit-card-head fig-enc-fade" style="--d:800">
                        <span class="fig-commit-card-tag fig-tag-smt">"SMT"</span>
                        <span class="fig-commit-card-size">"10M entries"</span>
                    </div>

                    // Tree — builds top-down
                    <div class="fig-tree">
                        <div class="fig-tree-nd fig-tree-root fig-enc-tree"
                             style="--d:870">"root"</div>
                        <div class="fig-tree-stem fig-enc-stem" style="--d:920"></div>
                        <div class="fig-tree-level fig-enc-fade" style="--d:920">
                            <div class="fig-tree-sub">
                                // Left subtree
                                <div class="fig-tree-nd fig-tree-inner fig-enc-tree"
                                     style="--d:970"></div>
                                <div class="fig-tree-stem fig-enc-stem" style="--d:1020"></div>
                                <div class="fig-tree-level fig-enc-fade" style="--d:1020">
                                    <div class="fig-tree-sub">
                                        <div class="fig-tree-nd fig-tree-leaf fig-tree-data fig-enc-tree"
                                             style="--d:1070"></div>
                                        <span class="fig-tree-tag fig-enc-fade"
                                              style="--d:1100">"data"</span>
                                    </div>
                                    <div class="fig-tree-sub">
                                        <div class="fig-tree-nd fig-tree-leaf fig-tree-empty fig-enc-tree"
                                             style="--d:1070"></div>
                                        <span class="fig-tree-tag fig-tree-tag-dim fig-enc-fade"
                                              style="--d:1100">"sparse"</span>
                                    </div>
                                </div>
                            </div>
                            <div class="fig-tree-sub">
                                // Right subtree
                                <div class="fig-tree-nd fig-tree-inner fig-enc-tree"
                                     style="--d:970"></div>
                                <div class="fig-tree-stem fig-enc-stem" style="--d:1020"></div>
                                <div class="fig-tree-level fig-enc-fade" style="--d:1020">
                                    <div class="fig-tree-sub">
                                        <div class="fig-tree-nd fig-tree-leaf fig-tree-empty fig-enc-tree"
                                             style="--d:1070"></div>
                                        <span class="fig-tree-tag fig-tree-tag-dim fig-enc-fade"
                                              style="--d:1100">"sparse"</span>
                                    </div>
                                    <div class="fig-tree-sub">
                                        <div class="fig-tree-nd fig-tree-leaf fig-tree-data fig-enc-tree"
                                             style="--d:1070"></div>
                                        <span class="fig-tree-tag fig-enc-fade"
                                              style="--d:1100">"data"</span>
                                    </div>
                                </div>
                            </div>
                        </div>
                    </div>

                    // Cost
                    <div class="fig-commit-card-ops">
                        <div class="fig-commit-op-cost fig-enc-fade" style="--d:1200">
                            "Cost: "
                            <span class="fig-commit-detail-big">"O(log n)"</span>
                            " \u{2014} batch amortized"
                        </div>
                    </div>
                </div>
            </div>

            // ── Phase 6: Footer ──
            <div class="fig-commit-footer fig-enc-fade" style="--d:1350">
                "Untouched columns require zero proof work"
            </div>
        </div>
    }
}
