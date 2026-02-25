mod demo;
mod feature_cards;
mod fig_compiler;
mod fig_encoding;
mod footer_cta;
mod hero;

use leptos::prelude::*;
use leptos_router::components::A;

use crate::layout::nav::docs_href;
use demo::Demo;
use fig_compiler::FigCompiler;
use fig_encoding::FigEncoding;

/// Landing page — academic paper style.
#[component]
pub fn HomePage() -> impl IntoView {
    view! {
        <article class="paper">
            // ── Title block ──
            <header class="paper-header">
                <h1 class="paper-project">"Tabula"</h1>
                <p class="paper-title">
                    "Zero-knowledge kernel for typed, tabular state transitions."
                </p>
            </header>

            <hr class="paper-rule" />

            // ── Abstract ──
            <section class="paper-section">
                <h2 class="paper-heading">
                    <span class="paper-heading-label">"Abstract."</span>
                </h2>
                <p class="paper-body">
                    "The overhead of general-purpose zkVMs is not inherent to verifiable "
                    "computation\u{2014}it is an artifact of the ISA abstraction layer. "
                    "A zkVM cannot distinguish application state from runtime infrastructure "
                    "because the ISA treats all memory uniformly. "
                    "It cannot specialize constraint encoding per value type "
                    "because the ISA erases types. "
                    "It cannot eliminate redundant consistency arguments "
                    "because the ISA makes memory access patterns opaque at compile time. "
                    "These are properties of the abstraction boundary, not of the computation."
                </p>
                <p class="paper-body">
                    "Tabula moves the abstraction boundary. "
                    "Instead of proving that a RISC-V program executed correctly, "
                    "it proves that a schema-typed state transition was applied correctly. "
                    "The IR, commitment scheme, and constraint system are co-designed "
                    "around a single structural primitive: "
                    "the typed, column-partitioned, key-addressed table. "
                    "The system compiles to 9 purpose-built AIR chips "
                    "connected by LogUp buses over 670 constraint columns."
                </p>
            </section>

            <hr class="paper-rule" />

            // ── Contents ──
            <section class="paper-section">
                <h2 class="paper-heading">
                    <span class="paper-heading-marker">"\u{00A7}"</span>
                    " Contents"
                </h2>
                <nav class="paper-toc">
                    <A href="/playground" attr:class="paper-toc-entry">
                        <span class="paper-toc-num">"1."</span>
                        <span class="paper-toc-label">"Playground"</span>
                        <span class="paper-toc-dots" />
                        <span class="paper-toc-desc">"Try the DSL and prover"</span>
                    </A>
                    <a href=docs_href() class="paper-toc-entry">
                        <span class="paper-toc-num">"2."</span>
                        <span class="paper-toc-label">"Documentation"</span>
                        <span class="paper-toc-dots" />
                        <span class="paper-toc-desc">"Architecture and spec"</span>
                    </a>
                    <a href="#architecture" class="paper-toc-entry">
                        <span class="paper-toc-num">"3."</span>
                        <span class="paper-toc-label">"Architecture"</span>
                        <span class="paper-toc-dots" />
                        <span class="paper-toc-desc">"How the proof system works"</span>
                    </a>
                </nav>
            </section>

            <hr class="paper-rule" />

            // ── 3. Architecture ──
            <section class="paper-section" id="architecture">
                <h2 class="paper-heading">
                    <span class="paper-heading-marker">"\u{00A7}"</span>
                    " Architecture"
                </h2>

                // ── 3.1 No ISA Overhead ──
                <div class="paper-subsection">
                    <h3 class="paper-subheading">"3.1 No ISA Overhead"</h3>
                    <p class="paper-body">
                        "A single logical operation\u{2014}\u{201C}read the balance of account X\u{201D}"
                        "\u{2014}expands in a zkVM into a sequence of ISA instructions: "
                        "load an address, issue a memory-read syscall, decode the result, "
                        "store to the stack. Each instruction is a row in the execution trace, "
                        "each row is fully constrained. "
                        "The application-level operation is one cell access; "
                        "the proof treats it as a dozen fetch-decode-execute cycles."
                    </p>
                    <p class="paper-body">
                        "Tabula\u{2019}s proof system decomposes into purpose-built AIR chips, "
                        "each responsible for a single semantic role. "
                        "Each IR instruction maps directly to chip rows: "
                        "a Read is one row in the execution trace and one entry "
                        "in the inter-transaction ordering chip. "
                        "State root computation is constrained by dedicated chips, "
                        "not hashed through an ISA. "
                        "The prover builds a trace, not a simulation."
                    </p>
                    <Demo />
                    <p class="paper-caption">
                        <span class="paper-caption-label">"Figure 1."</span>
                        " A "<code>"transfer"</code>" transaction: "
                        "2 Read, 1 Cmp, 1 Assert, 2 Arith, 2 Write\u{2014}"
                        "8 IR instructions mapped directly to AIR chip rows."
                    </p>
                </div>

                // ── 3.2 Typed Per-Column Commitment ──
                <div class="paper-subsection">
                    <h3 class="paper-subheading">"3.2 Typed Per-Column Commitment"</h3>
                    <p class="paper-body">
                        "A state cell is addressed by (TableId, ColId, RowKey). "
                        "Tables and columns are declared with explicit types. "
                        "Each column type has a known encoding width: "
                        "Bool occupies 1 field element, "
                        "U64 and I64 occupy 3 (30+30+4 bit limb decomposition over BabyBear), "
                        "Bytes32 occupies 8 (native Poseidon2 digest elements). "
                        "Constraint chips are parameterized by width class\u{2014}"
                        "no runtime type dispatch, no worst-case-width padding."
                    </p>
                    <p class="paper-body">
                        "Different columns need different commitment strategies. "
                        "A 50-row configuration table and a 10-million-row ledger "
                        "cannot share the same scheme. "
                        "Tabula uses a hybrid: "
                        "Sorted Sparse Map Commitment (SSMC) for small columns\u{2014}"
                        "a sorted list with streaming Poseidon hashing, "
                        "O(n) commitment with no tree overhead\u{2014}"
                        "and Sparse Merkle Tree (SMT) for large ones, "
                        "O(log n) per access, amortized over the batch. "
                        "Untouched columns require zero proof work."
                    </p>
                    <FigEncoding />
                    <p class="paper-caption">
                        <span class="paper-caption-label">"Figure 2."</span>
                        " Per-type encoding width determines constraint chip parameterization. "
                        "Commitment strategy is selected per-column by size."
                    </p>
                </div>

                // ── 3.3 Compiler as Trust Boundary ──
                <div class="paper-subsection">
                    <h3 class="paper-subheading">"3.3 Compiler as Trust Boundary"</h3>
                    <p class="paper-body">
                        "In a zkVM the proof system bears everything\u{2014}"
                        "execution, memory consistency, intermediate values. "
                        "Tabula introduces the compiler as a trust boundary. "
                        "Programs are registered as public IR that must satisfy "
                        "normal-form rules: "
                        "unique reads (NF-1), unique writes (NF-2), "
                        "no read-after-write (NF-3), key-alias resolvability (NF-4). "
                        "These are structural properties of the IR, "
                        "checkable in O(program size) by anyone\u{2014}"
                        "including the verifier\u{2014}without running the prover."
                    </p>
                    <p class="paper-body">
                        "Two consequences. "
                        "First, intra-transaction memory consistency is eliminated\u{2014}"
                        "the compiler enforces it structurally. "
                        "Second, local variables are SSA slots: "
                        "trace columns that carry values from definition to use, "
                        "never entering the memory table. "
                        "A transaction with 5 reads, 100 operations, and 3 writes "
                        "produces exactly 8 sorted-memory entries. "
                        "In a zkVM, the same logic generates 100+ memory rows. "
                        "The compiler shifts work from proof time to compile time."
                    </p>
                    <FigCompiler />
                    <p class="paper-caption">
                        <span class="paper-caption-label">"Figure 3."</span>
                        " The compiler enforces NF rules and SSA, "
                        "eliminating intra-transaction consistency from the proof. "
                        "Only inter-transaction state ordering remains\u{2014}"
                        "handled by the SortedMem chip."
                    </p>
                </div>

                // ── Thesis ──
                <blockquote class="paper-quote">
                    "For stateful applications, the natural proof boundary is "
                    "the state transition, not the machine execution. "
                    "By co-designing the IR, commitment scheme, and constraint system "
                    "around typed tabular state, "
                    "memory consistency cost scales with persistent state accesses\u{2014}"
                    "not total computation."
                </blockquote>
            </section>
        </article>
    }
}
