# Stage 6: Proof Topology Generalization (Deferred)

> **Status**: Deferred on purpose
> **Date**: 2026-03-24
> **Scope**: Records what Stage 6 means, why it is deferred, what should trigger
> it, and what must stay true if it is ever implemented.
> **Related**:
> [runtime-machine-proof-backend-roadmap.md](runtime-machine-proof-backend-roadmap.md),
> [witness-partition-and-batch-proof-plan-architecture.md](witness-partition-and-batch-proof-plan-architecture.md),
> [proof-hierarchy-and-grouping.md](proof-hierarchy-and-grouping.md),
> [proof-front-end-journal-architecture.md](proof-front-end-journal-architecture.md),
> [../design/architecture.md](../design/architecture.md)

---

## 1. Why This Note Exists

Stages 1 through 5 fixed the architectural problems that were actually blocking
the proof stack:

- runtime now owns batch-local planning,
- machine now consumes fully prepared input instead of interpreting routing
  conventions,
- root authority is bundled coherently,
- execution-tier authoring seams live in `tabula-ext`,
- and the remaining runtime/witness/metadata contracts have been consolidated.

That means the main reason to generalize proof topology is no longer "the
current architecture is messy." The only valid reason is:

> **there is a real product or research need that the current `C+2` topology
> cannot serve cleanly.**

This note exists so that future work does not start Stage 6 prematurely or
forget the constraints that the earlier stages were designed to protect.

---

## 2. Current State

The current proof topology is intentionally concrete:

- one execution tier,
- one proof unit per machine-managed column slot,
- one root tier.

This is the current `C+2` model:

- `1` execution proof,
- `C` column proofs,
- `1` root proof.

This is no longer an accidental architecture. It is now a deliberate baseline
with clear ownership boundaries:

- `ProofPlan` is the static runtime-owned slot contract,
- `BatchProofPlan` is the batch-local planning layer,
- `PreparedMachineInput` is the machine-facing payload,
- machine owns trace/prove/verify mechanics,
- runtime owns planning and preparation policy.

That is important because Stage 6 must preserve those ownership boundaries even
if the topology itself changes.

---

## 3. What Stage 6 Actually Means

Stage 6 is not "make the code more abstract."

It means introducing a topology model that can represent proof packaging shapes
more general than `C+2`, for example:

- grouped column proofs (`G+2`, where `1 < G < C`),
- additional root-side aggregation stages,
- proof packaging that is aware of shard groups rather than single column
  slots,
- future recursive aggregation layouts that operate over grouped native proofs.

The key distinction is:

- logical specialization can stay column-local,
- while proof packaging may become group-based.

That is why Stage 6 is about **proof topology**, not about removing
column-local specialization.

---

## 4. What Stage 6 Is Not

Stage 6 should **not** do any of the following by accident:

- collapse `ProofPlan`, `BatchProofPlan`, and `PreparedMachineInput` into one
  mega-object,
- move planning authority back into machine,
- make witness responsible for whole-batch orchestration,
- reopen the root-authority design,
- add public SDK vocabulary for speculative grouping objects before the
  implementation is stable,
- generalize topology merely because generic code looks elegant.

If a proposed Stage 6 design needs any of those things, it is probably mixing
up topology generalization with boundary regression.

---

## 5. Why It Is Deferred

Stage 6 is deferred because the current topology is no longer the main source
of risk or unnecessary complexity.

The current code now has:

- a runtime-owned planning layer,
- explicit backend authority,
- stable machine input,
- and a clean enough contract stack to measure the real cost of `C+2`.

Without real evidence, topology generalization would mostly introduce:

- more abstract planning objects,
- broader testing matrices,
- more proof-shape branches,
- and more chances to blur authority boundaries that were just cleaned up.

So the correct current decision is:

> **do not start Stage 6 until there is a concrete need that cannot be handled
> acceptably within the current `C+2` design.**

This is not "unfinished work." It is the intended state.

---

## 6. Valid Triggers For Starting Stage 6

Stage 6 should start only if at least one of these becomes real:

### 6.1 Prover cost from repeated native proof overhead is material

Examples:

- too many repeated PCS commitments,
- too many repeated FRI sessions,
- too much fixed per-proof overhead relative to useful work,
- poor performance when many columns are touched in one batch.

### 6.2 Proof size or verifier cost is materially worse under `C+2`

Examples:

- proof size dominated by per-proof repetition,
- verifier cost scaling poorly with the number of column proofs,
- recursion pipeline needing to compress too many small native proofs.

### 6.3 Workload skew suggests grouping opportunities

Examples:

- a stable subset of columns often move together,
- touched-column ratios cluster into recurring families,
- some columns are consistently too small to justify standalone native proof
  overhead.

### 6.4 A roadmap feature explicitly needs more than `C+2`

Examples:

- grouped native proofs as the default packaging layer,
- extra root-side proof stages,
- recursive compression over grouped proofs,
- sharding-aware packaging required by product constraints.

If none of these are true, Stage 6 should remain deferred.

---

## 7. Evidence That Should Exist Before Starting

The codebase should not begin Stage 6 from intuition alone.

At minimum, the team should have:

1. benchmarks comparing `C+2` fixed costs against realistic workloads,
2. touched-column ratio distributions from representative batches,
3. data on proof-size and verifier-cost sensitivity to column count,
4. a concrete target topology to optimize for,
5. a statement of what Stage 6 should improve:
   - prover time,
   - proof size,
   - verifier time,
   - recursive aggregation cost,
   - or operational simplicity.

If the expected win cannot be stated clearly, Stage 6 is probably premature.

---

## 8. The Most Likely Next Topology

If Stage 6 does happen, the most plausible first step is **grouped column
proofs**, not full arbitrary generality.

That means moving from:

- execution,
- one proof per column,
- root,

to something like:

- execution group,
- `G` column groups,
- root group.

In other words:

- keep logical column specialization,
- introduce grouped proof packaging as the amortization layer,
- recurse only after grouping if needed.

This is aligned with the existing analysis in
[proof-hierarchy-and-grouping.md](proof-hierarchy-and-grouping.md).

It is much more likely to be the right first generalization than inventing a
fully free-form topology graph up front.

---

## 9. Likely Design Direction If Stage 6 Starts

The cleanest likely path is:

1. keep `ProofPlan` as the static slot-order seed,
2. extend `BatchProofPlan` so it can describe grouped proof units,
3. keep `PreparedMachineInput` as payload rather than planning state,
4. teach machine to consume grouped prepared units,
5. keep witness as kernels and runtime as planner.

That implies the first object likely to change is not `PreparedMachineInput`
directly, but `BatchProofPlan`.

Conceptually, the planning layer may evolve toward something like:

```rust
struct GroupPlan {
    group_id: GroupId,
    members: Vec<ColumnSlotKey>,
    fri_profile: FriProfileClass,
}

struct BatchProofPlan {
    execution_group: GroupPlan,
    column_groups: Vec<GroupPlan>,
    root_group: GroupPlan,
}
```

The exact types do not matter yet. The key architectural point is:

> **grouping belongs in runtime planning, not in machine-side implicit policy.**

---

## 10. Invariants Stage 6 Must Preserve

Even if topology changes, these rules should remain true:

### 10.1 Runtime still owns planning

Machine should not rediscover grouping or routing from raw stores.

### 10.2 `PreparedMachineInput` stays payload

Prepared input can become more general, but it still should not embed planning
rules that machine must reinterpret.

### 10.3 Witness remains a kernel layer

Witness may gain new kernels for grouped packaging, but it should not become a
batch orchestrator.

### 10.4 Root authority stays bundled

Grouped topology must not reopen the already-solved root authority problem.

### 10.5 Authoring seams stay in `tabula-ext`

Topology changes should not pull stable authoring contracts back into runtime
or machine.

### 10.6 Column-local specialization remains possible

Grouping is an amortization layer, not a reason to give up narrow traces or
column-specific optimization.

---

## 11. Likely Impacted Areas

If Stage 6 begins, these layers are likely to change:

- `docs/notes/proof-hierarchy-and-grouping.md`
- runtime proving:
  - `ProofPlan`
  - `BatchProofPlan`
  - `PreparedProofRequest`
  - proof artifact preparation
- machine:
  - topology representation
  - subproof packaging
  - proof model / verifier metadata
- testing:
  - runtime architecture guardrails,
  - machine proof-shape regressions,
  - grouped prove/verify integration coverage.

The important thing is that Stage 6 is not a machine-only refactor. It is a
cross-layer planning and proof-packaging change.

---

## 12. Questions Future Work Must Answer

Before implementation, Stage 6 should answer these explicitly:

1. What is the target topology:
   - grouped columns,
   - extra root stages,
   - recursive packaging,
   - or something else?
2. What is the grouping authority:
   - static from `ProofPlan`,
   - dynamic in `BatchProofPlan`,
   - or hybrid?
3. What optimization is being targeted:
   - prover time,
   - proof size,
   - verifier time,
   - recursion,
   - or memory pressure?
4. What workloads justify the change?
5. What invariants from Stages 1-5 must be protected?

If these cannot be answered, the work is not ready.

---

## 13. Recommended Re-entry Checklist

When the team revisits Stage 6, the first step should be to confirm:

- Stage 1-5 assumptions still hold,
- `C+2` is now the limiting factor rather than a tolerable baseline,
- grouped-proof or generalized-topology needs are real,
- the desired end state is narrower than "fully generic graph just in case."

If the answer is still uncertain, prefer:

- more measurement,
- more benchmark notes,
- or a smaller design note about grouped proofs,

instead of starting code changes.

---

## 14. One-Sentence Decision

For now, the correct decision is:

> **Stage 6 is intentionally deferred. The codebase should keep the current
> `C+2` topology until measured product or research needs justify grouped or
> more general proof packaging.**
