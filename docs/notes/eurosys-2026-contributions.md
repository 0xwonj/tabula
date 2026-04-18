# EuroSys 2027 — Paper Contribution Lock

Working note capturing the **locked contribution list** for the EuroSys 2027
submission (deadline 2026-05-14; conference held April 2027). This note
is:

- **Authoritative for the paper plan** until it is superseded by a draft.
- **Not authoritative** for architecture or implementation contracts —
  the authority for those remains `docs/design/architecture.md` and the
  crate-level `README.md` files.

Supporting notes:

- [`distributed-proving.md`](distributed-proving.md) — Def 1 vs Def 2
  analysis; committed to Def 1 for this paper.
- [`eurosys-2026-workload.md`](eurosys-2026-workload.md) — **locked**
  workload spec: StarkEx-class multi-asset spot-trading rollup,
  Tabula-native, optimized for the co-design claim. Supersedes §8.4 of
  the harness note.
- [`eurosys-2026-related-work.md`](eurosys-2026-related-work.md) —
  **locked** related-work framing: 2×2 (sealing time × programmability)
  with Tabula occupying the previously empty programmable ×
  compile-time-sealed corner. Names per-cell systems and the
  empirical-vs-structural comparison split.
- [`eurosys-2026-section-outline.md`](eurosys-2026-section-outline.md)
  — **locked** section-level outline: 12p budget, figure/table plan,
  support-material plan, and the review-synthesis record. Promotes the
  mechanism-contribution figure (Fig 5) to the paper's C1-defending
  figure and splits A3 into A3-scaling + A3-seal.
- [`evaluation-harness.md`](evaluation-harness.md) — `tabula-eval`
  crate design: workload model, SystemAdapter trait, reuse policy,
  measurement schema.
- [`evaluation-stage-interfaces.md`](evaluation-stage-interfaces.md)
  — cross-role stage types the harness consumes (contract-owned
  `SealedArtifact` / `ExecutionRecord` / `ExpectedStatement` /
  `Proof` / `AttestedStatement`; prepared handles in runtime /
  machine; feature-matrix rules).

## Headline Claim

> **Compiler–proof co-design turns runtime re-proving into compile-time
> sealing, for typed tabular state transitions.**

Two sentences to carry the claim:

1. Typed tabular state transitions — batches over tables with fixed schemas
   and known `(table, col)` coordinates — admit compiler-enforced normal
   forms and sealed machine shapes that general-purpose zkVMs cannot
   architecturally assume.
2. Tabula instantiates this co-design as a zkVM where the compiler seals
   the program, schema, and proof topology at build time, and the prover
   and verifier consume those seals rather than re-deriving them per
   proof.

This is not a claim that Tabula is universally better than general zkVMs.
It is a claim that **if your workload fits the typed tabular shape, then
runtime work that a general zkVM must pay per proof can be lifted to
build time without loss of soundness relative to a trusted compiler**.

## Trust Boundary (Required Qualifier On C1)

The compiler is in the TCB. C1's "without loss of soundness" is scoped
*relative to the trusted compiler*. This matches Tabula's architecture
(compiler as semantic-registration authority, see
[`docs/design/architecture.md`](../design/architecture.md)) and matches
every system in the 2×2 with non-trivial compile-time behaviour
(Cairo's compiler, Valida's ISA bindings, SP1's ELF-hash check all
trust the compiler). Any sentence that reads as "Tabula is sound
against a malicious compiler" is **wrong** and must be rewritten.

Authoritative detail lives in §4.2 of the locked outline. Four points
that are *not* optional and must stay coherent with the outline:

1. **TCB quantification, not assertion.** The compiler is ~14 KLoC
   (`tabula-lang` + `compiler` crates). The verifier byte-check of the
   sealed `(program_hash, metadata_hash, static_table_root)` triple
   is ≤ **[1 ms]** (measured in §5). The replaced runtime work is
   approximately **[X%]** of proof constraints on typed workloads
   (measured in A1). These numbers instead of assertions are what
   prevent the "assume correctness" attack from landing — Cell A/B's
   TCB is comparable in structure but un-instrumented.
2. **Artifact provenance is integrity binding, NOT soundness.** Tabula
   targets deterministic compilation for the submission artifact; a
   deployer can run a planned
   `tabula verify-provenance <dsl_source> <artifact>`, which
   re-compiles and byte-compares the triple against the artifact
   header. This proves the artifact was produced by *this* compiler
   binary on *this* source; it does **not** prove the compiler is
   bug-free. Compiler correctness remains a trusted assumption. Value:
   removes trust in a third-party artifact distributor, but does not
   remove trust in the published compiler binary or its authenticity
   channel; it also makes compiler-bug visibility linear in deployer
   population.
3. **Deterministic compilation is a paper commitment, not an
   inheritance.** Non-deterministic sources are eliminated explicitly
   (BTreeMap, `--remap-path-prefix`, pinned keygen seeds, Docker
   image with pinned toolchain). Bit-reproducibility commits to
   `{Linux} × {pinned rustc versions}`; macOS is best-effort with
   disclosure.
4. **Adversaries (outside the TCB).** (i) malicious prover; (ii)
   malicious artifact transport (caught by provenance + triple
   byte-check); (iii) malicious statement substitution (caught by
   `BoundStatement` from M4); (iv) chosen-transaction adversary.

## Contribution List

### C1 — Principle

**Compiler–proof co-design for typed tabular state transitions.** A
zkVM-scoped statement of what compile-time sealing can replace at
runtime, with precise boundaries (what *can* be sealed vs. what must
remain dynamic), and what that buys in cost and reviewability.

### C2 — Tabula Instantiation (Five Co-Design Mechanisms)

The instantiation of C1 as an end-to-end system. Five mechanisms, each
tied to a concrete implementation surface.

#### M1 — Compile-time normal-form sealing (NF-1..4 + True SSA)

Four intra-tx normal forms (unique-read, unique-write, no-read-after-write,
key-alias resolvability) plus True SSA, enforced by the compiler and
**sealed into the program binding**, so the prover does not re-prove
intra-tx RAM consistency.

- Code: NF error types at `crates/core/src/error.rs:96-155`, implicit
  SSA coverage at `crates/ir/src/validate/entry.rs:48-84`, mandatory
  inline+canonicalize at `crates/compiler/src/pipeline/compile.rs:82-95`.
- Completion prerequisite: tighten NF enforcement surface (see
  *Completion Plan*).
- Paired with ablation **A1** (see C3).

#### M2 — Schema-typed chip width specialization with sealed machine shape

Static schema typing (Bool=1, U64=3, I64=3, Digest=8 FE) drives
width-specialized chips whose allocation is fixed at compile time and
sealed into the artifact. The verifier checks that the machine shape
matches the sealed shape before checking the proof.

- Code: width-per-type at `crates/profile/src/builtins.rs:229-299`,
  per-column chip allocation at `crates/chips/src/shards/ssmc.rs:82-106`.
- Paired with ablation **A2**.

#### M3 — Static-coordinate per-column proof sharding with compile-time scheme specialization

The headline mechanism. Static `(table, col)` coordinates plus per-column
commitments enable end-to-end per-column shard proofs, with scheme
selection and anchor multiplicities resolved at compile time. Structured
as four sub-mechanisms:

- **M3a — Static `(table, col)` coordinates + independent per-column
  sub-proof.**
  One `ProofInstance` per column shard, proved in parallel within one
  machine. Code: `crates/machine/src/proof/prover.rs:63-123`.

- **M3b — Per-column SSMC commitment.**
  Sorted-State Merkle Chain (Poseidon2 hash chain) per column, allowing
  each column's state to move independently. Code:
  `crates/chips/src/shards/ssmc.rs`, `crates/commitment/`.

- **M3c — Cross-shard LogUp cumulative-sum balance.**
  Bus cumsums are carried forward across tiers and balance is enforced
  end-to-end at compose time, which closes the cross-shard soundness
  gap without requiring a monolithic chip. Code:
  `crates/machine/src/proof/prover.rs:105-110`,
  `crates/machine/src/proof/verifier.rs:108-118`,
  `crates/machine/src/setup/metadata.rs:125-132`.

- **M3d — Compile-time scheme specialization.**
  Property-query analysis drives per-column scheme selection, conditional
  chip gating, and static anchor multiplicities — so the prover never
  pays for machinery a column doesn't need. Code:
  `crates/compiler/src/registration/keys.rs:27-48, 206-218, 308-317`.

Paired with ablation **A3** (core-scaling demonstration).

#### M4 — Statement-first verifier with sealed program binding

The verifier checks `(sealed artifact, expected PublicStatement, proof)`
as a unit, not "trust the statement in the proof." The paper-facing
theorem object is
`PublicStatement = (old_root, new_root, public_context_digest, applied_tx_digest, event_digest)`.
`BoundStatement` is the verifier-side object that binds that statement
to one sealed artifact. `semantic_hash` and `profile_hash` remain
metadata-envelope internals that explain how the artifact is identified;
they are not the theorem statement itself.

Code: `crates/contract/src/verification.rs:31-59, 95-125`,
`crates/compiler/src/registration/binding.rs:9-33`,
`crates/contract/src/format/static_tables.rs:47-76`,
`crates/chips/src/shards/meta/air.rs:54-62, 327-364`.

No dedicated ablation — M4 is the load-bearing soundness mechanism, not
a performance knob.

#### M5 — Compile-time resolution of relations into sealed static tables

The `tabula-lang` DSL's `relation` construct (enum / range / set / map /
function) is lowered at compile time into `RangeCheck` and
`StaticTableLookup` buses, with the resolved tables sealed under the
`static_table_root`. Range and set membership become static lookups
rather than runtime predicates.

- Code: `crates/compiler/src/hir_lower/manifest.rs:150-218`.

Smallest of the five mechanisms; included because it exemplifies the
co-design principle at the DSL layer.

### C3 — Empirical Validation With Explicit Ablation Taxonomy

End-to-end empirical validation on proof-capable workloads, structured
as a **taxonomy of ablations** (not a single benchmark table) that
isolates each co-design mechanism's contribution.

**Ablation taxonomy:**

| Ablation | Type | Targets | Notes |
|----------|------|---------|-------|
| A1 | compiler-mode toggle | M1 | NF-elision as a paper-required compiler mode (planned flag `--nf-elision`). Designed *together* with the NF enforcement implementation; not a runtime-assertion afterthought. |
| A2 | toggle | M2 | Uniform-width chip comparison; cheap (~1 day). |
| A3-scaling | scaling | M3 | Core-scaling demonstration — per-column shard parallelism vs. monolithic execution as shard count grows. |
| A3-seal | toggle | M3 | Compile-time-sealed shard topology vs. runtime-resolved topology at fixed cores. Isolates the *compile-time* share of M3 from the embarrassingly-parallel share. Closes the "parallelism ≠ co-design" attack. |
| A4 | baseline | M4 | M4 has no toggle; it is the soundness mechanism. No ablation. |
| A5 | structural / comparative | M5 + whole system | SP1 (required) and RISC0 (best-effort) end-to-end comparison on the same workload. Shows the *cost* of not having co-design, not an isolated M5 study. Includes the **transfer-of-representation micro-experiment** (port account rows to SP1 in multi-column vs. vault-flat layouts) to test whether representation choice, rather than compile-time sealing, explains the gain. |

**First-class evaluation axes (beyond the ablation toggles):**

- **Headline end-to-end cost** — `end_to_end_latency` (headline),
  `verify_latency`, `proof_size_bytes`, `peak_rss_bytes` at a fixed
  workload point, reported separately for `cold_first_proof` and
  `warm_steady_state` per the benchmark spec's cold/warm vocabulary.
  `prove_only_latency` is reported as secondary (systems expose
  different execution/proving seams).
- **Compile-time cost and amortization.** A claim of "runtime →
  compile time" is unmeasured unless compile-time cost is reported.
  One-time compile / setup latency per system (Tabula `tabula compile`,
  SP1 circuit setup, RISC0 ELF+keygen) plus a break-even batch-count
  curve showing when the lifted compile-time work pays back. Pulled
  from the benchmark spec's `compile_or_setup_latency` axis
  ([`../research/tabula-zkvm-benchmark-spec.md`](../research/tabula-zkvm-benchmark-spec.md)).
- **Mechanism-contribution figure** — Fig 5 of the section outline.
  Relative prover-time reduction per mechanism (A1-off → full Tabula
  for M1; A2-off → full for M2; monolithic-sharding → full for M3;
  relation-runtime → full for M5), with Supplementary C
  engineering-parity
  discipline bounding the heights as upper bounds under equal
  internal-variant optimization attention. External end-to-end
  comparison stays in Fig 4 + Table 3 so the attribution figure answers
  only the internal “why?” question.

Workload selection and concrete metrics are tracked separately; the
workload itself is locked in
[`eurosys-2026-workload.md`](eurosys-2026-workload.md), and the
harness that measures it is specified in
[`evaluation-harness.md`](evaluation-harness.md) (consuming the stage
types from [`evaluation-stage-interfaces.md`](evaluation-stage-interfaces.md)).

### C4 — Open Artifact

Reproducible open-source release: DSL → compiler → sealed artifact →
prover → verifier, with the `basic` and `membership` examples runnable
end-to-end. See `ARTIFACT.md` for what is proof-capable today.

## Explicitly Not A Contribution

Mechanisms and techniques that are present in the codebase but are
**not** being claimed as contributions (either well-known or narrow
engineering):

- LogUp (Haböck) — used, not claimed.
- Poseidon2 — used, not claimed.
- Range-check mechanism details — implementation of a known technique.
- SMT path verification mechanism — standard cryptographic construction.
- Extensible `BusId` — internal engineering.
- Schema versioning / migration machinery — engineering; not a research
  claim.
- Receipt bridge / external verifier integration — engineering.
- DSL itself as an independent contribution — `tabula-lang` is framed as
  part of C2 co-design scaffolding (specifically the M5 relation
  lowering), not as a standalone headline.

## Scope Guardrail

Tabula is **not** a general-purpose zkVM and does not compete with SP1 /
RISC0 / OpenVM / Jolt on their own axes (arbitrary C / Rust ELF
execution). The claim is restricted to typed tabular state transitions.
Any statement in the paper that risks reading as "Tabula is generically
better" is out of scope.

## Completion Work (Prerequisite For Contributions To Be True)

Hygiene / completion items that gate the paper being honest about the
contributions above. Timings tracked elsewhere.

- **NF enforcement tightening + NF-elision toggle.**
  ~150–250 LoC total across:
  - NF-1 / NF-2 / NF-3 linear-scan checks in IR validate (~20–30 LoC each).
  - True SSA explicit check (~5–10 LoC).
  - NF-4 symbolic row-key analysis in the frontend (~80–150 LoC).
  - NF-elision toggle designed jointly with the checks so A1 (ablation)
    is a first-class compiler mode.
- **Multi-shard (Def 1) integration test + docs.**
  Multi-shard is ~95% complete. Missing: integration test that
  exercises a multi-shard workload end-to-end, and a short
  `docs/design/` section describing the cross-shard bus balance
  contract.
- **Bucket-C hygiene.**
  `unreachable!()` at `crates/compiler/src/mir/lower.rs:64, 71` for
  function escapes needs a real error path, not a panic.
- **Doc drift: 9-bus → 15-bus.**
  Docs claim 9 buses; actual is 15
  (`crates/stark/src/air/interaction.rs:46-101`). Update
  `docs/design/architecture.md` and any downstream references.
- **Stage-interface migration.** Apply the clean-break migration
  steps in
  [`evaluation-stage-interfaces.md`](evaluation-stage-interfaces.md) §14
  (remove `OpenedProgram`, relocate `ReceiptBridge` into
  `tabula-contract`, drop hidden SDK caches, introduce
  `AttestedStatement`, tighten the `verify` feature). The harness
  depends on this migration being complete.

## Future Work (Explicitly Deferred)

These are **not** contributions of this paper, but they are structurally
enabled by the co-design and will be named in the Future Work section:

- **Separable shard artifacts (Def 2).** Full analysis in
  [`distributed-proving.md`](distributed-proving.md).
- **Incremental re-proving under state updates.** The most paper-native
  Def 2 follow-up: per-column SSMC commitments mean batches that touch
  only a subset of columns can re-prove only those columns' shards.

## Open Follow-Up Tasks (Not This Spec)

The following paper-shaping tasks are explicitly **not** locked by this
note and will be handled in separate follow-ups:

- Evaluation harness implementation — stage APIs, fixture pipeline, SP1
  / RISC0 port, measurement runners. Workload is locked
  ([`eurosys-2026-workload.md`](eurosys-2026-workload.md)); the harness
  itself is still an open build-out.
- Writing plan (draft cadence, figure-creation schedule,
  review/rebuttal rehearsal). Section outline is locked in
  [`eurosys-2026-section-outline.md`](eurosys-2026-section-outline.md).
