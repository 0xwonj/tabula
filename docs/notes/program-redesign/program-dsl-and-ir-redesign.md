# Program DSL and IR Redesign

> **Status**: Proposed architecture note
> **Date**: 2026-03-24
> **Scope**: Records the intended redesign of Tabula's source language,
> compiler layering, canonical IR, and control-flow model before the
> `lang` / `compiler` / `ir` rewrite begins.
> **Related**: [program-dsl-grammar-sketch.md](program-dsl-grammar-sketch.md),
> [program-hir-design.md](program-hir-design.md),
> [program-mir-design.md](program-mir-design.md),
> [program-canonical-ir-design.md](program-canonical-ir-design.md),
> [program-typing-and-effect-system.md](program-typing-and-effect-system.md),
> [program-final-seam-decisions.md](program-final-seam-decisions.md),
> [program-rewrite-roadmap.md](program-rewrite-roadmap.md),
> [verification vocabulary](../../design/architecture.md#verification-vocabulary),
> [../executor-proof-codesign-architecture.md](../executor-proof-codesign-architecture.md),
> [../proof-front-end-journal-architecture.md](../proof-front-end-journal-architecture.md),
> [../../research/conditional-branching.md](../../research/conditional-branching.md)

---

## 1. Why This Note Exists

Tabula's current front-end stack is structurally older than the architecture now
emerging in runtime, executor, and proving:

- the current language is still close to a small instruction-oriented tx DSL,
- the current compiler path is still close to `AST -> low proof-aware IR`,
- and the current IR already behaves like a canonical execution/proof contract.

That shape was sufficient for the current generation of features, but it does
not fit the next one. The next language needs:

- a clearer top-level ontology,
- first-class constants and relations,
- eventual external read/output surfaces,
- structured control at the source level,
- and a compiler pipeline that can preserve source semantics without polluting
  the canonical proof boundary.

This note therefore fixes the intended design before the rewrite starts.

The goal is not only "a nicer DSL". The goal is:

> **a closed-world, proof-aware state machine language whose compiler
> explicitly separates source semantics, compiler normalization, and canonical
> proof execution.**

---

## 2. Core Thesis

The intended future shape of Tabula is:

1. a **closed-world program language**, not an open-world multi-contract
   ecosystem,
2. a **state-transition-first DSL**, not a general-purpose imperative
   language,
3. a language with **proof-native semantic categories** such as relations and
   sealed constants,
4. a compiler stack with explicit **AST -> HIR -> MIR -> canonical IR**
   layering,
5. and a canonical proof IR that remains **flat, SSA-based, CFG-free, and
   fixed-shape**.

This implies a stronger claim:

> **Tabula should not evolve by incrementally accreting more syntax onto the
> current AST -> IR pipeline. It should be rebuilt around a better semantic
> program model, better intermediate layers, and a smaller canonical proof
> contract.**

---

## 3. World Model

### 3.1 Closed world, not open world

Tabula is currently best understood as a **single sealed program** that owns:

- one state universe,
- one set of transactions,
- one set of relations,
- one constant pool,
- one external query/output surface,
- and one proving contract.

This is not the EVM model.

In the EVM, many contracts coexist in an open world and call each other. In
Tabula, the natural model is:

- one program,
- one state machine,
- one proof boundary,
- no ambient cross-program invocation as a default assumption.

That is why `program` is preferred over `contract` as the top-level DSL noun.

### 3.2 State machine, not generic script

The primary semantic unit is not "a function call" but "a state transition".

The language is therefore centered on:

- persistent declared state,
- transition entrypoints,
- deterministic execution over that state,
- and proof of the resulting transition.

This also explains why `tx` is a good surface keyword: it names the true
semantic unit of external mutation.

---

## 4. Semantic Categories

The language should explicitly separate the following categories.

### 4.1 Mutable state

Mutable program-owned persistent data.

Examples:

- account balances,
- lifecycle status tables,
- registries,
- settlement ledgers.

This is the core execution surface.

### 4.2 Constants

Immutable value-bearing program data.

Constants are:

- not state,
- not relations,
- not public instance inputs,
- not capabilities.

They are sealed values that belong to the program definition itself.

Examples:

- thresholds,
- domain separators,
- small fixed blobs,
- sealed configuration vectors.

### 4.3 Relations

Immutable semantic relations over typed tuples.

Relations are not "table lookups" in the language model. They are higher-level
semantic contracts such as:

- membership in a finite allowed set,
- membership in an allowed tuple relation,
- deterministic evaluation of a fixed finite mapping,
- decomposition or range constraints,
- lifecycle transition admissibility.

This is a semantic category, not a proving backend choice. A lookup argument is
one possible proving realization of a relation, not the user-facing concept.

### 4.4 Capabilities

Algorithmic computation capabilities such as hashes, capabilities, and other
typed nontrivial operations.

These are neither state nor relation. They are operational capabilities.

The finalized initial taxonomy is:

- a very small blessed builtin `Hash` family for ubiquitous total hashes,
- and `Capability` / `CallCapability` for non-blessed or custom operational
  kernels.

### 4.5 Effects

Externally relevant consequences of execution, especially:

- state writes,
- event emission,
- future externally visible outputs.

These are distinguished from pure value computation.

### 4.6 Static effect axes

The language should not be modeled with a single pure/impure distinction.

The intended static discipline distinguishes:

- **world effects**
  - interaction with mutable program state and externally visible world surfaces
- **proof-observable semantic effects**
  - semantically meaningful operations that should remain visible to journaling
    and proof preparation even if they do not mutate state
- **failure or checked behavior**
  - operations that may fail, trap, or require guarded treatment under later
    control lowering

This distinction matters especially for:

- `query`
- `relation`
- `capability`
- `assert`
- and future guarded lowering in canonical IR

---

## 5. Source-Language Design Principles

### 5.1 Prefer semantic categories over backend-oriented terms

The DSL should speak in terms of:

- program,
- state,
- tx,
- const,
- relation,
- query,
- event,
- capability.

It should avoid leaking:

- machine,
- chip,
- bus,
- trace,
- lookup table internals,
- proof-slot structure,
- raw backend identifiers.

### 5.2 Prefer explicit categories over overloaded surface forms

The DSL should make it obvious whether something is:

- mutable state,
- immutable program data,
- a relation use,
- or a capability call.

This is why:

- `const` should exist,
- `relation` should exist,
- `tx` should remain distinct from `fn`,
- and `query` should be distinct from both.

### 5.3 Prefer source clarity over Solidity mimicry

Tabula can borrow some ergonomic lessons from Solidity, especially around:

- top-level declarations,
- persistent state declarations,
- entrypoints,
- helper functions,
- typed events.

However, Tabula should not inherit Solidity's ontology wholesale. In
particular, it should avoid becoming defined by:

- open-world contract interactions,
- visibility-heavy function taxonomy,
- EVM baggage,
- or message-call-centric thinking.

### 5.4 Keep the canonical proof boundary small

Surface expressiveness should not be achieved by bloating the canonical IR.

Instead:

- surface syntax may be rich,
- HIR may preserve source semantics,
- MIR may preserve structured control,
- but canonical IR should remain small and stable.

---

## 6. Proposed DSL Shape

The intended top-level structure is:

- `program`
- `state`
- `const`
- `relation`
- `event`
- `fn`
- `query`
- `tx`

For exact surface syntax, the authoritative note is
[program-dsl-grammar-sketch.md](program-dsl-grammar-sketch.md). The examples in
this note are architectural illustrations, not the final syntax source of
truth.

### 6.1 Example direction

```tabula
program Registry

state {
  table users(key id: UserId) {
    active: bool @ssmc
    tier: u8 @ssmc
  }
}

const MAX_TIER: u8 = 3;

relation AllowedTier(tier: u8) = enum { 0, 1, 2, 3 };

event UserRegistered(id: UserId, tier: u8);

fn ensure_valid_tier(tier: u8) {
  assert relation AllowedTier(tier);
}

query is_active(id: UserId) -> bool {
  return users[id].active;
}

tx register(id: UserId, tier: u8) {
  ensure_valid_tier(tier);
  users[id].active = true;
  users[id].tier = tier;
  emit UserRegistered(id, tier);
}
```

### 6.2 `program` over `contract`

`program` is preferred because it is:

- semantically broader,
- less tied to smart-contract-specific ontology,
- more consistent with sealed artifacts and runtime programs,
- and a better fit for the closed-world model.

### 6.3 `state` over `schema`

At the source level, authors declare state, not schemas.

`schema` remains a good internal term, such as `StateSchema`, but the DSL
keyword should be `state`.

### 6.4 `tx` and `fn`

`tx` and `fn` should remain distinct:

- `tx` = external mutating entrypoint,
- `fn` = internal helper logic.

This is better than collapsing everything into `public/private fn`.

Later, `query` can serve as the external read-only analogue.

### 6.5 Declarations live inside the program scope

The default placement for:

- `state`,
- `const`,
- `relation`,
- `event`,
- `fn`,
- `query`,
- `tx`

should be inside one program scope. This keeps the semantic universe sealed and
avoids cross-file or top-level namespace drift in the early language.

---

## 7. Constants

Constants are a foundational semantic category and should be designed from the
start, even if the first implementation is small.

### 7.1 Constant semantics

Constants are:

- immutable,
- program-sealed,
- read-only,
- value-bearing,
- and not part of the mutable state surface.

### 7.2 Constant implementation direction

At the language level:

- `const NAME: Type = expr`

At the compiler/runtime level:

- constant identity by `ConstId`,
- values carried in a `ConstantPool`,
- entries carried as `ConstantEntry`,
- and references lowered explicitly in IR.

Small literals may still remain inline literals; the constant pool is for
semantic constants, deduplicated values, and structured immutable program data.

---

## 8. Relations

Relations are the other foundational addition and should also be designed from
the start.

### 8.1 Why relations exist

Relations capture proof-native semantics that should not be modeled as:

- mutable state,
- generic capability calls,
- or low-level table reads.

Typical examples:

- membership in a small enumerated set,
- finite fixed mappings,
- allowed state transitions,
- decomposition and range constraints.

### 8.2 Definition versus usage

Relation definitions should state what a relation is:

- arity,
- typing,
- whether it is functional,
- and what semantic class it belongs to.

They should **not** encode whether it is used as `assert` or `eval`.

Those are use-site modes:

- `assert relation R(...)`
- `eval relation F(...)`

### 8.3 Relation implementation direction

The intended vocabulary is:

- `RelationDescriptor`
- `RelationManifest`
- `RelationBinding`

And the intended core IR operations are:

- `AssertRelation`
- `EvalRelation`

This preserves the distinction between:

- the semantic contract of a relation,
- the sealed set of relations referenced by a program,
- and the exact committed identity of a relation universe.

---

## 9. External Boundary Concepts

These concepts are not all V1 surface features, but they should be present in
the architecture from the beginning.

### 9.1 Context

The language needs a distinction between:

- per-transaction arguments,
- and per-instance or per-batch external inputs.

`context` is the current preferred noun for that second category.

This is not primarily about public versus private visibility. It is about
scope:

- tx-local versus program-instance-global.

The initial implementation policy is:

- context remains distinct from tx arguments,
- all initial context fields are public,
- and all initial context fields are statement-bound.

### 9.2 Query

`query` is an external semantic read interface.

It is not an internal helper and should not be confused with `fn`.

`query` should be understood as:

- externally callable,
- read-only in the state/world sense,
- result-bearing,
- but not necessarily pure in the stronger semantic sense.

A query may legitimately:

- read state,
- read state properties,
- use relations,
- and potentially call query-safe deterministic capabilities.

In the initial implementation, queries remain first-class canonical entry kinds,
but query proving is deferred to a later separate mode rather than folded into
ordinary tx proof statements.

The long-term role of queries is:

- program-defined state views,
- external read APIs,
- and potentially proveable state-derived claims.

### 9.3 Event

`event` is an externally meaningful execution output surface.

It should be typed, not topic-string-only.

Events are not strictly required for the smallest core DSL, but they are a
natural closed-world output interface and should be part of the long-term
program model from the beginning.

Events should also be understood as:

- world-observable outputs,
- explicit execution effects,
- and likely proof-boundary-relevant outputs even if the initial public binding
  policy remains conservative.

The initial proof-boundary policy is:

- full typed events remain in execution/runtime outputs and journals,
- while the verifier-visible statement binds only the ordered event digest.

---

## 10. Assertions, Predicates, and Invariants

### 10.1 `assert`

`assert` is a local execution-time check.

It is path-local and body-local.

It should also be treated as a source of failure or checked behavior in the
static model, not merely as an ordinary statement form.

### 10.2 `requires`

`requires` is a useful future surface for entry preconditions, but it is not
foundational enough to require immediate implementation. In early versions, a
front-of-body `assert` can serve the same operational role.

### 10.3 `predicate`

`predicate` is a good future concept for reusable logical conditions.

It is stronger than ad hoc helper functions for assertion reuse, but much
lighter than full invariants.

### 10.4 `invariant`

`invariant` is a global semantic law of the program, not a reusable local
assertion template.

It is interesting and potentially important, but it should be treated as a
later-stage feature because its proof and performance implications are
substantial.

---

## 11. Compiler Layering

The current pipeline is too shallow for the next-generation language.

The intended layering is:

1. **AST**
2. **HIR**
3. **MIR**
4. **canonical IR**

### 11.1 AST

Parser tree. Syntax-oriented. Carries surface structure and source spans.

### 11.2 HIR

High-level IR.

This is not merely parsed syntax. It is the first semantic form of the DSL:

- program structure resolved,
- declarations classified,
- source categories preserved,
- structured control preserved.

HIR should remain close to the language.

### 11.3 MIR

Mid-level IR.

This is the compiler's main normalization workspace:

- identifiers resolved to semantic IDs,
- typing finalized,
- sugar removed,
- effects and effect summaries made explicit,
- structured control retained in compiler-friendly form,
- legality checks and normalization passes performed,
- helper functions prepared for inlining or expansion,
- lowering to canonical IR prepared.

MIR exists because HIR is too rich and canonical IR is too small. Without MIR,
the language would either bloat the proof IR or collapse all compiler logic
into one oversized lowering pass.

### 11.4 Canonical IR

The final `tabula-ir` role should be:

- canonical execution/proof contract,
- small and stable instruction set,
- flat SSA,
- fixed-shape,
- no general CFG.

This is the IR consumed by execution and later runtime proving.

---

## 12. Control Flow Design

### 12.1 Canonical stance

The current preferred design is:

- structured control in HIR and MIR,
- but **no CFG in canonical proof IR**.

This note therefore updates and narrows earlier branching research that treated
CFG as a candidate canonical direction.

### 12.2 Why not CFG as canonical proof IR

A CFG-based canonical IR would shift Tabula toward a small zkVM model:

- program counter reasoning,
- dynamic path shape,
- edge semantics,
- block reachability,
- phi-like merge semantics,
- and path-dependent effect scheduling.

That is a valid architecture for some systems, but it is not the best fit for
Tabula's current goals:

- flat proof-friendly semantics,
- deterministic journaling,
- plan-aligned proof reduction,
- and a fixed-shape execution/proof contract.

### 12.3 Preferred model

The preferred model is:

- `if` and `match` exist at the surface,
- structured regions exist in HIR and MIR,
- and canonical IR uses selector-driven normalization:
  - `select` for value merging,
  - one-hot selectors for `match`,
  - and guarded effect semantics for side-effectful regions.

In short:

> **structured control at the top, predicated SSA at the proof core**

### 12.4 Implication for loops

General loops should not shape the canonical IR early.

If loops arrive later, they should first appear as:

- bounded,
- statically analyzable,
- and lowerable into structured regions plus predicated or unrolled forms.

---

## 13. MLIR Stance

The recommended position is:

- **MLIR-inspired**
- but **not MLIR-dependent**

That means:

- adopt region-based thinking,
- adopt structured control normalization,
- adopt block-argument style ideas where useful,
- and adopt layered canonicalization,

without adopting a full MLIR toolchain as the core implementation strategy.

The main reasons are:

- current repository and team context are Rust-centric,
- the canonical proof IR remains custom and smaller than a typical MLIR stack,
- and full MLIR adoption would add substantial infrastructure cost before it
  becomes truly necessary.

MLIR remains a useful reference model, not the current recommended foundation.

---

## 14. V1 / V2 / V3 Rollout

### 14.1 Shipping order

The recommended rollout remains:

- **V1**: core state machine language
- **V2**: external boundary surfaces
- **V3**: structured control and later spec/sugar

### 14.2 V1

Implement:

- `program`
- `state`
- `table`
- `const`
- `relation`
- `fn`
- `tx`
- `assert`
- straight-line bodies

### 14.3 V2

Implement:

- `context`
- `query`
- `event`
- optional `requires`

### 14.4 V3

Implement:

- `if`
- `match`
- bounded loop sugar
- `predicate`
- optional `ensures`
- restricted `invariant`
- additional surface sugar such as relation shorthand

### 14.5 Architecture versus rollout

The shipping order should not be confused with design order.

Even if V2/V3 features are implemented later, the architecture should reserve
them from the start:

- `ContextSchema`
- `QueryDefs`
- `EventDefs`
- region-capable MIR
- canonical IR extension seams for guarded effects

That prevents structural rewrites later.

---

## 15. Rewrite Strategy

The intended rewrite scope is:

- `lang`
- `compiler`
- `ir`

The preferred strategy is **parallel replacement, then cut over**, not "delete
everything first".

### 15.1 Why rewrite

The current front-end architecture predates:

- relations,
- constants,
- external read/output surfaces,
- structured control,
- and the desired layered compiler model.

Retrofitting all of that into the existing AST -> IR path is likely to be more
complex and less clean than rebuilding the front-end stack around the new
model.

### 15.2 Why not immediate deletion

The runtime, executor, witness, and proving stack now provide a relatively
stable downstream anchor. Replacing the front-end in parallel while preserving
an executable baseline is safer than deleting the old stack before the new one
lands.

The intended sequence is:

1. fix the architecture in writing,
2. build the new AST/HIR/MIR/canonical-IR path beside the old one,
3. land V1 end-to-end,
4. cut over,
5. then delete the old path.

---

## 16. Immediate Design Commitments

The following points are strong enough to treat as current design commitments.

### 16.1 Strong commitments

- Tabula is a closed-world program language.
- `program` is preferred over `contract`.
- `state` is the source-level noun; `StateSchema` remains an internal noun.
- `tx` and `fn` remain distinct semantic categories.
- `const` and `relation` are foundational and should be designed from the
  start.
- queries and events belong in the long-term program model even if implemented
  later.
- compiler layering should become AST -> HIR -> MIR -> canonical IR.
- canonical IR should remain flat, SSA-based, and CFG-free.
- structured control should live above canonical IR and lower into predicated
  SSA.

### 16.2 Deferred but reserved

- `requires`
- `predicate`
- `query` proofs
- event binding policy details
- `ensures`
- restricted `invariant`
- bounded loops

These are not rejected. They are simply later-stage work.

---

## 17. What This Note Makes Possible

Once this note is accepted as the current architectural baseline, the next work
can proceed coherently:

- define a concrete grammar sketch,
- define HIR node sets,
- define MIR node sets,
- redesign `tabula-ir` as the smaller canonical contract,
- and plan the actual front-end rewrite without re-litigating first principles.

That is the real purpose of this note: to preserve design context so that the
upcoming implementation work does not lose the reasoning behind it.
