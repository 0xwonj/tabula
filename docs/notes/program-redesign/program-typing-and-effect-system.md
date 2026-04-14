# Program Typing and Effect System

> **Status**: Implemented architecture note for the current rewritten stack
> **Date**: 2026-03-25
> **Scope**: Defines the intended typing and effect-system model for the
> redesigned Tabula program DSL and explains how it should guide HIR, MIR, and
> canonical IR design.
> **Related**: [program-dsl-and-ir-redesign.md](program-dsl-and-ir-redesign.md),
> [program-dsl-grammar-sketch.md](program-dsl-grammar-sketch.md),
> [program-hir-design.md](program-hir-design.md),
> [program-mir-design.md](program-mir-design.md),
> [program-canonical-ir-design.md](program-canonical-ir-design.md),
> [program-final-seam-decisions.md](program-final-seam-decisions.md),
> [../program-static-semantics-research-directions.md](../program-static-semantics-research-directions.md),
> [program-rewrite-roadmap.md](program-rewrite-roadmap.md),
> [verification vocabulary](../../design/architecture.md#verification-vocabulary)

---

## 1. Why This Note Exists

The redesign has already fixed:

- the language ontology,
- the compiler layering,
- and the canonical IR direction.

However, one major semantic question still needs to be made explicit:

> **what should the language statically know about values, effects, failure, and
> callable boundaries?**

This matters because the new language is not just:

- a typed syntax,
- or a small tx DSL,
- or a proof backend frontend.

It is a **closed-world, proof-aware state machine language**.

That means its static discipline must account not only for:

- value typing,

but also for:

- mutable-world effects,
- proof-observable semantic effects,
- and failure or checked behavior that affects guarded control lowering.

This note fixes that intended discipline.

---

## 2. Core Thesis

The correct static model for Tabula is not:

- a plain type system only,
- nor a single "pure versus impure" effect bit.

The correct model is:

1. a **value type system**
2. an **effect system**
3. and later a **specification layer**

with the following key claim:

> **Tabula must distinguish world effects, proof-observable semantic effects,
> and failure behavior.**

This is the most important typing/effect-system conclusion of the current
redesign.

Without that distinction:

- `query` will be modeled too weakly,
- `relation` will be misclassified as "pure" in the wrong sense,
- `capability` will remain under-specified,
- and `guard` will not have a clean static footing.

---

## 3. PL Framing

From a PL perspective, Tabula is best understood as:

- **first-order**
- **closed-world**
- **state-transition oriented**
- **effect-stratified**
- **non-Turing-complete by design**
- and **relationally enriched**

In particular, it is not:

- a general-purpose imperative language,
- an open-world contract language,
- or a pure constraint language.

The central semantic units are:

- program-owned state,
- externally callable transitions,
- program-sealed constants,
- immutable semantic relations,
- and explicit external read/output surfaces.

This means Tabula needs a stronger static story than "expressions have types".
It needs a static story about what kind of semantic interaction each construct
performs.

---

## 4. Three Static Layers

The intended long-term static model has three layers.

### 4.1 Value typing

This answers:

- what type a value has,
- whether a table key matches its schema,
- whether relation arguments match their signature,
- whether event arguments are well-typed,
- whether capability inputs and outputs are compatible.

### 4.2 Effect typing

This answers:

- whether a body reads state,
- writes state,
- emits events,
- uses relations,
- calls capabilities,
- or may fail.

This is the critical layer for:

- `fn` / `query` / `tx` discipline,
- guarded lowering,
- and legality checks.

### 4.3 Specification typing

This is later-stage work for:

- `requires`
- `ensures`
- `predicate`
- `invariant`

This should not be conflated with ordinary execution effects.

The key separation is:

- execution/effect typing belongs to the core language,
- specification typing belongs to a later spec layer.

---

## 5. Value Typing

The value type system is comparatively conventional.

At minimum, it should account for:

- scalar values such as `bool`, `u8`, `u64`, `Address`
- tuples
- lists or vectors if the type system allows them
- table key/value types
- const types
- relation signatures
- event signatures
- capability signatures

The interesting part is not ordinary scalar typing. The interesting part is that
the type system must coexist with several distinct semantic namespaces:

- locals
- state tables and fields
- constants
- relations
- capabilities
- event declarations
- context fields

This already suggests that the typing environment is richer than a simple local
context.

---

## 6. Suggested Typing Environments

The exact notation does not need to be user-facing, but conceptually the
compiler should reason with environments roughly like:

```text
Γ  = local bindings
Σ  = state schema
Χ  = context schema
Ρ  = relation environment
Κ  = capability environment
Ε  = event environment
```

Then judgements can be thought of as:

```text
Γ; Σ; Χ; Ρ; Κ; Ε ⊢ e : τ ! ε
Γ; Σ; Χ; Ρ; Κ; Ε ⊢ stmt ok ! ε
Γ; Σ; Χ; Ρ; Κ; Ε ⊢ body : τ ! ε
```

The point is not the exact notation. The point is that:

- state,
- const,
- relation,
- capability,
- and event names

must not collapse into one undifferentiated namespace.

That is already aligned with the HIR and MIR direction.

---

## 7. Why a Single Pure/Impure Bit Is Not Enough

This is the most important design conclusion.

In many languages, an effect system can begin with:

- pure
- impure

That is not good enough for Tabula.

Why?

Because the language has constructs that are:

- not pure in the proof sense,
- but also not mutating in the world sense.

Two important examples are:

- relation use,
- and state-property reads.

Similarly:

- `query` is read-only, but not necessarily pure,
- `capability` may be deterministic but still semantically important,
- `assert` and checked operations may fail without mutating state.

Therefore the language needs a more structured effect model.

---

## 8. Core Effect Distinction

The central proposal is:

### 8.1 World effects

These interact with the program's externally meaningful mutable world.

Examples:

- `StateRead`
- `StateWrite`
- `StateDelete`
- `EmitEvent`

### 8.2 Proof-observable semantic effects

These do not necessarily mutate state, but they still matter semantically and
should remain visible to execution journaling and proof preparation.

Examples:

- `RelationAssert`
- `RelationEval`
- `StatePropertyRead`
- `CapabilityCall`

### 8.3 Failure or checked behavior

These capture the fact that an operation may:

- require a domain condition,
- fail,
- trap,
- or otherwise need guarded treatment under control lowering.

Examples:

- `Assert`
- `DivMod`
- partial capability calls if any exist
- partial relation evaluation if any exist

This gives a three-way distinction:

- world interaction,
- proof-observable semantics,
- and failure/checked behavior.

That is much better aligned with Tabula than "pure/impure".

---

## 9. Proposed Effect Summary Shape

The exact Rust type can vary, but conceptually MIR analysis should summarize
callable behavior roughly like this:

```rust
pub struct EffectSummary {
    pub world: WorldEffects,
    pub proof: ProofEffects,
}

pub struct FailureSummary {
    pub semantic_may_fail: bool,
    pub host_contract_sensitive: bool,
}

pub struct PolicySummary {
    pub uses_builtin_hash: bool,
    pub uses_tx_only_capability: bool,
    pub uses_query_safe_capability: bool,
    pub uses_journaled_capability: bool,
    pub uses_opaque_runtime_capability: bool,
}

pub struct ContextDemandSummary {
    pub fields: BTreeSet<ContextFieldId>,
}
```

Where:

```rust
pub enum WorldEffect {
    StateRead,
    StateWrite,
    StateDelete,
    EmitEvent,
}

pub enum ProofEffect {
    RelationUse,
    StatePropertyRead,
    CapabilityCall,
}
```

This is only one possible encoding, but the separation itself is the important
part.

The important ownership rule is:

- raw MIR payload does not need to store these summaries directly
- the summaries can live in an analyzed wrapper such as
  `VerifiedProgram -> AnalyzedProgram`
- effect summary is best treated as derived compiler analysis, not as a second
  source of truth inside the structural IR payload
- failure and policy summaries are separate static axes, not extensions of
  effect classification
- context demand is another static axis:
  it is not a world effect, but still matters for public-input discipline

### 9.1 Why `StateRead` belongs in world effects

Reads do not mutate, but they still belong to the state/world surface because:

- they observe mutable program state,
- they affect admissible query discipline,
- and they are part of the state interaction model.

### 9.2 Why relation use belongs in proof effects

Relations are:

- deterministic,
- immutable,
- and not world-mutating.

But they are still semantically important:

- they should journal as relation uses,
- they shape proof input reduction,
- and they are not just "pure arithmetic".

So relation use deserves its own proof-observable category.

### 9.3 Why failure must stay separate from effects

`semantic_may_fail` and `host_contract_sensitive` matter for static reasoning,
but they are not world effects and not proof-observable effects. Keeping them
in a separate `FailureSummary` makes later lowering and legality rules much
clearer.

### 9.4 Why builtin/hash and capability policy facts belong in policy summary

Facts such as `uses_builtin_hash`, `uses_tx_only_capability`, and capability
visibility class are not themselves semantic effects. They are callable-policy
facts that analysis can use for legality and backend planning, so they belong
in a separate `PolicySummary`.

---

## 10. Callable Kinds Versus Effects

One of the most important clarifications is:

> **`fn`, `query`, and `tx` are callable categories, not effect summaries.**

They are different dimensions.

### 10.1 `fn`

`fn` is:

- internal,
- reusable,
- not externally callable,
- and effect-polymorphic only in a very limited compiler-internal sense.

It should carry an inferred effect summary, even if the source language does not
spell that summary explicitly.

### 10.2 `query`

`query` is:

- externally callable,
- read-only,
- result-bearing.

But it is **not** necessarily pure.

A query may legitimately:

- read state,
- read state properties,
- use relations,
- and possibly call query-safe deterministic capabilities.

So `query` should be modeled as:

- read-only in the world-effect sense,
- not pure in the "no semantically visible action" sense.

This is a crucial distinction.

### 10.3 `tx`

`tx` is:

- externally callable,
- state-mutating,
- and typically unit-returning.

Transactions may:

- read state,
- write state,
- use relations,
- call capabilities,
- emit events,
- and fail.

This is the broadest callable class.

---

## 11. Recommended Callable Policies

The language should enforce the following static discipline.

### 11.1 `fn`

Allowed in principle:

- any effect needed by the enclosing language,

but tracked by inferred summary and checked at call sites.

### 11.2 `query`

Allowed:

- `StateRead`
- `StatePropertyRead`
- `RelationUse`
- query-safe `CapabilityCall`
- `MayFail` if query assertions are allowed

Also freely usable because they are pure total value computation rather than
effects:

- arithmetic/comparison/boolean value ops
- builtin blessed `Hash`

Forbidden:

- `StateWrite`
- `StateDelete`
- `EmitEvent`

### 11.3 `tx`

Allowed:

- all V1/V2 world effects
- all proof-observable effects
- `MayFail`

This makes `query` a strict subset of `tx` in effect policy, without implying
that `query` is pure.

---

## 12. Relations in the Effect System

Relations are one of the trickiest cases because they look pure from one angle
and effectful from another.

### 12.1 Computationally deterministic

A relation use is:

- deterministic,
- immutable,
- and not state-mutating.

### 12.2 Proof-observable

At the same time, relation use should remain visible because:

- the executor should journal it,
- runtime proving should reduce it,
- and the backend may realize it via lookup, custom AIR, arithmetic lowering,
  or committed witness materialization.

### 12.3 Recommended classification

Relations should therefore be treated as:

- **not world effects**
- but **yes proof-observable effects**

This is much more accurate than calling them either:

- "pure" in the ordinary sense,
- or "impure" in the stateful sense.

### 12.4 Consequence for language design

This means the compiler should preserve relation use explicitly in:

- HIR,
- MIR,
- and canonical IR.

That is already consistent with the `AssertRelation` / `EvalRelation` direction.

---

## 13. Capabilities in the Effect System

Capabilities are another important category that need stronger static metadata.

### 13.1 Why signatures are not enough

A capability signature alone does not answer:

- is it deterministic?
- is it total?
- can it fail?
- is it query-safe?
- is it proof-observable?

Those properties matter directly for:

- effect checking,
- branch lowering,
- query legality,
- and canonical IR classification.

### 13.2 Recommended capability metadata

The capability descriptor should eventually carry at least:

- input types
- output types
- total versus checked/partial
- query-safe versus tx-only
- proof-observable versus not journaled
- whether it belongs to a blessed builtin family such as hash

The finalized execution semantics are:

- `Checked` means the capability may fail as part of ordinary program semantics.
- `Total` means the capability is semantically non-failing; any runtime/host
  error is a contract violation rather than a user-level semantic failure.

### 13.3 Why this improves the current design

Without this metadata, capability calls remain underspecified compared to:

- relation calls,
- query restrictions,
- and guarded-lowering policy.

Adding it would tighten the whole static model.

---

## 14. Hash Classification

Hashing deserves special mention because it sits on the border between:

- builtin pure computation,
- capability-like operation,
- and proof-observable semantics.

The current architecture now adopts a finalized hybrid policy:

- blessed ubiquitous hash families may lower to dedicated builtin canonical ops,
- other operational kernels remain `CapabilityCall`.

From the effect-system perspective, the important point is:

- hashes are normally total and deterministic,
- so they behave much more like pure builtin computations than like checked
  operational kernels.
- builtin `Hash` is therefore **not** a world effect and **not** a
  proof-observable effect family
- if MIR wants to remember that a callable used builtin hash, that should be an
  analysis bit such as `uses_builtin_hash`, not a proof-effect classification

This is why treating at least some hash families separately still makes sense.

---

## 15. Failure and Checked Behavior

`assert` and checked operations need explicit static recognition.

### 15.1 Why failure matters

Failure is not just another world effect.

It matters because:

- it changes admissibility,
- it affects control lowering,
- and it determines which operations can be safely speculated under predication.

### 15.2 `MayFail`

The MIR failure summary should therefore include a semantic-failure flag such
as:

- `semantic_may_fail`

This should cover:

- `assert`
- checked arithmetic such as `DivMod`
- partial relation evaluation if any exist
- checked capabilities if any exist

It should **not** cover:

- total capability host/runtime failures

Those belong to a separate operational axis such as:

- `host_contract_sensitive`

### 15.3 Why this matters for control lowering

When `if` / `match` eventually lower into a flat canonical IR, the compiler must
know which operations:

- may be evaluated speculatively,
- and which must be guarded because untaken paths may be invalid.

`MayFail` is therefore not only a typing concern. It is also a control-lowering
concern.

In the finalized lower-boundary architecture this means:

- checked capability calls contribute to `semantic_may_fail`
- total capability calls do not contribute to `semantic_may_fail`
- total capability calls may still set `host_contract_sensitive`

---

## 16. Guards and the Effect System

Guards are the canonical IR mechanism that lets effectful or checked operations
be conditionally active without introducing CFG.

### 16.1 What a guard means

If an op has:

- no guard, it always applies
- a true guard, it applies
- a false guard, it is semantically inactive

### 16.2 Why guards need static support

The language and compiler need to know which operations are:

- total and safe to compute speculatively,
- versus semantically sensitive and therefore guardable.

That boundary is exactly where the effect system helps.

### 16.3 Initial finalized guardable frontier

The initial guardable class is:

- assertions
- checked partial ops
- state reads
- state writes
- state deletions
- state-property reads
- relation operations
- capability calls
- event emission

Pure total value operations should stay outside that class and be merged later
through `Select`.

### 16.4 Inactive output semantics

For guarded operations that produce outputs, the initial canonical policy
should be:

- a false guard makes the op semantically inactive,
- but output locals still receive typed inactive default values.

This keeps:

- SSA local assignment total,
- executor behavior deterministic,
- and later value merging straightforward.

### 16.5 Static consequence

The compiler should not decide guarded lowering ad hoc per pass. It should rely
on an explicit static classification that already exists in MIR and carries into
canonical IR design.

---

## 17. Spec Layer Separation

The future spec features:

- `requires`
- `ensures`
- `predicate`
- `invariant`

should not be collapsed into the ordinary effect system.

They are better understood as a later specification layer, conceptually closer
to:

- preconditions,
- postconditions,
- and global laws

than to ordinary execution effects.

This suggests the right long-term separation:

- type system for values,
- effect system for execution semantics,
- spec system for proof obligations and contracts.

That is cleaner than trying to make one system do all three jobs at once.

---

## 18. Why This Fits a Non-Turing-Complete Language

The intended Tabula language is not Turing complete:

- no unrestricted loops,
- no recursion,
- bounded control only,
- flat canonical IR,
- fixed proof shape.

This is helpful for the static design.

It means:

- effect summaries can remain simple and finite,
- inference can stay lightweight,
- control-lowering legality is easier to analyze,
- and the language can favor explicit semantic categories over maximal
  expressiveness.

In other words, the restricted computational model is not a burden here. It is
what makes a simpler and stronger effect discipline possible.

---

## 19. Implications for HIR

HIR should remain source-semantic, but it should already reflect the following
typing/effect truths:

- `Const` is explicit
- relation assertion and evaluation are explicit
- `fn`, `query`, and `tx` remain distinct declaration kinds
- read-only versus mutating body categories are visible
- illegal body-category uses can already be diagnosed

HIR does **not** need full effect inference, but it should preserve enough
structure for later inference and checking.

---

## 20. Implications for MIR

MIR is where the effect system becomes concrete.

MIR should:

- infer or compute effect summaries for bodies
- classify operations by effect class
- enforce `query` versus `tx` discipline
- carry enough metadata for guarded-lowering decisions
- preserve relation and capability uses as explicit ops

This is the natural home for:

- effect checking,
- effect propagation through helper functions,
- and branch legality analysis.

### 20.1 Recommended MIR improvement

MIR should expose an explicit analyzed boundary instead of storing derived
effect summaries directly on raw callable payloads.

For example:

```rust
pub struct VerifiedProgram(Program);

pub struct AnalyzedProgram {
    pub verified: VerifiedProgram,
    pub analysis: ProgramAnalysis,
}

pub struct ProgramAnalysis {
    pub effect_summaries: BTreeMap<CallableId, EffectSummary>,
    pub failure_summaries: BTreeMap<CallableId, FailureSummary>,
    pub policy_summaries: BTreeMap<CallableId, PolicySummary>,
    pub context_demands: BTreeMap<CallableId, ContextDemandSummary>,
    pub call_graph: BTreeMap<CallableId, BTreeSet<CallableId>>,
}
```

This is the cleaner architecture because:

- raw MIR stays structural
- effect, failure, and policy summaries stay derived
- context demand stays derived and separate from effects
- query legality can use both call graph and capability policy
- inlining and canonicalization remain rewrite passes that invalidate analysis
- normalization and lowering can consume analyzed MIR without making analysis
  caches part of the IR payload itself

---

## 21. Implications for Canonical IR

Canonical IR should not need a large separate effect annotation language because
the op taxonomy itself already expresses most semantic effects.

However, the typing/effect analysis still informs canonical IR design in three
important ways.

### 21.1 Guardable frontier

The effect system determines which canonical ops need a guard seam.

### 21.2 Validation

Canonical IR validation should enforce:

- query bodies contain only permitted op families
- tx bodies satisfy their permitted return/output discipline
- guarded ops use boolean guards
- checked ops remain in the guardable class

### 21.3 Journal mapping

Because the effect system distinguishes proof-observable semantics from ordinary
world mutation, canonical IR can map more cleanly into:

- state-access journals
- relation journals
- capability journals
- event journals

without pretending everything non-pure is the same kind of effect.

---

## 22. Recommended Changes to the Current Design Set

This note suggests several concrete improvements to the current redesign.

### 22.1 Strengthen the definition of `query`

`query` should be described as:

- externally callable
- read-only
- result-bearing
- not necessarily pure

This is more precise than simply calling queries "read-only functions".

### 22.2 Make effect classes explicit in MIR documentation

The MIR design should more explicitly distinguish:

- world effects
- proof-observable effects
- checked or failure behavior

This is already implicit, but making it explicit would improve the design.

### 22.3 Add effect metadata to capability design

Capability descriptors should carry:

- totality
- query-safety
- proof-observability

at minimum.

### 22.4 Recognize relation as proof-observable, not merely pure

This should be a named design point, not just an accidental consequence of the
current op taxonomy.

### 22.5 Introduce explicit MIR analysis summaries

The most practical compiler-level improvement suggested by this note is to keep
`EffectSummary`, `FailureSummary`, and `PolicySummary` explicit at the analyzed
MIR boundary rather than collapsing them into a single summary kind.

---

## 23. Current Design Commitments

The following are the current design commitments aligned with the finalized
architecture.

- Do not model Tabula with a single pure/impure distinction.
- Distinguish world effects from proof-observable semantic effects.
- Track failure or checked behavior explicitly.
- Keep `fn`, `query`, and `tx` as callable kinds rather than replacing them with
  raw effect annotations.
- Infer effect summaries internally before exposing source-level effect
  annotations.
- Treat relations as proof-observable deterministic semantic operations.
- Give capabilities richer static metadata.
- Use effect classification to drive guarded canonical lowering.
- Keep spec constructs in a later specification layer rather than collapsing
  them into core effect typing.

---

## 24. Research Context

This design direction is informed by several PL traditions without attempting to
copy any one of them wholesale.

- Koka's row-polymorphic effect thinking:
  [Koka paper](https://www.microsoft.com/en-us/research/publication/koka-programming-with-row-polymorphic-effect-types/)
- Flix's practical effect sets and effect polymorphism:
  [Flix effect system](https://doc.flix.dev/effect-system.html),
  [Flix effect polymorphism](https://doc.flix.dev/effect-polymorphism.html)
- F*'s separation between ordinary effects and richer specification semantics:
  [F* PURE](https://fstar-lang.org/tutorial/book/part4/part4_pure.html),
  [F* effects overview](https://fstar-lang.org/eci2019/fstar-eci2019-lecture4.html)
- Move's resource/effect-oriented discipline as a reminder that not all useful
  static separation is ordinary typechecking:
  [Move abilities](https://move-book.com/reference/abilities/)

Tabula should remain simpler than those systems, but it can borrow their best
lesson:

> **a good language should make semantically important distinctions explicit in
> its static structure.**

---

## 25. What This Note Commits To

This note is intended to settle the following.

- Tabula needs both a type system and an effect system.
- The effect system must distinguish world effects, proof-observable semantic
  effects, and failure behavior.
- `query` is read-only, not necessarily pure.
- `relation` is deterministic but still proof-observable.
- `capability` needs richer static metadata than a raw signature alone.
- `MayFail` is an important static axis because it connects typing to guarded
  lowering.
- HIR should preserve the right semantic categories.
- MIR should own explicit effect summaries and effect checking.
- Canonical IR should remain small, but its op taxonomy and guard seam should be
  informed by this effect model.

With this in place, the next design question is no longer "do we need a typing
and effect story?" The answer is yes. The next question becomes:

- how to encode the exact HIR/MIR/canonical Rust data structures so this
  discipline is enforced cleanly.
