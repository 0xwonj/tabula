# Program Static Semantics Research Directions

> **Status**: Exploratory research note
> **Date**: 2026-03-24
> **Scope**: Surveys additional static-semantics research directions beyond the
> current typing and effect-system note, evaluates which ideas are worth
> borrowing for Tabula, and proposes a stronger long-term synthesis for a
> closed-world, proof-aware, journal-first DSL.
> **Related**: [program-redesign/program-typing-and-effect-system.md](program-redesign/program-typing-and-effect-system.md),
> [program-redesign/program-dsl-and-ir-redesign.md](program-redesign/program-dsl-and-ir-redesign.md),
> [program-redesign/program-mir-design.md](program-redesign/program-mir-design.md),
> [program-redesign/program-canonical-ir-design.md](program-redesign/program-canonical-ir-design.md),
> [proof-front-end-journal-architecture.md](proof-front-end-journal-architecture.md),
> [executor-proof-codesign-architecture.md](executor-proof-codesign-architecture.md)

---

## 1. Why This Note Exists

The current typing/effect note already settles the main baseline:

- Tabula needs both value typing and effect typing.
- Effects must distinguish:
  - world effects,
  - proof-observable semantic effects,
  - and failure or checked behavior.
- `query` is read-only, not necessarily pure.
- `relation` is deterministic but still proof-observable.

That baseline is strong.

However, an obvious follow-up question remains:

> **after reviewing the main reference points, is there anything important left
> to borrow, or any more Tabula-specific static idea worth exploring?**

This note records the answer.

The short answer is:

- the current direction remains correct,
- full general-purpose effect-system machinery should mostly be avoided,
- but several additional research directions are still worth considering,
- and their combination suggests a stronger long-term Tabula-specific static
  model than "types plus an effect summary" alone.

This is not a global novelty proof. It is a design-oriented research summary for
the current Tabula architecture.

---

## 2. Executive Conclusion

The current design has already borrowed the most important lessons from:

- Koka,
- Flix,
- F*,
- and Move.

So the next gains are unlikely to come from importing a completely different
full language design wholesale.

The most promising remaining directions are instead:

1. **coeffects or context-demand tracking**
2. **obligation summaries richer than a coarse `may_fail` bit**
3. **footprint-indexed effect summaries over the closed-world program schema**
4. **lightweight phase-ordered effects**
5. **optional quantitative or bounded summaries**
6. **a tighter integration between static capabilities and static effects**

The strongest synthesis for Tabula is therefore likely not:

- "just a better effect system"

but rather:

> **a closed-world static semantics that tracks effects, context demands,
> obligations, footprints, and later bounded cost-like summaries.**

That synthesis appears especially well matched to a language that is:

- closed-world,
- non-Turing-complete by design,
- proof-aware,
- and journal-first in runtime architecture.

---

## 3. What The Current References Already Contributed

### 3.1 Koka

Useful lesson:

- effects belong in the static signature of computations,
- effect inference should be precise,
- and effect polymorphism is real design leverage.

What Tabula has already absorbed:

- the idea that value typing alone is not enough,
- and the idea that inferred effect summaries are better than immediate
  user-facing effect syntax.

What Tabula should probably **not** copy directly:

- full row-polymorphic source-level effect syntax,
- or a general-purpose handler-oriented language design.

### 3.2 Flix

Useful lesson:

- effects can act as compiler-checked behavioral policy,
- not merely as a pure/impure distinction.

What Tabula has already absorbed:

- callable policy thinking for `fn` / `query` / `tx`,
- and the idea that allowed and forbidden effects are a first-class static
  question.

What Tabula may still borrow later:

- a more structured notion of effect combination,
- and effect exclusion-style policy constraints.

### 3.3 F*

Useful lesson:

- execution effects and specification semantics should not be collapsed into one
  undifferentiated system.

What Tabula has already absorbed:

- the core-vs-spec separation,
- and the idea that "failure behavior" matters semantically even when it is not
  an ordinary state mutation.

What Tabula should probably avoid in the core DSL:

- full weakest-precondition calculus in the execution language,
- or a proof assistant-style computation-type tower.

### 3.4 Move

Useful lesson:

- not every important static distinction is ordinary value typing,
- and permission-like static categories can be central to language safety.

What Tabula has already absorbed:

- the habit of treating semantically important categories explicitly,
- especially around state, relations, capabilities, and call kinds.

What Tabula may still borrow later:

- a stronger "ability-like" classification for capabilities and other external
  semantic surfaces.

---

## 4. Additional Research Directions Worth Considering

The following directions look more promising for Tabula than adding a fully
general higher-order effect language.

### 4.1 Coeffects and context demands

One of the strongest additional candidates comes from the coeffects literature.

Informally:

- an **effect** describes how a computation acts on the world,
- a **coeffect** describes what a computation requires from its context.

This maps surprisingly well onto Tabula because Tabula already distinguishes:

- tx-local arguments,
- and per-instance or per-batch external `context`.

That suggests a static split such as:

- effects for state, relations, capabilities, events, and failure,
- demands for external context dependencies.

Potential Tabula value:

- separates "reads mutable world state" from "depends on instance-global input",
- makes query legality and helper-call propagation cleaner,
- may improve caching, incremental recomputation, and external-input diagnostics,
- and gives `context` a first-class static meaning instead of treating it as
  just another variable namespace.

Possible future shape:

```rust
pub struct ContextDemandSummary {
    pub fields: BTreeSet<ContextFieldId>,
}
```

This is one of the most attractive additions because Tabula already has the
surface concept needed for it.

### 4.2 Obligation summaries instead of only `may_fail`

The current `may_fail: bool` is a good V1 choice, but it is intentionally
coarse.

Research in effect refinements, indexed effects, and specification-aware effect
systems suggests a stronger alternative:

- track not only that a computation may fail,
- but **why** guarded treatment is needed.

For Tabula, the most useful form is probably not a full theorem-proving calculus
but a compact obligation summary such as:

- `NeedsAssertDischarge`
- `NeedsNonZeroDivisor`
- `NeedsCapabilityPrecondition`
- `NeedsRelationTotality`
- `NeedsBoundsCheck`

Potential Tabula value:

- more precise guarded lowering,
- better diagnostics,
- clearer legality checking for `query`,
- and a cleaner bridge to future `requires` / `ensures` without collapsing the
  spec layer into the execution core.

Possible future shape:

```rust
pub enum ObligationKind {
    Assert,
    NonZero,
    CapabilityPrecondition(CapabilityId),
    RelationTotality(RelationId),
}

pub struct ObligationSummary {
    pub kinds: BTreeSet<ObligationKind>,
}
```

This looks more valuable than a heavy full-spec core language while still
strictly improving on a single bit.

### 4.3 Footprint-indexed effects

The current effect summary classifies **families** of effects:

- state read,
- state write,
- relation use,
- capability call,
- and so on.

Because Tabula is closed-world, the compiler can often know much more:

- which table fields are read,
- which table fields are written,
- which relations are referenced,
- which capabilities are called,
- which events may be emitted.

This suggests an effect summary indexed by a closed-world footprint.

Potential Tabula value:

- more precise helper-function summaries,
- better legality checks,
- better conflict detection and future parallel scheduling,
- clearer journal schema planning,
- and stronger optimizer invariants in MIR.

Possible future shape:

```rust
pub struct FootprintSummary {
    pub state_reads: BTreeSet<StateSlotId>,
    pub state_writes: BTreeSet<StateSlotId>,
    pub state_deletes: BTreeSet<StateSlotId>,
    pub relations: BTreeSet<RelationId>,
    pub capabilities: BTreeSet<CapabilityId>,
    pub events: BTreeSet<EventId>,
    pub state_properties: BTreeSet<StatePropertyKey>,
}
```

For Tabula, this may be more valuable than classic open-world effect
polymorphism because the language already owns the full semantic universe.

### 4.4 Lightweight phase-ordered effects

Another candidate comes from sequential-effect research.

The current effect summary is essentially order-insensitive:

- it records what kinds of effects may happen,
- but not the relative order in which they happen.

That is often fine.

However, Tabula is:

- journal-first,
- control-lowering aware,
- and already concerned with legality and phase discipline in MIR.

So a weak ordered summary may help, even if a full sequential-effect algebra is
overkill.

A practical Tabula-specific middle ground would be a small phase discipline
such as:

1. `Observe`
2. `Check`
3. `Mutate`
4. `Publish`

Example intent:

- reads, relation evaluation, state-property reads, and query-safe capabilities
  happen in `Observe`,
- assertions and checked preconditions happen in `Check`,
- writes and deletes happen in `Mutate`,
- event emission happens in `Publish`.

Potential Tabula value:

- easier legality checking,
- clearer canonical lowering expectations,
- cleaner event and journal discipline,
- and a better story for future diagnostics such as "emit occurs before all
  checked mutations are resolved".

This looks promising precisely because Tabula is not trying to be a general CFG
language.

### 4.5 Static capabilities plus static effects

The capability design space likely still has more to offer.

In many systems, capabilities and effects are two ways of tackling a related
problem:

- what actions are permitted,
- and under what static conditions.

Tabula already wants capability descriptors richer than raw signatures.

A stronger next step would be to treat each capability descriptor as a bundle of
static policy:

- value signature,
- totality,
- query-safety,
- proof-observability,
- journal family,
- phase family,
- and perhaps required context demands.

Possible future shape:

```rust
pub struct CapabilityDescriptor {
    pub inputs: Vec<Type>,
    pub outputs: Vec<Type>,
    pub totality: Totality,
    pub call_policy: CapabilityCallPolicy,
    pub journal_policy: JournalPolicy,
    pub phase: CapabilityPhase,
    pub context_requirements: BTreeSet<ContextFieldId>,
}
```

This would make capability calls fit the rest of the static model much more
cleanly.

### 4.6 Quantitative or bounded summaries

Because Tabula is intentionally not Turing complete:

- no unrestricted loops,
- no recursion,
- bounded control,
- and fixed proof-shape aspirations,

it may later support something that many general-purpose languages cannot infer
well:

- useful static upper bounds on semantic activity.

Examples:

- maximum number of state writes,
- maximum number of emitted events,
- maximum number of relation uses,
- maximum number of capability calls,
- and rough proof-front-end sizing estimates.

Potential Tabula value:

- cost estimation,
- proof planning,
- journal sizing,
- scheduler hints,
- and future fee or resource models.

This belongs later than ordinary effect typing, but the language shape makes it
a plausible future extension rather than a theoretical curiosity.

---

## 5. A Stronger Tabula-Specific Synthesis

The most interesting outcome of this research pass is not a single imported
feature.

It is the possibility that Tabula's best long-term static model is:

1. **value typing**
2. **effect typing**
3. **context-demand typing**
4. **obligation typing**
5. **closed-world footprint typing**
6. and later **bounded quantitative summaries**

In other words, a callable body may eventually need more than:

```rust
pub struct EffectSummary {
    pub world: WorldEffects,
    pub proof: ProofEffects,
    pub may_fail: bool,
}
```

It may ultimately want something more like:

```rust
pub struct StaticSummary {
    pub effects: EffectSummary,
    pub demands: ContextDemandSummary,
    pub obligations: ObligationSummary,
    pub footprint: FootprintSummary,
    pub phase: Option<PhaseSummary>,
    pub bounds: Option<BoundSummary>,
}
```

This would still be much simpler than full research-grade effect machinery.

But it would better reflect what Tabula actually cares about:

- which semantic surfaces are touched,
- what external context is required,
- what must be guarded,
- what may be journaled,
- and what proof-facing or runtime-facing bounds can be derived.

This looks especially aligned with Tabula because:

- execution already aims to emit a canonical typed effect journal,
- runtime proving already reduces that journal into proof-plan-aligned data,
- and MIR already needs explicit semantic legality and guarded-lowering support.

That combination makes Tabula unusually receptive to a richer static summary.

---

## 6. What Looks Most Novel For Tabula

No claim is made here that the following combination is globally unprecedented
in programming-language research.

However, within the current design space reviewed for Tabula, the most
distinctive synthesis appears to be:

> **a closed-world, proof-aware, journal-first DSL whose static semantics track
> not only effects, but also context demands, obligations, and closed-world
> semantic footprints.**

The most Tabula-specific part of that sentence is not "effect system".

It is the combination of:

- proof-observable semantics,
- explicit journal families,
- closed-world manifests,
- bounded control,
- and runtime reduction from semantic journal to proof input.

That suggests a useful design principle:

> **Tabula should optimize for semantic traceability, not for maximal effect
> language generality.**

This pushes design effort toward:

- precise summaries over a finite known universe,
- rather than open-ended source-level effect abstraction.

---

## 7. Recommended Adoption Order

The following order seems strongest.

### 7.1 Keep immediately

- the current value typing + effect typing split,
- the world / proof / failure distinction,
- `fn` / `query` / `tx` as callable categories,
- and MIR-owned inferred summaries.

### 7.2 Add next

- richer capability metadata,
- `ContextDemandSummary`,
- and a more informative `ObligationSummary`.

These appear to offer the best benefit-to-complexity ratio.

### 7.3 Add after that

- closed-world footprint summaries,
- especially for state fields, relations, capabilities, and events.

This likely becomes more valuable as MIR optimization and executor scheduling
grow more sophisticated.

### 7.4 Defer unless clearly needed

- lightweight phase ordering,
- and bounded quantitative summaries.

These are promising, but they should not delay the core rewrite.

---

## 8. What Tabula Should Probably Not Import

The following ideas are interesting in research systems but do not currently
look like the right next step for Tabula.

- full user-facing row-polymorphic effect syntax
- first-class algebraic effect handlers
- a proof-assistant-style weakest-precondition execution core
- a fully general sequential-effect algebra in V1
- open-world higher-order effect abstraction as a primary design goal

The reason is not that these ideas are weak.

The reason is that Tabula's strongest constraints are different:

- closed-world semantics,
- a small fixed canonical IR,
- executor/prover co-design,
- and deterministic journal reduction.

Those constraints reward compact semantic summaries more than maximal effect
expressiveness.

---

## 9. Open Questions

The following questions remain worth answering in later design work.

- Should `context` demands be part of callable signatures, or inferred only
  internally?
- Should obligation summaries remain coarse categories, or retain source-linked
  evidence?
- How precise should footprint summaries be:
  - effect family only,
  - declaration ID,
  - or per-table-field slot?
- Should journal family be derived from effect class, or explicitly declared in
  capability metadata?
- Is a phase summary useful enough to justify itself before bounded loops exist?
- Should future quantitative summaries be exact bounds, conservative upper
  bounds, or only rough cost classes?

---

## 10. Research Leads

The following references appear most relevant for the next round of design
thinking.

- Koka:
  [Programming with Row-Polymorphic Effect Types](https://www.microsoft.com/en-us/research/publication/koka-programming-with-row-polymorphic-effect-types/)
- Flix:
  [Effect System](https://doc.flix.dev/effect-system.html),
  [Effect Polymorphism](https://doc.flix.dev/effect-polymorphism.html)
- F*:
  [Computational Effects](https://fstar-lang.org/tutorial/book/part4/part4.html),
  [Primitive Effect Refinements](https://fstar-lang.org/tutorial/book/part4/part4_pure.html)
- Move:
  [The Move Reference](https://move-book.com/reference/),
  [Abilities](https://move-book.com/reference/abilities/)
- Coeffects:
  [Coeffects: Unified Static Analysis of Context-Dependence](https://tomasp.net/academic/papers/coeffects/),
  [Coeffects: A Calculus of Context-Dependent Computation](https://kar.kent.ac.uk/57493/)
- Indexed or parameterized effects:
  [Parameterised Notions of Computation](https://pure.strath.ac.uk/ws/portalfiles/portal/112786548/Atkey_MSFP_2006_Parameterised_notions_of_computation.pdf)
- Sequential effects:
  [A Generic Approach to Flow-Sensitive Polymorphic Effects](https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.ECOOP.2017.13),
  [Polymorphic Iterable Sequential Effect Systems](https://www.cs.drexel.edu/~csg63/publications/toplas21/)
- Static capabilities and effects:
  [Designing with Static Capabilities and Effects](https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.ECOOP.2020.10)
- Quantitative or graded systems:
  [The Syntax and Semantics of Quantitative Type Theory](https://strathprints.strath.ac.uk/64031/),
  [Graded Modal Dependent Type Theory](https://pmc.ncbi.nlm.nih.gov/articles/PMC7984552/)

---

## 11. What This Note Commits To

This note is intended to settle the following research-level conclusions.

- The current Tabula typing/effect direction remains strong.
- The biggest remaining opportunities are not "more general effect syntax".
- Coeffects or context-demand tracking appear highly relevant to Tabula.
- A richer obligation summary is likely more useful than a permanent
  `may_fail: bool`.
- Closed-world footprint summaries may fit Tabula better than classic open-world
  effect polymorphism.
- Lightweight phase ordering and quantitative summaries are promising later
  extensions, not immediate core requirements.
- The most distinctive long-term Tabula static model may be:
  - effects,
  - context demands,
  - obligations,
  - footprints,
  - and later bounded summaries,
  rather than effect typing alone.

If that synthesis is right, then the next design question is no longer only:

- how should Tabula classify effects?

It becomes:

- how should MIR and canonical IR carry a compact but expressive static summary
  over a closed semantic universe?
