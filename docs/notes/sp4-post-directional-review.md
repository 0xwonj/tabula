# SP-4 Post-Landing — Directional Review

> Status: review artifact, not authoritative architecture
> Date: 2026-04-19
> Scope: whole-workspace direction post SP-1..SP-4 landing, incl. SP-5/6/7
> Method: 4 parallel opus reviewers on distinct angles (layer direction,
> design patterns, future-scope gaps, research-fit)

This note consolidates the 4 directional reviews. It is intentionally
opinionated. Concrete contradictions with the code are flagged; everything
else is a judgement call to be discussed.

## Executive Verdict

**SP-1..SP-4 are directionally correct but expose three systemic risks
the current SP-5/6/7 plan does not address:**

1. **Paper-critical ablation A3-seal is architecturally blocked** — no
   runtime-resolved shard-topology prover path exists. Deadline 2026-05-14.
2. **The evaluation harness build-out has no SP owner** — 3-4 weeks of
   implementation needed, currently floating. Must parallelize with
   SP-5/6/7, not serialize after.
3. **`tabula-runtime` has drifted into a god layer** and `tabula-ext`'s
   as-landed layer assignment contradicts `docs/design/architecture.md`.
   SP-5 as written does not commit on either.

These are not "nice to have" findings — items 1 and 2 are load-bearing
for the EuroSys submission. Items 3+ are quality-of-life but compound
every subsequent SP's cost.

---

## Finding 1 — A3-seal is blocked (paper-critical)

**Claim:** The ablation `A3-seal` (compile-time-sealed topology vs.
runtime-resolved topology at fixed cores) currently has no architectural
home. `ShardTopology::RuntimeResolved` exists in the harness spec, but
the Tabula prover always seals topology into `ProofLayout` at compile
time — there is no code path that defers shard planning to prove time.

**Consequence:** Without this variant, A3-seal collapses into A3-scaling
under a different axis. The "parallelism ≠ co-design" reviewer attack
lands unopposed, and Fig 5's mechanism-contribution story loses its
strongest empirical defense.

**Options:**

- **(a) Build the variant.** Add a prover path that re-derives shard
  topology at prove time (fresh code, outside SP-5's decomposition
  scope). Estimate: 1-2 weeks.
- **(b) Reframe the ablation.** Rename A3-seal to a comparison the
  current architecture supports (e.g., "full-monolithic vs.
  sealed-per-column"), and amend §3.4.3 + Fig 5 accordingly.

**Recommended decision window:** next 7 days. Past that, paper prose
will calcify around whichever variant is assumed.

---

## Finding 2 — Harness build-out has no SP owner

**Claim:** `tabula-eval` crate, `SystemAdapter` trait, `TabulaAdapter`,
`Sp1Adapter`, `Risc0Adapter`, workload/fixture impl, measurement,
cache — none are owned by SP-1..SP-7. The umbrella explicitly states
"this refactoring is not harness-driven," which is defensible, but
treating the umbrella as the full pre-artifact plan leaves this critical
work floating.

**Consequence:** With ~25 days to deadline and SP-5/6/7 still
outstanding, starting the harness after SP-7 is infeasible. The
harness must parallelize with SP-5/6/7 — which in turn means every
rename in SP-5/6 must be mirrored in the harness within the same
commit.

**Recommendation:** Create **SP-9 — Evaluation harness build-out**
immediately and start it in parallel with SP-5. Scope is precisely
the "open follow-up tasks" in `eurosys-2026-contributions.md`. The
harness can consume SP-4's `PreparedProver`/`PreparedVerifier` today.

---

## Finding 3 — Floating prerequisites need a home

Several items in `eurosys-2026-contributions.md §Completion Work` are
not owned by any SP:

| Item | Currently | Proposed owner |
|------|-----------|----------------|
| NF-1/2/3/4 + True SSA tightening (~150-250 LoC) | floating | **SP-8** |
| `--nf-elision` compiler mode for A1 | floating | **SP-8** |
| Multi-shard (Def 1) integration test + bus-balance doc | floating | SP-5 extension |
| 9-bus → 15-bus doc drift | floating | SP-6 subtask (make explicit) |
| Bucket-C `unreachable!()` hygiene | floating | SP-8 |
| `tabula verify-provenance` CLI | floating | **SP-8** |
| Deterministic-compilation audit (BTreeMap, remap, Docker) | floating | **SP-8** |
| Reproducibility CI gate (byte-identity on x86_64+aarch64) | floating | **SP-10** |
| Layer-boundary CI enforcement script | SP-6 optional | **SP-10** (promote) |

**Recommendation:** Create **SP-8 — Paper prerequisites (compiler +
provenance)** covering NF tightening, `--nf-elision`, `verify-provenance`,
deterministic compile, Bucket-C. Create **SP-10 — Reproducibility gate
/ CI** covering the CI work currently scattered across optional
checkboxes.

---

## Finding 4 — Runtime is a de-facto god layer

**Claim:** `crates/runtime/Cargo.toml` depends on contract, compiler,
profile, ext, executor, machine, chips, stark, witness, commitment, plus
raw `p3-*` crates. `engine.rs` is still ~3 kLOC. SP-4's punt on
`TabulaRuntime` facade disposition compounds this: execute, prove,
verify, snapshot, statement materialization all share one assembler.

**Consequence:** "Runtime owns policy" has slid toward "runtime owns
everything hard to place." SP-5 as scoped only decomposes files — it
does not rethink the layer.

**Recommendation:** Rescope SP-5 to **commit on `TabulaRuntime`**.
Either:

- **(a)** promote it to a third named handle `PreparedExecutor`
  (symmetric with `PreparedProver`/`PreparedVerifier`), or
- **(b)** delete it and let SDK assemble execute-flavored calls
  directly.

Leaving it as a residual facade under the `verify` feature is a
long-term liability, not a scope save.

---

## Finding 5 — `tabula-ext` layer assignment is contradicted

**Claim:** `docs/design/architecture.md` places `tabula-ext` under
"Public Package Surfaces" (above the backend layer). But SP-3 relocated
`ChipWitnessKit` to `tabula-stark` because ext sits *above* witness
and cannot be imported downward. Meanwhile ext still owns
`ExecutionBackend`, an authoring protocol backend crates must see.
Authoring-protocol and public-package cannot occupy the same layer.

**Options:**

- **(a)** Split `tabula-ext` into `tabula-authoring-protocol` (traits,
  below machine/witness) + `tabula-ext` (public packaging, above).
- **(b)** Amend `docs/design/architecture.md` to show ext straddling,
  and accept that the layer doc has two categories collapsed.

**Recommendation:** Option (a) via a new SP-4.5 (or fold into SP-8).
Leaving layer-doc and code disagreeing erodes architecture.md's
authority claim.

---

## Finding 6 — Runtime "pre-stuff" pattern leaks chip awareness

**Claim:** SP-3's amendment admits `RelationTableKit` plus
context/tx-batch/event transcript kits require runtime to install row
buffers into the scratchpad before `prepare_execution_store`. This
weakens SP-3's headline ("adding a new chip touches only chips + machine");
runtime-sourced chips *do* require runtime edits. The SP-3 guardrail
was relaxed in `engine.rs` to allow `RelationTableWitnessRow` — evidence,
not a blip.

**Recommendation:** Promote pre-stuff to a named architectural
concept in SP-5. Either:

- give runtime a typed API (`install_relation_table_rows(...)`,
  `install_transcript_row(...)`), or
- push row production to executor so runtime never names chip rows.

Current shape — runtime installing chip row types into an `Any`-keyed
map — is the worst of both worlds.

---

## Finding 7 — `contract → commitment → p3-*` breaks neutrality

**Claim:** SP-1 kept the `contract → commitment` edge by reclassifying
commitment as a "shared foundation." Defensible, but
`commitment[stark]` pulls `p3-koala-bear`, `p3-field` etc. So
`tabula-contract` transitively imports Plonky3 crypto — it is not the
neutral wire-type authority the architecture claims.

**Recommendation:** In SP-6 or SP-7, either:

- **(a)** sink commitment primitives (`PoseidonHasher`, `NativeDigest`,
  `FieldHasher`) into `tabula-core` and make contract strictly
  core-only, or
- **(b)** amend `docs/design/architecture.md` to show commitment's
  primitive half as Shared Meaning, and stop claiming contract is
  crypto-free.

---

## Finding 8 — Pattern refinements (medium priority)

From the design-patterns review. Concrete, landable in SP-5/6:

1. **Narrow errors per surface.** Reintroduce `VerifyError`/`ProveError`
   (SP-4 landed a unified `RuntimeError`; callers now match against
   prove variants that can't be reached on a verify-only handle).
   Use `From` into a top-level type for callers that do both.
2. **Collapse `PreparedProverBuilder` + `prepare_prover` free fn into
   one `PreparedOptions` struct** marked `#[non_exhaustive]`. Current
   dual surface will bit-rot.
3. **Seal `ChipWitnessKit` or commit to third-party authoring.**
   Current posture (public trait, runtime duplicate check) is worst of
   both.
4. **Mark `VerifierState` `#[non_exhaustive]`** (or privatize fields).
   Cheap; removes a future breaking-change footgun.
5. **Decide the `tabula-ext` re-export story.** Either newtype the
   re-exported traits in ext, or explicitly document ext as a
   convenience facade over stark.

---

## Finding 9 — Missing architectural invariants

Umbrella §3 lists dep-direction invariants but omits these:

- **Feature-flag monotonicity.** Deferred to SP-7, but SP-5/6 can
  reintroduce non-monotone axes in the meantime.
- **`Send + Sync` on all prepared state.** SP-4 added one `const _`
  assert ad hoc; no workspace-wide policy.
- **No `tabula-chips::*Row` names in `tabula-runtime/src/**`.**
  Already broken by the runtime pre-stuff pattern.

**Recommendation:** Encode each as a guardrail test alongside the
existing SP-3 guardrail. Three SP-1..SP-3 guardrails proved their
value; extend the pattern rather than deferring.

---

## Finding 10 — M3c code citations will drift

**Claim:** Paper §3.4.3 and Supplementary B cite exact line ranges in
`crates/machine/src/proof/prover.rs:105-110`, `verifier.rs:108-118`,
`setup/metadata.rs:125-132`. SP-5's engine decomposition *will move
these lines*.

**Recommendation:** Either freeze SP-5 before paper draft, or pin every
§3.4.3 / Supplementary B citation to a commit hash rather than a moving
path.

---

## Proposed Updated SP Roadmap

Current: SP-1..SP-4 done; SP-5/6/7 planned.

Proposed:

| SP | Scope | Timing | Rationale |
|----|-------|--------|-----------|
| SP-5 | Runtime engine decomposition + TabulaRuntime disposition decision | start now | required for evaluation harness to have stable surface |
| SP-6 | SDK thinning + docs + 9-bus drift + explicit AttestedStatement | parallel with SP-5 | non-conflicting |
| SP-7 | Feature matrix unification | after SP-5/6 | boundaries must settle first |
| **SP-8** (new) | Paper prerequisites: NF tightening, `--nf-elision`, `verify-provenance`, deterministic compile, Bucket-C, ext-layer split | **start now in parallel** | gates A1 and artifact-track reviewers |
| **SP-9** (new) | Evaluation harness build-out (`tabula-eval`, SystemAdapter, adapters) | **start now in parallel** | 3-4 weeks, critical path for deadline |
| **SP-10** (new) | Reproducibility gate / CI (byte-identity, feature-matrix build, layer-boundary CI) | after SP-7, before artifact submission | artifact reviewers run these exact commands |

**Urgent decisions (7-day horizon):**

1. A3-seal: build variant or reframe?
2. `TabulaRuntime` facade: promote to `PreparedExecutor` or delete?
3. `tabula-ext` layer split vs. architecture.md amendment?
4. Commit to SP-8/SP-9/SP-10 as umbrella additions?

---

## Method Note

This review was generated by dispatching 4 opus reviewers in parallel,
each reading the SP-1..SP-4 design specs + umbrella +
`docs/design/architecture.md` + targeted notes. Findings are
cross-validated: items where ≥2 reviewers independently flagged the
same concern carry higher confidence. Items flagged by only one
reviewer are marked as such in the individual finding bodies.

Full reviewer transcripts are captured in the session JSONL and
summarized in-line above. Any claim about specific file contents is
intended to be verified before acting — the reviewers read from the
working tree, not from a fixed snapshot.
