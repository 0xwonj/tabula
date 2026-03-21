# Proof Hierarchy and Grouping Strategy

> **Status**: Design note
> **Date**: 2026-03-20
> **Scope**: Clarifies the relationship between whole-batch proofs, grouped proofs, and logical column sharding.
> **Related**: [proving-layer-architecture.md](proving-layer-architecture.md), [full-sharding-research.md](full-sharding-research.md), [sharded-protocol-design.md](sharded-protocol-design.md), [compiler-optimization-research.md](../research/compiler-optimization-research.md), [symbolic-air-compilation.md](../research/symbolic-air-compilation.md)

---

## 1. Why This Note Exists

Discussions around "column sharding" can mix together three different ideas:

1. **Logical column-local specialization**: narrow traces, type-aware encoding, touched-column routing, and per-column witness generation.
2. **Proof artifact decomposition**: how many PCS commitments / opening proofs / FRI instances are emitted.
3. **Whole-batch statement structure**: what ties execution, state transitions, and root updates into one sound statement.

These are related, but they are not the same lever.

The key architectural claim of this note is:

> **Tabula should preserve column-local specialization even when multiple logical column shards are packaged into one grouped proof with shared PCS and single FRI.**

In other words:

- **logical sharding** is where most proof-aware specialization lives,
- **grouping** is where fixed protocol costs are amortized,
- **whole-batch orchestration** is where global soundness is enforced.

---

## 2. The Three Logical Layers

Tabula is easiest to reason about as a three-layer proof hierarchy.

```text
Whole-batch proof / statement layer
├── execution tier
├── column proof groups
│   ├── logical column shard (t1, c1)
│   ├── logical column shard (t1, c2)
│   └── logical column shard (t2, c7)
└── root / directory tier
```

### 2.1 Whole-Batch Layer

This is the layer that proves the full `oldRoot -> newRoot` statement.

Responsibilities:

- bind the public statement,
- include the execution tier,
- include the root / directory tier,
- enforce cross-group bus balance,
- define what must be recursively aggregated or externally wrapped.

This layer answers: "What is the proof of the batch?"

### 2.2 Group Layer

This is the packaging layer for one or more logical column shards.

Responsibilities:

- define one `ProofInstance`,
- share PCS commitments across multiple matrices,
- share one opening proof / FRI instance across those matrices,
- amortize transcript and verifier overhead across the group.

This layer answers: "How many proof artifacts are we emitting?"

### 2.3 Column-Local Layer

This is the specialization layer.

Responsibilities:

- choose the column commitment scheme family,
- choose the chip set for that column,
- choose the value encoding width,
- avoid unused lanes and dummy columns,
- skip untouched columns entirely,
- preserve each column's natural trace height and degree profile.

This layer answers: "What does this column actually need to prove?"

---

## 3. Logical Shards vs Proof Groups

The most important distinction is:

- a **logical column shard** is a witness/trace specialization unit,
- a **proof group** is a PCS/FRI amortization unit.

These do **not** need to be identical.

### 3.1 What Must Stay Column-Local

The following optimizations belong to the logical shard, not to the proof artifact:

1. Width specialization
2. Type-specific encoding
3. Scheme selection (`ssmc`, `smt`, or future variants)
4. Touched / untouched routing
5. Per-column chip enablement
6. Per-column trace height and domain choice

These optimizations remain valid even if several logical shards are committed and opened together inside one grouped proof.

### 3.2 What Belongs to the Group

The following costs are paid at the proof-group level:

1. Fiat-Shamir transcript rounds
2. Quotient commitment packaging
3. FRI opening proof
4. Verifier work for that opening proof
5. Proof-size overhead from an additional proof artifact

Grouping exists mainly to amortize these costs.

---

## 4. Grouping Is Not Reverting to a Monolithic Wide Trace

Grouping multiple columns must **not** be interpreted as "collapse all columns into one universal fixed-width chip."

That would throw away the main benefits of Tabula's column-aware architecture.

Instead, the right mental model is:

1. Build each logical shard with its own narrow trace and its own natural domain.
2. Keep those traces as distinct matrices.
3. Commit to those matrices together in one `ProofInstance`.
4. Open them together with one grouped opening proof.

This distinction matters.

### 4.1 What Grouping Preserves

If grouping is done at the PCS layer rather than by physically merging traces, the following stay intact:

- narrow per-column trace width,
- type-aware column encoding,
- natural per-column heights,
- per-column chip composition,
- untouched-column skipping,
- future per-column caching opportunities.

### 4.2 What Grouping Does Not Preserve

Some knobs move from per-column to per-group:

- FRI parameter set,
- query count,
- challenge schedule,
- proof artifact independence.

This is the main reason grouping must be **homogeneous** rather than arbitrary.

---

## 5. The Design Spectrum

There is not just one sharding architecture. There is a spectrum.

| Architecture | Proof count | Parallelism | Fixed-cost overhead | Proof size / verifier | Column-local specialization |
|-------------|-------------|-------------|---------------------|-----------------------|-----------------------------|
| **Monolithic** | 1 | Low | Best | Best | Weakest |
| **Fully sharded** | `C+2` | Highest | Worst | Worst | Strongest |
| **Grouped** | `G+2`, where `1 < G < C` | High | Moderate | Moderate | Nearly as strong as full sharding |

The grouped design is the natural middle point:

- preserve most of the specialization benefit of full sharding,
- avoid paying a full opening-proof / verifier overhead for every column,
- leave recursion and wrapping to solve a smaller outer compression problem.

This is likely the most practical target state for Tabula.

---

## 6. What Grouping Actually Buys

Grouping multiple logical shards into one proof group can reduce:

1. Number of opening proofs
2. Number of verifier-side FRI checks
3. Number of transcript forks / challenge schedules
4. Repetition of commitment and quotient packaging overhead
5. Final proof size before recursion or wrapping

The group layer is therefore the main lever for reducing the downside of `C+2`.

Another way to say it:

- **column-local sharding** reduces wasted computation,
- **grouping** reduces repeated protocol overhead.

Both are needed.

---

## 7. What Grouping Can Accidentally Break

Grouping is not free. Bad grouping can erase the benefits of sharding.

### 7.1 Mixed-Degree Groups

If a low-degree, simple column is grouped with a high-degree, complex column, the simple column inherits the heavier FRI profile of the group.

This is the most direct way to lose the "small proof" benefit.

### 7.2 Mixed-Height Groups

If a tiny column is grouped with a very large column in a way that forces shared height assumptions in the surrounding machinery, the tiny column can lose its padding advantage.

The implementation must therefore preserve matrix-local domains and avoid silently reintroducing one global padded trace.

### 7.3 Mixed-Scheme Groups

Columns with very different commitment / witness structure can be hard to tune well together.

For example:

- tiny read-mostly SSMC columns,
- large sparse SMT-backed columns,
- heavy property-read columns,
- recursion-specialized columns.

These should not automatically share one profile.

---

## 8. What Still Stays Valid Inside a Group

The main concern behind grouping is usually:

> "If I stop making one proof per column, do I lose the column-level optimizations?"

The answer is:

> **Mostly no.**

The important column-local optimizations still survive if the group is only a packaging boundary.

### 8.1 Survives Grouping

These remain column-local:

1. Exact width selection
2. Type-specific value layout
3. Column-specific chip wiring
4. Column-specific trace height
5. Touched / untouched filtering
6. Per-column witness preparation

### 8.2 Does Not Fully Survive Grouping

These become group-level:

1. FRI parameters
2. Query schedule
3. Opening-proof count
4. Verifier-side batching unit

So the right rule is:

> **Group only columns whose proof profile is already close.**

---

## 9. Grouping Strategy

The ideal grouping strategy is not "one proof per column forever" and not "one proof for everything."

It is:

1. shard logically by column,
2. group physically by proof profile,
3. aggregate recursively only after grouping.

### 9.1 Good Grouping Keys

A group should usually be homogeneous across:

1. scheme family,
2. width / encoding class,
3. degree class,
4. expected FRI profile,
5. rough height bucket,
6. access density / hotness.

In practice:

- tiny columns should be packed,
- heavy or unusual columns should often be isolated,
- untouched columns should be skipped rather than grouped into empty proofs.

### 9.2 Bad Grouping Keys

Grouping by table ownership alone, or by arbitrary schema order, is unlikely to be optimal.

The grouping key should reflect proof cost, not only application semantics.

---

## 10. Compiler vs Runtime Responsibilities

The best design is not "the compiler decides everything" and not "the prover decides everything from scratch."

The clean split is:

### 10.1 Compiler Responsibilities

The compiler should emit a **proof-planning seed**:

- scheme family,
- width / encoding class,
- degree class,
- grouping eligibility,
- predicted height or resource budget,
- static disqualifiers for co-grouping.

This is the static information the compiler already understands well.

### 10.2 Runtime Responsibilities

The runtime should finalize the concrete plan using actual batch data:

- touched / untouched status,
- actual row counts,
- actual access density,
- actual height buckets,
- final group assignment,
- final FRI profile for each group.

This is the dynamic information only execution can know precisely.

### 10.3 Recommended Plan Objects

Conceptually, the planning hierarchy should look like:

```rust
struct ColumnProofProfile {
    scheme_family: SchemeFamily,
    width_class: WidthClass,
    degree_class: DegreeClass,
    fri_class: FriProfileClass,
    grouping_class: GroupingClass,
}

struct GroupPlan {
    columns: Vec<ColumnId>,
    fri_profile: FriProfile,
}

struct BatchProofPlan {
    execution_group: GroupPlan,
    column_groups: Vec<GroupPlan>,
    root_group: GroupPlan,
}
```

The exact types can differ, but the separation of concerns should remain.

---

## 11. Relationship to Recursion and Wrapping

This layering also clarifies where recursion belongs.

Recursion should be viewed as an **outer compression layer**, not as the primary mechanism for fixing an overly fragmented inner proof design.

Recommended order:

1. get strong column-local specialization,
2. group logical shards to amortize native proof overhead,
3. recurse over grouped proofs if needed,
4. apply Groth16 wrapping or similar final compression for on-chain verification.

This avoids the worst case of recursing over a very large number of tiny proofs with avoidable fixed costs.

---

## 12. Research Implications

This note suggests that the most important empirical question is not:

> "Monolithic or fully sharded?"

It is:

> "What grouped architecture best preserves column-local specialization while minimizing repeated PCS/FRI overhead?"

That leads directly to the benchmark matrix Tabula should care about:

1. Monolithic (`1`)
2. Fully sharded (`C+2`)
3. Grouped (`G+2`)

Measured over:

1. prover wall-clock time,
2. total CPU work,
3. proof size,
4. verifier time,
5. recursion overhead,
6. sensitivity to skewed column heights,
7. sensitivity to touched-column ratio.

---

## 13. Decision Summary

The intended design direction should be:

1. **Keep logical sharding at the column level.**
2. **Introduce grouped proofs as the main amortization unit.**
3. **Treat shared PCS as a packaging mechanism, not as a reason to give up narrow traces.**
4. **Select groups by proof profile, not by arbitrary schema structure.**
5. **Use compiler-provided proof-plan seeds and runtime refinement together.**
6. **Apply recursion after grouping, not instead of grouping.**

In short:

> **Column-local specialization is the optimization layer. Grouping is the amortization layer. Whole-batch orchestration is the soundness layer.**

