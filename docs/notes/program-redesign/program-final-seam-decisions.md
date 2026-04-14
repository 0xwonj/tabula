# Program Final Seam Decisions

> **Status**: Finalized architecture decisions
> **Date**: 2026-03-24
> **Scope**: Records the final decisions for the four remaining design seams
> before the language/compiler/IR rewrite moves into exact Rust data models and
> implementation planning.
> **Related**: [program-dsl-and-ir-redesign.md](program-dsl-and-ir-redesign.md),
> [program-dsl-grammar-sketch.md](program-dsl-grammar-sketch.md),
> [program-typing-and-effect-system.md](program-typing-and-effect-system.md),
> [program-hir-design.md](program-hir-design.md),
> [program-mir-design.md](program-mir-design.md),
> [program-canonical-ir-design.md](program-canonical-ir-design.md),
> [program-rewrite-roadmap.md](program-rewrite-roadmap.md),
> [verification vocabulary](../../design/architecture.md#verification-vocabulary)

---

## 1. Why This Note Exists

The redesign has now reached the point where the remaining architectural seams
should stop being treated as open discussion topics.

The following areas were the last meaningful seams before implementation:

- hash taxonomy
- context visibility and binding policy
- event/query proof-boundary policy
- guarded operation frontier

This note finalizes those decisions.

The goal is to ensure that:

- HIR, MIR, and canonical IR data models can now be defined against fixed
  policies,
- the executor and proof frontend can be migrated without hidden assumptions,
- and future implementation planning no longer has to relitigate these points.

---

## 2. Decision Summary

The final decisions are:

1. **Hash is hybrid**
   - blessed ubiquitous hash families remain builtin canonical ops
   - non-blessed or custom kernels remain capability calls
2. **Context is initially public-only**
   - `context` remains a separate scope from tx arguments
   - initial implementation binds all context fields as statement-visible public
     instance inputs
   - private context remains a reserved future extension
3. **Event and query policies are conservative but first-class**
   - events remain typed canonical effects and full journal entries
   - the initial proof statement binds only the ordered event digest
   - queries remain distinct canonical entry kinds
   - query proving remains a future separate mode rather than being folded into
     ordinary tx proof statements
4. **Guards are introduced, but only for effectful or checked ops**
   - total pure value ops remain unguarded
   - value merging remains `Select`-based
   - guarded lowering applies only where speculative execution would be unsound
     or semantically wrong

These decisions are now the architecture baseline.

---

## 3. Decision 1: Hash Taxonomy

### 3.1 Final decision

Tabula adopts a **hybrid hash model**.

The canonical IR may contain a builtin `Hash` operation for a small blessed set
of ubiquitous, total, deterministic hash families.

All other hash-like or operational kernels continue to lower as:

- `CallCapability`

### 3.2 Meaning of builtin hash

Builtin `Hash` should be treated as:

- total
- deterministic
- pure in the canonical value-op sense
- not guarded in the initial canonical model

### 3.3 Meaning of capability hash

Hash-like operations that do not belong to the blessed builtin family remain:

- capability calls
- governed by capability descriptor metadata
- and subject to query-safety / checkedness / proof-observability policy

### 3.4 Why this is the right compromise

If all hashes are modeled as capabilities:

- the effect system becomes heavier than needed,
- query legality becomes noisier,
- executor and canonical IR lose a useful builtin fast path,
- and extremely common operations are forced through a more general and more
  expensive classification path.

If too many hashes become builtin:

- the canonical IR becomes special-case heavy,
- and the blessed set becomes unstable.

The hybrid decision gets the best of both:

- small canonical builtin support where it matters,
- capability generality everywhere else.

### 3.5 Final policy

- builtin `Hash` is a **blessed family**, not an open-ended escape hatch
- non-blessed/custom hash-like operations use `CallCapability`
- the initial builtin hash family should remain very small

---

## 4. Decision 2: Context Visibility and Binding

### 4.1 Final decision

`context` remains a distinct language concept from tx arguments, and the initial
implementation uses:

- **public-only context**

All initial context fields are:

- instance-global or batch-global
- statement-bound
- verifier-visible

### 4.2 Why `context` remains distinct from tx args

The distinction is about **scope**, not primarily visibility.

- tx arguments are per-entry-call inputs
- context values are per-instance or per-batch inputs shared across execution
  within that instance

This distinction is architecturally important even before private context exists.

### 4.3 Why public-only first

Private global witness-like context would add complexity immediately to:

- runtime API design
- statement model
- replay/debug semantics
- testing and differential validation

The architecture should reserve private context for later, but the initial
implementation should not open that dimension.

### 4.4 Final policy

- `ContextSchema` remains part of the canonical program model
- the grammar need not expose visibility syntax initially
- all initial context fields are public and statement-bound
- private context remains a reserved future extension only

---

## 5. Decision 3: Event and Query Proof Boundary

## 5.1 Events

### Final decision

Events remain:

- typed language constructs
- canonical IR effects
- full execution-journal entries

But the initial proof statement binds only:

- the **ordered event digest**

### Why this is the right initial policy

If events are ignored by the proof boundary entirely:

- the program's explicit output surface becomes weakly connected to proving.

If full typed event payloads are immediately embedded into every proof
statement:

- statements become heavier than needed,
- and the system commits too early to a richer public-output format.

Digest-only binding is the correct conservative midpoint:

- executor and runtime keep full typed events
- proof statements bind integrity of the event stream
- richer public event exposure remains possible later

### Final policy

- `EmitEvent` stays in canonical IR
- execution journals carry full typed events
- the initial verifier-visible binding is the ordered event digest only

## 5.2 Queries

### Final decision

Queries remain:

- distinct canonical entry kinds
- external read-only surfaces
- result-bearing program interfaces

But the initial proof architecture does **not** fold query results into the
ordinary tx proof statement.

Instead:

- runtime query execution is supported
- and query proving remains a future separate mode, such as `prove_query`

### Why this is the right initial policy

If query is treated as just an internal helper:

- the language loses a valuable external semantic read surface.

If query proof is forced into the initial tx-proof statement:

- the statement model becomes unnecessarily complex before the base rewrite is
  stable.

Keeping query as a first-class entry kind while deferring query-proof as a
separate mode preserves both architectural clarity and implementation realism.

### Final policy

- `query` remains a distinct entry kind in HIR, MIR, and canonical IR
- initial implementation focuses on runtime query execution and validation
- future query proof should be a separate statement/proof mode rather than an
  unconditional part of tx proof

---

## 6. Decision 4: Guarded Operation Frontier

### 6.1 Final decision

Canonical IR introduces:

- **guards**

but only for:

- effectful semantic ops
- and checked or partial ops

Total pure value ops remain unguarded.

### 6.2 Guard meaning

If an op has:

- no guard, it always applies
- a true guard, it applies
- a false guard, it is semantically inactive

### 6.3 Initial guardable class

The initial guardable frontier is:

- `Assert`
- `DivMod` and other checked partial ops
- `ReadState`
- `WriteState`
- `DeleteState`
- `ReadStateProperty`
- `AssertRelation`
- `EvalRelation`
- `CallCapability`
- `EmitEvent`

### 6.4 Initial non-guardable class

The initial non-guardable class is:

- arithmetic
- comparisons
- boolean ops
- `Select`
- builtin `Hash`
- other total pure value ops

These are handled through ordinary evaluation plus value merging with `Select`.

### 6.5 Output semantics for guarded ops

For output-producing guarded ops, the initial recommended executor semantics are:

- if the guard is false, the op becomes semantically inactive
- but output locals still receive typed inactive default values

Examples:

- `ReadState` may produce `present = false` and a default value
- checked ops such as `DivMod` may produce default quotient/remainder values
- `EvalRelation` / `CallCapability` may produce default output tuples

This keeps:

- SSA local assignment total,
- executor behavior deterministic,
- and later value merging simpler.

### 6.6 Why this is the right frontier

Guarding every op would bloat the canonical IR and blur the difference between:

- value predication
- and effect predication

Guarding nothing would make structured control lowering unsound or force a CFG
back into canonical IR.

The chosen frontier is the correct compromise:

- pure total ops stay simple
- semantically sensitive ops get the guard seam they need

---

## 7. Consequences for the Rewrite

These seam decisions now imply the following implementation constraints.

### 7.1 HIR

HIR should preserve:

- callable-category distinctions
- explicit relation use
- explicit query/event/context categories

### 7.2 MIR

MIR should own:

- effect summaries
- query-versus-tx legality
- capability metadata checking
- control-lowering feasibility analysis against the chosen guard frontier

### 7.3 Canonical IR

Canonical IR should now be treated as settled on the following points:

- builtin `Hash` exists for a small blessed family
- `CallCapability` remains the generic operational escape hatch
- `query` remains a distinct entry kind
- `EmitEvent` remains first-class
- guards exist only on effectful or checked ops

### 7.4 Runtime and proof

The runtime and proof frontend should assume:

- public statement-bound context initially
- digest-bound event outputs initially
- separate query-proof mode later
- relation-aware and capability-aware journaling

---

## 8. What This Note Commits To

This note is intended to settle the following.

- The four major remaining architecture seams are now closed.
- Hash is hybrid: blessed builtin family plus general capability fallback.
- Context remains distinct from tx args and is initially public-only.
- Events are typed effects with digest-only initial statement binding.
- Queries remain canonical external read entries, with proof support deferred to
  a separate later mode.
- Guards exist, but only for effectful and checked operations.
- Total pure value operations remain unguarded and merge through `Select`.

With these decisions in place, the next step is no longer architectural debate.
The next step is exact data-model and lowering design.
