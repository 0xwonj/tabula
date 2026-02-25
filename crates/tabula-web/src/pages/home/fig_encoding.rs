use leptos::prelude::*;

/// Figure 2: Typed per-column commitment — schema table, per-column
/// independence arrow, and SSMC vs SMT comparison with CSS diagrams.
#[component]
pub fn FigEncoding() -> impl IntoView {
    view! {
        <div class="fig-panel fig-commit">
            // ── Top: Table schema + sample data ──
            <div class="fig-commit-table-section">
                <div class="fig-commit-table-header">
                    <span class="fig-commit-table-name">"balances"</span>
                </div>
                <table class="fig-commit-schema-table">
                    <thead>
                        <tr>
                            <th class="fig-commit-th-row"></th>
                            <th>
                                <div class="fig-commit-col-header">
                                    <span class="fig-commit-col-name">"active"</span>
                                    <span class="fig-commit-col-type">"Bool"</span>
                                    <div class="fig-commit-fe-blocks">
                                        <span class="fig-commit-fe-block fe-bool"></span>
                                    </div>
                                    <span class="fig-commit-fe-label">"1 FE"</span>
                                </div>
                            </th>
                            <th>
                                <div class="fig-commit-col-header">
                                    <span class="fig-commit-col-name">"balance"</span>
                                    <span class="fig-commit-col-type">"U64"</span>
                                    <div class="fig-commit-fe-blocks">
                                        <span class="fig-commit-fe-block fe-u64"></span>
                                        <span class="fig-commit-fe-block fe-u64"></span>
                                        <span class="fig-commit-fe-block fe-u64"></span>
                                    </div>
                                    <span class="fig-commit-fe-label">"3 FE"</span>
                                </div>
                            </th>
                            <th>
                                <div class="fig-commit-col-header">
                                    <span class="fig-commit-col-name">"owner"</span>
                                    <span class="fig-commit-col-type">"Bytes32"</span>
                                    <div class="fig-commit-fe-blocks">
                                        <span class="fig-commit-fe-block fe-digest"></span>
                                        <span class="fig-commit-fe-block fe-digest"></span>
                                        <span class="fig-commit-fe-block fe-digest"></span>
                                        <span class="fig-commit-fe-block fe-digest"></span>
                                        <span class="fig-commit-fe-block fe-digest"></span>
                                        <span class="fig-commit-fe-block fe-digest"></span>
                                        <span class="fig-commit-fe-block fe-digest"></span>
                                        <span class="fig-commit-fe-block fe-digest"></span>
                                    </div>
                                    <span class="fig-commit-fe-label">"8 FE"</span>
                                </div>
                            </th>
                        </tr>
                    </thead>
                    <tbody>
                        <tr>
                            <td class="fig-commit-row-id">"0"</td>
                            <td class="fig-commit-val-bool">"true"</td>
                            <td class="fig-commit-val-num">"1\u{202f}000"</td>
                            <td class="fig-commit-val-hash">"0xab\u{2026}3f"</td>
                        </tr>
                        <tr>
                            <td class="fig-commit-row-id">"1"</td>
                            <td class="fig-commit-val-bool">"true"</td>
                            <td class="fig-commit-val-num">"50"</td>
                            <td class="fig-commit-val-hash">"0xcd\u{2026}91"</td>
                        </tr>
                        <tr>
                            <td class="fig-commit-row-id">"2"</td>
                            <td class="fig-commit-val-bool">"false"</td>
                            <td class="fig-commit-val-num">"0"</td>
                            <td class="fig-commit-val-hash">"0x00\u{2026}00"</td>
                        </tr>
                    </tbody>
                </table>
            </div>

            // ── Middle: Independence arrow ──
            <div class="fig-commit-divider">
                <div class="fig-commit-arrows">
                    <span class="fig-commit-arrow">{"\u{2193}"}</span>
                    <span class="fig-commit-arrow">{"\u{2193}"}</span>
                    <span class="fig-commit-arrow">{"\u{2193}"}</span>
                </div>
                <span class="fig-commit-divider-label">
                    "Each column committed independently"
                </span>
            </div>

            // ── Bottom: SSMC vs SMT ──
            <div class="fig-commit-compare">
                // ── Left: SSMC ──
                <div class="fig-commit-card">
                    <div class="fig-commit-card-head">
                        <span class="fig-commit-card-tag fig-tag-ssmc">"SSMC"</span>
                        <span class="fig-commit-card-size">"3 entries"</span>
                    </div>

                    // Hash chain visualization
                    <div class="fig-commit-chain">
                        <div class="fig-commit-chain-node">
                            <span class="fig-chain-key">"k\u{2080}"</span>
                            <span class="fig-chain-sep">","</span>
                            <span class="fig-chain-val">"v\u{2080}"</span>
                        </div>
                        <span class="fig-commit-chain-arrow">{"\u{2192}"}</span>
                        <div class="fig-commit-chain-node">
                            <span class="fig-chain-key">"k\u{2081}"</span>
                            <span class="fig-chain-sep">","</span>
                            <span class="fig-chain-val">"v\u{2081}"</span>
                        </div>
                        <span class="fig-commit-chain-arrow">{"\u{2192}"}</span>
                        <div class="fig-commit-chain-node">
                            <span class="fig-chain-key">"k\u{2082}"</span>
                            <span class="fig-chain-sep">","</span>
                            <span class="fig-chain-val">"v\u{2082}"</span>
                        </div>
                        <span class="fig-commit-chain-arrow">{"\u{2192}"}</span>
                        <div class="fig-commit-chain-node fig-commit-chain-com">
                            "Com"
                        </div>
                    </div>

                    <div class="fig-commit-card-details">
                        <div class="fig-commit-detail">
                            <span class="fig-commit-detail-op">"Write"</span>
                            " re-sort, re-hash entire chain"
                        </div>
                        <div class="fig-commit-detail">
                            <span class="fig-commit-detail-op">"Read"</span>
                            " binary search in sorted list"
                        </div>
                        <div class="fig-commit-detail fig-commit-detail-cost">
                            "Cost: "
                            <span class="fig-commit-detail-big">"O(n)"</span>
                            " \u{2014} no tree overhead"
                        </div>
                    </div>
                </div>

                // ── Right: SMT ──
                <div class="fig-commit-card">
                    <div class="fig-commit-card-head">
                        <span class="fig-commit-card-tag fig-tag-smt">"SMT"</span>
                        <span class="fig-commit-card-size">"10M entries"</span>
                    </div>

                    // Binary tree (nested CSS connectors)
                    <div class="fig-tree">
                        <div class="fig-tree-nd fig-tree-root">"root"</div>
                        <div class="fig-tree-stem"></div>
                        <div class="fig-tree-level">
                            // left subtree
                            <div class="fig-tree-sub">
                                <div class="fig-tree-nd fig-tree-inner"></div>
                                <div class="fig-tree-stem"></div>
                                <div class="fig-tree-level">
                                    <div class="fig-tree-sub">
                                        <div class="fig-tree-nd fig-tree-leaf fig-tree-data">
                                        </div>
                                        <span class="fig-tree-tag">"data"</span>
                                    </div>
                                    <div class="fig-tree-sub">
                                        <div class="fig-tree-nd fig-tree-leaf fig-tree-empty">
                                        </div>
                                        <span class="fig-tree-tag fig-tree-tag-dim">
                                            "sparse"
                                        </span>
                                    </div>
                                </div>
                            </div>
                            // right subtree
                            <div class="fig-tree-sub">
                                <div class="fig-tree-nd fig-tree-inner"></div>
                                <div class="fig-tree-stem"></div>
                                <div class="fig-tree-level">
                                    <div class="fig-tree-sub">
                                        <div class="fig-tree-nd fig-tree-leaf fig-tree-empty">
                                        </div>
                                        <span class="fig-tree-tag fig-tree-tag-dim">
                                            "sparse"
                                        </span>
                                    </div>
                                    <div class="fig-tree-sub">
                                        <div class="fig-tree-nd fig-tree-leaf fig-tree-data">
                                        </div>
                                        <span class="fig-tree-tag">"data"</span>
                                    </div>
                                </div>
                            </div>
                        </div>
                    </div>

                    <div class="fig-commit-card-details">
                        <div class="fig-commit-detail">
                            <span class="fig-commit-detail-op">"Write"</span>
                            " update path from leaf to root"
                        </div>
                        <div class="fig-commit-detail">
                            <span class="fig-commit-detail-op">"Read"</span>
                            " inclusion proof along path"
                        </div>
                        <div class="fig-commit-detail fig-commit-detail-cost">
                            "Cost: "
                            <span class="fig-commit-detail-big">"O(log n)"</span>
                            " \u{2014} batch amortized"
                        </div>
                    </div>
                </div>
            </div>

            // ── Footer ──
            <div class="fig-commit-footer">
                "Untouched columns require zero proof work"
            </div>
        </div>
    }
}
