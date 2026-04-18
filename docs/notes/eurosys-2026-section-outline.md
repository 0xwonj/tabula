# EuroSys 2027 — Section Outline Lock

Working note capturing the **locked section-level outline** for the
EuroSys 2027 submission (deadline 2026-05-14, conference April 2027).
This note is:

- **Authoritative for paper structure** until superseded by a draft.
- **Not authoritative** for architecture — trust
  [`docs/design/architecture.md`](../design/architecture.md).

Supporting notes (locked sequentially; this note consumes all of them):

- [`eurosys-2026-contributions.md`](eurosys-2026-contributions.md) —
  headline claim, C1-C4, M1-M5, A1-A5.
- [`eurosys-2026-related-work.md`](eurosys-2026-related-work.md) —
  2×2 (sealing time × programmability) frame.
- [`eurosys-2026-workload.md`](eurosys-2026-workload.md) — workload
  spec and signature-elision rationale.
- [`distributed-proving.md`](distributed-proving.md) — Def 2 / incremental
  proving deferred to Future Work.

## How This Outline Was Decided

The outline went through multiple review cycles before locking:

- **Self-review** against seven criteria.
- **Three-lens agent review (round 1)**: story architect, hostile PC
  reviewer, EuroSys norms.
- **External agent (Codex) review (round 2)**: flagged 12-page CFP
  limit, TCB contradiction, compile-time cost unmeasured, A1 drift,
  missing running example, artifact overclaim.
- **Three-lens agent review (round 3)**: methodology / claim-integrity,
  narrative / structure, deployment / practitioner. Triggered the
  current revision — in particular: artifact provenance as **integrity
  binding not soundness**, Tabula-internal baseline engineering-parity
  discipline, determinism as a **paper commitment not inheritance**,
  M3c complexity caveat, "when Tabula loses" moved to §5.4 with numeric
  thresholds, §6 Cell B op-def test as Table 5, SpotTrade fee row
  planted upfront in §2.

Each review converged on several findings and diverged on a few. The
locked outline below resolves each divergence explicitly. Key
decisions summarised below; full review deltas captured as *Applied
Changes*.

## Page Budget (11.3 authored pages, 12-page EuroSys 2027 ceiling)

EuroSys 2027 CFP: the main PDF is limited to 12 pages of technical
content; references are excluded from the page count. Any appendix kept
inside the main PDF consumes that same 12-page budget.

| Section | Pages | Notes |
|---------|-------|-------|
| §1 Introduction | 0.85 | Fig 1 — 2×2 puzzle; SpotTrade-on-SP1-vs-Tabula concrete hook |
| §2 Background, Motivation, and Running Example | 0.90 | running-example slice of the evaluated workload; ends with boxed **Principle (C1)** |
| §3 Design | 4.25 | headline section; running example threaded; minimal support material in body |
| §4 Implementation and Trust Model | 1.05 | TCB qualifier + soundness boundary; artifact logistics moved out of body |
| §5 Evaluation | 3.25 | question chain: win? / why? / misleading? |
| §6 Related Work | 0.55 | Table 5 operational audit + short Cell A/B/C contrast |
| §7 Discussion and Limitations | 0.25 | domain fit + future work + out-of-scope practitioner concerns |
| §8 Conclusion | 0.20 | |
| **Total** | **11.30** | leaves ~0.7p slack for captions, measured numbers, and layout drift |

This outline assumes **no in-paper appendix budget**. Formal proof
details, full ablation tables with CIs, SP1/RISC0 guest listings,
metadata decomposition, and reproducibility manifests move to
supplementary material or AE packaging by default.

## Running Example — SpotTrade (with fee)

A **single-transaction slice** of the locked StarkEx-class workload,
planted in §2 and threaded through §3. It uses the same account-table
state model and fee-bearing SpotTrade semantics as the workload note,
but isolates one transaction so the paper can explain the mechanisms
without carrying the full mixed-batch machinery in every paragraph.
The fee row is kept upfront so NF-4's hard aliasing case is exercised
naturally: if `maker == fee_collector` or `taker == fee_collector`,
NF-4 must resolve the aliasing at compile time or fall back to a
runtime check (the basis of §5.4's losing regime (c)).

```
// Typed tabular state:
//   accounts: table<id: Digest, balance_base: U64,
//                   balance_quote: U64, nonce: U64, frozen: Bool>
//
// Transaction:
//   SpotTrade(maker, taker, size, price, fee_rate, fee_collector):
//     let mb  = accounts[maker].balance_base
//     let mq  = accounts[maker].balance_quote
//     let tb  = accounts[taker].balance_base
//     let tq  = accounts[taker].balance_quote
//     let fq  = accounts[fee_collector].balance_quote
//     let mn  = accounts[maker].nonce
//     let tn  = accounts[taker].nonce
//     let mf  = accounts[maker].frozen
//     let tf  = accounts[taker].frozen
//     let fee = size * price * fee_rate
//     assert(!mf && !tf)
//     accounts[maker].balance_base  = mb - size
//     accounts[maker].balance_quote = mq + size * price - fee
//     accounts[taker].balance_base  = tb + size
//     accounts[taker].balance_quote = tq - size * price
//     accounts[fee_collector].balance_quote = fq + fee
//     accounts[maker].nonce         = mn + 1
//     accounts[taker].nonce         = tn + 1
```

Every sealing claim in §3 points back at this transaction.

## Full Outline

### §1 Introduction — 0.85p

- **Hook (4 sentences, concrete).** A fee-bearing SpotTrade on a
  multi-asset rollup reads 9 cells and writes 7 across up to 3 account
  rows. SP1 and RISC0 reconstruct the memory- and lookup-shape
  consequences of that transaction inside each proof. Tabula's compiler
  seals the normal-form checks, typed machine shape, and per-column
  shard plan ahead of time, so for workloads where NF-4 resolves
  statically the prover need not re-emit that work at proof time. Under
  our evaluation the resulting batch proof costs **[X%]** of SP1's at
  the reference workload. *(Numbers populated from measurements.)*
- **Problem.** Programmable zkVMs re-derive sealing work per proof:
  memory consistency, lookup topology, shard layout, program/metadata
  binding.
- **Observation.** Much of this could be lifted to compile time *if*
  the compiler can reason about the program's proof topology — which
  for general programs is undecidable or prohibitively conservative.
- **Figure 1 — 2×2 motivating frame.** Sealing time × programmability.
  Three cells populated with named systems (SP1/RISC0/Jolt/Ceno/OpenVM;
  Cairo/Valida/Miden; Lighter/zkLedger). Top-left **labeled "?"** —
  what would it take to reach here? Puzzle, not reveal.
- **Claim and scope.** Co-design between compiler and proof system
  reaches the empty corner for *typed tabular state transitions*. Not
  a general-zkVM replacement.
- **Contributions (C1-C4).** Name only: principle, Tabula's five
  mechanisms, empirical validation, and an open artifact.

### §2 Background, Motivation, and Running Example — 0.9p

- **zkVM sealing vocabulary.** Three concrete items general zkVMs
  re-derive per proof: (i) memory-consistency permutation, (ii)
  lookup-argument topology, (iii) trace/shard layout. Short — the §3
  operational-definition box pins this formally.
- **Typed tabular state transitions.** Definition: batches over
  tables with fixed schemas, static `(table, column)` coordinates,
  dynamic `row`, scalar types {Bool, U64, I64, Digest}. Why this
  admits compile-time reasoning: static aliasing, finite schemas,
  bounded row coordinates.
- **Plain-language mental model.** Runtime-sealed systems rebuild proof
  shape for each proof. Compile-time-sealed systems fix more of that
  shape once at build time. Tabula can do more of this than a general
  zkVM because the schema and `(table, column)` structure are static and
  only the row IDs remain dynamic.
- **Running example — SpotTrade with fee** (see listing above).
  Compiler-visible facts: reads and writes are statically known; the
  `(account, column)` coordinates are static except for the
  `row = account_id` dynamic component; `size ∈ [1, 2^40]` and
  `price ∈ PriceGrid` are bounded by relation declarations; the
  account set `{maker, taker, fee_collector}` is symbolic and NF-4
  must resolve aliasing.
- **Principle (C1)** — boxed:

  > **Principle (C1).** Given a domain admitting compile-time
  > analysis of proof topology and a trusted compiler, the compiler
  > can seal at build time what a general prover must re-derive per
  > proof — without loss of soundness relative to the trusted
  > compiler.

  The trusted-compiler qualifier is load-bearing. §4.2 pins the TCB
  and instruments it with numbers.

  The evaluation in §5 uses mixed batches over this same account-table
  state model; the running example is the single-transaction slice that
  keeps the design discussion readable.

### §3 Design — 4.25p

Each §3.2–§3.6 lands its M-mechanism on the running example, then
generalises, then closes with one rejected alternative. §3.4 (M3)
carries a full alternative paragraph because per-column vs. per-row
is genuinely load-bearing.

#### §3.1 Overview and Operational Definition — 0.35p

- **System pipeline (inline listing).** DSL source → IR → compiler
  (NF + relation lowering + shard planner) → sealed artifact → prover
  / verifier.
- **Operational-definition box.**
  > A system is **compile-time-sealed** iff the following five
  > quantities are fixed before proving, and the verifier checks the
  > sealed artifact that binds them: (1) program-logic commitment, (2)
  > constraint-family inventory, (3) routing plan, (4) lookup
  > parameterization, and (5) materialized constant commitment.

  §6/Table 5 turns this into a paper-local operational audit by naming,
  for each system, the verifier-checked field or hash chain that binds
  each quantity. The next five subsections show, on the SpotTrade
  slice, how Tabula seals them.

#### §3.2 M1 — Compile-time NF sealing — 0.9p

Four normal forms + True SSA, enforcement as linear-scan IR checks,
what each replaces at runtime.

- **On SpotTrade-with-fee.** NF-1/2/3 trivially certified on the 7
  distinct reads and 7 distinct writes. NF-4 (key-alias resolvability)
  is the interesting case: the transaction references three symbolic
  accounts `{maker, taker, fee_collector}`. If the caller supplies
  distinct concrete IDs, NF-4 succeeds statically. If the caller
  supplies symbolic IDs (e.g., a rollup flow where the fee collector
  is a protocol address potentially equal to a trader), NF-4 runs a
  symbolic disjointness check; when that check cannot prove disjointness,
  NF-4 falls back to a runtime aliasing assertion — partially degrading
  M1 (§5.4 losing regime (c) quantifies this threshold empirically).
- **General statement.** For any transaction in the typed tabular
  shape, NF-sealing eliminates a fixed cost (intra-tx RAM consistency)
  from the prover's runtime obligation.
- **What Cell A/B still pays.** SP1 discharges intra-tx RAM consistency
  per proof via the RAM argument; Cell B systems embed analogous work
  in the algebraic RAM or hand-coded constraints.
- **Rejected alternative.** Per-proof NF checking in the AIR. Rejected
  because it re-introduces the RAM consistency cost M1 aims to
  eliminate — the whole mechanism disappears.

#### §3.3 M2 — Schema-typed chip widths — 0.4p

- **Table 1** — per-type chip widths (Bool=1, U64=3, I64=3, Digest=8)
  vs. uniform-Digest baseline. Width-inflation factor as derived
  metric.
- **On SpotTrade-with-fee.** Balance, nonce, and fee-delta columns
  are U64 (3 FE); `account_id` column is Digest (8 FE). The chip-width
  vector is sealed into the artifact; the verifier refuses proofs that
  do not match.
- **Rejected alternative.** Uniform Digest-width chips. Rejected for
  2.6× width inflation on typed workloads (measured in §5 A2).

#### §3.4 M3 — Static-coordinate per-column sharding — 1.65p [HEADLINE]

- **§3.4.1 M3a — Static `(table, col)` coordinates.** Enabling
  independent per-column `ProofInstance`. On SpotTrade: four value
  columns each become a separable shard.
- **§3.4.2 M3b — Per-column SSMC commitment.** Sorted State Merkle
  Chain per column, allowing columns to move independently. On
  SpotTrade: `balance_base` advances its SSMC by two entries (maker,
  taker); `balance_quote` advances by three (maker, taker,
  fee_collector); `nonce` advances by two. Independent SSMC evolution
  is the structural prerequisite for future-work incremental
  re-proving.
- **§3.4.3 M3c — Cross-shard LogUp cumulative-sum balance.**
  **Figure 3** — shard topology + cross-shard bus-balance closure.
  The subtlest sub-mechanism. Main body keeps the intuition and proof
  sketch; Supplementary B carries the formal soundness argument with
  pointers to `crates/machine/src/proof/prover.rs:105-110`,
  `verifier.rs:108-118`, `setup/metadata.rs:125-132`. **Asymptotic
  cost caveat**: cross-shard cumsum verification adds a
  `O(shard_count · |buses|)` term to the verifier side on top of the
  per-shard seal check; this term is surfaced in §3.7.
- **§3.4.4 M3d — Compile-time scheme specialization.** Property-query
  analysis drives per-column scheme selection, conditional chip
  gating, and static anchor multiplicities. On SpotTrade: the `nonce`
  column's monotonicity lowers to a specific LogUp multiplicity
  pattern; `balance_*` columns use the default scheme.
- **Design alternative — per-column vs. per-row sharding (full
  paragraph).** Why not shard by row? Row-based shards have
  run-time-decided boundaries because row coordinates are dynamic;
  shard topology then cannot be compile-time-sealed (quantity (3) in
  the operational definition). Per-column shards have static
  `(table, col)` boundaries — column identity is compile-time invariant,
  row identity is the dynamic variable. This is not an incidental
  engineering choice; it is the reason M3 exists as a compile-time
  sealing mechanism rather than a run-time parallelism mechanism. A
  per-row co-design would occupy a different cell of the 2×2 —
  structurally a different mechanism, not a direct comparator on the
  same axis.

#### §3.5 M4 — Statement-first verifier — 0.55p

Framed explicitly as a **soundness mechanism, not a performance knob**
(hence no ablation — A4 intentionally does not exist). Tabula's
paper-facing theorem object is
`PublicStatement = (old_root, new_root, public_context_digest, applied_tx_digest, event_digest)`.
`BoundStatement` is the verifier-side object that binds that statement
to one sealed artifact, including artifact context, program ID, and
schema version. `semantic_hash` and `profile_hash` remain
metadata-envelope internals that explain how the artifact is identified;
they are not the theorem statement itself. On SpotTrade, the verifier
checks `(sealed artifact, expected PublicStatement, proof)` as a unit.
Soundness sketch here; threat-model treatment is §4.2.

**Rejected alternative.** Statement-in-proof with separate binding
hash (Cell A/B pattern). Rejected because it opens adversary (iii)
from §4.2 (statement substitution at verification time) unless the
binding is tight — and a tight binding is essentially M4 under a
different name.

#### §3.6 M5 — Relation lowering — 0.35p

`relation` construct (enum / range / set / map / function) lowering to
`RangeCheck` / `StaticTableLookup` buses. Sealed under
`static_table_root`. On SpotTrade: `size ∈ [1, 2^40]` compiles to a
`RangeCheck` bus entry; `price ∈ PriceGrid(asset_base, asset_quote)`
compiles to a `StaticTableLookup`; `fee_rate ∈ FeeTiers` likewise.

**Rejected alternative.** Runtime predicate evaluation in the AIR.
Rejected because it re-introduces runtime constraints for relations
that are deterministic at compile time — the entire point of M5 is
that relations are static facts, not runtime predicates.

#### §3.7 Synthesis — 0.05p

**Claim-to-evidence map.**

- **C1 end-to-end payoff** → Fig 4 + Table 3.
- **M1/M2/M3/M5 contribution** → Fig 5.
- **Compile-time share of M3 vs. pure parallelism** → A3-seal in §5.3;
  detailed scaling sweep moves to supplementary.
- **M4 soundness boundary** → §4.2.

This closes §3 with the evidence ladder the reader should expect in §5.

### §4 Implementation and Trust Model — 1.05p

> Section titled "Trust Model" rather than "Threat Model" to signal
> that the scope includes what is trusted in addition to adversary
> capabilities.

#### §4.1 System — 0.25p

LoC breakdown per crate, Plonky3 version and stock-vs-modified parts,
Poseidon2 parameters, KoalaBear31 field rationale, 15-bus LogUp
layout (closes the `9-bus → 15-bus` doc-drift gap), SSMC construction.
Detailed inventory moves to supplementary / AE material.

#### §4.2 Trust Model — 0.8p

- **TCB.** The compiler is in the TCB (correctness assumption, same
  class as Cairo's compiler, Valida's ISA bindings, SP1's ELF-hash
  check). The verifier trusts the compiler's output triple
  `(program_hash, metadata_hash, static_table_root)` as the reference
  against which the artifact header is byte-checked.
- **TCB quantification.** Compiler ~14 KLoC (`tabula-lang` + `compiler`
  crates). Verifier byte-check ≤ **[1 ms]** (measured in §5). The
  replaced runtime work is approximately **[X%]** of proof constraints
  on typed workloads (measured in A1 of §5.3). These numbers instead
  of assertions are what prevent the "assume correctness" attack from
  landing — Cell A/B's TCB is comparable in structure but
  un-instrumented.
- **Artifact provenance — integrity binding, NOT soundness.** The
  submission artifact targets deterministic compilation. For
  submission-time artifact review, a deployer can run a planned
  `tabula verify-provenance <dsl_source> <artifact>` command, which
  re-compiles the DSL source with the published compiler binary and
  byte-compares the resulting triple against the artifact header.
  **This is an integrity property, not a correctness property.** It
  proves the artifact was produced by *this* compiler binary on *this*
  source; it does not prove the compiler is bug-free. Compiler
  correctness remains a trusted assumption. The operational value of
  provenance:
  - Removes trust in a third-party artifact distributor.
  - Does **not** remove trust in the published compiler binary or its
    authenticity channel.
  - Makes compiler-bug visibility linear in deployer population
    (every deployer runs the compiler themselves, so surprising
    output surfaces faster than with a centralized build).
  - Defeats adversary (ii) below: a prover who substitutes a
    semantically-different artifact cannot match the triple.
- **Deterministic compilation — a paper commitment, not an
  inheritance.** Non-deterministic sources eliminated explicitly:
  `BTreeMap` in compiler state (no `HashMap` iteration-order leakage),
  `--remap-path-prefix` rustc flag (no build-path embedding), pinned
  keygen seeds (no RNG leak into artifact), and a submission-time
  reproducibility image with a pinned toolchain (no glibc / linker
  variance). The current public repo still pins `stable`; the
  submission artifact targets bit-reproducibility across `{Linux} ×
  {pinned rustc versions}`. macOS parity is best-effort, disclosed if
  not achieved.
  **This is an engineering pre-requisite the paper must satisfy — not
  a property Tabula inherits from Plonky3 or Rust.**
- **Adversary capabilities (outside TCB)**: (i) malicious prover;
  (ii) malicious artifact transport (caught by provenance + triple
  byte-check); (iii) malicious statement substitution (caught by
  `BoundStatement` from M4); (iv) chosen-transaction adversary on
  the prover side.
- **Measurement slot.** The verifier triple byte-check and planned
  provenance byte-compare are reported as verifier-only microbenches and
  folded into `verify_latency` when the end-to-end external verification
  flow is measured.
- **Soundness sketch for M4.** `BoundStatement`'s collision-resistance
  argument; why statement-first check tightens against (ii) and (iii)
  — Cell A/B's statement-in-proof idiom does not close (iii) without
  effectively reconstructing M4.
- **Soundness sketch for M3c.** Cross-shard cumsum balance ⇒
  cross-shard equivalence with a monolithic LogUp argument; formal
  details move to supplementary.

### §5 Evaluation — 3.25p

Structured as a question chain. §5.4 ("Could it mislead?") absorbs
"regimes where Tabula does not win" with numeric thresholds — this
is the honest place for losing regimes, not §7.

#### §5.1 Methodology and Engineering Parity — 0.55p

- **Workload.** StarkEx-class sweep `(N, M, S)`; see
  [`eurosys-2026-workload.md`](eurosys-2026-workload.md).
- **Bridge from the running example.** The evaluation uses mixed batches
  over the same `accounts` / `withdrawal_queue` state model as the
  SpotTrade slice in §2; the running example is the one-transaction view
  of the same workload family, not a separate toy benchmark.
- **Comparison mode.** This is a semantic-native systems comparison with
  aligned Poseidon2-SMT commitments. It does **not** claim a
  constraint-equalized benchmark outside that axis.
- **Poseidon2-SMT parity rule** (forced for all systems).
- **Signature-elision disclosure with extrapolation.** Starkware
  Pedersen+ECDSA as real-deployment reference; extrapolation row
  projects total-cost if signatures were included.
- **RISC0 drop criterion (tightened, falsifiable)**. Included unless
  **(a)** ≤ 5 engineer-days of integration effort fail to establish
  functional parity with SP1 on the workload, **(b)** the chosen
  hashing primitive dominates ≥ 80% of cycles *and* no Poseidon2
  precompile is available by the submission freeze, or **(c)**
  the workload exceeds RISC0's documented memory or segment limits
  at the reference point `(N=16, M=5k)`. Disclosure in-paper, not in
  rebuttal.
- **Engineering-parity discipline for internal ablation variants.**
  The ablation variants in §5.3 — A1-off (NF-elision), A2-off
  (uniform-width chips), monolithic-sharding (M3 disabled) — are not
  toy implementations. Each receives equal optimization attention:
  profiler-driven hot-path fixes, same engineer-hour budget, published
  commit log per variant (Supplementary C). Measured deltas in Fig 5 are
  therefore **upper bounds on mechanism contribution given equal (not
  infinite) engineering effort**. Without this discipline Fig 5's
  top-panel heights would be inflated by our own underinvestment in
  the degenerate baselines.
- **Scope of the parity discipline — internal only.** The discipline
  above applies to **Tabula's internal ablation variants only**. SP1
  and RISC0 are evaluated as published upstream projects (best-effort
  reproducibility from their canonical builds at pinned commits); we
  do not and cannot match their multi-year optimization investment.
  The internal parity argument therefore does **not** imply
  parity-matched external comparison against SP1 or RISC0.
- Hardware spec, repetition count (≥ 5), 95% CIs.

#### §5.2 Does Tabula Win End-to-End? — 0.9p

- **Figure 4 — A5 headline bar chart.** `end_to_end_latency`
  (headline), `verify_latency`, `proof_size_bytes`, `peak_rss_bytes`
  for Tabula / SP1 / RISC0 at fixed `(N=16, M=5k, S=100k)`. Each
  system reports `cold_first_proof` and `warm_steady_state`
  separately per the benchmark spec; the headline bar is
  `warm_steady_state`, with `cold_first_proof` shown as an annotation
  on the same column. `prove_only_latency` is reported as secondary,
  not headline, because the three systems expose different
  execution/proving seams and a prove-only comparison silently hides
  cost that one system pays in execution and another pays in proving.
  This framing follows the internal benchmark spec (see
  [`../research/tabula-zkvm-benchmark-spec.md`](../research/tabula-zkvm-benchmark-spec.md)
  §"Required Timing Categories" and §"Warm vs Cold Policy").
- **Table 3 — Compile-time cost, amortization, schema-evolution
  cadence.** Per system: one-time compile/setup latency (Tabula
  `tabula compile`, SP1 circuit setup, RISC0 ELF+keygen). Break-even
  batch count at three cadence tiers: **per-release** (every code
  change), **monthly**, **quarterly**. Schema-delta scenarios
  measured as concrete events that reset amortization:
  - (i) Add one column to `accounts` (width-vector change).
  - (ii) Add one asset to `PriceGrid` (relation-table growth).
  - (iii) Widen `nonce` from U64 to U128 (type change).
  Each produces a different recompile cost. This makes the amortization
  argument conditional on measurable deployment scenarios rather than a
  generic "compile-time wins" narrative.

#### §5.3 Which Mechanisms Contributed? — 1.10p

- **Figure 5 — mechanism-contribution deltas (Tabula-internal).**
  For each mechanism in {M1, M2, M3, M5} (M4 has no ablation),
  relative % reduction in prover runtime from the corresponding
  off-variant to full Tabula, with error bars. Engineering-parity
  discipline from §5.1 applies; heights are upper-bound claims under
  equal internal-variant optimization attention.
- **A1 NF-elision compiler mode.** Submission-time compiler mode
  `--nf-elision`. Per-mechanism ablation point feeding Fig 5.
- **A2 uniform-width chip.** Full re-compile with uniform Digest
  widths; width-inflation factor as derived metric.
- **A3-seal.** At fixed cores and `N`, compile-time-sealed shard
  topology vs. runtime-resolved topology. Isolates the compile-time
  share of M3 from the embarrassingly-parallel share.
- **A3-scaling (supplementary).** Per-column shard parallelism as
  `N ∈ {4,8,16,32}`, cores `∈ {1..}`. Detailed scaling curves move to
  supplementary so the main paper can keep §5 focused on end-to-end
  answer, attribution, and misleading-regime checks.

#### §5.4 Could the Comparison Mislead? — 0.60p

- **Transfer-of-representation micro-experiment.** Port account rows
  to SP1 in both multi-column layout (Tabula DSL shape) and vault-flat
  layout. Tests whether representation choice, rather than compile-time
  sealing, explains the speedup.
- **Regimes where Tabula does not win — with numeric thresholds**
  (measured, not asserted):
  - (a) **Small-batch deployments.** Below `M < K_min` transactions
    per artifact, compile-time cost fails to amortize. `K_min` is
    obtained from the Table 3 amortization-curve crossing at the
    reference `(N=16, S=100k)` — **[TBD: typically 10³–10⁴ range]**.
  - (b) **Low column-variance workloads.** When the per-column width
    distribution is near-uniform, M2's width-specialization benefit
    approaches zero. Threshold reported as width-inflation factor `<
    1.Y×`, defined as the A2 sweep point where the M2 delta falls within
    95% CI of zero.
  - (c) **Heavy runtime key-aliasing.** When the aliased-transaction
    fraction exceeds `α_max`, NF-4's symbolic check falls back to
    runtime aliasing assertions often enough to degrade M1. Threshold
    is the aliased-fraction sweep point where the A1 delta falls within
    95% CI of zero — **[TBD]**.
  - (d) **Schema-evolution-heavy deployments.** When recompile
    cadence drops below `τ_min` batches per schema event,
    compile-time amortization resets faster than it accrues.
    `τ_min` is the cadence point where Tabula's amortized end-to-end
    cost exceeds the reference SP1 warm steady-state cost in Table 3.
- Signature-elision extrapolation.
- Tabula-native workload caveat (mitigation: transfer-of-representation
  above).
- Unary-key restriction acknowledged.
- Statistical rigor caveat.

### §6 Related Work — 0.55p

- **Table 5 — 5-quantity operational audit applied across systems.**
  Rows: Cairo, Valida, Miden, SP1, RISC0 (Cell A included as sanity
  check), Tabula. Columns: (1) program-logic commitment, (2)
  constraint-family inventory, (3) routing plan, (4) lookup
  parameterization, (5) materialized constant commitment. **Cells name
  the specific verifier-checked field, sealed object, or hash chain**
  that binds that quantity for that system; cells marked *unbound* when
  no verifier-checked object covers it. This is the paper's operational
  audit for compile-time sealing, not a universal taxonomy. The table
  makes the binding mechanism explicit per system so readers can inspect
  the comparison on a shared, auditable footing.
- **Cell A paragraph** — short; SP1, RISC0, Jolt, Ceno, OpenVM.
  Precompile half-step noted.
- **Cell B paragraph** — short; Cairo, Valida, Miden. Points to
  Table 5 for the per-seal breakdown. Mentions a hand-counted
  constraint-complexity for a Miden port of one reference-transaction
  family, to make the exclusion rationale auditable. Cell B treated as
  structural, not empirical: no production StarkEx-class reference
  exists in any Cell-B system within the 4-week window.
- **Cell C paragraph** — short; Lighter (co-design by hand), zkLedger
  (tabular-semantics precedent). Used to **position Tabula in the
  design space**: programmable runtime-sealed systems (Cell A) pay
  per-proof sealing work; hand-tuned non-programmable systems (Cell
  C) achieve low prover cost at the cost of re-doing the co-design
  manually for every application. Tabula's contribution is to recover
  Cell C's compile-time-sealed benefit inside a programmable domain,
  without requiring Lighter's per-app manual re-engineering. This
  positioning is made in prose only — no Cell C numbers are plotted
  against Tabula, in line with the empirical-vs-structural rule below.
- **Empirical-vs-structural comparison rule** stated explicitly.
  Cell B and Cell C are contrasted *structurally, not benchmarked*;
  all plotted numbers in §5 come from Cell A + Tabula measurements on
  identically-configured workloads.

### §7 Discussion and Limitations — 0.25p

> Renamed from "Discussion / Future Work" to foreground limitations.
> Note: "regimes where Tabula does not win" lives in §5.4, not here —
> that question is part of the evaluation, not a post-hoc confession.

- **Domain restriction as design choice, not bug.** What workloads
  fit (DeFi primitives: spot trading, AMM, orderbook match, bank
  transfers, payroll, rollup bridging state). What workloads do not
  fit (arbitrary smart contracts, zkML on unstructured tensors,
  proof-of-equivalence for general programs).
- **Explicit limitations.** Unary key constraint
  (`NATIVE_MAX_KEY_COMPONENTS=1`), no signature proving,
  Tabula-native evaluation workload.
- **Future work.** Def 2 separable shard artifacts; incremental
  re-proving under state updates (the most paper-native follow-up —
  per-column SSMC means a batch touching a subset of columns can
  re-prove only those columns' shards). Both point to
  [`distributed-proving.md`](distributed-proving.md).
- **Out-of-scope for this paper but practitioner-relevant** (named
  explicitly so reviewers see awareness):
  data-availability-layer binding of `BoundStatement`; reorg / fork
  re-proving; compiler-machine operator key management; coordinated
  upgrade paths when a schema change requires prover + verifier +
  on-chain rotation. These are deployment concerns for a production
  Tabula rollup, outside the co-design contribution this paper makes.

### §8 Conclusion — 0.2p

Three short paragraphs: claim recap (C1 + co-design), empirical
takeaway (headline + attribution), invitation to build on the sealed
artifact.

## Figures and Tables (4 figures, 3 tables)

| # | Where | What |
|---|-------|------|
| Fig 1 | §1 | 2×2 sealing × programmability, **top-left marked "?"** |
| Fig 3 | §3.4.3 | Per-column shard topology + cross-shard LogUp balance |
| Fig 4 | §5.2 | A5 headline bar chart |
| Fig 5 | §5.3 | Mechanism-contribution deltas (Tabula-internal, engineering-parity bounded) |
| Table 1 | §3.3 | Chip-width matrix (Bool/U64/I64/Digest vs uniform) |
| Table 3 | §5.2 | Compile-time cost + amortization + schema-evolution cadence |
| Table 5 | §6 | **5-quantity operational audit per system** |

Figure 5 is the highest-leverage figure in the paper; without it, the
internal attribution story risks collapsing into an unstructured list of
ablations.

## Support Material Plan (supplementary / AE by default)

- **No in-paper appendix budget assumed.** If the main PDF keeps any
  appendix material, it consumes the slack above and must be justified
  against evaluation readability.
- **Supplementary A — artifact reproduction + LoC table + metadata
  binding note.** AE template content, planned `tabula
  verify-provenance` usage, reproducibility-image manifest, expected
  runtimes, and reduced-configuration reviewer path.
- **Supplementary B — M3c cross-shard bus balance soundness argument.**
  Formal equivalence with monolithic LogUp; pointer to
  `crates/machine/src/proof/prover.rs:105-110`,
  `verifier.rs:108-118`, `setup/metadata.rs:125-132`.
- **Supplementary C — ablation-variant engineering-parity log.** Commit
  hashes + hours-per-variant table.

Supplementary upload (outside 12p budget, optional for reviewers): NF
formal statements as IR predicates, DSL grammar and relation-lowering
rules, full ablation tables with 95% CIs, SP1/RISC0 guest source
listings and parity logs, A3-scaling curves, signature extrapolation
derivation, reproducibility-build audit.

## Key Decisions (Where Reviews Diverged)

- **Related Work placement: moved post-Evaluation.** EuroSys convention
  outweighed the story-architect argument for §3 placement. Compromise:
  §1 Fig 1 poses the puzzle; §6 resolves it with Table 5 + prose.
- **§1 presents 2×2 as a figure, not a second related-work table.**
  The reveal stays in prose; the in-paper related-work visual budget is
  spent on the operational audit in Table 5.
- **M4 gets no ablation (A4 = none is deliberate).** §3.5 + §4.2
  together carry the soundness argument. Threat-model treatment in
  §4.2, not an invented ablation.
- **A3 split into A3-scaling and A3-seal.** Prevents
  "parallelism ≠ co-design"; isolates the compile-time share of M3.
- **Fig 5 kept internal-only.** External end-to-end answer lives in
  Fig 4; Fig 5 is reserved for Tabula-internal attribution so §5.2 and
  §5.3 answer different questions cleanly.
- **"When Tabula does not win" belongs in §5.4, not §7.** Round-3
  narrative reviewer: §5's question chain "Does it win? / Why? /
  Could it mislead?" structurally makes losing-regimes the literal
  answer to "Could it mislead?". Leaving it in §7 would frame §5 as
  rigged.
- **Artifact provenance is integrity binding, not soundness.** Round-3
  methodology + practitioner reviewers converged: "re-compile +
  byte-compare" proves integrity (artifact matches a specific
  compiler-on-source pair), not correctness (compiler is sound).
  Framing repaired in §4.2.
- **Deterministic compilation is a paper commitment, not an
  inheritance.** Round-3 practitioner reviewer: HashMap / Cargo paths
  / RNG seeds / build-host variance are all realistic determinism
  breakers. Paper explicitly commits to BTreeMap, `--remap-path-prefix`,
  pinned seeds, and a submission-time reproducibility image.
- **§6 op-def test as Table 5 across all Cell B systems, not inline
  Cairo-only walk.** Round-3 narrative reviewer: inline walk of one
  system reads as adversarial. Table 5 keeps the operational audit
  explicit without spending a second related-work visual on the same
  2×2 reveal.
- **SpotTrade fee row planted upfront in §2.** Round-3 narrative
  reviewer: two-version running example (simple in §2, extended in
  §3.2) makes readers flip pages. Committing to the fee row from the
  start exercises NF-4's hard case naturally without polluting §2's
  didactic clarity.
- **M3 per-column vs. per-row as full paragraph.** Round-3 narrative
  reviewer: most rejected-alternative one-liners are fine, but this
  one is load-bearing for M3's mechanism identity. Full paragraph
  inside §3.4.
- **Signature-elision disclosure + extrapolation row** retained
  (prior round).
- **Cairo/Valida/Miden structural only, with Miden constraint-count
  reference** retained (prior round).

## Applied Changes

### Round 3 (current revision) — triggered by three-lens agent review

| Change | Source |
|--------|--------|
| Provenance reframed as integrity binding, not soundness | Methodology (HARD) + Practitioner (HARD) |
| §4.2 TCB quantification with LoC + byte-check µs + adversary example | Methodology (HARD) |
| Determinism as paper commitment with enumerated non-determinism sources | Practitioner (HARD) |
| `tabula verify-provenance` tool specified in §4.2 and Appendix A | Practitioner (HARD) |
| Fig 5 reframed from attribution stack to two-panel | Methodology (HARD) |
| Engineering-parity discipline for internal ablation variants in §5.1 + Appendix C | Methodology (HARD) |
| §3.7 asymptotic complexity sidebar with M3c `shard_count · buses` caveat | Methodology (HARD) |
| "When Tabula does not win" moved §7 → §5.4 with numeric thresholds | Narrative (HARD) + Practitioner (HARD) |
| §6 op-def test as Table 5, not inline prose | Narrative (HARD) |
| SpotTrade extended with fee row planted upfront in §2 | Narrative (HARD) |
| M3 per-column vs. per-row as full design-alternative paragraph | Narrative (SOFT) |
| Other M-sections get one-line rejected-alternative | Narrative (SOFT) |
| §5.2 Table 3 with named schema-delta scenarios + cadence curves | Practitioner (HARD) |
| RISC0 drop criterion: added memory/segment limit criterion (c) | Practitioner (SOFT) |
| §5.3 Fig 5 bottom panel adds Lighter-style hand-tuned lower bound | Methodology (SOFT) |
| §7 adds out-of-scope list (DA / reorg / key-mgmt / upgrade path) | Practitioner (SOFT) |
| §1 hook becomes SpotTrade-on-SP1-vs-Tabula 4-sentence concrete opener | Narrative (f) |
| Page budget rebalanced: §1 0.9, §2 1.0, §3 4.6, §4 1.3, §5 2.9, §6 0.75, §7 0.35, §8 0.2 | Self (page pressure from additions) |

### Round 4 — triggered by Codex round-3 review + self-check

| Change | Source |
|--------|--------|
| §3.1 op-def box clarifies "byte-check" admits collision-resistant hash binding (Tabula `metadata_hash` / Cairo `program_hash` / SP1 `vkey`) | Codex (taxonomy fairness — presentational fix) |
| §3.4 per-row rejected-alternative: "empirically" → "structurally a different mechanism" | Codex (argument is structural, not empirical) |
| §5.1 parity-discipline block made internal-only explicit; SP1/RISC0 named as external anchors, not parity-matched baselines | Self-check (Codex silent on internal/external asymmetry) |
| §5.2 Fig 4 reworked around `end_to_end_latency` (headline) + cold/warm separation; `prove_only_latency` demoted to secondary | Codex (headline metric ambiguity) + benchmark-spec alignment |
| §5.2 Table 3 CLI verb `tabula build` → `tabula compile` | Codex (CLI drift) |
| §5.3 Fig 5 bottom panel: Lighter-style lower bound removed; anchors reduced to Tabula / SP1 / RISC0 (cold + warm) | Codex (Cell-C structural-only commitment drift) |
| §6 Cell C paragraph absorbs Lighter positioning narrative (programmable-runtime-sealed vs. hand-tuned-non-programmable design-space claim) | Self-check (preserve positioning after removing Lighter from Fig 5) |
| Figures/Tables table + "highest-leverage figure" commentary synchronized with new Fig 5 framing | Self-check (cross-reference hygiene) |
| Workload spec: `SpotTrade` gains `fee_collector_id` + `fee_rate`; retains semantic fee collection while eliding order-flow fee routing | User decision (§5.4(c) heavy-aliasing threshold needs empirical NF-4 hard-case basis in main workload) |
| `contributions.md` Fig 5 block rewritten from "additive stack" to two-panel description (cross-doc consistency with outline) | Self-check (outline-vs-contributions drift) |
| §3.1 op-def rubric upgraded from ✓/✗ count to **named-field test**; §6 Table 5 cells now carry specific artifact field / hash pre-image names per system; Appendix A includes `metadata_hash` pre-image decomposition | Codex round-3 residual (Medium-high — "Tabula-shaped rubric" attack not fully closed by hash-binding clause alone) |

### Round 5 — final harmonization before drafting

| Change | Source |
|--------|--------|
| Exact-12.0 budget replaced with 11.3 authored-page target plus explicit slack | Final review (packaging realism) |
| No in-paper appendix budget assumed; support material moved to supplementary / AE by default | Final review (12p realism) |
| Hook rewritten in reviewer-visible state terms; running example explicitly framed as a single-transaction slice of the evaluated workload | Final review (story continuity + readability) |
| §3.1 compressed to pipeline + compact op-def; most taxonomy-defense prose moved out of the design opening | Final review (story transport) |
| M4 rewritten to live `PublicStatement` / `BoundStatement` vocabulary; `semantic_hash` / `profile_hash` demoted to internal metadata terms | Final review (theorem vocabulary drift) |
| §4.2 provenance/determinism language shifted to submission-time target tense and scoped against compiler-binary authenticity | Final review (readiness + trust-model precision) |
| Provenance byte-check assigned to verifier-only microbench / `verify_latency` accounting | Final review (evidence mapping) |
| §5.1 adds explicit running-example bridge and semantic-native comparison-mode declaration | Final review (reader continuity + fairness framing) |
| Fig 5 reduced to internal attribution only; A3-scaling moved to supplementary | Final review (question-chain separation + page pressure) |
| §5.4 thresholds now state their fitting procedure | Final review (evidence sufficiency) |
| Fig 2, Fig 6, Table 2, and Table 4 removed from the main-paper inventory | Final review (float reduction) |

### Prior-round changes (still in force)

| Change | Source |
|--------|--------|
| 14p → 12p rebudget (CFP includes figures/tables/appendices) | Codex (CFP) |
| §4 renamed "Trust Model", compiler pinned as TCB | Codex (TCB contradiction) |
| §5.2 adds Table 3 compile-time cost + amortization | Codex (runtime→compile unmeasured) |
| A1 phrasing fixed to first-class compiler mode | Codex (drift) |
| §4.3 scoped to submission-time intended (disclosed) | Codex (overclaim) |
| `bank.tab` removed from §7 limitations | Codex |
| §5 restructured as question chain | Codex |
| Related Work moved to §6 (post-Eval) | Round 1 Norms |
| §1 Fig 1 shows "?" in top-left (puzzle, not reveal) | Round 1 Story |
| §2 ends with boxed **Principle (C1)** | Round 1 Story |
| §3.1 operational-definition box | Round 1 Hostile |
| Mechanism-attribution figure promoted | Round 1 Hostile |
| A3 split into A3-scaling + A3-seal | Round 1 Hostile |
| Transfer-of-representation micro-experiment | Round 1 Hostile |
| Signature-elision disclosure + extrapolation | Round 1 Hostile |
| §7 renamed "Discussion and Limitations" | Round 1 + Norms |
| Miden constraint-count reference in §6 | Round 1 Hostile |
| §3.7 Synthesis subsection + Table 2 | Round 1 Story |

## Remaining Open Decisions

- **Fig 1 style.** Quadrant figure vs. 2×2 with system-logo clusters.
- **Fig 5 axes styling.** Stacked bar vs. line chart; decide after
  first measurements.
- **macOS bit-reproducibility parity.** Best-effort; fall back to
  Linux-only if blocked by toolchain variance.
- **§5.4 numeric thresholds.** `K_min`, `α_max`, `τ_min` are
  TBD-by-measurement and filled during draft production.

## Pointers

- Contributions and M1-M5: [`eurosys-2026-contributions.md`](eurosys-2026-contributions.md)
- 2×2 frame: [`eurosys-2026-related-work.md`](eurosys-2026-related-work.md)
- Workload spec: [`eurosys-2026-workload.md`](eurosys-2026-workload.md)
- Def 2 / incremental proving (future work): [`distributed-proving.md`](distributed-proving.md)
- Benchmark spec (compile-time cost axis): [`../research/tabula-zkvm-benchmark-spec.md`](../research/tabula-zkvm-benchmark-spec.md)
- Architecture canon: [`../design/architecture.md`](../design/architecture.md)
