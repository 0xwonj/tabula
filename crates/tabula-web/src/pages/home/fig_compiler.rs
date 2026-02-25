use leptos::prelude::*;

/// Figure 3: Compiler as trust boundary — without vs with NF+SSA.
/// Hover replays the animation sequence.
///
/// Animation narrative:
///  1. Source + exec trace appear fast (execution happening)
///  2. Sorted entries settle from execution-order positions (global sort cost)
///  3. "Without" verdict in red (this is expensive)
///  4. "With" section rises in (the solution)
///  5. Summary with colored contrast
#[component]
pub fn FigCompiler() -> impl IntoView {
    let fig_ref = NodeRef::<leptos::html::Div>::new();

    let on_mouseenter = move |_| {
        if let Some(el) = fig_ref.get() {
            let el: &web_sys::HtmlElement = el.as_ref();
            let cl = el.class_list();
            let _ = cl.remove_1("fig-compiler-replay");
            let _ = el.offset_width(); // force reflow to restart animations
            let _ = cl.add_1("fig-compiler-replay");
        }
    };

    view! {
        <div class="fig-panel" node_ref=fig_ref on:mouseenter=on_mouseenter>
            // ── Pipeline header (no animation — always-visible context) ──
            <div class="fig-pipeline">
                <div class="fig-pipeline-stage">
                    <div class="fig-pipeline-box">"Program IR"</div>
                    <span class="fig-pipeline-sub">"public"</span>
                </div>
                <span class="fig-pipeline-arrow">"\u{2192}"</span>
                <div class="fig-pipeline-stage fig-pipeline-boundary">
                    <div class="fig-pipeline-box">"Compiler (NF-1\u{2013}4, SSA)"</div>
                    <span class="fig-pipeline-sub">"anyone can verify"</span>
                </div>
                <span class="fig-pipeline-arrow">"\u{2192}"</span>
                <div class="fig-pipeline-stage fig-pipeline-result">
                    <div class="fig-pipeline-box">"ZK Proof"</div>
                    <span class="fig-pipeline-sub">"optimized"</span>
                </div>
            </div>

            // ── Without NF + SSA ──
            <div class="fig-compiler-section">
                <div class="fig-compiler-section-head fig-head-without fig-fade" style="--d:0">
                    "Without NF + SSA"
                </div>
                <div class="fig-compiler-split">
                    // Left: source code
                    <div class="fig-compiler-src fig-fade" style="--d:50">
                        <div class="fig-compiler-col-bar">"transfer.tab"</div>
                        <pre class="fig-compiler-src-code">{
"let a = bal[from]
let b = bal[to]
assert a >= amount
bal[from] = a - amount
bal[to]   = b + amount"
                        }</pre>
                    </div>

                    // Middle: execution-order trace (fast cascade — execution happening)
                    <div class="fig-compiler-trace">
                        <div class="fig-compiler-col-bar fig-fade" style="--d:100">
                            "Execution Trace"
                        </div>
                        <div class="fig-compiler-trace-body">
                            <ExecEntry kind="state" tau="0"  addr="bal[from]" op="read"  delay=150 />
                            <ExecEntry kind="local" tau="1"  addr="local.a"   op="store" delay=170 />
                            <ExecEntry kind="state" tau="2"  addr="bal[to]"   op="read"  delay=190 />
                            <ExecEntry kind="local" tau="3"  addr="local.b"   op="store" delay=210 />
                            <ExecEntry kind="local" tau="4"  addr="local.a"   op="load"  delay=230 />
                            <ExecEntry kind="local" tau="5"  addr="amount"    op="load"  delay=250 />
                            <ExecEntry kind="local" tau="6"  addr="cmp"       op="store" delay=270 />
                            <ExecEntry kind="local" tau="7"  addr="flag"      op="store" delay=290 />
                            <ExecEntry kind="local" tau="8"  addr="local.a"   op="load"  delay=310 />
                            <ExecEntry kind="local" tau="9"  addr="amount"    op="load"  delay=330 />
                            <ExecEntry kind="local" tau="10" addr="sub"       op="store" delay=350 />
                            <ExecEntry kind="state" tau="11" addr="bal[from]" op="write" delay=370 />
                            <ExecEntry kind="local" tau="12" addr="local.b"   op="load"  delay=390 />
                            <ExecEntry kind="local" tau="13" addr="amount"    op="load"  delay=410 />
                            <ExecEntry kind="local" tau="14" addr="add"       op="store" delay=430 />
                            <ExecEntry kind="state" tau="15" addr="bal[to]"   op="write" delay=450 />
                        </div>
                    </div>

                    // Right: sorted memory trace (entries sort-settle simultaneously)
                    <div class="fig-compiler-trace">
                        <div class="fig-compiler-col-bar fig-fade" style="--d:550">
                            "\u{2192} Sorted by (addr, \u{03C4})"
                        </div>
                        <div class="fig-compiler-trace-body">
                            // offset = (original_tau - sorted_idx) in row units
                            <SortedEntry kind="local" addr="add"       op="store" offset=14  />
                            <SortedEntry kind="local" addr="amount"    op="load"  offset=4   />
                            <SortedEntry kind="local" addr="amount"    op="load"  offset=7   />
                            <SortedEntry kind="local" addr="amount"    op="load"  offset=10  />
                            <SortedEntry kind="state" addr="bal[from]" op="read"  offset=-4  />
                            <SortedEntry kind="state" addr="bal[from]" op="write" offset=6   />
                            <SortedEntry kind="state" addr="bal[to]"   op="read"  offset=-4  />
                            <SortedEntry kind="state" addr="bal[to]"   op="write" offset=8   />
                            <SortedEntry kind="local" addr="cmp"       op="store" offset=-2  />
                            <SortedEntry kind="local" addr="flag"      op="store" offset=-2  />
                            <SortedEntry kind="local" addr="local.a"   op="store" offset=-9  />
                            <SortedEntry kind="local" addr="local.a"   op="load"  offset=-7  />
                            <SortedEntry kind="local" addr="local.a"   op="load"  offset=-4  />
                            <SortedEntry kind="local" addr="local.b"   op="store" offset=-10 />
                            <SortedEntry kind="local" addr="local.b"   op="load"  offset=-2  />
                            <SortedEntry kind="local" addr="sub"       op="store" offset=-5  />
                        </div>
                    </div>
                </div>
                <div class="fig-compiler-verdict fig-verdict-without fig-fade" style="--d:1300">
                    "5 lines \u{2192} 16 memory entries \u{00B7} "
                    "global sort by (addr, \u{03C4}) \u{00B7} prove every access"
                </div>
            </div>

            // ── With NF + SSA ──
            <div class="fig-compiler-section">
                <div class="fig-compiler-section-head fig-head-with fig-fade" style="--d:1550">
                    "With NF + SSA"
                </div>
                <div class="fig-compiler-shards-row fig-rise" style="--d:1700">
                    <Shard label="balances[from].balance">
                        <ShardEntry op="Read" tau="\u{03C4}\u{2080}" nf="NF-1" />
                        <ShardEntry op="Write" tau="\u{03C4}\u{2086}" nf="NF-2" />
                    </Shard>
                    <Shard label="balances[to].balance">
                        <ShardEntry op="Read" tau="\u{03C4}\u{2081}" nf="NF-1" />
                        <ShardEntry op="Write" tau="\u{03C4}\u{2087}" nf="NF-2" />
                    </Shard>
                    <div class="fig-compiler-ssa">
                        <div class="fig-compiler-ssa-label">"SSA columns"</div>
                        <span class="fig-compiler-ssa-ops">
                            "Cmp \u{00B7} Assert \u{00B7} Arith \u{00B7} Arith"
                        </span>
                        <span class="fig-compiler-ssa-note">
                            "trace only \u{2014} no memory"
                        </span>
                    </div>
                </div>
                <div class="fig-compiler-verdict fig-verdict-with fig-fade" style="--d:1900">
                    "4 entries \u{00B7} 2 independent shards \u{00B7} no intra-tx sort"
                </div>
            </div>

            // ── Summary bar ──
            <div class="fig-compiler-summary fig-fade" style="--d:2100">
                <span class="fig-compiler-count fig-count-bad">"16"</span>" memory rows \u{2192} "
                <span class="fig-compiler-count fig-count-good">"4"</span>" entries in "
                <span class="fig-compiler-count fig-count-good">"2"</span>" shards"
            </div>
        </div>
    }
}

/// Execution-order trace entry (τ, addr, op).
/// Fast cascade: each entry appears 20ms after the previous.
#[component]
fn ExecEntry(
    kind: &'static str,
    tau: &'static str,
    addr: &'static str,
    op: &'static str,
    delay: u32,
) -> impl IntoView {
    let class = match kind {
        "state" => "fig-compiler-entry fig-entry-state fig-exec-in",
        _ => "fig-compiler-entry fig-entry-local fig-exec-in",
    };
    let style = format!("--d:{delay}");
    view! {
        <div class=class style=style>
            <span class="fig-entry-tau">{tau}</span>
            <span class="fig-entry-addr">{addr}</span>
            <span class="fig-entry-op">{op}</span>
        </div>
    }
}

/// Sorted trace entry (addr, op).
/// Sort-settle animation: starts at original execution-order Y position,
/// slides to final sorted position. All entries animate simultaneously.
#[component]
fn SortedEntry(
    kind: &'static str,
    addr: &'static str,
    op: &'static str,
    offset: i32,
) -> impl IntoView {
    let class = match kind {
        "state" => "fig-compiler-entry fig-entry-state fig-sort-slide",
        _ => "fig-compiler-entry fig-entry-local fig-sort-slide",
    };
    let style = format!("--d:700;--offset:{offset}");
    view! {
        <div class=class style=style>
            <span class="fig-entry-addr">{addr}</span>
            <span class="fig-entry-op">{op}</span>
        </div>
    }
}

/// Per-column shard card.
#[component]
fn Shard(label: &'static str, children: Children) -> impl IntoView {
    view! {
        <div class="fig-compiler-shard">
            <div class="fig-compiler-shard-label">{label}</div>
            <div class="fig-compiler-shard-body">{children()}</div>
        </div>
    }
}

/// Single entry inside a shard.
#[component]
fn ShardEntry(op: &'static str, tau: &'static str, nf: &'static str) -> impl IntoView {
    view! {
        <div class="fig-compiler-shard-entry">
            <span class="fig-shard-op">{op}</span>
            <span class="fig-shard-tau">{tau}</span>
            <span class="fig-compiler-nf">{nf}</span>
        </div>
    }
}
