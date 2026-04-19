# SP-5 Review Findings + Fix Plan

**Review window:** 2026-04-20
**Branch:** `sp5-runtime-decomposition` (14 commits ahead of `main`)
**HEAD at review:** `c41fdbe docs(architecture): refresh runtime handle paragraph for SP-5 landing`
**Reviewers (4 parallel passes):**
- **Pass A — Architecture + Proof Semantics** (opus)
- **Pass B — API Surface + Error Taxonomy** (opus)
- **Pass C — Mechanical Audit** (sonnet)
- **Pass D — Code Quality + Rust Idioms** (opus)

**Input artifact:** `/tmp/sp5-review-diff.patch` (11988 lines, `git diff main..HEAD`)
**Spec:** `docs/superpowers/specs/2026-04-19-sp5-runtime-decomposition-design.md`

---

## Verdict

| Pass | Result | Findings |
|---|---|---|
| A | YELLOW | 1 Blocker, 3 Important, 2 Nits |
| B | YELLOW | 0 Blockers, 6 Important, 4 Nits |
| C | 13 / 14 PASS | 0 Blockers, 0 Important, ~4 Nits (observations) |
| D | "Land with follow-ups" | 0 Blockers, 13 Important, 8 Nits |

**Aggregate (after dedup):** 1 Blocker, ~15 Important, ~18 Nits.
**Disposition:** Blocker must resolve before merge to `main`.
Important items land on the same branch (batched by theme). Nits may
land here or roll into SP-6 / SP-7 polish at author discretion.

---

## Module LOC census (Pass C)

| Module | LOC | Budget (§6) | Status |
|---|---:|---:|---|
| `semantics.rs` | 1370 | exempt | OK |
| `prover_relation_tests.rs` | 1204 | 800 | **over** (test-only) |
| `proof_artifacts.rs` | 698 | 800 | near |
| `prover.rs` | 469 | 800 | OK |
| `state_runtime.rs` | 434 | 800 | OK |
| `verifier.rs` | 353 | 800 | OK |
| others | < 350 | 800 | OK |

Also noted: 0 new `TODO` markers introduced, 1 new `#[allow]` (justified),
2 non-test `unwrap`/`expect` (both justified at call site).

---

## Blocker

### B-1 — Chip-row leak into `crates/runtime/src/**` (A-B-1)

**Spec violation.** §8.1 states: *"Post-SP-5, these identifiers [`InstructionRecord`,
`RelationTableWitnessRow`, …] do not appear under `crates/runtime/src/**`.
Guardrail test enforces this (§12)."* §12 requires
`crates/runtime/tests/no_chip_rows_in_runtime.rs`.

**Reality on branch `c41fdbe`:**
- Production code still imports and constructs chip row types:
  - `crates/runtime/src/proof_artifacts.rs:9,81,447`
  - `crates/runtime/src/prelude.rs:28,206-278`
- Test code uses them directly:
  - `crates/runtime/src/prover_relation_tests.rs:20,22,110,650,684,730,1099`
- The guardrail file `crates/runtime/tests/no_chip_rows_in_runtime.rs` **does not exist**.

**§18 Landed is false.** The Deviations section claims the invariant "held at T10 close without introducing `LogicalRelationTableRow`/`LogicalExecutionPrelude`" and that "the boundary is enforced by the guardrail test". Neither holds: the boundary is broken and no guardrail exists.

**Resolution (decided 2026-04-20):** **Option B-1a — implement §8 as originally specced.**

- Add `LogicalRelationTableRow` + `LogicalExecutionPrelude` in `tabula-stark::witness_kit`.
- Rewire `proof_artifacts.rs`, `prelude.rs`, and `prover_relation_tests.rs` through the logical types.
- Delete chip-type imports from `crates/runtime/src/**`.
- Land `crates/runtime/tests/no_chip_rows_in_runtime.rs` as a **passing** guardrail test (not `#[ignore]`).
- Correct §18 Landed to reflect the true post-F0 state.

**Why B-1a over B-1b (spec-amendment defer).** The considered alternative — rewriting §8.1 / §8.2 / §12 / §18 to defer to SP-5.5 — was rejected because: (1) the cost delta is ~1 day vs. ~1 hour, no proof-semantic or public-API churn; (2) accepting "no current consumer" as a reason to amend the spec is the exact logic that produced the false §18 in the first place, and normalizing that pattern weakens spec authority; (3) SP-6 and later work will accrete on top of this boundary — leakage hardens the longer it stays; (4) CLAUDE.md clean-break posture favors landing the correct structure directly over shims and deferrals.

---

## Important

Grouped by theme. Duplicates across passes are merged with cross-reference labels.

### I-E — Error taxonomy (6 findings)

| ID | Source | Detail |
|---|---|---|
| **I-E-1** | B-I-1 | `CommittedStateSnapshot::canonical_bytes` returns `RuntimeError`; should be `SetupError` (construction-time failure, not an execute error). |
| **I-E-2** | B-I-2 | `HostEnvironment` / `RuntimeRegistries` / `InstalledSchemes` methods return `RuntimeError`; should narrow to `SetupError`. Same motivation as I-E-1. |
| **I-E-3** | B-I-3 ≡ D-06 | `prove_and_verify` maps `VerifyError::StatementBuild` and `VerifyError::Validation` to `ProveError::WitnessGeneration`. These are post-prove verification errors, not witness-generation errors. Introduce `ProveError::PostVerify` (already exists for `VerifyError::Verification`) and route the other two variants there. |
| **I-E-4** | B-I-4 ≡ D-10 | `route_to_prove` / `route_to_verify` / `route_to_execute` stringify errors via `.to_string()`, erasing source chains (`detail: String` pattern occurs 110+ times across runtime). Keep the source error as a nested field (`#[source]`) and lean on `thiserror`'s display composition instead of flattening. |
| **I-E-5** | B-I-5 ≡ D-18 | `route_to_execute` uses `unreachable!` as its fallback; panics on any future widening of `RuntimeError`. Change to a validation-fallback that maps unknown variants to `ExecuteError::Validation { detail }`. Asymmetric with `route_to_prove` / `route_to_verify` (which stringify) — adopt the uniform pattern chosen in I-E-4 across all three. |
| **I-E-6** | D-10 (rollup) | After I-E-4/5 land, audit remaining `detail: String` sites and prefer structured `#[source]` chains. Not all 110+ will convert; the ones that can, should. |

### I-S — Public surface polish (5 findings)

| ID | Source | Detail |
|---|---|---|
| **I-S-1** | B-I-6 | `ProveInput` has `pub` fields **and** a `::new` constructor. Pick one: either all `pub` fields + `#[non_exhaustive]` (builder-free init), or private fields + `::new` + accessors. Mixed pattern invites future drift. |
| **I-S-2** | D-02 | `ProveResult` and `VerifiedResult` are near-duplicates (same private fields, same accessors). Collapse to one type with an optional `bound_statement: Option<BoundStatement>`, or introduce a shared `ProofOutcome` base. Current shape is code duplication that will rot. |
| **I-S-3** | D-04 | `PreparedOptions` builder methods (`with_host_environment`, `with_machine_stark_config`, `with_root_backend`, …) lack `#[must_use]`. Builders that return `Self` must be `#[must_use]` or callers silently drop the modification. |
| **I-S-4** | D-05 | Public handle types (`PreparedProver`, `PreparedVerifier`, `PreparedExecutor`, `PreparedOptions`, `VerifierState`, `ProveResult`, `VerifiedResult`) lack `Debug`. At minimum, derive a non-secret-leaking `Debug` or implement manually. Rust ecosystem expectation for any `pub` struct. |
| **I-S-5** | D-07 | `Arc::try_unwrap(x).unwrap_or_else(\|shared\| (*shared).clone())` is spelled `Arc::unwrap_or_clone(x)` in `std` since 1.76. Use it. Three occurrences (executor, prover, verifier prepare paths). |

### I-N — Naming consistency (4 findings)

| ID | Source | Detail |
|---|---|---|
| **I-N-1** | D-01 | Three handles have inconsistent field names for the same prepared-state concept: `PreparedExecutor { state }`, `PreparedVerifier { prepared }`, `PreparedProver { runtime_program, verifier_state, kit_registry, … }`. Converge on one name — recommend `state: PreparedRuntimeState` (executor) or `prepared: PreparedRuntimeState` (verifier); apply to all three. |
| **I-N-2** | D-12 | `PreparedRuntimeBuild::runtime_program: PreparedRuntimeState` is misnamed; the field is prepared state, not a program. Rename to `state` or `prepared`. |
| **I-N-3** | D-13 | `VerifierState` re-exported as top-level. Since `PreparedProver` also holds one, it is not "the verifier's" state; it is the prepared runtime's verify-side state. Rename to `PreparedVerifierState` for symmetry with `PreparedRuntimeState`. |
| **I-N-4** | D-08 | `crates/runtime/src/lib.rs:24` comment still mentions "engine". `engine.rs` was deleted in T10; the comment is stale. |

### I-M — Module structure + guardrails (4 findings)

| ID | Source | Detail |
|---|---|---|
| **I-M-1** | A-I-1 ≡ D-16 | `prover_relation_tests.rs` is 1204 LOC, over §6's 800 budget. Test-only so less critical, but splitting into {`witness_labels_tests`, `relation_trace_tests`, `byte_identity_tests`} matches the existing section seams and halves cognitive load. |
| **I-M-2** | A-I-2 | `crates/runtime/tests/prepared_handle_bounds.rs` required by §12 is **missing**. The `Send + Sync + 'static` invariant is asserted inline in each handle module via `const _: fn() = ...`, which satisfies the compile-time check, but §12 wants a named test file for CI greppability. Add it. |
| **I-M-3** | A-I-3 | `crates/runtime/tests/error_conversions.rs` exists but the "no `From` between narrowed enums" negative probe is doc-comment only, not enforced. §12 calls for a `trybuild` compile-fail probe. Add one. |
| **I-M-4** | D-14 | `proof_artifacts.rs` at 698 LOC is near §6's 800 budget. Not over — but given its responsibility (column extraction + recomposition + writers), a split into `{proof_artifacts/writers.rs, proof_artifacts/columns.rs, proof_artifacts/mod.rs}` would leave room for SP-6 growth. Defer until the file crosses 800 unless done opportunistically with I-M-1. |

**Dedup map:**
B-I-3 = D-06 = I-E-3 · B-I-4 = D-10 (partial) = I-E-4 · B-I-5 = D-18 = I-E-5 · A-I-1 = D-16 = I-M-1

---

## Nits

Nit-level findings were surfaced by each reviewer but detailed-text for
many was lost in post-review context compaction. Pass-C produced 0
Important items but flagged four observations worth triaging. Passes
A, B, D each surfaced 2 – 8 additional nits (A-N-1/2, B-N-1/2/3/4,
C-N-1/2/3/4, D-03, D-09, D-11, D-15, D-17, D-19, D-20, D-21).

**Disposition:** triage during the Important-fix sweep. For each
Important fix, the implementer should scan nearby code for the
nit-label and address opportunistically. A final nit audit before
merge (opus, 20-min pass over the post-fix diff) will catch anything
that survived.

Known nit themes from the summary:
- Spelling / doc-comment polish
- `#[inline]` opportunities on cold paths
- `cargo doc` warnings on private items the `missing_docs` lint doesn't catch
- Dead imports under partial feature flag combinations

---

## Fix Plan

**Structure:** 5 batches (F0 – F4), each one reviewable commit or small
commit series. Every batch runs the three standard gates: `cargo test
--workspace --all-features`, `cargo clippy --workspace --all-features
--all-targets -- -D warnings`, `scripts/sp5_byte_identity.sh` (content
equality).

### F0 — Blocker resolution (B-1): implement §8

**Decision:** Option B-1a (see Blocker section). Alternative B-1b was considered and rejected; rationale lives there.

**Work (likely 2 commits):**

*F0a — introduce logical types in `tabula-stark::witness_kit`.*
1. Design `LogicalRelationTableRow` as the runtime-facing mirror of `RelationTableWitnessRow`. Field set must be sufficient for `proof_artifacts.rs` consumption; cross-check against lines 447-... of the current file.
2. Design `LogicalExecutionPrelude` similarly, covering what `prelude.rs` needs from `InstructionRecord`.
3. Provide `From<LogicalRelationTableRow> for RelationTableWitnessRow` (and prelude analogue) inside `tabula-stark` so backend witness code keeps consuming chip rows while runtime holds logical rows. This is the layer seam — runtime never imports the chip types; the conversion lives in the backend.
4. Add rustdoc on both logical types naming their purpose and the layer invariant (§8 of SP-5 spec).
5. Commit.

*F0b — rewire runtime to consume logical types; land guardrail.*
1. `crates/runtime/src/proof_artifacts.rs` (lines 9, 81, 447): replace `tabula_chips::execution::trace::InstructionRecord` / `tabula_chips::relation_table::RelationTableWitnessRow` uses with the logical types. Insert conversions at the layer boundary (where runtime hands rows to the backend / receives them from witness generation).
2. `crates/runtime/src/prelude.rs` (lines 28, 206-278): same pattern.
3. `crates/runtime/src/prover_relation_tests.rs` (lines 20, 22, 110, 650, 684, 730, 1099): tests move to logical types. If an individual assertion fundamentally depends on chip-row internals (rare), move that test into `tabula-chips` and leave a brief note.
4. Add `crates/runtime/tests/no_chip_rows_in_runtime.rs`. Body: a runtime `#[test]` that `grep`s `crates/runtime/src/**/*.rs` for `tabula_chips::execution::trace::InstructionRecord` and `tabula_chips::relation_table::RelationTableWitnessRow` and asserts zero hits. Passes at F0b close-out.
5. Update `docs/superpowers/specs/2026-04-19-sp5-runtime-decomposition-design.md` §18 Landed: the §8.1 / §8.2 / §12 guardrail invariant now holds truthfully; remove the incorrect claim language and replace with the post-F0 factual state.
6. Commit.

**Agent:** opus (module-boundary design in F0a requires judgment on field selection and conversion seam placement; F0b is mostly mechanical but runs on the same branch).
**Gate:** all three standard gates; `no_chip_rows_in_runtime.rs` must pass; `scripts/sp5_byte_identity.sh` must remain green (no proof-byte change expected since this is internal rewiring).

### F1 — Error taxonomy cleanup (I-E-1 through I-E-6)

**Work (single commit or 2 commits):**
1. Widen `SetupError` to cover `CommittedStateSnapshot::canonical_bytes`, `HostEnvironment::*`, `RuntimeRegistries::*`, `InstalledSchemes::*` failures.
2. Add `ProveError::PostVerify` routes for `VerifyError::StatementBuild` + `VerifyError::Validation` (not only `VerifyError::Verification`).
3. Replace `detail: String` + `.to_string()` with `#[source]` chains in `route_to_prove` / `route_to_verify` / `route_to_execute`. Adopt uniform fallback (`Validation { detail }` or preserved `#[source]`).
4. `route_to_execute`: remove `unreachable!`; fall back to `ExecuteError::Validation`.

**Agent:** opus (error-taxonomy design decisions).
**Test impact:** existing error-path tests may need assertion updates. No proof bytes touched; byte-identity gate passes trivially.

### F2 — Public surface polish (I-S-1 through I-S-5)

**Pre-check (before dispatch):** the byte-identity gate in `scripts/sp5_byte_identity.sh` only checks `proof.bin` and `public_statement.json` contents. API-return-type serialization changes (e.g., collapsing `ProveResult` / `VerifiedResult`) would slip past the gate if those types are ever persisted elsewhere. Before collapsing:
1. `grep -rn 'serde::Serialize' crates/runtime/src/prover.rs crates/runtime/src/proof_summary.rs` to find any `#[derive(Serialize)]` on `ProveResult` / `VerifiedResult` / `ProofSummary` variants.
2. Audit SDK and CLI call sites for `.save`, `to_writer`, `to_bytes`, or any wire/persistence transformation that consumes those types by value (not just their `.proof()` / `.statement()` accessors).
3. If a persistence surface exists, either preserve the shape (keep both types, collapse only the impl) or add an explicit compat-gate check to the PR.
4. Record findings as a one-paragraph preamble to the F2 commit message.

**Work (single commit):**
1. Decide `ProveInput` shape (recommend: all `pub` + `#[non_exhaustive]`; drop `::new`).
2. Collapse `ProveResult` + `VerifiedResult` to one type with optional bound statement — only if the pre-check clears it; otherwise defer to SP-6 and note in commit message.
3. Add `#[must_use]` to all `PreparedOptions::with_*` builder methods.
4. Derive `Debug` on public handle types; verify no secret fields leak (none expected — prepared state is public information).
5. Replace `Arc::try_unwrap(x).unwrap_or_else(…)` with `Arc::unwrap_or_clone(x)` (3 sites).

**Agent:** sonnet (mechanical, well-scoped).
**Test impact:** if `ProveResult`/`VerifiedResult` collapse, SDK / CLI / tests update. Byte-identity gate passes for proof artifacts; return-type serialization covered by pre-check audit.

### F3 — Naming consistency (I-N-1 through I-N-4)

**Work (single commit):**
1. Unify `PreparedProver` / `PreparedVerifier` / `PreparedExecutor` state-field name. Apply pick across all three. Recommend `state`.
2. Rename `PreparedRuntimeBuild::runtime_program` → `state`.
3. Rename `VerifierState` → `PreparedVerifierState`; adjust re-exports and docs.
4. Delete stale "engine" reference at `crates/runtime/src/lib.rs:24`.

**Agent:** sonnet (rename sweep).
**Test impact:** call-site mechanical churn across SDK / CLI / tests. Byte-identity gate passes.

### F4 — Module structure + guardrail gaps (I-M-1 through I-M-4)

**Work (2 commits):**
1. **F4a — guardrails (I-M-2, I-M-3):** add `crates/runtime/tests/prepared_handle_bounds.rs` (`Send + Sync + 'static` asserts) and promote `error_conversions.rs`'s no-`From` probe to a `trybuild` compile-fail test.
2. **F4b — test split (I-M-1):** split `prover_relation_tests.rs` (1204 LOC) into `prover_relation_tests/{mod.rs, witness_labels.rs, relation_trace.rs, byte_identity.rs}` (or similar seams found at split time). I-M-4 (`proof_artifacts.rs` near budget) deferred unless opportunistic split emerges during F4b.

**Agent:** sonnet (mechanical).
**Test impact:** new test files execute; byte-identity unchanged.

### F5 — Final nit sweep + merge prep

**F5a — explicit nit-only re-dispatch (opus).**
Re-dispatch one opus agent against `/tmp/sp5-review-diff.patch` (or a fresh `git diff main..HEAD` snapshot if the patch has been cleaned up) with the prompt scoped strictly to nit-level findings:
- In scope: A-N-*, B-N-*, C-N-*, D-03, D-09, D-11, D-15, D-17, D-19, D-20, D-21, plus any nit labels whose detail was lost in compaction.
- Out of scope: anything already classified Blocker or Important in this doc.
- Output shape: structured findings text — one bullet per nit with file/line cite and one-line fix sketch.
- The opus call regenerates the nit detail that was lost to context compaction; do not rely on prior nit labels alone.

**F5b — mechanical nit landing (sonnet).**
Take the structured nit list from F5a. Sort into:
- **Land now:** mechanical fixes (typos, `#[inline]` on cold paths, dead imports under partial feature flags, stale `// engine` doc-comment leftovers, etc.).
- **Punt to SP-6 / SP-7:** anything that requires design judgment, cross-crate refactor, or doc-tree restructuring.
Commit the landed set as one final polish commit.

**F5c — merge prep.**
1. Run all three gates one last time: `cargo test --workspace --all-features`, `cargo clippy --workspace --all-features --all-targets -- -D warnings`, `scripts/sp5_byte_identity.sh`.
2. Confirm `rg -n 'tabula_chips::(execution::trace::InstructionRecord|relation_table::RelationTableWitnessRow)' crates/runtime/src` result matches the F0 disposition (zero hits under B-1a; unchanged under B-1b).
3. Open merge PR to `main`; copy the verdict table + blocker resolution summary into the PR description.

**Agent:** opus (F5a), sonnet (F5b), user (F5c).

---

## Agent Dispatch Order

| Order | Batch | Agent | Rationale |
|---|---|---|---|
| 1 | F0 | opus | Design judgment — scope/spec call (path A or B) |
| 2 | F1 | opus | Error-taxonomy design choices compound; one brain for consistency |
| 3 | F2 | sonnet | Surface polish is mechanical once shape is picked |
| 4 | F3 | sonnet | Rename sweep; low risk with compile-check gate |
| 5 | F4 | sonnet | Guardrail files + test split — formulaic |
| 6 | F5a | opus | Nit-only re-dispatch to regenerate lost nit detail |
| 7 | F5b | sonnet | Mechanical nit landing from F5a's structured list |
| 8 | F5c | user | Final gates + merge PR |

**Sequential, not parallel.** F1 / F2 / F3 / F4 all touch `crates/runtime/src/prover.rs`, `verifier.rs`, or `executor.rs`. Parallel dispatch would cause rebase conflicts and duplicate reviews of overlapping diffs. Strict gating: a batch starts only after the previous batch's three-gate run is green and its commits are on the branch.

**Gates between batches:** `cargo test --workspace --all-features`, `cargo clippy --workspace --all-features --all-targets -- -D warnings`, `scripts/sp5_byte_identity.sh`. Any regression blocks advancement.

---

## Out of Scope

The following appear in reviewer notes but are deferred to later sub-projects:

- **Partial feature-flag build (`--features verify` alone).** Pre-existing break (see `scripts/sp5_feature_matrix.sh` header). SP-7 owns.
- **SDK cache layer restructure.** SP-6 owns.
- **`docs/notes/*` stale-reference cleanup.** Low priority; roll into SP-6 docs polish.
- **`semantics.rs` 1370-LOC size.** Explicitly exempt per §6.
- **Byte-identity baseline line-order quirk** (`sort` needed for identity). §18 Landed acknowledges; fold into SP-6 tooling pass.
