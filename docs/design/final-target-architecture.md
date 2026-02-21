# Tabula Ideal Target Architecture and Execution Plan

> Status: Proposed v2.0 (ideal target)
> Date: 2026-02-21
> Audience: maintainers across compiler/runtime/proof/platform
> Normative specs: [semantics-spec.md](../spec/semantics-spec.md), [proof-spec.md](../spec/proof-spec.md)
> Companion docs:
> - [compiler-research-architecture.md](./compiler-research-architecture.md)
> - [architecture.md](./architecture.md)
> - [m12-completion-gate.md](./m12-completion-gate.md)
> - [state-machine-centric-runtime-architecture.md](./state-machine-centric-runtime-architecture.md)

---

## 1. Purpose

This document defines the **ideal end-state architecture** that fully satisfies the proposed structure:

1. Contract-first protocol ownership.
2. Driver-centered static semantics.
3. Orchestrator-centered runtime workflows.
4. Canonical artifact as the only semantic interchange format.
5. Proof/runtime/CLI/daemon/web all sharing one semantic contract.

This is not a patch plan.
This is a full target blueprint with an executable migration plan.

---

## 2. Non-Negotiable Invariants

The target architecture is complete only if all invariants hold:

1. There is exactly one semantic authority for each rule class.
2. No adapter (`cli`, `daemon`, `web`) owns domain semantics.
3. Compiled artifact semantics are identical to executed and proved semantics.
4. Profile/hash mismatch is fail-closed at every entrypoint.
5. Statement and bus schemas are defined once and consumed everywhere.
6. Runtime and proof traces are contract-aligned by construction.

---

## 3. Current-State Gap Summary

Current refactor progress already achieved:

1. `tabula-driver` exists and is used by CLI/daemon.
2. `tabula-contract` exists with fail-closed compatibility checks.
3. proof-side M12 orchestration and bus tests were strengthened.

Critical remaining gaps to ideal state:

1. canonical artifact still does not enforce post-canonical semantic identity.
2. multi-IR (`HIR/MIR/LIR`) architecture is not yet in place.
3. `SemanticProfile` is not yet a first-class core object.
4. orchestrator layer is not yet the single runtime workflow owner.
5. proof receipt verification can still be context-weak unless expected context is supplied.

---

## 4. Ideal Architecture Overview

### 4.1 Planes

```text
Semantic Plane
  - language semantics
  - IR invariants
  - profile policy
  - contract schemas

Execution Plane
  - deterministic state transition
  - workflow orchestration
  - receipt/proof lifecycle

Adapter Plane
  - CLI transport
  - HTTP transport
  - Web UI transport
```

### 4.2 Single flow

```text
source/.json input
  -> Driver static pipeline
  -> Canonical Bundle (.tcb)
  -> Orchestrator execute/prove/verify/apply
  -> outputs (state_after, receipt/proof, reports)
```

No bypass path is allowed around this flow.

---

## 5. Final Workspace Topology

```text
crates/
  tabula-core/            # domain types, traits, semantic profile, diagnostics ids
  tabula-front/           # lexer/parser/source maps (syntax only)
  tabula-hir/             # resolved + principal-typed IR
  tabula-mir/             # effect IR + obligations + canonicalization
  tabula-lir/             # runtime executable low-level IR
  tabula-contract/        # statement schema, bus schema, compatibility policy
  tabula-artifact/        # canonical bundle (.tcb), ids, serialization
  tabula-driver/          # static pipeline (parse->hir->mir->lir->contract->artifact)
  tabula-executor/        # deterministic LIR execution
  tabula-commitment/      # field-level commitment/hashing backend
  tabula-proof/           # witness/proof/verify over contract + traces
  tabula-orchestrator/    # execute/prove/verify/apply workflows and job policy
  tabula-daemon/          # axum adapter
  tabula-cli/             # CLI adapter
  tabula-web-ide/         # browser adapter
```

Transitional allowance:
- Until split completes, `tabula-ir` may host HIR/MIR/LIR submodules under strict feature-gated boundaries.

---

## 6. Dependency and Ownership Rules

### 6.1 Dependency direction

```text
core -> front/hir/mir/lir/contract/artifact
front -> hir
hir -> mir
mir -> lir
contract + lir + mir -> driver
driver + executor + proof + artifact + contract -> orchestrator
orchestrator -> daemon/cli
web-ide -> daemon api only
```

### 6.2 Ownership matrix

| Concern | Owner crate | Not allowed elsewhere |
|---|---|---|
| Syntax parsing | `tabula-front` | `driver`, adapters |
| Type/operator legality | `tabula-hir` + `tabula-core` | executor, adapters |
| NF/obligations/canonicalization | `tabula-mir` | driver custom hacks |
| Executable instructions | `tabula-lir` | adapters |
| Contract schemas/bus tuples | `tabula-contract` | proof-local duplicates |
| Artifact format/hash ids | `tabula-artifact` | ad hoc JSON hashing |
| Static pipeline orchestration | `tabula-driver` | CLI/daemon custom flow |
| Dynamic workflow orchestration | `tabula-orchestrator` | daemon handlers |

---

## 7. Canonical Semantic Model

### 7.1 `SemanticProfile` (core)

`SemanticProfile` is mandatory and hashed into artifacts.

It includes:

1. language/version vectors.
2. operator algebra policy.
3. nullability policy.
4. hash/codec policy.
5. alias/obligation policy.
6. contract schema version and statement binding version.

Every runtime/proof action must check profile hash equality.

### 7.2 Operator legality table

One operator signature table in core is reused by HIR, MIR, LIR lowering, and runtime assertions.

Minimum policy:

1. `Add/Sub/Mul/Div/Mod`: only numeric domains (`U64`, `I64`) with same-type operands.
2. `Eq/Ne`: all first-class values, same-type requirement.
3. ordered compares: only ordered numeric domains.
4. boolean logic: `Bool` only.

### 7.3 Obligation semantics

Unproven static constraints become tracked obligations, never hidden rewrites.

Obligation lifecycle:

1. generated in MIR.
2. discharged statically or materialized as runtime guard.
3. serialized into canonical MIR metadata.
4. included in semantic hash.

---

## 8. IR Tower (Ideal)

### 8.1 S-AST

Lossless syntax tree with spans/comments.
No semantic defaults.

### 8.2 HIR

Resolved symbols, principal typing, explicit nullable forms.

### 8.3 MIR

Effect graph + SSA values + obligations.
No execution layout decisions yet.

### 8.4 Canonical MIR (C-MIR)

Deterministic order/ids/obligations.
This is the semantic hash boundary.
No semantic mutation allowed after this stage.

### 8.5 LIR

Execution-ready instruction stream with explicit guards/checks and full typing.

### 8.6 Contract IR (K-IR)

Statement fields, bus tuple schemas, public value layout, binding rules.
Consumed by runtime/proof and stored in artifact.

---

## 9. Canonical Artifact Model

### 9.1 Artifact: `.tcb` (Tabula Canonical Bundle)

Sections:

1. header (magic/version/profile hash/semantic hash).
2. semantic profile.
3. canonical schema set.
4. C-MIR.
5. LIR.
6. Contract IR.
7. optional debug/source maps.

### 9.2 Identity

`semantic_hash = H(canonical(schema_set) || canonical(c_mir) || canonical(contract_ir) || canonical(profile))`

### 9.3 Compatibility

Hard-fail on:

1. bundle major version mismatch.
2. profile hash mismatch.
3. contract schema/version mismatch.
4. semantic hash recheck mismatch.

---

## 10. Runtime and Proof Workflow Model

### 10.1 Orchestrator use-cases

`tabula-orchestrator` owns:

1. `check`
2. `compile`
3. `execute`
4. `prove`
5. `verify`
6. `apply`

### 10.2 Execute path

1. resolve input refs (inline/file/artifact).
2. ensure canonical artifact availability.
3. execute LIR deterministically.
4. compute typed consistency report.
5. emit E-Trace aligned to Contract IR.

### 10.3 Prove path

1. requires canonical artifact + execution trace.
2. builds witness using contract binding rules.
3. runs selected proof backend (`receipt` or `stark`).
4. emits proof object with profile and contract metadata.

### 10.4 Verify path

1. validates proof/receipt format and version.
2. validates profile/contract hash match.
3. verifies proof statement binding completeness.
4. if expected context provided, must match statement exactly.
5. policy can require expected context for production mode.

---

## 11. Adapter Architecture

### 11.1 `tabula-daemon`

Responsibilities:

1. auth/cors/routing.
2. request/response codec.
3. orchestrator invocation.

Forbidden:

1. compiling logic.
2. schema validation logic.
3. proof semantics logic.

### 11.2 `tabula-cli`

Responsibilities:

1. command-line UX.
2. local orchestrator mode and remote daemon mode.
3. output formatting and exit code mapping.

### 11.3 `tabula-web-ide`

Responsibilities:

1. UI state and user workflows.
2. daemon API client only.

Forbidden:

1. local fallback proof semantics.
2. client-side semantic reinterpretation.

---

## 12. Operational and Security Policies

### 12.1 Fail-closed policy

Fail immediately on:

1. unknown schema/version.
2. profile mismatch.
3. missing statement binding classification.
4. artifact hash inconsistency.

### 12.2 Determinism policy

1. sorted collections for canonical serialization.
2. deterministic pass order and stable id allocation.
3. deterministic output ordering in read/write/trace artifacts.

### 12.3 Trust boundaries

1. adapters are untrusted transport boundaries.
2. artifact loader is validation boundary.
3. proof verifier is cryptographic boundary.

---

## 13. Quality Gates (Definition of Valid Change)

A change is mergeable only if all gates pass:

1. lint/format/doc gates.
2. unit/property/metamorphic tests.
3. compile->execute consistency regression corpus.
4. compile->execute->prove->verify e2e tests.
5. contract schema snapshot + compatibility tests.
6. artifact determinism tests (repeat runs, reorder stress).

---

## 14. Execution Plan (Program-Level)

This plan assumes full convergence to ideal architecture while keeping production usable.

### 14.1 Program structure

Workstreams:

1. W1 Semantic Core
2. W2 IR Stack
3. W3 Artifact and Driver
4. W4 Orchestrator and Runtime
5. W5 Proof and Contract Alignment
6. W6 Adapter Convergence
7. W7 Hardening and Rollout

Each phase below has explicit entry/exit criteria.

---

## 15. Phase Plan

### Phase 0: Baseline Freeze and Regression Corpus

Objective:
- freeze current behavior and known failure classes.

Changes:
1. capture regression fixtures for R1-R7 failure classes.
2. pin existing command outputs/snapshots.

Exit criteria:
1. corpus exists and is automated in CI.
2. baseline reproducibility documented.

Rollback:
- none needed (non-invasive).

### Phase 1: Semantic Profile Foundation (W1)

Objective:
- make profile/hash explicit and mandatory.

Changes:
1. add `SemanticProfile` to `tabula-core`.
2. add profile hash plumbing in artifact metadata.
3. hard-fail profile mismatch in execute/prove/verify.

Exit criteria:
1. profile mismatch tests pass.
2. no path can run without resolved profile.

Rollback:
- feature gate profile enforcement by environment in emergency mode only.

### Phase 2: Canonical Artifact Hard Boundary (W3)

Objective:
- make artifact semantics identical to runtime semantics.

Changes:
1. introduce `.tcb` in `tabula-artifact`.
2. driver compile emits post-canonical body only.
3. semantic hash validation on load.

Exit criteria:
1. no compile/check/execute semantic drift repro remains.
2. old JSON path wrapped by compatibility adapter.

Rollback:
- keep compatibility reader for one prior version.

### Phase 3: IR Stack Split (W2)

Objective:
- establish HIR/MIR/LIR responsibilities.

Changes:
1. introduce HIR with principal typing.
2. introduce MIR with effects + obligations.
3. isolate LIR lowering.
4. remove lowering defaults in `tabula-lang`.

Exit criteria:
1. operator unsoundness class rejected before LIR.
2. pass invariants enforced in driver.

Rollback:
- temporary dual compiler path behind feature flag until parity achieved.

### Phase 4: Obligation Engine and Canonical MIR (W1+W2)

Objective:
- replace hidden semantic rewrites with obligation lifecycle.

Changes:
1. add obligation data model.
2. add static discharge pass.
3. add runtime materialization pass.
4. include discharge records in semantic hash.

Exit criteria:
1. every obligation has discharge state.
2. NF/alias policy is fully explicit in artifact.

Rollback:
- fallback to previous canonicalization only in non-production branch.

### Phase 5: Contract IR Consolidation (W5)

Objective:
- unify runtime and proof schemas.

Changes:
1. expand `tabula-contract` to full K-IR ownership.
2. runtime emits contract-aligned E-Trace.
3. proof consumes K-IR instead of local tuple duplication.

Exit criteria:
1. statement field binding completeness passes.
2. bus schema drift tests fail when mismatched.

Rollback:
- compatibility adapter may map old trace shape to K-IR temporarily.

### Phase 6: Orchestrator Centralization (W4)

Objective:
- move all dynamic workflows behind orchestrator.

Changes:
1. add `tabula-orchestrator` use-case APIs.
2. migrate daemon engine and CLI execution logic.
3. centralize queue/timeouts/concurrency/cancellation policy.

Exit criteria:
1. daemon and CLI both call orchestrator only.
2. no duplicated runtime workflow logic remains.

Rollback:
- keep old engine path behind short-lived fallback toggle.

### Phase 7: Adapter Convergence and Strict Mode (W6)

Objective:
- make adapters thin and deterministic.

Changes:
1. remove semantic logic from daemon handlers and CLI commands.
2. remove web fallback local-proof semantics.
3. enforce strict expected-context verify policy in production mode.

Exit criteria:
1. adapter crates only transport/format logic.
2. security review sign-off on verify behavior.

Rollback:
- adapter rollback possible without touching semantic core.

### Phase 8: Hardening and Cutover (W7)

Objective:
- complete rollout and deprecate legacy paths.

Changes:
1. remove deprecated artifact and legacy compile path.
2. enable strict gates as default.
3. publish migration tools and docs.

Exit criteria:
1. all production paths run on ideal architecture only.
2. long-run determinism/latency dashboards stable.

Rollback:
- release-level rollback to previous stable tag, with artifact compatibility window.

---

## 16. Milestones and Suggested Timeline

This is a suggested execution cadence for one focused team.

| Milestone | Scope | Suggested duration |
|---|---|---|
| M0 | Phase 0 complete | 1 week |
| M1 | Phases 1-2 complete | 2-3 weeks |
| M2 | Phases 3-4 complete | 3-4 weeks |
| M3 | Phase 5 complete | 2 weeks |
| M4 | Phases 6-7 complete | 2-3 weeks |
| M5 | Phase 8 + rollout | 1-2 weeks |

Total: ~11-15 weeks depending on team size and parallelization.

---

## 17. Execution Backlog by Workstream

### W1 Semantic Core

1. `SemanticProfile` type and hash pipeline.
2. operator legality table extraction.
3. diagnostic code taxonomy standardization.

### W2 IR Stack

1. HIR and MIR module creation.
2. pass contract framework (`requires/ensures`).
3. obligation model and discharge passes.

### W3 Artifact and Driver

1. `.tcb` serializer/deserializer.
2. semantic hash verifier.
3. compatibility reader for legacy JSON.

### W4 Orchestrator and Runtime

1. orchestrator command API.
2. runtime execute/prove/verify/apply use-cases.
3. job queue and timeout policies.

### W5 Proof and Contract Alignment

1. K-IR schema expansion.
2. trace binding adapters removal.
3. proof input boundary cleanup.

### W6 Adapter Convergence

1. daemon handler thinness refactor.
2. CLI in-proc/remote mode unification.
3. web client API contract stabilization.

### W7 Hardening and Rollout

1. stress/perf tests.
2. security review and failure-injection tests.
3. release playbook and migration docs.

---

## 18. Risk Register and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Large refactor integration conflicts | High | feature-gated phased landing, strict owner boundaries |
| Legacy artifact compatibility break | High | compatibility reader + migration command |
| Proof/runtime schema drift during transition | High | Contract IR as mandatory source and snapshot tests |
| Adapter code reintroduces domain logic | Medium | architecture lint/review checklist |
| Performance regressions in new pipeline | Medium | benchmark gates from Phase 2 onward |

---

## 19. Rollback and Recovery Strategy

1. Every phase is shipped behind a guardable capability flag until its gate passes.
2. Bundle format changes are versioned; old reader remains for one supported window.
3. Release rollback points are tagged at end of each milestone.
4. No destructive data migration without reversible transform.

---

## 20. Governance Model

### 20.1 Architecture review gate

Required approvers before phase completion:

1. compiler owner
2. runtime owner
3. proof owner
4. platform/adapter owner

### 20.2 Required review artifacts per phase

1. ADR delta summary.
2. compatibility impact statement.
3. test evidence report.
4. rollback readiness checklist.

---

## 21. Definition of Done for Ideal Architecture

Architecture is complete only when all statements are true:

1. canonical `.tcb` is default and authoritative across all surfaces.
2. profile/contract/hash checks are fail-closed globally.
3. adapters contain no semantic domain logic.
4. runtime and proof consume shared Contract IR.
5. compile/check/execute/prove/verify all flow through Driver + Orchestrator layers.
6. historical drift classes (R1-R7) are covered by permanent regression tests.

---

## 22. Immediate Next Actions (Next 2 Weeks)

1. Fix compile artifact drift by serializing post-registration canonical tx bodies.
2. Add strict verify mode requiring expected context in daemon and web integration.
3. Define `SemanticProfile` in `tabula-core` and wire profile hash into execute/prove/verify reports.
4. Draft `tabula-artifact` crate skeleton and `.tcb` header format.
5. Create `tabula-orchestrator` crate with one migrated use-case (`execute`) as pilot.

These five actions materially reduce risk and establish the ideal architecture runway.
